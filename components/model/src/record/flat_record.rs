use bytes::{Buf, BufMut, Bytes, BytesMut};
use flatbuffers::FlatBufferBuilder;
use protocol::flat_model::records::RecordBatchMeta;
use std::io::Cursor;

use crate::{error::DecodeError, RecordBatch};

#[derive(Debug)]
#[repr(i8)]
pub enum RecordMagic {
    Magic0 = 0x22, // The first version of the record batch format.
}

/// Relative offset of `BaseOffset` within `RecordBatch`.
pub const BASE_OFFSET_POS: usize = 1;

// The minimum length of the record batch, which at least includes the magic byte, the metadata length and payload length.
pub const MIN_RECORD_BATCH_LEN: usize = 1 + 4 + 4;

/// FlatRecordBatch is a flattened version of RecordBatch that is used for serialization.
/// As the storage and network layout, the schema of FlatRecordBatch with magic 0 is given below:
///
/// RecordBatch =>
///  Magic => Int8
///  MetaLength => Int32
///  Meta => RecordBatchMeta
///  PayloadLength => Int32
///  BatchPayload => Bytes
///
/// Currently, the payload of the record batch is a raw bytes buffer, we may support a specific format in the future.
///
/// The RecordBatchMeta is complying with the layout of the FlatBuffers schema, please refer to the model.fbs in the protocol crate.
#[derive(Debug, Default)]
pub struct FlatRecordBatch {
    pub magic: Option<i8>,
    pub metadata: Bytes,
    pub payload: Bytes,
}

impl FlatRecordBatch {
    /// Converts a RecordBatch to a FlatRecordBatch.
    fn init_from_struct(record_batch: RecordBatch) -> Self {
        // Build up a serialized buffer for the specific record_batch.
        // Initialize it with a capacity of 1024 bytes.
        let mut fbb = FlatBufferBuilder::with_capacity(1024);

        // Serialize the meta of RecordBatch to the FlatBuffer
        // The returned value is an offset used to track the location of this serialized data.
        let meta_offset = record_batch.metadata.pack(&mut fbb);

        // Serialize the root of the object, without providing a file identifier.
        fbb.finish(meta_offset, None);
        let meta_buf = fbb.finished_data();

        FlatRecordBatch {
            magic: Some(RecordMagic::Magic0 as i8),
            metadata: Bytes::copy_from_slice(meta_buf),
            payload: record_batch.payload,
        }
    }

    /// Inits a FlatRecordBatch from a buffer of bytes received from storage or network layer.
    ///
    /// * Side effect: the position of the given buf will be advanced after a successful init, if any error occurs, the position will not be changed.
    pub fn init_from_buf(buf: &mut Bytes) -> Result<Self, DecodeError> {
        let mut cursor = Cursor::new(&buf[..]);
        if cursor.remaining() < MIN_RECORD_BATCH_LEN {
            return Err(DecodeError::DataLengthMismatch);
        }

        // Read the magic
        let magic = cursor.get_i8();

        // We only support the Magic0 for now.
        if magic != RecordMagic::Magic0 as i8 {
            return Err(DecodeError::InvalidMagic);
        }

        // Read the metadata length from the given buf
        let metadata_len = cursor.get_i32() as usize;
        if cursor.remaining() < metadata_len + 4 {
            return Err(DecodeError::DataLengthMismatch);
        }
        cursor.advance(metadata_len);

        // Read the payload length from the given buf
        let payload_len = cursor.get_i32() as usize;
        if cursor.remaining() != payload_len {
            return Err(DecodeError::DataLengthMismatch);
        }

        // Slice metadata
        let metadata_from =  1 /* magic-code */ + 4 /* metadata length field */;
        let metadata_to = 1 /* magic-code */ + 4 /* metadata length field */ + metadata_len;
        let metadata = buf.slice(metadata_from..metadata_to);

        // Slice payload
        let payload_from = 1 /* magic-code */ + 4 /* metadata length field */ + metadata_len + 4 /* payload length field */;
        let payload_to = payload_from + payload_len;
        let payload = buf.slice(payload_from..payload_to);

        buf.advance(payload_to);

        Ok(FlatRecordBatch {
            magic: Some(magic),
            metadata,
            payload,
        })
    }

