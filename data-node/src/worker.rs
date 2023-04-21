use std::{
    cell::RefCell,
    error::Error,
    net::SocketAddr,
    rc::Rc,
    sync::Arc,
    time::{Duration, Instant},
};

use client::Client;
use config::Configuration;
use slog::{debug, error, info, o, trace, warn, Logger};
use store::ElasticStore;
use tokio::{
    sync::{broadcast, mpsc, watch::Ref},
    time,
};
use tokio_uring::net::{TcpListener, TcpStream};
use transport::connection::Connection;
use util::metrics::{dump, http_serve};

use crate::{
    connection_tracker::ConnectionTracker,
    handler::ServerCall,
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

                    let store = Rc::clone(&self.store);
                    let stream_manager = Rc::clone(&self.stream_manager);
                    let connection_tracker = Rc::clone(&connection_tracker);
                    let server_config = Arc::clone(&self.config.server_config);
                    tokio_uring::spawn(async move {
                        Worker::process(store, stream_manager, connection_tracker, peer_socket_address, stream, server_config, logger).await;
                    });
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

    async fn process(
        store: Rc<ElasticStore>,
        stream_manager: Rc<RefCell<StreamManager>>,
        connection_tracker: Rc<RefCell<ConnectionTracker>>,
        peer_address: SocketAddr,
        stream: TcpStream,
        server_config: Arc<Configuration>,
        logger: Logger,
    ) {
        // Channel to transfer responses from handlers to the coroutine that is in charge of response write.
        let (tx, mut rx) = mpsc::unbounded_channel();

        // Put current connection into connection-tracker, such that when TERM/STOP signal is received,
        // nodes send go-away frame to each connection, requesting clients to complete and migrate as soon
        // as possible.
        connection_tracker
            .borrow_mut()
            .insert(peer_address.clone(), tx.clone());
        let channel = Rc::new(Connection::new(
            stream,
            &peer_address.to_string(),
            logger.new(o!()),
        ));

        let read_write_since = Rc::new(RefCell::new(Instant::now()));

        // TODO: Extract and define an idle handler
        // Check and close idle connection
        let idle_since = Rc::clone(&read_write_since);
        let idle_channel = Rc::downgrade(&channel);
        let idle = Rc::clone(&idle_since);
        let idle_log = logger.clone();
        let idle_conn_tracker = Rc::clone(&connection_tracker);
        let idle_peer_address = peer_address.clone();
        tokio_uring::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(10));
            loop {
                interval.tick().await;
                match idle_channel.upgrade() {
                    Some(channel) => {
                        if idle.borrow().clone() + server_config.connection_idle_duration()
                            < Instant::now()
                        {
                            idle_conn_tracker.borrow_mut().remove(&idle_peer_address);
                            if let Ok(_) = channel.close() {
                                info!(
                                    idle_log,
                                    "Close connection to {} since it has been idle for {}ms",
                                    channel.peer_address(),
                                    idle_since.borrow().elapsed().as_millis()
                                );
                            }
                        }
                    }
                    None => {
                        // If the connection were destructed, stop idle checking automatically.
                        break;
                    }
                }
            }
        });

        // Coroutine to read requests from network connection
        let request_logger = logger.clone();
        let _channel = Rc::clone(&channel);
        let read_since = Rc::clone(&read_write_since);
        tokio_uring::spawn(async move {
            let channel = _channel;
            let logger = request_logger;
            loop {
                match channel.read_frame().await {
                    Ok(Some(frame)) => {
                        // Update last read instant.
                        *read_since.borrow_mut() = Instant::now();
                        let log = logger.clone();
                        let sender = tx.clone();
                        let store = Rc::clone(&store);
                        let stream_manager = Rc::clone(&stream_manager);
                        let mut server_call = ServerCall {
                            request: frame,
                            sender,
                            logger: log,
                            store,
                            stream_manager,
                        };
                        tokio_uring::spawn(async move {
                            server_call.call().await;
                        });
                    }
                    Ok(None) => {
                        info!(logger, "Connection to {} is closed", peer_address);
                        break;
                    }
                    Err(e) => {
                        warn!(
                            logger,
                            "Connection reset. Peer address: {}. Cause: {e:?}", peer_address
                        );
                        break;
                    }
                }
            }

            connection_tracker.borrow_mut().remove(&peer_address);
        });

        // Coroutine to write responses to network connection
        let write_since = Rc::clone(&read_write_since);
        tokio_uring::spawn(async move {
            let peer_address = channel.peer_address().to_owned();
            loop {
                match rx.recv().await {
                    Some(frame) => match channel.write_frame(&frame).await {
                        Ok(_) => {
                            // Update last write instant
                            *write_since.borrow_mut() = Instant::now();
                            trace!(
                                logger,
                                "Response frame[stream-id={}, opcode={}] written to {}",
                                frame.stream_id,
                                frame.operation_code,
                                peer_address
                            );
                        }
                        Err(e) => {
                            warn!(
                                logger,
                                "Failed to write response frame[stream-id={}, opcode={}] to {}. Cause: {:?}",
                                frame.stream_id,
                                frame.operation_code,
                                peer_address,
                                e
                            );
                            break;
                        }
                    },
                    None => {
                        info!(
                            logger,
                            "Channel to receive responses from handlers has been closed. Peer[address={}] should have already closed the read-half of the connection",
                            peer_address);
                        break;
                    }
                }
            }
        });
    }
}
