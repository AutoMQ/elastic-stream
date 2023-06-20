use std::{sync::Arc, time::Duration};

use log::{error, info, trace};
use tokio::sync::{mpsc, oneshot};

use crate::{
    request::{
        AppendRequest, AppendResponse, CloseStreamRequest, CreateStreamRequest, OpenStreamRequest,
        ReadRequest, ReadResponse, Request, TrimRequest,
    },
    stream::stream_manager::StreamManager,
    ReplicationError,
};

/// `StreamClient` is designed to be `Send`
#[derive(Debug, Clone)]
pub struct StreamClient {
    tx: mpsc::UnboundedSender<Request>,
}

impl StreamClient {
    pub fn new(config: Arc<config::Configuration>, id: usize) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        let _ = std::thread::Builder::new()
            .name(format!("Runtime-{}", id))
            .spawn(move || {
                tokio_uring::builder().entries(32768).start(async move {
                    let stream_manager = StreamManager::new(config);
                    Self::spawn_loop(stream_manager, rx).await;
                })
            });

        Self { tx }
    }

    async fn spawn_loop(
        mut stream_manager: StreamManager,
        mut rx: mpsc::UnboundedReceiver<Request>,
    ) {
        while let Some(request) = rx.recv().await {
            match request {
                Request::Append { request, tx } => {
                    stream_manager.append(request, tx);
                }
                Request::Read { request, tx } => {
                    stream_manager.fetch(request, tx);
                }
                Request::CreateStream { request, tx } => {
                    stream_manager.create(request, tx);
                }
                Request::OpenStream { request, tx } => {
                    stream_manager.open(request, tx);
                }
                Request::CloseStream { request, tx } => {
                    stream_manager.close(request, tx);
                }
                Request::StartOffset { request, tx } => {
                    stream_manager.start_offset(request, tx);
                }
                Request::NextOffset { request, tx } => {
                    stream_manager.next_offset(request, tx);
                }
                Request::Trim { request, tx } => {
                    stream_manager.trim(request, tx);
                }
            }
        }
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
