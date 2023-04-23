use std::{cell::RefCell, net::SocketAddr, rc::Rc, sync::Arc};

use config::Configuration;
use slog::{info, o, trace, warn, Logger};
use store::ElasticStore;
use tokio::sync::mpsc;
use tokio_uring::net::TcpStream;
use transport::connection::Connection;

use crate::{
    connection_handler, connection_tracker::ConnectionTracker, handler::ServerCall,
    stream_manager::StreamManager,
};

pub(crate) struct Session {
    config: Arc<Configuration>,
    stream: TcpStream,
    addr: SocketAddr,
    store: Rc<ElasticStore>,
    stream_manager: Rc<RefCell<StreamManager>>,
    connection_tracker: Rc<RefCell<ConnectionTracker>>,
    log: Logger,
}

impl Session {
    pub(crate) fn new(
        config: Arc<Configuration>,
        stream: TcpStream,
        addr: SocketAddr,
        store: Rc<ElasticStore>,
        stream_manager: Rc<RefCell<StreamManager>>,
        connection_tracker: Rc<RefCell<ConnectionTracker>>,
        log: Logger,
    ) -> Self {
        Self {
            config,
            stream,
            addr,
            store,
            stream_manager,
            connection_tracker,
            log,
        }
    }

    pub(crate) fn process(self) {
        tokio_uring::spawn(async move {
            Self::process0(
                self.store,
                self.stream_manager,
                self.connection_tracker,
                self.addr,
                self.stream,
                self.config,
                self.log,
            )
            .await;
        });
    }

    async fn process0(
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

        let idle_handler = connection_handler::idle::IdleHandler::new(
            Rc::downgrade(&channel),
            peer_address.clone(),
            Arc::clone(&server_config),
            Rc::clone(&connection_tracker),
            logger.clone(),
        );

        // Coroutine to read requests from network connection
        let request_logger = logger.clone();
        let _channel = Rc::clone(&channel);
        let read_idle_handler = Rc::clone(&idle_handler);
        tokio_uring::spawn(async move {
            let channel = _channel;
            let logger = request_logger;
            loop {
                match channel.read_frame().await {
                    Ok(Some(frame)) => {
                        // Update last read instant.
                        read_idle_handler.on_read();
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
        tokio_uring::spawn(async move {
            let peer_address = channel.peer_address().to_owned();
            loop {
                match rx.recv().await {
                    Some(frame) => match channel.write_frame(&frame).await {
                        Ok(_) => {
                            // Update last write instant
                            idle_handler.on_write();
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
