use std::str::FromStr;

use bytes::{Buf, BufMut, Bytes, BytesMut};

use chrono::Utc;
use flatbuffers::FlatBufferBuilder;
use protocol::flat_model::records::{
    KeyValue, KeyValueArgs, RecordBatchMeta, RecordBatchMetaArgs, RecordBatchMetaT, RecordMeta,
    RecordMetaArgs,
};

use crate::{error::DecodeError, header::Common, Record, RecordBatch};

enum RecordMagic {
    Magic0 = 0x22, // The first version of the record batch format.
}

/// Relative offset of `BaseOffset` within `RecordBatch`.
pub const BASE_OFFSET_POS: usize = 1;

/// FlatRecordBatch is a flattened version of RecordBatch that is used for serialization.
/// As the storage and network layout, the schema of FlatRecordBatch with magic 0 is given below:
///
/// RecordBatch =>
///  Magic => Int8
///  BaseOffset => Int64
///  MetaLength => Int32
///  Meta => RecordBatchMeta
///  Records => [Record]
///
/// The record schema is given below:
/// Record =>
///   MetaLength => Int32
///   BodyLength => Int32
///   Meta => RecordMeta
///   Body => Bytes
///
/// The RecordMeta and RecordBatchMeta are complying with the layout of the FlatBuffers schema, other contents are managed by ourselves.
#[derive(Debug, Default)]
pub struct FlatRecordBatch {
    pub magic: Option<i8>,
    // The base offset of the record batch.
    // Note: the meta_buffer already contains the base_offset,
    // but we need to store it here since the rust flatbuffers doesn't support update the value so far.
    pub base_offset: Option<i64>,
    pub meta_buffer: Bytes,
    pub records: Vec<FlatRecord>,
}

impl FlatRecordBatch {
    /// Converts a RecordBatch to a FlatRecordBatch.
    pub fn init_from_struct(record_batch: RecordBatch) -> Self {
        // Build up a serialized buffer for the specific record_batch.
        // Initialize it with a capacity of 1024 bytes.
        let mut builder = FlatBufferBuilder::with_capacity(1024);

        // Make a RecordBatchMeta
        let args = RecordBatchMetaArgs {
            stream_id: record_batch.stream_id(),
            range_index: record_batch.range_index(),
            base_timestamp: record_batch.base_timestamp(),
            last_offset_delta: record_batch.records().len() as i32,
            base_offset: 0, // The base offset will be set by the storage.
            flags: 0,       // We now only support the default flag.
            properties: None,
        };

        // Serialize the data to the FlatBuffer
        // The returned value is an offset used to track the location of this serialized data.
        let record_batch_meta_offset = RecordBatchMeta::create(&mut builder, &args);

        // Serialize the root of the object, without providing a file identifier.
        builder.finish(record_batch_meta_offset, None);

        let result_buf = builder.finished_data();

        let mut flat_record_batch = FlatRecordBatch {
            magic: Some(RecordMagic::Magic0 as i8),
            base_offset: None, // The base offset will be set by the storage.
            meta_buffer: Bytes::copy_from_slice(result_buf),
            records: Vec::new(),
        };

        let base_timestamp = record_batch.base_timestamp();

        for (index, record) in record_batch.take_records().into_iter().enumerate() {
            // Reuse the builder
            builder.reset();

            let mut headers = Vec::new();
            let mut properties = Vec::new();
            for (key, value) in record.headers_iter() {
                let args = KeyValueArgs {
                    key: Some(builder.create_string(&key.to_string())),
                    value: Some(builder.create_string(value)),
                };
                headers.push(KeyValue::create(&mut builder, &args));
            }

            for (key, value) in record.properties_iter() {
                let args = KeyValueArgs {
                    key: Some(builder.create_string(key)),
                    value: Some(builder.create_string(value)),
                };
                properties.push(KeyValue::create(&mut builder, &args));
            }

            let now = Utc::now().timestamp();
            let record_timestamp = record
                .created_at()
                .map_or(now, |ts| ts.parse().unwrap_or(now));

            let args = RecordMetaArgs {
                offset_delta: index as i32,
                timestamp_delta: (record_timestamp - base_timestamp) as i32,
                headers: Some(builder.create_vector(headers.as_slice())),
                properties: Some(builder.create_vector(properties.as_slice())),
            };

            let record_meta_offset = RecordMeta::create(&mut builder, &args);
            builder.finish(record_meta_offset, None);
            let result_buf = builder.finished_data();

            let flat_record = FlatRecord {
                meta_buffer: Bytes::copy_from_slice(result_buf),
                body: record.take_body(),
            };
            flat_record_batch.records.push(flat_record);
        }

        flat_record_batch
    }

