use std::{cell::RefCell, cmp::min, collections::BTreeMap, ops::Bound, rc::Rc};

use bytes::{Buf, Bytes, BytesMut};
use model::{error::DecodeError, object::ObjectMetadata, record::flat_record::RecordMagic};
use opendal::{services::Fs, Operator};
use protocol::flat_model::records::RecordBatchMeta;

use crate::error::ObjectReadError;
use tokio::sync::oneshot;

pub(crate) trait RangeObjectReader {
    async fn read_block(
        &self,
        start_offset: u64,
        end_offset: Option<u64>,
        size_hint: u32,
        hint: Option<ReadBlockHint>,
        object_metadata_manager: &RangeObjectMetadataManager,
    ) -> Result<Vec<ObjectBlock>, ObjectReadError>;
}

#[derive(Debug)]
pub(crate) struct DefaultRangeObjectReader {
    object_reader: Rc<ObjectReader>,
}

impl DefaultRangeObjectReader {
    pub(crate) fn new(object_reader: Rc<ObjectReader>) -> Self {
        Self { object_reader }
    }
}

impl RangeObjectReader for DefaultRangeObjectReader {
    async fn read_block(
        &self,
        mut start_offset: u64,
        end_offset: Option<u64>,
        mut size_hint: u32,
        mut hint: Option<ReadBlockHint>,
        object_metadata_manager: &RangeObjectMetadataManager,
    ) -> Result<Vec<ObjectBlock>, ObjectReadError> {
        let mut all_object_blocks = vec![];
        loop {
            let mut object_blocks = vec![];

            // try get range object to satisfy read_block
            let rst = object_metadata_manager.find(start_offset, end_offset, size_hint, hint);
            for (object_metadata, range) in rst {
                let mut object_read_rst = self.object_reader.read(&object_metadata, range).await?;
                object_blocks.append(&mut object_read_rst);
            }

            if object_blocks.is_empty() {
                break;
            }

            let first = object_blocks.first_mut().unwrap();
            first.remove_before(start_offset);

            let last = object_blocks.last().unwrap();
            start_offset = last.end_offset;
            let last_block_end_position = last.end_position;

            let blocks_len = object_blocks.iter().map(|b| b.len()).sum();
            size_hint -= min(size_hint, blocks_len);

            all_object_blocks.append(&mut object_blocks);

            // check whether read enough. if not read enough, continue read.
            let size_hint_satisfy = size_hint == 0;
            let end_offset_satisfy = end_offset
                .map(|end_offset| start_offset >= end_offset)
                .unwrap_or(false);
            if size_hint_satisfy || end_offset_satisfy {
                break;
            }

            hint = Some(ReadBlockHint {
                prev_block_end_position: last_block_end_position,
            });
        }
        Ok(all_object_blocks)
    }
}

pub(crate) struct ObjectBlock {
    records: Vec<ObjectBlockRecord>,
    end_offset: u64,
    end_position: u32,
}

impl ObjectBlock {
    fn remove_before(&mut self, start_offset: u64) {
        self.records
            .retain(|r| (r.start_offset + r.end_offset_delta as u64) > start_offset);
    }

    pub(crate) fn get_records(&self, start_offset: u64, end_offset: u64) -> Vec<ObjectBlockRecord> {
        let mut records = vec![];
        for record in &self.records {
            let record_end_offset = record.start_offset + record.end_offset_delta as u64;
            if record_end_offset <= start_offset {
                continue;
            }
            if record.start_offset >= end_offset {
                break;
            }
            records.push(record.clone());
        }
        records
    }

    fn len(&self) -> u32 {
        self.records.iter().map(|r| r.data.len()).sum::<usize>() as u32
    }
}

#[derive(Clone)]
pub(crate) struct ObjectBlockRecord {
    pub(crate) start_offset: u64,
    pub(crate) end_offset_delta: u32,
    pub(crate) data: Bytes,
}

#[derive(Clone)]
pub(crate) struct ReadBlockHint {
    prev_block_end_position: u32,
}

#[derive(Debug)]
pub(crate) struct ObjectReader {
    op: Operator,
}

impl ObjectReader {
    pub(crate) fn new() -> Self {
        // new op from config
        let mut builder = Fs::default();
        builder.root("/tmp");
        let op: Operator = Operator::new(builder).unwrap().finish();
        Self { op }
    }

    async fn read(
        &self,
        object: &ObjectMetadata,
        range: (u32, u32),
    ) -> Result<Vec<ObjectBlock>, ObjectReadError> {
        // // TODO: dispatch task to different thread.
        let (tx, rx) = oneshot::channel();
        let object_key = object.key.clone().unwrap();
        self.read0(object_key, range, object.data_len, tx);
        rx.await.expect("object read rx await fail")
    }

    fn read0(
        &self,
        object_key: String,
        range: (u32, u32),
        _object_data_len: u32,
        tx: oneshot::Sender<Result<Vec<ObjectBlock>, ObjectReadError>>,
    ) {
        let op = self.op.clone();
        tokio_uring::spawn(async move {
            // TODO: 最后一个 block 可能没有数据，每个 block 必须读到数据,object_reader 负责。
            let rst = op
                .range_read(&object_key, range.0 as u64..range.1 as u64)
                .await;
            let rst = match rst {
                Ok(b) => match Self::parse_block(Bytes::from(b), range.0, 1024 * 1024 * 1024) {
                    Ok(blocks) => Ok(blocks),
                    Err(_) => Err(ObjectReadError::Unexpected(crate::Error::new(
                        0,
                        "parse block fail",
                    ))),
                },
                Err(_) => Err(ObjectReadError::ReqStoreFail(crate::Error::new(
                    0,
                    "req store fail",
                ))),
            };
            let _ = tx.send(rst);
        });
    }

