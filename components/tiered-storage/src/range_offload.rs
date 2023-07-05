use bytes::Buf;
use bytes::BufMut;
use bytes::Bytes;
use bytes::BytesMut;
use log::debug;
use log::warn;
use model::error::DecodeError;
use model::object::BLOCK_DELIMITER;
use model::object::FOOTER_MAGIC;
use model::record::flat_record::RecordMagic;
use opendal::Operator;
use protocol::flat_model::records::RecordBatchMeta;
use std::cell::RefCell;
use std::io::IoSlice;
use std::rc::Rc;
use std::time::Duration;
use std::vec::Vec;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio::time::sleep;

use crate::ObjectManager;
use crate::ObjectMetadata;
use util::bytes::BytesSliceCursor;

const SPARSE_SIZE: u32 = 16 * 1024 * 1024;

/// Object format:
///   data block            => bytes
///   delimiter magic       => u8
///   sparse index *
///     relative end offset => u32
///     position            => u32
///   footer                => fix size, 48 bytes
///     sparse index pos    => u32
///     sparse index size   => u32
///     padding
///     magic               => u64
///
///
pub struct RangeOffload<M: ObjectManager + 'static> {
    stream_id: u64,
    range_index: u32,
    object_size: u32,
    multi_part_object: RefCell<Option<MultiPartObject>>,
    op: Operator,
    object_manager: Rc<M>,
}

impl<M> RangeOffload<M>
where
    M: ObjectManager + 'static,
{
    pub fn new(
        stream_id: u64,
        range_index: u32,
        op: Operator,
        object_manager: Rc<M>,
        object_size: u32,
    ) -> RangeOffload<M> {
        Self {
            stream_id,
            range_index,
            object_size,
            multi_part_object: RefCell::new(None),
            op,
            object_manager,
        }
    }

    pub fn write(&self, start_offset: u64, payload: Vec<Bytes>) -> u64 {
        // TODO: 统计 inflight bytes 来进行背压，避免网络打爆。或者通过一个全局的并发限制器。如果限制则直接返回 false，或者卡住。
        let payload_length: usize = payload.iter().map(|p| p.len()).sum();
        let payload_length = payload_length as u32;
        // TODO: key name, reverse hex representation of start_offset as prefix.
        let key = format!(
            "ess3test/{}-{}/{}",
            self.stream_id, self.range_index, start_offset
        );

        let mut multi_part_object = self.multi_part_object.borrow_mut();
        if let Some(multi_part_obj) = multi_part_object.as_ref() {
            // the last multi-part object exist, then write to it.
            let (object_full, end_offset) = multi_part_obj.write(payload);
            if object_full {
                multi_part_obj.close();
                *multi_part_object = None;
            }
            return end_offset;
        }

        if payload_length >= self.object_size {
            let object_metadata =
                ObjectMetadata::new(self.stream_id, self.range_index, start_offset);
            // direct write when payload length is larger than object_size.
            let object = Object {
                key,
                op: self.op.clone(),
                object_manager: self.object_manager.clone(),
            };
            let (end_offset, _) = object.write(payload, object_metadata);
            return end_offset;
        }

        // start a new multi-part object.
        let object_metadata = ObjectMetadata::new(self.stream_id, self.range_index, start_offset);
        let new_multi_part_object = MultiPartObject::new(
            object_metadata,
            key,
            self.op.clone(),
            self.object_manager.clone(),
            self.object_size,
        );
        let (_, end_offset) = new_multi_part_object.write(payload);
        *multi_part_object = Some(new_multi_part_object);
        end_offset
    }

    /// force inflight multi-part object to complete.
    pub fn flush(&self) {
        todo!("flush multi-part object")
    }
}

struct MultiPartObject {
    start_offset: u64,
    object_size: u32,
    size: RefCell<u32>,
    last_pass_through_size: RefCell<u32>,
    tx: mpsc::UnboundedSender<(Vec<Bytes>, Bytes, u64)>,
}

