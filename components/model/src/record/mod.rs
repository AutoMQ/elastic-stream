pub mod flat_record;
use crate::error::RecordError;
use bytes::Bytes;
use chrono::prelude::*;
use protocol::flat_model::records::{KeyValueT, RecordBatchMetaT};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct RecordBatch {
    batch_meta: RecordBatchMetaT,
    batch_payload: Bytes,
}

impl RecordBatch {
    /// Create a new record batch with the given meta and payload.
    fn new(batch_meta: RecordBatchMetaT, batch_payload: Bytes) -> Self {
        Self {
            batch_meta,
            batch_payload,
        }
    }

    /// New a builder to build a record batch.
    pub fn new_builder() -> RecordBatchBuilder {
        RecordBatchBuilder::default()
    }

    /// Return the stream id of the record batch.
    pub fn stream_id(&self) -> i64 {
        self.batch_meta.stream_id
    }

    /// Return the range index of the record batch.
    pub fn range_index(&self) -> i32 {
        self.batch_meta.range_index
    }

    pub fn base_timestamp(&self) -> i64 {
        self.batch_meta.base_timestamp
    }
}

#[derive(Debug, Default)]
pub struct RecordBatchBuilder {
    stream_id: Option<i64>,
    range_index: Option<i32>,
    flags: Option<i16>,
    base_offset: Option<i64>,
    last_offset_delta: Option<i32>,
    base_timestamp: Option<i64>,
    properties: Option<HashMap<String, String>>,
    batch_payload: Option<Bytes>,
}

impl RecordBatchBuilder {
    pub fn with_stream_id(mut self, stream_id: i64) -> Self {
        self.stream_id = Some(stream_id);
        self
    }

    pub fn with_range_index(mut self, range_index: i32) -> Self {
        self.range_index = Some(range_index);
        self
    }

    pub fn with_flags(mut self, flags: i16) -> Self {
        self.flags = Some(flags);
        self
    }

    pub fn with_base_offset(mut self, base_offset: i64) -> Self {
        self.base_offset = Some(base_offset);
        self
    }

    pub fn with_last_offset_delta(mut self, last_offset_delta: i32) -> Self {
        self.last_offset_delta = Some(last_offset_delta);
        self
    }

    pub fn with_base_timestamp(mut self, base_timestamp: i64) -> Self {
        self.base_timestamp = Some(base_timestamp);
        self
    }

    pub fn with_property(mut self, key: String, value: String) -> Self {
        let mut properties = self.properties.take().unwrap_or_default();
        properties.insert(key, value);
        self.properties = Some(properties);
        self
    }

    pub fn with_batch_payload(mut self, batch_payload: Bytes) -> Self {
        self.batch_payload = Some(batch_payload);
        self
    }

    pub fn build(self) -> Result<RecordBatch, RecordError> {
        let stream_id = self.stream_id.ok_or(RecordError::RequiredFieldMissing)?;
        let range_index = self.range_index.ok_or(RecordError::RequiredFieldMissing)?;
        let base_offset = self.base_offset.ok_or(RecordError::RequiredFieldMissing)?;
        let last_offset_delta = self
            .last_offset_delta
            .ok_or(RecordError::RequiredFieldMissing)?;
        let batch_payload = self
            .batch_payload
            .ok_or(RecordError::RequiredFieldMissing)?;

        // Convert properties to flatbuffers table
        let properties = self.properties.and_then(|map| {
            let mut vec = Vec::with_capacity(map.len());
            for (key, value) in map {
                let mut kv = KeyValueT::default();
                kv.key = key;
                kv.value = value;
                vec.push(kv);
            }
            Some(vec)
        });

        let mut batch_meta = RecordBatchMetaT::default();
        batch_meta.stream_id = stream_id;
        batch_meta.range_index = range_index;
        batch_meta.flags = self.flags.unwrap_or(0);
        batch_meta.base_offset = base_offset;
        batch_meta.last_offset_delta = last_offset_delta;
        batch_meta.base_timestamp = self.base_timestamp.unwrap_or(Utc::now().timestamp());
        batch_meta.properties = properties;

        Ok(RecordBatch {
            batch_meta,
            batch_payload,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_record_batch_builder() {
        // Build a right record batch
        let builder = RecordBatch::new_builder()
            .with_stream_id(1)
            .with_range_index(0)
            .with_base_offset(1024)
            .with_last_offset_delta(10)
            .with_batch_payload(Bytes::from("test"));
        let record_batch = builder.build().unwrap();

        assert_eq!(record_batch.stream_id(), 1);
        assert_eq!(record_batch.range_index(), 0);
        assert_eq!(record_batch.batch_meta.base_offset, 1024);
        assert_eq!(record_batch.batch_meta.last_offset_delta, 10);
        assert_eq!(record_batch.batch_payload, Bytes::from("test"));
    }
}
