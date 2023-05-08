use std::{error::Error, net::ToSocketAddrs};

use model::{Record, RecordBatch, RecordMetadata};

/// Writes `Record`s to `Stream`s.
///
///# Examples
/// ```
/// use std::error::Error;
/// use bytes::BytesMut;
/// use front_end_sdk::Writer;
/// use model::Record;
///
/// #[tokio::main]
/// pub async fn main() -> Result<(), Box<dyn Error>> {
///     let access_point = "localhost:80";
///     let writer = Writer::new(access_point);
///
///     let body = BytesMut::with_capacity(128).freeze();
///     let record = Record::new_builder()
///         .with_stream_id(3)
///         .with_range_index(0)
///         .with_body(body)
///         .build()?;
///
///     match writer.append(&record).await {
///         Ok(receipt) => {
///             println!("Send record OK {receipt:#?}")
///         }
///         Err(e) => {
///             eprintln!("Failed to send record. Cause {e:?}");
///         }
///     }
///
///     Ok(())
/// }
/// ```
pub struct Writer {}

impl Writer {
    /// Create a new `Writer` instance to append records to `Stream`s.
    ///
    ///
    pub fn new<A>(_addr: A) -> Self
    where
        A: ToSocketAddrs,
    {
        Self {}
    }

    /// Append the specified record to stream.
    ///
    /// * `record` - A `Record` that contains headers, properties and body.
    ///
    pub async fn append(&self, record: &Record) -> Result<RecordMetadata, Box<dyn Error>> {
        Ok(RecordMetadata::new(record.stream_id(), 0))
    }

    /// Append the specified record batch to stream.
    ///
    /// * `record_batch` - A `RecordBatch` that contains a vector of record.
    pub async fn append_batch(
        &self,
        record_batch: &RecordBatch,
    ) -> Result<Vec<RecordMetadata>, Box<dyn Error>> {
        Ok(vec![RecordMetadata::new(record_batch.stream_id(), 0)])
    }
}
