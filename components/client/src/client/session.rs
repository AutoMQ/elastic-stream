use super::response;
use super::session_state::SessionState;
use codec::{
    error::FrameError,
    frame::{Frame, OperationCode},
};
use model::{
    data_node::DataNode, range::StreamRange, request::Request, PlacementManagerNode, Status,
};
use protocol::rpc::header::{
    DescribePlacementManagerClusterResponse, ErrorCode, HeartbeatResponse, IdAllocationResponse,
    ListRangesResponse, SystemErrorResponse,
};
use transport::connection::Connection;

use log::{error, info, trace, warn};
use std::{
    cell::{RefCell, UnsafeCell},
    collections::HashMap,
    net::SocketAddr,
    rc::{Rc, Weak},
    sync::Arc,
    time::Instant,
};
use tokio::sync::{
    broadcast::{self, error::RecvError},
    oneshot,
};
use tokio_uring::net::TcpStream;

pub(crate) struct Session {
    pub(crate) target: SocketAddr,

    config: Arc<config::Configuration>,

    state: SessionState,

    // Unlike {tokio, monoio}::TcpStream where we need to split underlying TcpStream into two owned mutable,
    // tokio_uring::TcpStream requires immutable references only to perform read/write.
    connection: Rc<Connection>,

    /// In-flight requests.
    inflight_requests: Rc<UnsafeCell<HashMap<u32, oneshot::Sender<response::Response>>>>,

    idle_since: RefCell<Instant>,
}

