use std::io::IoSlice;

use model::stream::StreamMetadata;
use tokio::sync::mpsc;

use crate::{command::Command, AppendResult, ClientError};

pub struct Stream {
    metadata: StreamMetadata,
    tx: mpsc::UnboundedSender<Command>,
}

impl Stream {
    pub(crate) fn new(metadata: StreamMetadata, tx: mpsc::UnboundedSender<Command>) -> Self {
        Self { metadata, tx }
    }

    pub async fn min_offset(&self) -> Result<i64, ClientError> {
        todo!()
    }

    pub async fn max_offset(&self) -> Result<i64, ClientError> {
        todo!()
    }

    /// Append data to the stream.
    ///
    /// # Arguments
    ///
    /// `data` - Encoded representation of the `RecordBatch`.
    pub async fn append(&self, data: IoSlice<'_>) -> Result<AppendResult, ClientError> {
        todo!()
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
        todo!()
    }
}
