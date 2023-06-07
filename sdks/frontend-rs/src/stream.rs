use bytes::Bytes;
use log::{error, info, trace};
use model::record::flat_record::FlatRecordBatch;
use replication::StreamClient;

use crate::{AppendResult, ClientError};

pub struct Stream {
    id: u64,
    stream_client: StreamClient,
}

impl Stream {
    pub(crate) fn new(id: u64, stream_client: StreamClient) -> Self {
        Self { id, stream_client }
    }

    pub async fn start_offset(&self) -> Result<i64, ClientError> {
        self.stream_client
            .start_offset(self.id)
            .await
            .map(|v| v as i64)
            .map_err(Into::into)
    }

    pub async fn next_offset(&self) -> Result<i64, ClientError> {
        self.stream_client
            .next_offset(self.id)
            .await
            .map(|v| v as i64)
            .map_err(Into::into)
    }

    /// Append data to the stream.
    ///
    /// # Arguments
    ///
    /// `buffer` - Encoded representation of the `RecordBatch`. It contains exactly one append entry.
    pub async fn append(&self, mut buffer: Bytes) -> Result<AppendResult, ClientError> {
        let record_batch = FlatRecordBatch::decode_to_record_batch(&mut buffer).map_err(|e| {
            error!("Invalid record batch {e:?}");
            ClientError::Internal("".to_owned())
        })?;
        trace!("RecordBatch to append: {record_batch:?}");

        debug_assert_eq!(
            self.id,
            record_batch.stream_id() as u64,
            "Stream ID should be identical"
        );
        let count = record_batch.last_offset_delta();
        let request = replication::request::AppendRequest {
            stream_id: self.id,
            record_batch,
        };

        self.stream_client
            .append(request)
            .await
            .map(|response| {
                trace!(
                    "{count} records appended to stream[id={}], base offset={}",
                    self.id,
                    response.offset
                );
                AppendResult {
                    base_offset: response.offset as i64,
                }
            })
            .map_err(Into::into)
    }

    /// Read data from the stream.
    ///
    /// # Arguments
    /// `start_offset` - The start offset of the first record to be read.
    /// `end_offset` - The exclusive end offset of last records to be read.
    /// `max_bytes` - The maximum number of bytes to be read.
    ///
    /// # Returns
    /// The data read from the stream.
    pub async fn read(
        &self,
        start_offset: i64,
        end_offset: i64,
        batch_max_bytes: i32,
    ) -> Result<Vec<Bytes>, ClientError> {
        trace!(
            "Reading records from stream[id={}], start-offset={}, end-offset={}",
            self.id,
            start_offset,
            end_offset
        );
        let request = replication::request::ReadRequest {
            stream_id: self.id,
            start_offset: start_offset as u64,
            end_offset: end_offset as u64,
            batch_max_bytes: batch_max_bytes as u32,
        };
        self.stream_client
            .read(request)
            .await
            .map(|mut response| {
                let total: usize = response.data.iter().map(|buf| buf.len()).sum();
                trace!("{total} bytes read from stream[id={}]", self.id);
                std::mem::take(&mut response.data)
            })
            .map_err(Into::into)
    }
    /// Close stream.
    ///
    /// # Arguments
    ///
    /// # Returns
    ///
    pub async fn close(&self) -> Result<(), ClientError> {
        let stream_id = self.id;
        info!("Closing stream[id={}]", stream_id);
        self.stream_client
            .close_stream(stream_id)
            .await
            .map_err(Into::into)
    }
}

impl Drop for Stream {
    fn drop(&mut self) {
        let client = self.stream_client.clone();
        let stream_id = self.id;
        info!("Dropping stream[id={}]", stream_id);
        tokio_uring::spawn(async move {
            match client.close_stream(stream_id).await {
                Ok(_) => {
                    info!("Closed stream[id={stream_id}]");
                }

                Err(e) => {
                    error!("Failed to close stream[id={stream_id}]. Cause: {e:?}");
                }
            }
        });
    }
}