    /// Inits a FlatRecordBatch from a buffer of bytes received from storage or network layer.
    pub fn init_from_buf(mut buf: Bytes) -> Result<Self, DecodeError> {
        let basic_part_len = 13;
        if buf.len() < basic_part_len {
            return Err(DecodeError::DataLengthMismatch);
        }
        // Backup the buffer for the slice operation.
        let buf_for_slice = buf.slice(0..);
        let mut cur_ptr = 0;

        // Read the magic
        let magic = buf.get_i8();
        // Read the records count
        let base_offset = buf.get_i64();
        // Read the meta length
        let meta_len = buf.get_i32();

        cur_ptr += basic_part_len;
        if buf.len() < meta_len as usize {
            return Err(DecodeError::DataLengthMismatch);
        }
        // Read the meta buffer
        let meta_buffer = buf_for_slice.slice(cur_ptr..(cur_ptr + meta_len as usize));

        buf.advance(meta_len as usize);
        cur_ptr += meta_len as usize;

        let mut flat_record_batch = FlatRecordBatch {
            magic: Some(magic),
            meta_buffer,
            base_offset: Some(base_offset),
            records: Vec::new(),
        };

        while !buf.is_empty() {
            let record_basic_len = 4 + 4;
            if buf.len() < record_basic_len {
                return Err(DecodeError::DataLengthMismatch);
            }
            // Read the meta length
            let meta_len = buf.get_i32();

            // Read the body length
            let body_len = buf.get_i32();

            if buf.len() < meta_len as usize + body_len as usize {
                return Err(DecodeError::DataLengthMismatch);
            }

            cur_ptr += record_basic_len;
            // Read the meta buffer
            let meta_buffer = buf_for_slice.slice(cur_ptr..(cur_ptr + meta_len as usize));
            buf.advance(meta_len as usize);
            cur_ptr += meta_len as usize;

            // Read the body buffer
            let body = buf_for_slice.slice(cur_ptr..(cur_ptr + body_len as usize));
            buf.advance(body_len as usize);
            cur_ptr += body_len as usize;

            let flat_record = FlatRecord { meta_buffer, body };
            flat_record_batch.records.push(flat_record);
        }

        Ok(flat_record_batch)
    }

    // Encodes the flat_record_batch to a vector of bytes, which can be sent to the storage or network layer.
    // The first element of the returned vector of bytes, while the second element is the total length of the encoded bytes.
    pub fn encode(self) -> (Vec<Bytes>, i32) {
        let mut bytes_vec = Vec::new();

        // The total length of encoded flat records.
        let mut total_len = 13;
        let meta_len = self.meta_buffer.len();

        let mut basic_part = BytesMut::with_capacity(total_len);
        basic_part.put_i8(self.magic.unwrap_or(0));
        basic_part.put_i64(self.base_offset.unwrap_or(0));
        basic_part.put_i32(meta_len as i32);

        bytes_vec.insert(0, basic_part.freeze());

        // Store the Magic to MetaLength
        bytes_vec.push(self.meta_buffer);
        total_len += meta_len;

        for record in self.records {
            let mut meta_part = BytesMut::with_capacity(8);
            meta_part.put_i32(record.meta_buffer.len() as i32);
            meta_part.put_i32(record.body.len() as i32);

            total_len += 8 + record.meta_buffer.len() + record.body.len();
            bytes_vec.push(meta_part.freeze());
            bytes_vec.push(record.meta_buffer);
            bytes_vec.push(record.body);
        }

        (bytes_vec, total_len as i32)
    }

