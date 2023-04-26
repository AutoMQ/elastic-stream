use std::sync::Arc;

use client::Client;
use config::Configuration;
use tokio::sync::{broadcast, mpsc, oneshot};

use crate::request::{AppendRequest, AppendResponse, ReadRequest, ReadResponse, Request};

pub(crate) struct StreamManager {
    config: Arc<Configuration>,
    rx: mpsc::UnboundedReceiver<Request>,
    client: Client,
}

impl StreamManager {
    pub(crate) fn new(config: Arc<Configuration>, rx: mpsc::UnboundedReceiver<Request>) -> Self {
        let (shutdown, _rx) = broadcast::channel(1);
        let client = Client::new(Arc::clone(&config), shutdown);
        Self { config, rx, client }
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
}
