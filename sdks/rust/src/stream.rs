use std::io::IoSlice;

use bytes::Bytes;
use model::stream::StreamMetadata;
use replication::StreamClient;

use crate::{AppendResult, ClientError};

pub struct Stream {
    metadata: StreamMetadata,
    stream_client: StreamClient,
}

impl Stream {
    pub(crate) fn new(metadata: StreamMetadata, stream_client: StreamClient) -> Self {
        debug_assert!(metadata.stream_id.is_some(), "stream-id should be present");
        Self {
            metadata,
            stream_client,
        }
    }

    pub async fn min_offset(&self) -> Result<i64, ClientError> {
        self.stream_client
            .min_offset(
                self.metadata
                    .stream_id
                    .expect("stream-id should be present"),
            )
            .await
            .map(|v| v as i64)
            .map_err(Into::into)
    }

    pub async fn max_offset(&self) -> Result<i64, ClientError> {
        self.stream_client
            .max_offset(
                self.metadata
                    .stream_id
                    .expect("stream-id should be present"),
            )
            .await
            .map(|v| v as i64)
            .map_err(Into::into)
    }

    /// Append data to the stream.
    ///
    /// # Arguments
    ///
    /// `data` - Encoded representation of the `RecordBatch`.
    pub async fn append(&self, data: IoSlice<'_>, count: u32) -> Result<AppendResult, ClientError> {
        let request = replication::request::AppendRequest {
            stream_id: self.metadata.stream_id.unwrap(),
            data: Bytes::copy_from_slice(&data),
            count,
        };
        self.stream_client
            .append(request)
            .await
            .map(|response| AppendResult {
                base_offset: response.offset as i64,
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
        end_offset: i32,
        batch_max_bytes: i32,
    ) -> Result<IoSlice<'_>, ClientError> {
        let request = replication::request::ReadRequest {
            stream_id: self.metadata.stream_id.unwrap(),
            start_offset: start_offset as u64,
            end_offset: end_offset as u64,
            batch_max_bytes: batch_max_bytes as u32,
        };
        self.stream_client
            .read(request)
            .await
            .map(|_response| {
                let data = "mock";
                IoSlice::new(data.as_bytes())
            })
            .map_err(Into::into)
    }
}