    /// Encodes the flat_record_batch to a vector of bytes, which can be sent to the storage or network layer.
    ///
    /// # Return
    ///  - The first element is the encoded bytes of the flat_record_batch.
    ///  - The second element is the total length of the encoded bytes.
    pub fn encode(self) -> (Vec<Bytes>, i32) {
        let mut bytes_vec = Vec::new();

        // The total length of encoded flat records.
        let meta_len = self.metadata.len();
        let mut total_len = MIN_RECORD_BATCH_LEN;

        let mut batch_prefix = BytesMut::with_capacity(total_len);
        batch_prefix.put_i8(self.magic.unwrap_or(RecordMagic::Magic0 as i8));
        batch_prefix.put_i32(meta_len as i32);

        // Insert the prefix to the first element of the bytes_vec.
        bytes_vec.insert(0, batch_prefix.freeze());

        // Push the meta buffer to the bytes_vec.
        bytes_vec.push(self.metadata);
        total_len += meta_len;

        // Push the payload length to the bytes_vec.
        let mut payload_len_buf = BytesMut::with_capacity(4);
        payload_len_buf.put_i32(self.payload.len() as i32);
        bytes_vec.push(payload_len_buf.freeze());

        // Push the payload to the bytes_vec.
        total_len += self.payload.len();
        bytes_vec.push(self.payload);

        (bytes_vec, total_len as i32)
    }

    pub fn decode(self) -> Result<RecordBatch, DecodeError> {
        let batch_meta = root_as_record_batch_meta(self.metadata.as_ref())
            .map_err(|_| DecodeError::InvalidDataFormat)?;

        let batch_meta_t = batch_meta.unpack();

        Ok(RecordBatch {
            metadata: batch_meta_t,
            payload: self.payload,
        })
    }
}

impl From<RecordBatch> for FlatRecordBatch {
    fn from(record_batch: RecordBatch) -> Self {
        FlatRecordBatch::init_from_struct(record_batch)
    }
}

/// Verifies that a buffer of bytes contains a `RecordBatchMeta` and returns it.
/// For this unchecked behavior to be maximally performant, use unchecked function
fn root_as_record_batch_meta(
    buf: &[u8],
) -> Result<RecordBatchMeta, flatbuffers::InvalidFlatbuffer> {
    flatbuffers::root::<RecordBatchMeta>(buf)
}

#[cfg(test)]
mod tests {
    use crate::{payload::Payload, record::RecordBatchBuilder};

    use super::*;

    #[test]
    fn test_encode_and_decode_of_flat_batch() {
        let stream_id = 1 as i64;

        let batch = RecordBatchBuilder::default()
            .with_stream_id(stream_id)
            .with_range_index(0)
            .with_base_offset(1024)
            .with_last_offset_delta(10)
            .with_payload(Bytes::from("hello world"))
            .with_property("key".to_string(), "value".to_string())
            .build()
            .unwrap();

        // Init a FlatRecordBatch from original RecordBatch
        let flat_batch = FlatRecordBatch::init_from_struct(batch);

        assert_eq!(flat_batch.magic, Some(RecordMagic::Magic0 as i8));

        // Encode the above flat_batch to Vec[Bytes]
        let (bytes_vec, mut total_len) = flat_batch.encode();
        let mut bytes_mute = BytesMut::with_capacity(total_len as usize);
        for ele in bytes_vec {
            total_len -= ele.len() as i32;
            bytes_mute.put_slice(&ele);
        }

        assert_eq!(total_len, 0);

        // Decode the above bytes to FlatRecordBatch
        let mut batch_buf = bytes_mute.freeze();

        if let (Some(entry), len) = Payload::parse_append_entry(&batch_buf).unwrap() {
            assert_eq!(len, batch_buf.len());
            assert_eq!(entry.stream_id as i64, stream_id);
            assert_eq!(entry.index, 0);
            assert_eq!(entry.offset, 1024);
            assert_eq!(entry.len, 10);
        } else {
            panic!("parse_append_entry failed");
        }

        let flat_batch = FlatRecordBatch::init_from_buf(&mut batch_buf).unwrap();
        let mut record_batch = flat_batch.decode().unwrap();

        // Side effect test: the position of batch_buf should be advanced to the end.
        assert_eq!(batch_buf.len(), 0);

        assert_eq!(record_batch.stream_id(), stream_id);
        assert_eq!(record_batch.range_index(), 0);
        assert_eq!(record_batch.metadata.base_offset, 1024);
        assert_eq!(record_batch.metadata.last_offset_delta, 10);
        assert_eq!(record_batch.payload, Bytes::from("hello world"));

        let properties = record_batch.metadata.properties.take();
        assert!(properties.is_some());

        let properties = properties.unwrap();
        assert_eq!(properties.len(), 1);
        assert_eq!(properties.get(0).unwrap().key, "key");
        assert_eq!(properties.get(0).unwrap().value, "value");
    }

    #[test]
    fn test_decode_error() {
        let mut bytes_mut = BytesMut::with_capacity(10);
        bytes_mut.put_i32(1024); // Length
        bytes_mut.put_i32(10); // Data

        let result = FlatRecordBatch::init_from_buf(&mut bytes_mut.freeze());
        assert_eq!(result.is_err(), true);
    }
}
