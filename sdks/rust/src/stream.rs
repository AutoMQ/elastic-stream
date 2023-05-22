use std::io::IoSlice;

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
    pub async fn append(&self, data: IoSlice<'_>) -> Result<AppendResult, ClientError> {
        let request = replication::request::AppendRequest {};
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
    /// `offset` - The offset of the first record to be read.
    /// `limit` - The maximum number of records to be read.
    /// `max_bytes` - The maximum number of bytes to be read.
    ///
    /// # Returns
    /// The data read from the stream.
    pub async fn read(
        &self,
        offset: i64,
        limit: i32,
        max_bytes: i32,
    ) -> Result<IoSlice<'_>, ClientError> {
        let request = replication::request::ReadRequest {};
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
