use crate::{request, response, NodeState};
use codec::{error::FrameError, frame::Frame};

use model::client_role::ClientRole;
use protocol::rpc::header::OperationCode;
use transport::connection::Connection;

use local_sync::oneshot;
use log::{error, info, trace, warn};
use std::{
    cell::{RefCell, UnsafeCell},
    collections::HashMap,
    net::SocketAddr,
    rc::{Rc, Weak},
    sync::Arc,
    time::Instant,
};
use tokio::sync::broadcast::{self, error::RecvError};
use tokio_uring::net::TcpStream;

use crate::invocation_context::InvocationContext;

pub(crate) struct Session {
    pub(crate) target: SocketAddr,

    config: Arc<config::Configuration>,

    // Unlike {tokio, monoio}::TcpStream where we need to split underlying TcpStream into two owned mutable,
    // tokio_uring::TcpStream requires immutable references only to perform read/write.
    connection: Rc<Connection>,

    /// In-flight requests.
    inflight_requests: Rc<UnsafeCell<HashMap<u32, InvocationContext>>>,

    idle_since: Rc<RefCell<Instant>>,

    state: Rc<RefCell<NodeState>>,
}

impl Session {
    /// Spawn a loop to continuously read responses and server-side requests.
    fn spawn_read_loop(
        connection: Rc<Connection>,
        inflight_requests: Rc<UnsafeCell<HashMap<u32, InvocationContext>>>,
        sessions: Weak<RefCell<HashMap<SocketAddr, Session>>>,
        target: SocketAddr,
        mut shutdown: broadcast::Receiver<()>,
    ) {
        tokio_uring::spawn(async move {
            trace!("Start read loop for session[target={}]", target);
            loop {
                tokio::select! {
                    stop = shutdown.recv() => {
                        match stop {
                            Ok(_) => {
                                info!("Received a shutdown signal. Stop session read loop");
                            },
                            Err(RecvError::Closed) => {
                                // should NOT reach here.
                                info!("Shutdown broadcast channel is closed. Stop session read loop");
                            },
                            Err(RecvError::Lagged(_)) => {
                                // should not reach here.
                                info!("Shutdown broadcast channel is lagged behind. Stop session read loop");
                            }
                        }
                        break;
                    },
                    read = connection.read_frame() => {
                        match read {
                            Err(e) => {
                                match e {
                                    FrameError::Incomplete => {
                                        // More data needed
                                        continue;
                                    }
                                    FrameError::ConnectionReset => {
                                        error!( "Connection to {} reset by peer", target);
                                    }
                                    FrameError::BadFrame(message) => {
                                        error!( "Read a bad frame from target={}. Cause: {}", target, message);
                                    }
                                    FrameError::TooLongFrame{found, max} => {
                                        error!( "Read a frame with excessive length={}, max={}, target={}", found, max, target);
                                    }
                                    FrameError::MagicCodeMismatch{found, expected} => {
                                        error!( "Read a frame with incorrect magic code. Expected={}, actual={}, target={}", expected, found, target);
                                    }
                                    FrameError::TooLongFrameHeader{found, expected} => {
                                        error!( "Read a frame with excessive header length={}, max={}, target={}", found, expected, target);
                                    }
                                    FrameError::PayloadChecksumMismatch{expected, actual} => {
                                        error!( "Read a frame with incorrect payload checksum. Expected={}, actual={}, target={}", expected, actual, target);
                                    }
                                }
                                // Close the session
                                if let Some(sessions) = sessions.upgrade() {
                                    let mut sessions = sessions.borrow_mut();
                                    if let Some(_session) = sessions.remove(&target) {
                                        warn!( "Closing session to {}", target);
                                    }
                                }
                                break;
                            }
                            Ok(Some(frame)) => {
                                trace!( "Read a frame from channel={}", target);
                                let inflight = unsafe { &mut *inflight_requests.get() };
                                if frame.is_response() {
                                    Session::handle_response(inflight, frame, target);
                                } else {
                                    warn!( "Received an unexpected request frame from target={}", target);
                                }
                            }
                            Ok(None) => {
                                info!( "Connection to {} is closed by peer", target);
                                if let Some(sessions) = sessions.upgrade() {
                                    let mut sessions = sessions.borrow_mut();
                                    if let Some(_session) = sessions.remove(&target) {
                                        info!( "Remove session to {} from composite-session", target);
                                    }
                                }
                                break;
                            }

                        }
                    }
                }

                let inflight = unsafe { &mut *inflight_requests.get() };
                inflight.retain(|stream_id, ctx| {
                    if ctx.is_closed() {
                        info!(
                            "Caller has cancelled request[stream-id={}], potentially due to timeout",
                            stream_id
                        );
                    }
                    !ctx.is_closed()
                });
            }
            trace!("Read loop for session[target={}] completed", target);
        });
    }

