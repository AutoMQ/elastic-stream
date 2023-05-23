use std::io::IoSlice;

use bytes::{Bytes, BytesMut};
use log::{error, info, trace};
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

    pub async fn min_offset(&self) -> Result<i64, ClientError> {
        self.stream_client
            .start_offset(self.id)
            .await
            .map(|v| v as i64)
            .map_err(Into::into)
    }

    pub async fn max_offset(&self) -> Result<i64, ClientError> {
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
    /// `data` - Encoded representation of the `RecordBatch`.
    pub async fn append(&self, data: IoSlice<'_>, count: u32) -> Result<AppendResult, ClientError> {
        trace!("Appending {} bytes to stream-id={}", data.len(), self.id);
        let request = replication::request::AppendRequest {
            stream_id: self.id,
            data: Bytes::copy_from_slice(&data),
            count,
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
        end_offset: i32,
        batch_max_bytes: i32,
    ) -> Result<Bytes, ClientError> {
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
            .map(|response| {
                let total = response.data.iter().map(|buf| buf.len()).sum();
                trace!("{total} bytes read from stream[id={}]", self.id);

                // TODO: Avoid copy
                if response.data.len() == 1 {
                    response.data[0].clone()
                } else {
                    let mut buf = BytesMut::with_capacity(total);
                    response.data.iter().for_each(|item| {
                        buf.extend_from_slice(&item[..]);
                    });
                    buf.freeze()
                }
            })
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
                    error!("Failed to close stream[id={stream_id}]. Cause: {e:#?}");
                }
            }
        });
    }
}
