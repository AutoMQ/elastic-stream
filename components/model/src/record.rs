use crate::{error::RecordError, header::Headers, range};
use bytes::Bytes;
use chrono::prelude::*;
use std::collections::HashMap;
#[derive(Debug, Clone, PartialEq)]
pub struct Record {
    stream_id: i64,
    range_index: i32,
    headers: Headers,
    properties: HashMap<String, String>,
    body: Bytes,
}

#[derive(Debug, Clone)]
pub struct RecordBatch {
    stream_id: i64,
    range_index: i32,
    base_timestamp: i64,
    records: Vec<Record>,
}

impl Record {
    fn new(stream_id: i64, range_index: i32, body: Bytes) -> Self {
        let mut record = Self {
            stream_id: stream_id,
            range_index: range_index,
            headers: Headers::new(),
            properties: HashMap::new(),
            body,
        };
        record.headers.add_header(
            crate::header::Common::CreatedAt,
            Utc::now().timestamp().to_string(),
        );
        record
    }

    pub fn new_builder() -> RecordBuilder {
        RecordBuilder::default()
    }

    /// Returns the stream id that the record belongs to.
    pub fn stream_id(&self) -> i64 {
        self.stream_id
    }

    /// Returns the range index that the record belongs to.
    pub fn range_index(&self) -> i32 {
        self.range_index
    }

    pub fn keys(&self) -> Option<&String> {
        self.headers.get_header(crate::header::Common::Keys)
    }

    pub fn tag(&self) -> Option<&String> {
        self.headers.get_header(crate::header::Common::Tag)
    }

    pub fn record_id(&self) -> Option<&String> {
        self.headers.get_header(crate::header::Common::RecordId)
    }

    pub fn created_at(&self) -> Option<&String> {
        self.headers.get_header(crate::header::Common::CreatedAt)
    }

    /// Returns the body of the record and take the ownership.
    pub fn take_body(self) -> Bytes {
        self.body
    }

    pub fn body(&self) -> &Bytes {
        &self.body
    }

    /// Adds a key-value property pair into the properties.
    ///
    /// If the underlying map did not have this key present, [`None`] is returned.
    ///
    /// If the map did have this key present, the value is updated, and the old
    /// value is returned.
    pub fn add_property(&mut self, key: String, value: String) -> Option<String> {
        self.properties.insert(key, value)
    }

    /// Adds a Common key-value header pair into the headers.
    /// The behavior is the same as `add_property`.
    pub fn add_header(&mut self, key: crate::header::Common, value: String) -> Option<String> {
        self.headers.add_header(key, value)
    }

    /// Returns a iterator that iterates over the headers.
    pub fn headers_iter(&self) -> impl Iterator<Item = (&crate::header::Common, &String)> {
        self.headers.iter()
    }

    /// Returns a iterator that iterates over the properties.
    pub fn properties_iter(&self) -> impl Iterator<Item = (&String, &String)> {
        self.properties.iter()
    }
}

impl RecordBatch {
    fn new(stream_id: i64, range_index: i32, records: Vec<Record>) -> Self {
        Self {
            stream_id,
            range_index,
            records,
            base_timestamp: 0,
        }
    }

    pub fn new_builder() -> RecordBatchBuilder {
        RecordBatchBuilder::default()
    }

    pub fn stream_id(&self) -> i64 {
        self.stream_id
    }

    pub fn range_index(&self) -> i32 {
        self.range_index
    }

    pub fn base_timestamp(&self) -> i64 {
        self.base_timestamp
    }

    pub fn records(&self) -> &Vec<Record> {
        &self.records
    }

    // Take the ownership of the records.
    pub fn take_records(self) -> Vec<Record> {
        self.records
    }
}

#[derive(Debug, Default)]
pub struct RecordBatchBuilder {
    stream_id: Option<i64>,
    range_index: Option<i32>,
    records: Vec<Record>,
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

    pub fn add_record(mut self, record: Record) -> Self {
        self.records.push(record);
        self
    }

    pub fn build(self) -> Result<RecordBatch, RecordError> {
        let stream_id = self.stream_id.ok_or(RecordError::RequiredFieldMissing)?;
        let range_index = self.range_index.ok_or(RecordError::RequiredFieldMissing)?;
        let mut base_timestamp = 0;

        for record in self.records.iter() {
            if record.stream_id != stream_id {
                Err(RecordError::StreamIdMismatch)?
            }
            if base_timestamp == 0 {
                base_timestamp = match record
                    .created_at()
                    .unwrap_or(&"0".to_string())
                    .parse::<i64>()
                {
                    Ok(it) => it,
                    Err(_) => return Err(RecordError::ParseHeader),
                };
            }
        }

        let mut record_batch = RecordBatch::new(stream_id, range_index, self.records);
        record_batch.base_timestamp = base_timestamp;
        Ok(record_batch)
    }
}

#[derive(Debug, Default)]
pub struct RecordBuilder {
    body: Option<Bytes>,
    stream_id: Option<i64>,
    range_index: Option<i32>,
    keys: Option<String>,
    tag: Option<String>,
    record_id: Option<String>,
}

impl RecordBuilder {
    pub fn with_body(mut self, body: Bytes) -> Self {
        self.body = Some(body);
        self
    }

    pub fn with_stream_id(mut self, stream_id: i64) -> Self {
        self.stream_id = Some(stream_id);
        self
    }