    pub(crate) fn new(
        target: SocketAddr,
        stream: TcpStream,
        endpoint: &str,
        config: &Arc<config::Configuration>,
        sessions: Weak<RefCell<HashMap<SocketAddr, Session>>>,
        shutdown: broadcast::Sender<()>,
    ) -> Self {
        let connection = Rc::new(Connection::new(stream, endpoint));
        let inflight = Rc::new(UnsafeCell::new(HashMap::new()));

        Self::spawn_read_loop(
            Rc::clone(&connection),
            Rc::clone(&inflight),
            sessions,
            target,
            shutdown.subscribe(),
        );

        Self {
            config: Arc::clone(config),
            target,
            connection,
            inflight_requests: inflight,
            idle_since: Rc::new(RefCell::new(Instant::now())),
            state: Rc::new(RefCell::new(NodeState::Unknown)),
        }
    }

    pub(crate) async fn write(
        &self,
        request: request::Request,
        response_observer: oneshot::Sender<response::Response>,
    ) -> Result<(), InvocationContext> {
        trace!("Sending {} to {}", request, self.target);

        // Update last read/write instant.
        *self.idle_since.borrow_mut() = Instant::now();
        let mut frame = Frame::new(OperationCode::UNKNOWN);

        // Set frame header
        frame.header = Some((&request).into());

        // Set operation code
        match &request.headers {
            request::Headers::Heartbeat { .. } => {
                frame.operation_code = OperationCode::HEARTBEAT;
            }

            request::Headers::CreateStream { .. } => {
                frame.operation_code = OperationCode::CREATE_STREAM;
            }

            request::Headers::DescribeStream { .. } => {
                frame.operation_code = OperationCode::DESCRIBE_STREAM;
            }

            request::Headers::ListRange { .. } => {
                frame.operation_code = OperationCode::LIST_RANGE;
            }

            request::Headers::AllocateId { .. } => {
                frame.operation_code = OperationCode::ALLOCATE_ID;
            }

            request::Headers::DescribePlacementDriver { .. } => {
                frame.operation_code = OperationCode::DESCRIBE_PLACEMENT_DRIVER;
            }

            request::Headers::CreateRange { .. } => {
                frame.operation_code = OperationCode::CREATE_RANGE;
            }

            request::Headers::SealRange { .. } => {
                frame.operation_code = OperationCode::SEAL_RANGE;
            }

            request::Headers::Append => {
                frame.operation_code = OperationCode::APPEND;
            }

            request::Headers::Fetch { .. } => {
                frame.operation_code = OperationCode::FETCH;
            }

            request::Headers::ReportMetrics { .. } => {
                frame.operation_code = OperationCode::REPORT_METRICS;
            }
        };

        frame.payload = request.body.clone();

        let inflight_requests = unsafe { &mut *self.inflight_requests.get() };
        let context = InvocationContext::new(self.target, request.clone(), response_observer);
        inflight_requests.insert(frame.stream_id, context);

        // Write frame to network
        let stream_id = frame.stream_id;
        let opcode = frame.operation_code;
        match self.connection.write_frame(frame).await {
            Ok(_) => {
                trace!(
                    "Write request[opcode={}] bounded for {} using stream-id={} to socket buffer",
                    opcode.variant_name().unwrap_or("INVALID_OPCODE"),
                    self.connection.peer_address(),
                    stream_id,
                );
            }
            Err(e) => {
                error!(
                    "Failed to write request[opcode={}] bounded for {} to socket buffer. Cause: {:?}",
                    opcode.variant_name().unwrap_or("INVALID_OPCODE"), self.connection.peer_address(), e
                );
                if let Some(ctx) = inflight_requests.remove(&stream_id) {
                    return Err(ctx);
                }
            }
        }

        Ok(())
    }

    pub(crate) async fn heartbeat(&self, role: ClientRole) {
        let last = *self.idle_since.borrow();
        if Instant::now() - last < self.config.client_heartbeat_interval() {
            return;
        }

        let request = request::Request {
            timeout: self.config.client_io_timeout(),
            headers: request::Headers::Heartbeat {
                client_id: self.config.client.client_id.clone(),
                range_server: if role == ClientRole::RangeServer {
                    Some(self.config.server.range_server())
                } else {
                    None
                },
                role,
            },
            body: None,
        };

        let mut frame = Frame::new(OperationCode::HEARTBEAT);
        frame.header = Some((&request).into());
        let stream_id = frame.stream_id;
        let opcode = frame.operation_code;
        match self.connection.write_frame(frame).await {
            Ok(_) => {
                trace!(
                    "Write request[opcode={}] bounded for {} using stream-id={} to socket buffer",
                    opcode.variant_name().unwrap_or("INVALID_OPCODE"),
                    self.connection.peer_address(),
                    stream_id,
                );
                *self.idle_since.borrow_mut() = Instant::now();
            }
            Err(e) => {
                error!(
                    "Failed to write request[opcode={}] bounded for {} to socket buffer. Cause: {:?}",
                    opcode.variant_name().unwrap_or("INVALID_OPCODE"), self.connection.peer_address(), e
                );
            }
        }
    }

