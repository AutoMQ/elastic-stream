use std::io::IoSlice;

use crate::{AppendResult, ClientError};

pub struct Stream {
    id: i64,
}

impl Stream {
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
