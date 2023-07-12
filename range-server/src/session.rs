use std::{
    cell::{RefCell, UnsafeCell},
    net::SocketAddr,
    rc::Rc,
    sync::Arc,
};

use config::Configuration;
use local_sync::mpsc;
use log::{info, trace, warn};
use store::Store;
use tokio_uring::net::TcpStream;
use transport::connection::Connection;

use crate::{
    connection_handler, connection_tracker::ConnectionTracker, handler::ServerCall,
    range_manager::RangeManager,
};

pub(crate) struct Session<S, M> {
    config: Arc<Configuration>,
    stream: TcpStream,
    addr: SocketAddr,
    store: Rc<S>,
    range_manager: Rc<UnsafeCell<M>>,
    connection_tracker: Rc<RefCell<ConnectionTracker>>,
}

impl<S, M> Session<S, M>
where
    S: Store + 'static,
    M: RangeManager + 'static,
{
    pub(crate) fn new(
        config: Arc<Configuration>,
        stream: TcpStream,
        addr: SocketAddr,
        store: Rc<S>,
        range_manager: Rc<UnsafeCell<M>>,
        connection_tracker: Rc<RefCell<ConnectionTracker>>,
    ) -> Self {
        Self {
            config,
            stream,
            addr,
            store,
            range_manager,
            connection_tracker,
        }
    }

    pub(crate) fn process(self) {
        tokio_uring::spawn(async move {
            Self::process0(
                self.store,
                self.range_manager,
                self.connection_tracker,
                self.addr,
                self.stream,
                self.config,
            )
            .await;
        });
    }

    async fn process0(
        store: Rc<S>,
        range_manager: Rc<UnsafeCell<M>>,
        connection_tracker: Rc<RefCell<ConnectionTracker>>,
        peer_address: SocketAddr,
        stream: TcpStream,
        server_config: Arc<Configuration>,
    ) {
        // Channel to transfer responses from handlers to the coroutine that is in charge of response write.
        let (tx, mut rx) = mpsc::unbounded::channel();

        // Put current connection into connection-tracker, such that when TERM/STOP signal is received,
        // servers send go-away frame to each connection, requesting clients to complete and migrate as soon
        // as possible.
        connection_tracker
            .borrow_mut()
            .insert(peer_address, tx.clone());
        let channel = Rc::new(Connection::new(stream, &peer_address.to_string()));

        let idle_handler = connection_handler::idle::IdleHandler::new(
            Rc::downgrade(&channel),
            peer_address,
            Arc::clone(&server_config),
            Rc::clone(&connection_tracker),
        );

        // Coroutine to read requests from network connection
        let _channel = Rc::clone(&channel);
        let read_idle_handler = Rc::clone(&idle_handler);
        tokio_uring::spawn(async move {
            let channel = _channel;
            loop {
                match channel.read_frame().await {
                    Ok(Some(frame)) => {
                        // Update last read instant.
                        read_idle_handler.on_read();
                        let sender = tx.clone();
                        let store = Rc::clone(&store);
                        let range_manager = Rc::clone(&range_manager);
                        let mut server_call = ServerCall {
                            request: frame,
                            sender,
                            store,
                            range_manager,
                        };
                        tokio_uring::spawn(async move {
                            server_call.call().await;
                        });
                    }
                    Ok(None) => {
                        info!("Connection to {} is closed", peer_address);
                        break;
                    }
                    Err(e) => {
                        warn!(
                            "Connection reset. Peer address: {}. Cause: {e:?}",
                            peer_address
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
                    Some(frame) => {
                        let stream_id = frame.stream_id;
                        let opcode = frame.operation_code;
                        match channel.write_frame(frame).await {
                            Ok(_) => {
                                // Update last write instant
                                idle_handler.on_write();
                                trace!(
                                    "Response frame[stream-id={}, opcode={}] written to {}",
                                    stream_id,
                                    opcode.variant_name().unwrap_or("INVALID_OPCODE"),
                                    peer_address
                                );
                            }
                            Err(e) => {
                                warn!(
                                "Failed to write response frame[stream-id={}, opcode={}] to {}. Cause: {:?}",
                                stream_id,
                                opcode.variant_name().unwrap_or("INVALID_OPCODE"),
                                peer_address,
                                e
                            );
                                break;
                            }
                        }
                    }
                    None => {
                        info!(
                            "Channel to receive responses from handlers has been closed. Peer[address={}] should have already closed the read-half of the connection",
                            peer_address);
                        break;
                    }
                }
            }
        });
    }
}