    pub(crate) fn state(&self) -> NodeState {
        *self.state.borrow()
    }

    pub(crate) fn set_state(&self, state: NodeState) {
        let current = *self.state.borrow();
        if current != state {
            info!(
                "Node-state of {} is changed: {:?} --> {:?}",
                self.target, current, state
            );
            *self.state.borrow_mut() = state;
        }
    }

    fn handle_response(
        inflight: &mut HashMap<u32, InvocationContext>,
        frame: Frame,
        target: SocketAddr,
    ) {
        let stream_id = frame.stream_id;
        trace!(
            "Received {} response for stream-id={} from {}",
            frame
                .operation_code
                .variant_name()
                .unwrap_or("INVALID_OPCODE"),
            stream_id,
            target
        );

        if frame.operation_code == OperationCode::HEARTBEAT {
            return;
        }

        match inflight.remove(&stream_id) {
            Some(mut ctx) => {
                let mut response = response::Response::new(frame.operation_code);
                if frame.system_error() {
                    response.on_system_error(&frame);
                } else {
                    match frame.operation_code {
                        OperationCode::LIST_RANGE => {
                            response.on_list_ranges(&frame);
                        }

                        OperationCode::UNKNOWN => {
                            warn!("Received an unknown operation code");
                            return;
                        }

                        OperationCode::PING => todo!(),

                        OperationCode::GOAWAY => todo!(),

                        OperationCode::ALLOCATE_ID => {
                            response.on_allocate_id(&frame);
                        }

                        OperationCode::APPEND => {
                            response.on_append(&frame, &ctx);
                        }

                        OperationCode::FETCH => {
                            response.on_fetch(&frame, &ctx);
                        }

                        OperationCode::CREATE_RANGE => {
                            response.on_create_range(&frame, &ctx);
                        }

                        OperationCode::SEAL_RANGE => {
                            response.on_seal_range(&frame, &ctx);
                        }

                        OperationCode::SYNC_RANGE => {
                            warn!("Received an unexpected `SyncRanges` response");
                            return;
                        }

                        OperationCode::CREATE_STREAM => {
                            response.on_create_stream(&frame, &ctx);
                        }

                        OperationCode::DESCRIBE_STREAM => {
                            response.on_describe_stream(&frame, &ctx);
                        }

                        OperationCode::DELETE_STREAM => {
                            warn!("Received an unexpected `DeleteStreams` response");
                            return;
                        }

                        OperationCode::UPDATE_STREAM => {
                            warn!("Received an unexpected `UpdateStreams` response");
                            return;
                        }

                        OperationCode::TRIM_STREAM => todo!(),

                        OperationCode::REPORT_METRICS => {
                            response.on_report_metrics(&frame);
                        }

                        OperationCode::DESCRIBE_PLACEMENT_DRIVER => {
                            response.on_describe_placement_driver(&frame);
                        }

                        OperationCode::HEARTBEAT => {
                            unreachable!();
                        }

                        _ => {
                            unreachable!("Unsupported operation code");
                        }
                    }
                }

                ctx.write_response(response);
            }
            None => {
                warn!(
                    "Expected inflight request[stream-id={}] is missing",
                    frame.stream_id
                );
            }
        }
    }
}

impl Clone for Session {
    fn clone(&self) -> Self {
        Self {
            target: self.target,
            config: Arc::clone(&self.config),
            connection: Rc::clone(&self.connection),
            inflight_requests: Rc::clone(&self.inflight_requests),
            idle_since: Rc::clone(&self.idle_since),
            state: Rc::clone(&self.state),
        }
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use mock_server::run_listener;
    use std::error::Error;

    /// Verify it's OK to create a new session.
    #[test]
    fn test_new() -> Result<(), Box<dyn Error>> {
        tokio_uring::start(async {
            let port = run_listener().await;
            let target = format!("127.0.0.1:{}", port);
            let stream = TcpStream::connect(target.parse()?).await?;
            let config = Arc::new(config::Configuration::default());
            let sessions = Rc::new(RefCell::new(HashMap::new()));
            let (tx, _rx) = broadcast::channel(1);
            let _session = Session::new(
                target.parse()?,
                stream,
                &target,
                &config,
                Rc::downgrade(&sessions),
                tx,
            );

            Ok(())
        })
    }
}
