use std::{rc::Rc, sync::Arc, thread};

use client::Client;
use config::Configuration;

use log::{error, info};
use model::stream::StreamMetadata;
use replication::StreamClient;
use tokio::sync::{broadcast, mpsc};

use crate::{command::Command, ClientError};

pub(crate) struct Worker {
    config: Arc<Configuration>,
    rx: mpsc::UnboundedReceiver<Command>,
    client: Rc<Client>,
    stream_client: Rc<StreamClient>,
    shutdown: broadcast::Receiver<()>,
}

impl Worker {
    pub(crate) fn spawn(
        rx: mpsc::UnboundedReceiver<Command>,
        config: Arc<Configuration>,
    ) -> Result<thread::JoinHandle<()>, ClientError> {
        thread::Builder::new()
            .name("front-end".to_owned())
            .spawn(move || {
                let (shutdown_tx, _shutdown_rx) = broadcast::channel(1);
                let client = Rc::new(Client::new(Arc::clone(&config), shutdown_tx.clone()));
                let stream_client = Rc::new(StreamClient::new(Arc::clone(&config)));
                let worker = Self {
                    config,
                    rx,
                    client,
                    stream_client,
                    shutdown: shutdown_tx.subscribe(),
                };

                Self::run(worker);
            })
            .map_err(|e| {
                error!("Failed to spawn front-end thread");
                ClientError::Internal("Failed to spawn front-end thread".to_owned())
            })
    }

    fn run(mut worker: Self) {
        tokio_uring::spawn(async move {
            loop {
                tokio::select! {
                    _ = worker.shutdown.recv() => {
                        break;
                    },

                    cmd = worker.rx.recv() => {
                        match cmd {
                            Some(command) => {
                                let client = Rc::clone(&worker.client);
                                let stream_client = Rc::clone(&worker.stream_client);
                                // Process each command in a dedicated coroutine.
                                tokio_uring::spawn(async move {
                                    Self::process_command(client, stream_client, command).await;
                                });
                            }
                            None => {
                                info!("The command MPSC channel has been closed and there are no remaining messages in the channel’s buffer");
                                break;
                            }
                        }

                    }
                }
            }
        });
    }

    async fn process_command(
        client: Rc<Client>,
        stream_client: Rc<StreamClient>,
        command: Command,
    ) {
        match command {
            Command::CreateStream { options, observer } => {
                let metadata = StreamMetadata {
                    stream_id: None,
                    replica: options.replica,
                    ack_count: options.ack,
                    retention_period: options.retention,
                };
                let res = client.create_stream(metadata).await.map_err(Into::into);
                if let Err(_e) = observer.send(res) {
                    error!("Failed to propagate create-stream result: {_e:#?}");
                }
            }

            Command::OpenStream {
                stream_id,
                observer,
            } => {}
        }
    }
}