    /// Parse read range object bytes to object blocks with block size.
    /// The object cache will add and free in object block dimension.
    fn parse_block(
        bytes: Bytes,
        start_position: u32,
        block_size: u32,
    ) -> Result<Vec<ObjectBlock>, DecodeError> {
        let mut cursor = bytes.clone();
        let mut blocks = vec![];
        let mut block = ObjectBlock {
            records: vec![],
            end_offset: 0,
            end_position: 0,
        };
        let mut block_length = 0;
        let mut relative_position = 0;
        let mut reach_end = false;
        loop {
            if block_length >= block_size {
                blocks.push(block);
                block = ObjectBlock {
                    records: vec![],
                    end_offset: 0,
                    end_position: 0,
                };
                block_length = 0;
            }
            if cursor.remaining() < 9 || reach_end {
                if !block.records.is_empty() {
                    blocks.push(block);
                }
                break;
            }
            let magic_code = cursor.get_i8();
            if magic_code != RecordMagic::Magic0 as i8 {
                return Err(DecodeError::InvalidMagic);
            }
            let metadata_len = cursor.get_i32() as usize;
            if cursor.remaining() < metadata_len {
                reach_end = true;
                continue;
            }
            let metadata_slice = cursor.slice(0..metadata_len);
            let metadata = flatbuffers::root::<RecordBatchMeta>(&metadata_slice)?;
            let start_offset = metadata.base_offset() as u64;
            let end_offset_delta = metadata.last_offset_delta() as u32;
            cursor.advance(metadata_len);
            let payload_len = cursor.get_i32();
            if cursor.remaining() < payload_len as usize {
                reach_end = true;
                continue;
            }
            cursor.advance(payload_len as usize);

            // deep copy the bytes to quick free memory when record don't need.
            let record_len = 1 + 4 + metadata_len + 4 + payload_len as usize;
            let mut record_bytes = BytesMut::with_capacity(record_len);
            record_bytes
                .copy_from_slice(&bytes.slice(relative_position..(relative_position + record_len)));
            block.records.push(ObjectBlockRecord {
                start_offset,
                end_offset_delta,
                data: record_bytes.freeze(),
            });

            relative_position += record_len;
            block.end_offset = start_offset + end_offset_delta as u64;
            block.end_position = start_position + relative_position as u32;
            block_length += record_len as u32;
        }
        Ok(blocks)
    }
}

#[derive(Debug)]
pub(crate) struct RangeObjectMetadataManager {
    metadata_map: RefCell<BTreeMap<u64, ObjectMetadata>>,
}

impl RangeObjectMetadataManager {
    pub(crate) fn new() -> Self {
        Self {
            metadata_map: RefCell::new(BTreeMap::new()),
        }
    }

    #[allow(dead_code)]
    pub(crate) fn add_object_metadata(&self, metadata: &ObjectMetadata) {
        let mut metadata_map = self.metadata_map.borrow_mut();
        metadata_map
            .entry(metadata.start_offset)
            .or_insert_with(|| metadata.clone());
    }

    fn find(
        &self,
        mut start_offset: u64,
        end_offset: Option<u64>,
        mut size_hint: u32,
        mut hint: Option<ReadBlockHint>,
    ) -> Vec<(ObjectMetadata, (u32, u32))> {
        let mut objects = vec![];
        let metadata_map = self.metadata_map.borrow();
        let cursor = metadata_map.upper_bound(Bound::Included(&start_offset));
        let mut position = None;
        loop {
            // check whether read enough. if not read enough, continue read.
            let size_hint_satisfy = size_hint == 0;
            let end_offset_satisfy = end_offset
                .map(|end_offset| start_offset >= end_offset)
                .unwrap_or(false);
            if size_hint_satisfy || end_offset_satisfy {
                break;
            }

            let object = if let Some(object) = cursor.value() {
                object
            } else {
                break;
            };
            let object_end_offset = object.start_offset + object.end_offset_delta as u64;
            if object_end_offset <= start_offset {
                // object is before start_offset
                break;
            }
            if let Some(h) = hint {
                // Read block hint only use once to determine the position of the first object.
                // - If the first object's start_offset is equal to start_offset, it means read move
                // to the next object.
                // - Else, it means read still in the previous read object, and we can use the prev block
                // end position as current start position.
                if start_offset != object.start_offset {
                    position = Some(h.prev_block_end_position);
                }
                hint = None;
            }
            if let Some(range) = object.find_bound(start_offset, end_offset, size_hint, position) {
                size_hint -= min(size_hint, range.1 - range.0);
                objects.push((object.clone(), range));
                // move to next object
                start_offset = object_end_offset;
                // only the first object need position hint, the following object default position to zero.
                position = None;
            } else {
                panic!("object#find_bound should not return None");
            }
        }
        objects
    }
}
