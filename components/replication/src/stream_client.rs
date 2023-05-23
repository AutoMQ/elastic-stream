use std::{sync::Arc, time::Duration};

use log::{error, info, trace};
use tokio::sync::{mpsc, oneshot};

use crate::{
    request::{
        AppendRequest, AppendResponse, CloseStreamRequest, CreateStreamRequest, OpenStreamRequest,
        ReadRequest, ReadResponse, Request, TrimRequest,
    },
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

    pub async fn create_stream(
        &self,
        replica: u8,
        ack_count: u8,
        retention_period: Duration,
    ) -> Result<u64, ReplicationError> {
        let request = CreateStreamRequest {
            replica,
            ack_count,
            retention_period,
        };

        let (tx, rx) = oneshot::channel();
        let req = Request::CreateStream { request, tx };
        if let Err(e) = self.tx.send(req) {
            error!("Failed to dispatch create-stream request to stream manager {e}");
            return Err(ReplicationError::Internal);
        }

        match rx.await {
            Ok(resp) => resp.map(|res| res.stream_id),
            Err(e) => {
                error!(
                    "Failed to receive create-stream response from internal stream manager: {e}"
                );
                Err(ReplicationError::RpcTimeout)
            }
        }
    }

    pub async fn open_stream(&self, stream_id: u64, epoch: u64) -> Result<(), ReplicationError> {
        let request = OpenStreamRequest { stream_id, epoch };

        let (tx, rx) = oneshot::channel();
        let req = Request::OpenStream { request, tx };
        if let Err(e) = self.tx.send(req) {
            error!("Failed to dispatch open-stream request to stream manager {e}");
            return Err(ReplicationError::Internal);
        }

        rx.await
            .map_err(|e| {
                error!("Failed to receive open-stream response from internal stream manager: {e}");
                ReplicationError::RpcTimeout
            })?
            .map(|_res| {
                info!("Open stream[id={stream_id}, epoch={epoch}] OK");
            })
    }

    pub async fn close_stream(&self, stream_id: u64) -> Result<(), ReplicationError> {
        let request = CloseStreamRequest { stream_id };

        let (tx, rx) = oneshot::channel();
        let req = Request::CloseStream { request, tx };
        if let Err(e) = self.tx.send(req) {
            error!("Failed to dispatch open-stream request to stream manager {e}");
            return Err(ReplicationError::Internal);
        }

        rx.await
            .map_err(|e| {
                error!("Failed to receive close-stream response from internal stream manager: {e}");
                ReplicationError::RpcTimeout
            })?
            .map(|_res| {
                info!("Close stream[id={stream_id}] OK");
            })
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
