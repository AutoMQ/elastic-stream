use std::{
    cell::RefCell,
    error::Error,
    rc::Rc,
    sync::Arc,
    time::{Duration, Instant},
};

use client::Client;
use slog::{debug, error, info, warn, Logger};
use store::ElasticStore;
use tokio::sync::{broadcast, mpsc};
use tokio_uring::net::{TcpListener};
use util::metrics::http_serve;

use crate::{
    connection_tracker::ConnectionTracker,
    stream_manager::{fetcher::FetchRangeTask, StreamManager},
    worker_config::WorkerConfig,
};

/// A server aggregates one or more `Worker`s and each `Worker` takes up a dedicated CPU
/// processor, following the Thread-per-Core design paradigm.
///
/// There is only one primary worker, which is in charge of the stream/range management
/// and communication with the placement manager.
///
/// Inter-worker communications are achieved via channels.
pub(crate) struct Worker {
    logger: Logger,
    config: WorkerConfig,
    store: Rc<ElasticStore>,
    stream_manager: Rc<RefCell<StreamManager>>,
    client: Rc<Client>,
    channels: Option<Vec<mpsc::UnboundedReceiver<FetchRangeTask>>>,
}

impl Worker {
    pub fn new(
        config: WorkerConfig,
        store: Rc<ElasticStore>,
        stream_manager: Rc<RefCell<StreamManager>>,
        client: Rc<Client>,
        channels: Option<Vec<mpsc::UnboundedReceiver<FetchRangeTask>>>,
        logger: &Logger,
    ) -> Self {
        Self {
            config,
            store,
            stream_manager,
            client,
            channels,
            logger: logger.clone(),
        }
    }

    pub fn serve(&mut self, shutdown: broadcast::Sender<()>) {
        core_affinity::set_for_current(self.config.core_id);
        if self.config.primary {
            info!(
                self.logger,
                "Bind primary worker to processor-{}", self.config.core_id.id
            );
        } else {
            info!(
                self.logger,
                "Bind worker to processor-{}", self.config.core_id.id
            );
        }

        info!(
            self.logger,
            "The number of Submission Queue entries in uring: {}",
            self.config.server_config.server.uring.queue_depth
        );
        tokio_uring::builder()
            .entries(self.config.server_config.server.uring.queue_depth)
            .uring_builder(
                tokio_uring::uring_builder()
                    .dontfork()
                    .setup_attach_wq(self.config.sharing_uring),
            )
            .start(async {
                let bind_address = format!("0.0.0.0:{}", self.config.server_config.server.port);
                let listener =
                    match TcpListener::bind(bind_address.parse().expect("Failed to bind")) {
                        Ok(listener) => {
                            info!(self.logger, "Server starts OK, listening {}", bind_address);
                            listener
                        }
                        Err(e) => {
                            eprintln!("{}", e);
                            return;
                        }
                    };

                self.stream_manager
                    .borrow_mut()
                    .start()
                    .await
                    .expect("Failed to bootstrap stream ranges from placement managers");

                if self.config.primary {
                    tokio_uring::spawn(async {
                        http_serve().await;
                    });
                }

                self.heartbeat(shutdown.subscribe());

                match self
                    .run(listener, self.logger.clone(), shutdown.subscribe())
                    .await
                {
                    Ok(_) => {}
                    Err(e) => {
                        error!(self.logger, "Runtime failed. Cause: {}", e.to_string());
                    }
                }
            });
    }

    fn heartbeat(&self, mut shutdown_rx: broadcast::Receiver<()>) {
        let client = Rc::clone(&self.client);
        let config = Arc::clone(&self.config.server_config);
        let logger = self.logger.clone();
        tokio_uring::spawn(async move {
            let mut interval = tokio::time::interval(config.client_heartbeat_interval());
            loop {
                tokio::select! {
                    _ = shutdown_rx.recv() => {
                        info!(logger, "Received shutdown signal. Stop accepting new connections.");
                        break;
                    }
                    _ = interval.tick() => {
                        let _ = client
                        .broadcast_heartbeat(&config.server.placement_manager)
                        .await;
                    }

                }
            }
        });
    }

    async fn run(
        &self,
        listener: TcpListener,
        logger: Logger,
        mut shutdown_rx: broadcast::Receiver<()>,
    ) -> Result<(), Box<dyn Error>> {
        let connection_tracker = Rc::new(RefCell::new(ConnectionTracker::new(logger.clone())));
        loop {
            let logger = logger.clone();
            tokio::select! {
                _ = shutdown_rx.recv() => {
                    info!(logger, "Received shutdown signal. Stop accepting new connections.");
                    break;
                }

                incoming = listener.accept() => {
                    let (stream, peer_socket_address) = match incoming {
                        Ok((stream, socket_addr)) => {
                            debug!(logger, "Accepted a new connection from {socket_addr:?}");
                            stream.set_nodelay(true).unwrap_or_else(|e| {
                                warn!(logger, "Failed to disable Nagle's algorithm. Cause: {e:?}, PeerAddress: {socket_addr:?}");
                            });
                            debug!(logger, "Nagle's algorithm turned off");

                            (stream, socket_addr)
                        }
                        Err(e) => {
                            error!(
                                logger,
                                "Failed to accept a connection. Cause: {}",
                                e.to_string()
                            );
                            break;
                        }
                    };

                    let session = super::session::Session::new(
                        Arc::clone(&self.config.server_config),
                        stream, peer_socket_address,
                        Rc::clone(&self.store),
                        Rc::clone(&self.stream_manager),
                        Rc::clone(&connection_tracker),
                        logger.clone());
                    session.process();
                }
            }
        }

        // Send GoAway frame to all existing connections
        connection_tracker.borrow_mut().go_away();

        // Await till all existing connections disconnect
        let start = Instant::now();
        loop {
            let remaining = connection_tracker.borrow().len();
            if 0 == remaining {
                break;
            }

            info!(
                logger,
                "There are {} connections left. Waited for {}/{} seconds",
                remaining,
                start.elapsed().as_secs(),
                self.config.server_config.server_grace_period().as_secs()
            );
            tokio::time::sleep(Duration::from_secs(1)).await;
            if Instant::now() >= start + self.config.server_config.server_grace_period() {
                break;
            }
        }

        info!(logger, "Node#run completed");
        Ok(())
    }
}