impl MultiPartObject {
    pub fn new<M: ObjectManager + 'static>(
        object_metadata: ObjectMetadata,
        key: String,
        op: Operator,
        object_manager: Rc<M>,
        object_size: u32,
    ) -> Self {
        let (tx, rx) = mpsc::unbounded_channel::<(Vec<Bytes>, Bytes, u64)>();
        let this = Self {
            start_offset: object_metadata.start_offset,
            object_size,
            size: RefCell::new(0),
            last_pass_through_size: RefCell::new(0),
            tx,
        };
        {
            let key = key;
            let op = op;
            Self::write_loop(object_metadata, key, op, object_manager, rx);
        }
        this
    }

    pub fn write(&self, part: Vec<Bytes>) -> (bool, u64) {
        let mut size = self.size.borrow_mut();
        let part_length: usize = part.iter().map(|p| p.len()).sum();
        *size += part_length as u32;

        let (index, new_end_offset, remain_pass_through_size) = gen_sparse_index(
            self.start_offset,
            &part,
            *self.last_pass_through_size.borrow(),
            SPARSE_SIZE,
        )
        .unwrap_or_else(|_| panic!("parse record fail {:?}", part));
        *self.last_pass_through_size.borrow_mut() = remain_pass_through_size;
        let _ = self.tx.send((part, index, new_end_offset));
        (*size >= self.object_size, new_end_offset)
    }

    pub fn close(&self) {
        let _ = self.tx.send((Vec::new(), Bytes::new(), 0));
    }

    fn write_loop<M: ObjectManager + 'static>(
        mut object_metadata: ObjectMetadata,
        key: String,
        op: Operator,
        object_manager: Rc<M>,
        mut rx: mpsc::UnboundedReceiver<(Vec<Bytes>, Bytes, u64)>,
    ) {
        tokio_uring::spawn(async move {
            // TODO: delay init multi-part object, if there is only one part or several parts is too small. the multi-part object is not necessary.
            // use single Object put instead to save API call.
            let mut data_len = 0;
            let mut end_offset = object_metadata.start_offset;
            let mut sparse_index = BytesMut::with_capacity(64);
            let mut writer = None;
            loop {
                match rx.recv().await {
                    Some((part, index, new_end_offset)) => {
                        sparse_index.extend(index);
                        if new_end_offset > end_offset {
                            end_offset = new_end_offset;
                        }
                        // TODO:
                        // get multi-part object writer.
                        if writer.is_none() {
                            loop {
                                writer = match op.writer(&key).await {
                                    Ok(w) => Some(w),
                                    Err(e) => {
                                        warn!("{key} get writer fail, retry later, {e}");
                                        sleep(Duration::from_secs(1)).await;
                                        continue;
                                    }
                                };
                                break;
                            }
                        }
                        let writer = writer.as_mut().unwrap();

                        // write part or complete object.
                        let payload_length: usize = part.iter().map(|p| p.len()).sum();
                        data_len += payload_length;
                        let mut bytes = BytesMut::with_capacity(payload_length);
                        for b in part.iter() {
                            bytes.extend_from_slice(b);
                        }
                        let bytes = bytes.freeze();
                        let close_part = bytes.is_empty();
                        if close_part {
                            // write index and footer.
                            let mut left_part = BytesMut::with_capacity(256);
                            left_part.put_u8(BLOCK_DELIMITER);
                            let index_len = sparse_index.len();
                            left_part.extend_from_slice(&sparse_index);
                            left_part
                                .extend_from_slice(&gen_footer(data_len as u32, index_len as u32));
                            let left_part = left_part.freeze();
                            loop {
                                match writer.write(left_part.clone()).await {
                                    Ok(_) => {
                                        debug!("{key} write multi-part object footer success");
                                        break;
                                    }
                                    Err(e) => {
                                        warn!("{key} write footer fail, retry later, {e}");
                                        sleep(Duration::from_secs(1)).await;
                                        continue;
                                    }
                                }
                            }
                            // complete multi-part object.
                            loop {
                                match writer.close().await {
                                    Ok(_) => {
                                        debug!("{key} complete multi-part object success");
                                        object_metadata.end_offset_delta =
                                            (end_offset - object_metadata.start_offset) as u32;
                                        object_manager.commit_object(object_metadata);
                                        break;
                                    }
                                    Err(e) => {
                                        warn!("{key} complete multi-part object fail, retry later, {e}");
                                        sleep(Duration::from_secs(1)).await;
                                        continue;
                                    }
                                }
                            }
                            break;
                        } else {
                            loop {
                                match writer.write(bytes.clone()).await {
                                    Ok(_) => {
                                        debug!("{key} write multi-part object part[len={payload_length}] success");
                                        break;
                                    }
                                    Err(e) => {
                                        warn!("{key} write part fail, retry later, {e}");
                                        sleep(Duration::from_secs(1)).await;
                                        continue;
                                    }
                                }
                            }
                        }
                    }
                    None => {
                        todo!("handle rx close")
                    }
                }
            }
        });
    }
}

struct Object<M: ObjectManager + 'static> {
    key: String,
    op: Operator,
    object_manager: Rc<M>,
}

