use std::sync::Arc;

use log::{error, trace};
use tokio::sync::{mpsc, oneshot};

use crate::{
    request::{AppendRequest, AppendResponse, ReadRequest, ReadResponse, Request, TrimRequest},
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

        match rx.await {
            Ok(result) => result,
            Err(e) => {
                error!("Failed to receive append response from internal stream manager: {e}");
                Err(ReplicationError::RpcTimeout)
            }
        }
    }

    pub async fn read(&self, request: ReadRequest) -> Result<ReadResponse, ReplicationError> {
        let (tx, rx) = oneshot::channel();
        let req = Request::Read { tx, request };
        if let Err(e) = self.tx.send(req) {
            error!("Failed to dispatch request to stream-manager {e}");
            return Err(ReplicationError::Internal);
        }
        trace!("Submitted read request to internal stream manager and await response");

        match rx.await {
            Ok(result) => result,
            Err(e) => {
                error!("Failed to receive read response from stream manager: {e}");
                Err(ReplicationError::RpcTimeout)
            }
        }
    }

    pub async fn start_offset(&self, stream_id: u64) -> Result<u64, ReplicationError> {
        let (tx, rx) = oneshot::channel();
        let req = Request::StartOffset {
            request: stream_id,
            tx,
        };
        if let Err(e) = self.tx.send(req) {
            error!("Failed to dispatch request to stream-manager {e}");
            return Err(ReplicationError::Internal);
        }
        trace!("Submitted min offset request to internal stream manager and await response");

        match rx.await {
            Ok(result) => result,
            Err(e) => {
                error!("Failed to receive read response from stream manager: {e}");
                Err(ReplicationError::RpcTimeout)
            }
        }
    }

    pub async fn next_offset(&self, stream_id: u64) -> Result<u64, ReplicationError> {
        let (tx, rx) = oneshot::channel();
        let req = Request::NextOffset {
            request: stream_id,
            tx,
        };
        if let Err(e) = self.tx.send(req) {
            error!("Failed to dispatch request to stream-manager {e}");
            return Err(ReplicationError::Internal);
        }
        trace!("Submitted next offset request to internal stream manager and await response");

        match rx.await {
            Ok(result) => result,
            Err(e) => {
                error!("Failed to receive read response from stream manager: {e}");
                Err(ReplicationError::RpcTimeout)
            }
        }
    }

    pub async fn trim(&self, request: TrimRequest) -> Result<(), ReplicationError> {
        let (tx, rx) = oneshot::channel();
        let req = Request::Trim { request, tx };
        if let Err(e) = self.tx.send(req) {
            error!("Failed to dispatch request to stream-manager {e}");
            return Err(ReplicationError::Internal);
        }
        trace!("Submitted trim request to internal stream manager and await response");

        match rx.await {
            Ok(result) => result,
            Err(e) => {
                error!("Failed to receive read response from stream manager: {e}");
                Err(ReplicationError::RpcTimeout)
            }
        }
    }
}