impl Session {
    /// Spawn a loop to continuously read responses and server-side requests.
    fn spawn_read_loop(
        connection: Rc<Connection>,
        inflight_requests: Rc<UnsafeCell<HashMap<u32, oneshot::Sender<response::Response>>>>,
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
                                    Session::handle_response(inflight, frame);
                                } else {
                                    warn!( "Received an unexpected request frame from target={}", target);
                                }
                            }
                            Ok(None) => {
                                info!( "Connection to {} is closed", target);
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
                inflight.drain_filter(|stream_id, tx| {
                    if tx.is_closed() {
                        info!(
                            "Caller has cancelled request[stream-id={}], potentially due to timeout",
                            stream_id
                        );
                    }
                    tx.is_closed()
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
        shutdown: broadcast::Receiver<()>,
    ) -> Self {
        let connection = Rc::new(Connection::new(stream, endpoint));
        let inflight = Rc::new(UnsafeCell::new(HashMap::new()));

        Self::spawn_read_loop(
            Rc::clone(&connection),
            Rc::clone(&inflight),
            sessions,
            target,
            shutdown,
        );

        Self {
            config: Arc::clone(config),
            target,
            state: SessionState::Active,
            connection,
            inflight_requests: inflight,
            idle_since: RefCell::new(Instant::now()),
        }
    }

    pub(crate) async fn write(
        &self,
        request: &Request,
        response_observer: oneshot::Sender<response::Response>,
    ) -> Result<(), oneshot::Sender<response::Response>> {
        trace!("Sending request {:?}", request);

        // Update last read/write instant.
        *self.idle_since.borrow_mut() = Instant::now();
        let mut frame = Frame::new(OperationCode::Unknown);
        let inflight_requests = unsafe { &mut *self.inflight_requests.get() };
        inflight_requests.insert(frame.stream_id, response_observer);

        match request {
            Request::Heartbeat { .. } => {
                frame.operation_code = OperationCode::Heartbeat;
                let header = request.into();
                frame.header = Some(header);
                match self.connection.write_frame(&frame).await {
                    Ok(_) => {
                        trace!(
                            "Write `Heartbeat` request bounded for {} using stream-id={} to socket buffer",
                            self.connection.peer_address(),
                            frame.stream_id,
                        );
                    }
                    Err(e) => {
                        error!(
                            "Failed to write `Heartbeat` request bounded for {} to socket buffer. Cause: {:?}", 
                            self.connection.peer_address(), e
                        );
                        if let Some(observer) = inflight_requests.remove(&frame.stream_id) {
                            return Err(observer);
                        }
                    }
                }
            }

            Request::ListRanges { .. } => {
                frame.operation_code = OperationCode::ListRanges;
                let header = request.into();
                frame.header = Some(header);

                match self.connection.write_frame(&frame).await {
                    Ok(_) => {
                        trace!(
                            "Write `ListRange` request to {}, stream-id={}",
                            self.connection.peer_address(),
                            frame.stream_id,
                        );
                    }
                    Err(e) => {
                        warn!(
                            "Failed to write `ListRange` request to {}. Cause: {:?}",
                            self.connection.peer_address(),
                            e
                        );
                        if let Some(observer) = inflight_requests.remove(&frame.stream_id) {
                            return Err(observer);
                        }
                    }
                }
            }

            Request::AllocateId { .. } => {
                frame.operation_code = OperationCode::AllocateId;
                let header = request.into();
                frame.header = Some(header);
                match self.connection.write_frame(&frame).await {
                    Ok(_) => {
                        trace!(
                            "Write `AllocateId` request to {}, stream-id={}",
                            self.connection.peer_address(),
                            frame.stream_id
                        );
                    }
                    Err(e) => {
                        error!(
                            "Failed to write `AllocateId` request to network. Cause: {:?}",
                            e
                        );
                        if let Some(observer) = inflight_requests.remove(&frame.stream_id) {
                            return Err(observer);
                        }
                    }
                }
            }

            Request::DescribePlacementManager { .. } => {
                frame.operation_code = OperationCode::DescribePlacementManager;
                let header = request.into();
                frame.header = Some(header);
                match self.connection.write_frame(&frame).await {
                    Ok(_) => {
                        trace!(
                            "Write `DescribePlacementManager` request to {}, stream-id={}",
                            self.connection.peer_address(),
                            frame.stream_id
                        );
                    }
                    Err(e) => {
                        error!(
                            "Failed to write `DescribePlacementManager` request to network. Cause: {:?}", e
                        );
                        if let Some(observer) = inflight_requests.remove(&frame.stream_id) {
                            return Err(observer);
                        }
                    }
                }
            }
        };

        Ok(())
    }

    fn handle_heartbeat_response(frame: &Frame) -> response::Response {
        let mut resp = response::Response::Heartbeat {
            status: Status::ok(),
        };
        if let Some(ref buf) = frame.header {
            match flatbuffers::root::<HeartbeatResponse>(buf) {
                Ok(heartbeat) => {
                    trace!("Received Heartbeat response: {:?}", heartbeat);
                    let hb = heartbeat.unpack();
                    let _client_id = hb.client_id;
                    let _client_role = hb.client_role;
                    let _status = hb.status;
                    if let response::Response::Heartbeat { ref mut status } = resp {
                        *status = _status.as_ref().into();
                    }
                }

                Err(e) => {
                    error!("Failed to parse Heartbeat response header: {:?}", e);
                }
            }
        }
        resp
    }

    fn handle_list_ranges_response(frame: &Frame) -> response::Response {
        let mut response = response::Response::ListRange {
            status: Status::ok(),
            ranges: None,
        };
        if let Some(ref buf) = frame.header {
            if let Ok(list_ranges) = flatbuffers::root::<ListRangesResponse>(buf) {
                let _ranges = list_ranges
                    .unpack()
                    .list_responses
                    .iter()
                    .flat_map(|result| result.iter())
                    .flat_map(|res| res.ranges.as_ref())
                    .flat_map(|e| e.iter())
                    .map(|range| {
                        let mut stream_range = if range.end_offset >= 0 {
                            StreamRange::new(
                                range.stream_id,
                                range.range_index,
                                range.start_offset as u64,
                                range.end_offset as u64,
                                Some(range.end_offset as u64),
                            )
                        } else {
                            StreamRange::new(
                                range.stream_id,
                                range.range_index,
                                range.start_offset as u64,
                                range.start_offset as u64,
                                None,
                            )
                        };

                        range
                            .replica_nodes
                            .iter()
                            .flat_map(|nodes| nodes.iter())
                            .for_each(|node| {
                                stream_range.replica_mut().push(node.into());
                            });

                        stream_range
                    })
                    .collect::<Vec<_>>();
                if let response::Response::ListRange { ranges, .. } = &mut response {
                    *ranges = Some(_ranges);
                }
            }
        }
        response
    }

    fn handle_allocate_id_response(frame: &Frame) -> response::Response {
        let mut resp = response::Response::AllocateId {
            status: Status::decode(),
            id: -1,
        };

        if frame.system_error() {
            if let Some(ref buf) = frame.header {
                match flatbuffers::root::<SystemErrorResponse>(buf) {
                    Ok(error_response) => {
                        let response = error_response.unpack();
                        // Update status
                        if let response::Response::AllocateId { ref mut status, .. } = resp {
                            *status = response.status.as_ref().into();
                        }
                    }

                    Err(e) => {
                        // Deserialize error
                        warn!(
                            "Failed to decode `SystemErrorResponse` using FlatBuffers. Cause: {}",
                            e
                        );
                    }
                }
            }
        } else if let Some(ref buf) = frame.header {
            match flatbuffers::root::<IdAllocationResponse>(buf) {
                Ok(response) => {
                    let response = response.unpack();
                    if response.status.code == ErrorCode::OK {
                        if let response::Response::AllocateId {
                            ref mut id,
                            ref mut status,
                        } = resp
                        {
                            *id = response.id;
                            *status = Status::ok();
                        }
                    } else if let response::Response::AllocateId {
                        ref mut id,
                        ref mut status,
                    } = resp
                    {
                        *id = response.id;
                        *status = response.status.as_ref().into();
                    }
                }
                Err(e) => {
                    // Deserialize error
                    warn!( "Failed to decode `IdAllocation` response header using FlatBuffers. Cause: {}", e);
                }
            }
        }

        resp
    }

    fn handle_describe_placement_manager_response(frame: &Frame) -> response::Response {
        let mut resp = response::Response::DescribePlacementManager {
            status: Status::decode(),
            nodes: None,
        };

        if frame.system_error() {
            if let Some(ref buf) = frame.header {
                match flatbuffers::root::<SystemErrorResponse>(buf) {
                    Ok(response) => {
                        let response = response.unpack();
                        // Update status
                        if let response::Response::DescribePlacementManager {
                            ref mut status, ..
                        } = resp
                        {
                            *status = response.status.as_ref().into();
                        }
                    }
                    Err(e) => {
                        // Deserialize error
                        warn!(
                            "Failed to decode `SystemErrorResponse` using FlatBuffers. Cause: {}",
                            e
                        );
                    }
                }
            }
        } else if let Some(ref buf) = frame.header {
            match flatbuffers::root::<DescribePlacementManagerClusterResponse>(buf) {
                Ok(response) => {
                    let response = response.unpack();
                    if ErrorCode::OK == response.status.code {
                        let nodes_ = response
                            .cluster
                            .nodes
                            .iter()
                            .map(Into::into)
                            .collect::<Vec<PlacementManagerNode>>();

                        if let response::Response::DescribePlacementManager {
                            ref mut nodes,
                            ref mut status,
                        } = resp
                        {
                            *status = Status::ok();
                            *nodes = Some(nodes_);
                        }
                    } else {
                    }
                }
                Err(_e) => {
                    // Deserialize error
                    warn!( "Failed to decode `DescribePlacementManagerClusterResponse` response header using FlatBuffers. Cause: {}", _e);
                }
            }
        }
        resp
    }

    fn handle_response(
        inflight: &mut HashMap<u32, oneshot::Sender<response::Response>>,
        frame: Frame,
    ) {
        let stream_id = frame.stream_id;
        trace!(
            "Received {} response for stream-id={}",
            frame.operation_code,
            stream_id
        );

        match inflight.remove(&stream_id) {
            Some(sender) => {
                let response = match frame.operation_code {
                    OperationCode::Heartbeat => Self::handle_heartbeat_response(&frame),
                    OperationCode::ListRanges => Self::handle_list_ranges_response(&frame),
                    OperationCode::Unknown => {
                        warn!("Received an unknown operation code");
                        return;
                    }
                    OperationCode::Ping => todo!(),
                    OperationCode::GoAway => todo!(),
                    OperationCode::AllocateId => Self::handle_allocate_id_response(&frame),
                    OperationCode::Append => {
                        warn!("Received an unexpected `Append` response");
                        return;
                    }
                    OperationCode::Fetch => {
                        warn!("Received an unexpected `Fetch` response");
                        return;
                    }
                    OperationCode::SealRanges => {
                        warn!("Received an unexpected `SealRanges` response");
                        return;
                    }
                    OperationCode::SyncRanges => {
                        warn!("Received an unexpected `SyncRanges` response");
                        return;
                    }
                    OperationCode::DescribeRanges => {
                        warn!("Received an unexpected `DescribeRanges` response");
                        return;
                    }
                    OperationCode::CreateStreams => {
                        warn!("Received an unexpected `CreateStreams` response");
                        return;
                    }
                    OperationCode::DeleteStreams => {
                        warn!("Received an unexpected `DeleteStreams` response");
                        return;
                    }
                    OperationCode::UpdateStreams => {
                        warn!("Received an unexpected `UpdateStreams` response");
                        return;
                    }
                    OperationCode::DescribeStreams => {
                        warn!("Received an unexpected `DescribeStreams` response");
                        return;
                    }
                    OperationCode::TrimStreams => todo!(),
                    OperationCode::ReportMetrics => todo!(),
                    OperationCode::DescribePlacementManager => {
                        Self::handle_describe_placement_manager_response(&frame)
                    }
                };
                sender.send(response).unwrap_or_else(|response| {
                    warn!("Failed to forward response to client: {:?}", response);
                });
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

impl Drop for Session {
    fn drop(&mut self) {
        let requests = unsafe { &mut *self.inflight_requests.get() };
        requests.drain().for_each(|(_stream_id, sender)| {
            let aborted_response = response::Response::ListRange {
                status: Status::pm_internal("Aborted".to_owned()),
                ranges: None,
            };
            sender.send(aborted_response).unwrap_or_else(|_response| {
                warn!("Failed to notify connection reset");
            });
        });
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use std::error::Error;
    use test_util::run_listener;

    /// Verify it's OK to create a new session.
    #[test]
    fn test_new() -> Result<(), Box<dyn Error>> {
        tokio_uring::start(async {
            let port = run_listener().await;
            let target = format!("127.0.0.1:{}", port);
            let stream = TcpStream::connect(target.parse()?).await?;
            let config = Arc::new(config::Configuration::default());
            let sessions = Rc::new(RefCell::new(HashMap::new()));
            let (_tx, rx) = broadcast::channel(1);
            let _session = Session::new(
                target.parse()?,
                stream,
                &target,
                &config,
                Rc::downgrade(&sessions),
                rx,
            );

            Ok(())
        })
    }
}