impl<M> Object<M>
where
    M: ObjectManager + 'static,
{
    pub fn write(
        &self,
        payload: Vec<Bytes>,
        object_metadata: ObjectMetadata,
    ) -> (u64, JoinHandle<()>) {
        self.write0(payload, object_metadata, SPARSE_SIZE)
    }

    pub fn write0(
        &self,
        payload: Vec<Bytes>,
        mut object_metadata: ObjectMetadata,
        sparse_size: u32,
    ) -> (u64, JoinHandle<()>) {
        let key = self.key.clone();
        let op = self.op.clone();
        let object_manager = self.object_manager.clone();
        let (sparse_index, end_offset, _) =
            gen_sparse_index(object_metadata.start_offset, &payload, 0, sparse_size)
                .unwrap_or_else(|_| panic!("parse record fail {:?}", payload));
        let join_handle = tokio_uring::spawn(async move {
            object_metadata.end_offset_delta = (end_offset - object_metadata.start_offset) as u32;
            // data block
            let payload_length: usize = payload.iter().map(|p| p.len()).sum();
            let mut bytes =
                BytesMut::with_capacity(payload_length + 256 /* sparse index + footer */);
            for b in payload.iter() {
                bytes.extend_from_slice(b);
            }
            // delimiter
            bytes.put_u8(BLOCK_DELIMITER);
            // sparse index
            let sparse_index_len = sparse_index.len();
            bytes.extend_from_slice(&sparse_index);
            // footer
            bytes.extend_from_slice(&gen_footer(payload_length as u32, sparse_index_len as u32));
            loop {
                debug!("{key} start write object[len={payload_length}]");
                match op.write(&key, bytes.clone()).await {
                    Ok(_) => {
                        debug!("{key} write object success[len={payload_length}]");
                        object_manager.commit_object(object_metadata);
                        break;
                    }
                    Err(e) => {
                        // retry later until success.
                        warn!("{key} write object, retry later, {e}");
                        sleep(Duration::from_secs(1)).await;
                        continue;
                    }
                }
            }
        });
        (end_offset, join_handle)
    }
}

/// sparse index format:
///   (
///   record relative end offset: u32,
///   position: u32,
///   )*
///
/// return (sparse index bytes, record end offset, remain pass through size)
fn gen_sparse_index(
    start_offset: u64,
    payload: &Vec<Bytes>,
    init_pass_through_size: u32,
    sparse_size: u32,
) -> Result<(Bytes, u64, u32), DecodeError> {
    let mut slices = payload
        .iter()
        .map(|b| IoSlice::new(&b[..]))
        .collect::<Vec<_>>();
    let mut cursor = BytesSliceCursor::new(&mut slices);
    let mut sparse_index_bytes = BytesMut::with_capacity(payload.len() / sparse_size as usize);
    let mut pass_through_size = init_pass_through_size;
    let mut record_position = 0;
    let mut last_record_size = 0;
    // construct a sparse index after pass through sparse size records.
    loop {
        if !cursor.has_remaining() {
            break;
        }
        if cursor.remaining() < 9 {
            return Err(DecodeError::DataLengthMismatch);
        }
        let magic_code = cursor.get_i8();
        if magic_code != RecordMagic::Magic0 as i8 {
            return Err(DecodeError::InvalidMagic);
        }
        let metadata_len = cursor.get_i32() as usize;
        if cursor.remaining() < metadata_len {
            return Err(DecodeError::DataLengthMismatch);
        }
        record_position += last_record_size;
        let record_sparse_index = pass_through_size >= sparse_size;
        if record_sparse_index {
            let mut metadata_slice = BytesMut::zeroed(metadata_len);
            cursor.copy_to_slice(&mut metadata_slice);
            let metadata = flatbuffers::root::<RecordBatchMeta>(&metadata_slice)?;
            let record_end_offset =
                metadata.base_offset() as u64 + metadata.last_offset_delta() as u64;
            sparse_index_bytes.put_u32((record_end_offset - start_offset) as u32);
            sparse_index_bytes.put_u32(record_position);
        } else {
            cursor.advance(metadata_len);
        }
        let payload_len = cursor.get_i32() as usize;
        if cursor.remaining() < payload_len {
            return Err(DecodeError::DataLengthMismatch);
        }
        cursor.advance(payload_len);
        last_record_size = (9 + metadata_len + payload_len) as u32;
        if record_sparse_index {
            pass_through_size = 0;
        } else {
            pass_through_size += last_record_size;
        }
    }
    // get last record end_offset
    let mut slices = payload
        .iter()
        .map(|b| IoSlice::new(&b[..]))
        .collect::<Vec<_>>();
    let mut cursor = BytesSliceCursor::new(&mut slices);
    cursor.advance((record_position + 1) as usize);
    let metadata_len = cursor.get_i32() as usize;
    let mut metadata_slice = BytesMut::zeroed(metadata_len);
    cursor.copy_to_slice(&mut metadata_slice);
    let metadata = flatbuffers::root::<RecordBatchMeta>(&metadata_slice)?;
    let end_offset = metadata.base_offset() as u64 + metadata.last_offset_delta() as u64;
    Ok((sparse_index_bytes.freeze(), end_offset, pass_through_size))
}

