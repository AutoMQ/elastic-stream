use std::{collections::HashMap, rc::Rc, sync::Arc};

use client::Client;
use config::Configuration;
use log::warn;
use model::stream::StreamMetadata;
use tokio::sync::{broadcast, mpsc, oneshot};

use crate::{
    request::{AppendRequest, AppendResponse, ReadRequest, ReadResponse, Request},
    stream_manager::replication_stream::ReplicationStream,
    ReplicationError,
};

pub(crate) struct StreamManager {
    config: Arc<Configuration>,
    rx: mpsc::UnboundedReceiver<Request>,
    client: Rc<Client>,
    streams: HashMap<i64, Rc<ReplicationStream>>,
}

impl StreamManager {
    pub(crate) fn new(config: Arc<Configuration>, rx: mpsc::UnboundedReceiver<Request>) -> Self {
        let (shutdown, _rx) = broadcast::channel(1);
        let client = Rc::new(Client::new(Arc::clone(&config), shutdown));
        let streams = HashMap::new();
        Self {
            config,
            rx,
            client,
            streams,
        }
    }

    pub(crate) fn spawn_loop(mut this: Self) {
        tokio_uring::spawn(async move {
            loop {
                match this.rx.recv().await {
                    Some(request) => match request {
                        Request::Append { request, tx } => {
                            this.append(request, tx).await;
                        }
                        Request::Read { request, tx } => {
                            this.read(request, tx).await;
                        }
                    },
                    None => {
                        break;
                    }
                }
            }
        });
    }

    async fn append(&mut self, request: AppendRequest, tx: oneshot::Sender<AppendResponse>) {}

    async fn read(&mut self, request: ReadRequest, tx: oneshot::Sender<ReadResponse>) {}

    async fn create(
        &mut self,
        metadata: StreamMetadata,
    ) -> Result<StreamMetadata, ReplicationError> {
        self.client.create_stream(metadata).await.map_err(|e| {
            warn!("Failed to create stream, {}", e);
            ReplicationError::Internal
        })
    }

    async fn open(
        &mut self,
        stream_id: i64,
        epoch: u64,
    ) -> Result<Rc<ReplicationStream>, ReplicationError> {
        let client = Rc::downgrade(&self.client);
        let mut stream = ReplicationStream::new(stream_id, epoch, client);
        (*stream).open().await?;
        Ok(stream)
    }

    async fn close(&mut self, stream_id: i64, range_id: i32, offset: i64) {}
}