    pub fn decode(self) -> Result<RecordBatch, DecodeError> {
        let mut record_batch_builder = RecordBatch::new_builder();

        let batch_meta = root_as_record_batch_meta_unchecked(self.meta_buffer.as_ref());
        let stream_id = batch_meta.stream_id();
        let range_index = batch_meta.range_index();
        record_batch_builder = record_batch_builder
            .with_stream_id(stream_id)
            .with_range_index(range_index);

        for flat_record in self.records {
            let record_meta = root_as_record_meta_unchecked(flat_record.meta_buffer.as_ref());
            let mut record = Record::new_builder()
                .with_stream_id(stream_id)
                .with_range_index(range_index)
                .with_body(flat_record.body)
                .build()?;

            if let Some(properties) = record_meta.properties() {
                for property in properties {
                    if let Some(key) = property.key() {
                        if let Some(value) = property.value() {
                            record.add_property(key.to_string(), value.to_string());
                        }
                    }
                }
            }

            if let Some(headers) = record_meta.headers() {
                for header in headers {
                    if let (Some(key), Some(value)) = (header.key(), header.value()) {
                        if let Ok(header_key) = Common::from_str(key) {
                            record.add_header(header_key, value.to_string());
                        }
                    }
                }
            }
            record_batch_builder = record_batch_builder.add_record(record);
        }
        let record_batch = record_batch_builder.build()?;
        Ok(record_batch)
    }

    pub fn set_base_offset(&mut self, base_offset: i64) {
        self.base_offset = Some(base_offset);
    }

    /// For test purpose only.
    pub fn dummy() -> Self {
        let meta = RecordBatchMetaT::default();
        let mut fbb = flatbuffers::FlatBufferBuilder::new();
        let record_batch_meta = meta.pack(&mut fbb);
        fbb.finish(record_batch_meta, None);
        let data = fbb.finished_data();
        Self {
            magic: Some(0x22),
            base_offset: Some(0),
            meta_buffer: Bytes::copy_from_slice(data),
            records: vec![],
        }
    }
}

/// FlatRecord is a flattened version of Record that is used for serialization and zero deserialization.
#[derive(Debug, Default)]
pub struct FlatRecord {
    pub meta_buffer: Bytes,
    pub body: Bytes,
}

/// Verifies that a buffer of bytes contains a `RecordBatchMeta` and returns it.
/// For this unchecked behavior to be maximally performant, use unchecked function
fn root_as_record_batch_meta(
    buf: &[u8],
) -> Result<RecordBatchMeta, flatbuffers::InvalidFlatbuffer> {
    flatbuffers::root::<RecordBatchMeta>(buf)
}

/// Assumes, without verification, that a buffer of bytes contains a RecordBatchMeta and returns it.
/// # Safety
/// Callers must trust the given bytes do indeed contain a valid `RecordBatchMeta`.
fn root_as_record_batch_meta_unchecked(buf: &[u8]) -> RecordBatchMeta {
    unsafe { flatbuffers::root_unchecked::<RecordBatchMeta>(buf) }
}

/// Verifies that a buffer of bytes contains a `RecordMeta` and returns it.
/// For this unchecked behavior to be maximally performant, use unchecked function
fn root_as_record_meta(buf: &[u8]) -> Result<RecordMeta, flatbuffers::InvalidFlatbuffer> {
    flatbuffers::root::<RecordMeta>(buf)
}

/// Assumes, without verification, that a buffer of bytes contains a RecordMeta and returns it.
/// # Safety
/// Callers must trust the given bytes do indeed contain a valid `RecordMeta`.
fn root_as_record_meta_unchecked(buf: &[u8]) -> RecordMeta {
    unsafe { flatbuffers::root_unchecked::<RecordMeta>(buf) }
}

#[cfg(test)]
mod tests {
    use crate::record::{RecordBatchBuilder, RecordBuilder};

    use super::*;