/// footer                => fix size, 48 bytes
///   sparse index pos    => u32
///   sparse index size   => u32
///   padding
///   magic               => u64
fn gen_footer(data_len: u32, index_len: u32) -> Bytes {
    let mut footer = BytesMut::with_capacity(48);
    footer.put_u32(data_len + 1 /* delimiter magic */);
    footer.put_u32(index_len);
    footer.put_bytes(0, 40 - 8);
    footer.put_u64(FOOTER_MAGIC);
    footer.freeze()
}

#[cfg(test)]
mod tests {
    use model::{record::flat_record::FlatRecordBatch, RecordBatch};
    use opendal::services::Fs;

    use crate::MockObjectManager;

    use super::*;

    #[test]
    fn test_sparse_index() {
        let mut payload: Vec<Bytes> = Vec::new();
        for i in 0..10 {
            let builder = RecordBatch::new_builder()
                .with_stream_id(1)
                .with_range_index(0)
                .with_base_offset(233 + i * 10)
                .with_last_offset_delta(10)
                .with_payload(Bytes::from("test"));
            let record_batch = builder.build().unwrap();
            let flat: FlatRecordBatch = record_batch.into();
            // encoded size = 69
            let (mut encoded, _) = flat.encode();
            payload.append(&mut encoded);
        }
        let (mut sparse_index, end_offset, remain) =
            gen_sparse_index(233, &payload, 0, 100).unwrap();

        assert_eq!(333, end_offset);
        assert_eq!(69, remain);

        assert_eq!(3 * 8, sparse_index.len());
        // the 3th record
        assert_eq!(30, sparse_index.get_u32());
        assert_eq!(138, sparse_index.get_u32());
        // the 6th record
        assert_eq!(60, sparse_index.get_u32());
        assert_eq!(345, sparse_index.get_u32());
        // the 9th record
        assert_eq!(90, sparse_index.get_u32());
        assert_eq!(552, sparse_index.get_u32());
    }

    #[test]
    fn test_object_write() {
        tokio_uring::start(async move {
            let mut fs_builder = Fs::default();
            fs_builder.root("/tmp/estest/");
            let op = Operator::new(fs_builder).unwrap().finish();

            let builder = RecordBatch::new_builder()
                .with_stream_id(1)
                .with_range_index(0)
                .with_base_offset(233)
                .with_last_offset_delta(10)
                .with_payload(Bytes::from("test"));
            let record_batch = builder.build().unwrap();
            let flat: FlatRecordBatch = record_batch.into();
            // encoded size = 69
            let (encoded, _) = flat.encode();

            let mut object_manager = MockObjectManager::new();
            object_manager
                .expect_commit_object()
                .times(1)
                .returning(|_| ());

            let obj = Object {
                key: "test_object_write".to_string(),
                op: op.clone(),
                object_manager: Rc::new(object_manager),
            };

            let object_metadata = ObjectMetadata::new(1, 0, 233);
            let (end_offset, join_handle) = obj.write(encoded.clone(), object_metadata);
            assert_eq!(243, end_offset);
            join_handle.await.unwrap();

            // data
            let read_data = op.read("test_object_write").await.unwrap();
            let record_slice = &read_data[0..69];
            let record_batch =
                FlatRecordBatch::decode_to_record_batch(&mut Bytes::copy_from_slice(record_slice))
                    .unwrap();
            assert_eq!(1, record_batch.stream_id());
            assert_eq!(0, record_batch.range_index());
            assert_eq!(233, record_batch.base_offset());
            assert_eq!(10, record_batch.last_offset_delta());
            let mut buf = Bytes::copy_from_slice(&read_data[69..]);
            assert_eq!(BLOCK_DELIMITER, buf.get_u8());
            // empty index
            // footer
            assert_eq!(48, buf.remaining());
            assert_eq!(70, buf.get_u32());
            assert_eq!(0, buf.get_u32());
            buf.advance(32);
            assert_eq!(FOOTER_MAGIC, buf.get_u64());
        });
    }
}
