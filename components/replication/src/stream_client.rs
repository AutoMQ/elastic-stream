use std::sync::Arc;

use client::error::ClientError;
use log::{error, trace};
use tokio::sync::{mpsc, oneshot};

use crate::{
    request::{AppendRequest, AppendResponse, ReadRequest, ReadResponse, Request},
    stream_manager::stream_manager::StreamManager,
    ReplicationError,
};

#[derive(Debug, Clone)]
pub struct StreamClient {
    tx: mpsc::UnboundedSender<Request>,
}

impl StreamClient {
    pub fn new(config: Arc<config::Configuration>) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        let stream_manager = StreamManager::new(config, rx);
        StreamManager::spawn_loop(stream_manager);
        Self { tx }
    }

    pub async fn append(&self, request: AppendRequest) -> Result<AppendResponse, ReplicationError> {
        let (tx, rx) = oneshot::channel();
        let req = Request::Append { tx, request };
        if let Err(e) = self.tx.send(req) {
            error!("Failed to dispatch request to stream-manager {e}");
            return Err(ReplicationError::Internal);
        }
        trace!("Submitted append request to internal stream manager and await response");

        rx.await.map_err(|e| {
            error!("Failed to receive append response from internal stream manager: {e}");
            ReplicationError::RpcTimeout
        })
    }

    pub async fn read(&self, request: ReadRequest) -> Result<ReadResponse, ReplicationError> {
        let (tx, rx) = oneshot::channel();
        let req = Request::Read { tx, request };
        if let Err(e) = self.tx.send(req) {
            error!("Failed to dispatch request to stream-manager {e}");
            return Err(ReplicationError::Internal);
        }
        trace!("Submitted read request to internal stream manager and await response");

        rx.await.map_err(|e| {
            error!("Failed to receive read response from stream manager: {e}");
            ReplicationError::RpcTimeout
        })
    }

    pub async fn min_offset(&self, stream_id: u64) -> Result<u64, ClientError> {
        todo!()
    }

    pub async fn max_offset(&self, stream_id: u64) -> Result<u64, ClientError> {
        todo!()
    }
}