    #[test]
    fn test_encode_and_decode_of_flat_batch() {
        let stream_id = 1 as i64;
        let mut record1 = RecordBuilder::default()
            .with_stream_id(stream_id)
            .with_range_index(0)
            .with_body(Bytes::from("hello"))
            .with_keys("foo bar".to_string())
            .with_tag("baz".to_string())
            .with_record_id("123".to_string())
            .build()
            .unwrap();
        record1.add_property(
            "my_property_key".to_string(),
            "my_property_value".to_string(),
        );
        record1.add_property(
            "my_another_property_key".to_string(),
            "my_another_property_value".to_string(),
        );
        let base_timestamp = record1.created_at().unwrap().parse::<i64>().unwrap();
        let mut record2 = RecordBuilder::default()
            .with_stream_id(stream_id)
            .with_range_index(0)
            .with_body(Bytes::from("world"))
            .with_keys("foo bar".to_string())
            .with_tag("baz".to_string())
            .with_record_id("234".to_string())
            .build()
            .unwrap();
        record2.add_property(
            "my_property_key".to_string(),
            "my_property_value".to_string(),
        );
        let batch = RecordBatchBuilder::default()
            .with_stream_id(stream_id)
            .with_range_index(0)
            .add_record(record1)
            .add_record(record2)
            .build()
            .unwrap();

        // Init a FlatRecordBatch from original RecordBatch
        let mut flat_batch = FlatRecordBatch::init_from_struct(batch);

        assert_eq!(flat_batch.magic, Some(RecordMagic::Magic0 as i8));
        assert_eq!(flat_batch.records.len(), 2);
        assert_eq!(flat_batch.records[0].body, Bytes::from("hello"));
        assert_eq!(flat_batch.records[1].body, Bytes::from("world"));

        // Update the base offset
        flat_batch.set_base_offset(1024 as i64);

        // Encode the above flat_batch to Vec[Bytes]
        let (bytes_vec, total_len) = flat_batch.encode();
        let mut bytes_mute = BytesMut::with_capacity(total_len as usize);
        for ele in bytes_vec {
            bytes_mute.put_slice(&ele);
        }

        // Decode the above bytes to FlatRecordBatch
        let flat_batch = FlatRecordBatch::init_from_buf(bytes_mute.freeze()).unwrap();
        // Test base_offset
        assert_eq!(flat_batch.base_offset.unwrap(), 1024 as i64);

        let record_batch = flat_batch.decode().unwrap();

        assert_eq!(record_batch.stream_id(), stream_id);
        assert_eq!(record_batch.base_timestamp(), base_timestamp);
        assert_eq!(record_batch.records().len(), 2);
        assert_eq!(record_batch.records()[0].body(), &Bytes::from("hello"));
        assert_eq!(record_batch.records()[1].body(), &Bytes::from("world"));

        // Test the properties
        let mut properties: Vec<(&String, &String)> =
            record_batch.records()[0].properties_iter().collect();
        assert_eq!(properties.len(), 2);
        properties.sort_by(|a, b| a.0.cmp(b.0));
        assert_eq!(properties[1].0, "my_property_key");
        assert_eq!(properties[1].1, "my_property_value");
        assert_eq!(properties[0].0, "my_another_property_key");
        assert_eq!(properties[0].1, "my_another_property_value");

        // Test the headers
        assert_eq!(record_batch.records()[0].tag(), Some(&"baz".to_string()));
        assert_eq!(record_batch.records()[1].tag(), Some(&"baz".to_string()));
        assert_eq!(
            record_batch.records()[0].record_id(),
            Some(&"123".to_string())
        );
        assert_eq!(
            record_batch.records()[1].record_id(),
            Some(&"234".to_string())
        );
        assert_eq!(
            record_batch.records()[0].keys(),
            Some(&"foo bar".to_string())
        );
        assert_eq!(
            record_batch.records()[1].keys(),
            Some(&"foo bar".to_string())
        );
    }

    #[test]
    fn test_decode_error() {
        let mut bytes_mut = BytesMut::with_capacity(10);
        bytes_mut.put_i32(1024); // Length
        bytes_mut.put_i32(10); // Data

        let result = FlatRecordBatch::init_from_buf(bytes_mut.freeze());
        assert_eq!(result.is_err(), true);
    }
}