    pub fn with_range_index(mut self, range_index: i32) -> Self {
        self.range_index = Some(range_index);
        self
    }

    /// Associate multiple keys with the record.
    /// The keys are separated by space.
    pub fn with_keys(mut self, keys: String) -> Self {
        self.keys = Some(keys);
        self
    }

    /// Associate a tag with the record
    pub fn with_tag(mut self, tag: String) -> Self {
        self.tag = Some(tag);
        self
    }

    /// Associate a record id with the record
    /// The record id acts as primary key of a record, can be used to do a search.
    /// But it is not necessary, since the store uses the stream-id/offset to identify the record in the stream.
    pub fn with_record_id(mut self, record_id: String) -> Self {
        self.record_id = Some(record_id);
        self
    }

    pub fn build(self) -> Result<Record, RecordError> {
        let body = self.body.ok_or(RecordError::RequiredFieldMissing)?;
        let stream_id = self.stream_id.ok_or(RecordError::RequiredFieldMissing)?;
        let range_index = self.range_index.ok_or(RecordError::RequiredFieldMissing)?;
        let mut record = Record::new(stream_id, range_index, body);
        if let Some(keys) = self.keys {
            record.add_header(crate::header::Common::Keys, keys);
        }

        if let Some(tag) = self.tag {
            record.add_header(crate::header::Common::Tag, tag);
        }

        if let Some(record_id) = self.record_id {
            record.add_header(crate::header::Common::RecordId, record_id);
        }
        Ok(record)
    }
}

#[derive(Debug)]
pub struct RecordMetadata {
    /// The stream the record was saved into
    pub stream_id: i64,

    /// The offset of the record in the stream.
    pub offset: i64,

    /// Timestamp when the record was received and persisted by `ElasticStore`.
    pub timestamp: i64,
}

impl RecordMetadata {
    pub fn new(stream_id: i64, offset: i64) -> Self {
        Self {
            stream_id,
            offset,
            timestamp: Utc::now().timestamp(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_record_new() {
        let record = Record::new(1, 0, Bytes::from("hello"));
        assert_eq!(record.stream_id(), 1);
        assert_eq!(record.range_index(), 0);
        assert!(record.created_at().is_some());
        // Assert that the record's other attributes are empty
        assert!(record.keys().is_none());
        assert!(record.tag().is_none());
        assert!(record.record_id().is_none());
    }

    #[test]
    fn test_record_add_property() {
        let mut record = Record::new(2, 0, Bytes::from("world"));
        let old_value = record.add_property("foo".to_string(), "bar".to_string());
        assert_eq!(old_value, None);
        // Add another property key-value pair to the record, overwrite the previous value
        let old_value = record.add_property("foo".to_string(), "baz".to_string());
        // Assert that the old value is "bar", because there was such a key and value before
        assert_eq!(old_value, Some("bar".to_string()));
    }

    #[test]
    fn test_record_batch_new() {
        // Create an empty record batch, pass in stream ID
        let batch_empty = RecordBatch::new(3, 0, vec![]);
        assert_eq!(batch_empty.stream_id(), 3);
        assert_eq!(batch_empty.range_index(), 0);
        assert_eq!(batch_empty.records().len(), 0);
        // Create two new records
        let record1 = Record::new(3, 0, Bytes::from("foo"));
        let record2 = Record::new(3, 0, Bytes::from("bar"));

        let batch_two = RecordBatch::new(3, 0, vec![record1.clone(), record2.clone()]);
        // Assert that there are two records in the batch
        assert_eq!(batch_two.records().len(), 2);
        assert_eq!(batch_two.records()[0], record1);
        assert_eq!(batch_two.records()[1], record2);
    }

    #[test]
    fn test_record_builder() {
        let record = RecordBuilder::default()
            .with_stream_id(1)
            .with_range_index(0)
            .with_body(Bytes::from("hello"))
            .with_keys("foo bar".to_string())
            .with_tag("baz".to_string())
            .with_record_id("123".to_string())
            .build()
            .unwrap();
        assert_eq!(record.stream_id(), 1);
        assert_eq!(record.range_index(), 0);
        assert_eq!(record.body(), &Bytes::from("hello"));
        assert_eq!(record.keys().unwrap(), "foo bar");
        assert_eq!(record.tag().unwrap(), "baz");
        assert_eq!(record.record_id().unwrap(), "123");
    }

    #[test]
    fn test_record_batch_builder() {
        let record1 = RecordBuilder::default()
            .with_stream_id(1)
            .with_range_index(0)
            .with_body(Bytes::from("hello"))
            .with_keys("foo bar".to_string())
            .with_tag("baz".to_string())
            .with_record_id("123".to_string())
            .build()
            .unwrap();
        let base_timestamp = record1.created_at().unwrap().parse::<i64>().unwrap();
        let record2 = RecordBuilder::default()
            .with_stream_id(1)
            .with_range_index(0)
            .with_body(Bytes::from("world"))
            .with_keys("foo bar".to_string())
            .with_tag("baz".to_string())
            .with_record_id("123".to_string())
            .build()
            .unwrap();
        let batch = RecordBatchBuilder::default()
            .with_stream_id(1)
            .with_range_index(0)
            .add_record(record1)
            .add_record(record2)
            .build()
            .unwrap();
        assert_eq!(batch.stream_id(), 1);
        assert_eq!(batch.range_index(), 0);
        assert_eq!(batch.records().len(), 2);
        assert_eq!(batch.base_timestamp, base_timestamp);
    }
}
