use super::response;
use super::session_state::SessionState;
use codec::frame::{Frame, OperationCode};
use model::data_node::DataNode;
use model::range::StreamRange;
use model::PlacementManagerNode;
use model::Status;
use model::{client_role::ClientRole, request::Request};
use protocol::rpc::header::{
    DescribePlacementManagerClusterResponse, ErrorCode, HeartbeatResponse, IdAllocationResponse,
    ListRangesResponse, SystemErrorResponse,
};
use slog::{error, trace, warn, Logger};
use std::cell::RefCell;
use std::net::SocketAddr;
use std::sync::Arc;
use std::{
    cell::UnsafeCell,
    collections::HashMap,
    rc::Rc,
    time::{Duration, Instant},
};
use tokio::sync::oneshot;
use tokio_uring::net::TcpStream;
use transport::connection::Connection;

pub(crate) struct Session {
    target: SocketAddr,

    config: Arc<config::Configuration>,

    log: Logger,

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
        log: Logger,
    ) {
        tokio_uring::spawn(async move {
            loop {
                match connection.read_frame().await {
                    Err(e) => {
                        // Handle connection reset
                        todo!()
                    }
                    Ok(Some(frame)) => {
                        trace!(log, "Read a frame from channel");
                        let inflight = unsafe { &mut *inflight_requests.get() };
                        if frame.is_response() {
                            Session::handle_response(inflight, frame, &log);
                        } else {
                            warn!(log, "Received an unexpected request frame");
                        }
                    }
                    Ok(None) => {
                        // TODO: Handle normal connection close
                        break;
                    }
                }
            }
        });
    }

    pub(crate) fn new(
        target: SocketAddr,
        stream: TcpStream,
        endpoint: &str,
        config: &Arc<config::Configuration>,
        log: &Logger,
    ) -> Self {
        let connection = Rc::new(Connection::new(stream, endpoint, log.clone()));
        let inflight = Rc::new(UnsafeCell::new(HashMap::new()));

        Self::spawn_read_loop(Rc::clone(&connection), Rc::clone(&inflight), log.clone());

        Self {
            config: Arc::clone(config),
            target,
            log: log.clone(),
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
        trace!(self.log, "Sending request {:?}", request);

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
                            self.log,
                            "Write `Heartbeat` request to {}, stream-id={}",
                            self.connection.peer_address(),
                            frame.stream_id,
                        );
                    }
                    Err(e) => {
                        error!(
                            self.log,
                            "Failed to write request to network. Cause: {:?}", e
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
                            self.log,
                            "Write `ListRange` request to {}, stream-id={}",
                            self.connection.peer_address(),
                            frame.stream_id,
                        );
                    }
                    Err(e) => {
                        warn!(
                            self.log,
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
                            self.log,
                            "Write `AllocateId` request to {}, stream-id={}",
                            self.connection.peer_address(),
                            frame.stream_id
                        );
                    }
                    Err(e) => {
                        error!(
                            self.log,
                            "Failed to write `AllocateId` request to network. Cause: {:?}", e
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
                            self.log,
                            "Write `DescribePlacementManager` request to {}, stream-id={}",
                            self.connection.peer_address(),
                            frame.stream_id
                        );
                    }
                    Err(e) => {
                        error!(
                            self.log,
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

    pub(crate) fn state(&self) -> SessionState {
        self.state
    }

    fn active(&self) -> bool {
        SessionState::Active == self.state
    }

    pub(crate) fn need_heartbeat(&self, duration: &Duration) -> bool {
        self.active() && (Instant::now() - *self.idle_since.borrow() >= *duration)
    }

    pub(crate) async fn heartbeat(&mut self) -> Option<oneshot::Receiver<response::Response>> {
        // If the current session is being closed, heartbeat will be no-op.
        if self.state == SessionState::Closing {
            return None;
        }

        let request = Request::Heartbeat {
            client_id: self.config.client.client_id.clone(),
            role: ClientRole::DataNode,
            data_node: Some(self.config.server.data_node()),
        };
        let (response_observer, rx) = oneshot::channel();
        if self.write(&request, response_observer).await.is_ok() {
            trace!(
                self.log,
                "Heartbeat sent to {}",
                self.connection.peer_address()
            );
        }
        Some(rx)
    }

    fn handle_heartbeat_response(log: &Logger, frame: &Frame) -> response::Response {
        let mut resp = response::Response::Heartbeat {
            status: Status::ok(),
        };
        if let Some(ref buf) = frame.header {
            if let Ok(heartbeat) = flatbuffers::root::<HeartbeatResponse>(&buf) {
                trace!(log, "Heartbeat response: {:?}", heartbeat);
                let hb = heartbeat.unpack();
                let _client_id = hb.client_id;
                let _client_role = hb.client_role;
                let _status = hb.status;
                if let response::Response::Heartbeat { ref mut status } = resp {
                    *status = _status.as_ref().into();
                }
            }
        }
        resp
    }

    fn handle_list_ranges_response(log: &Logger, frame: &Frame) -> response::Response {
        let mut response = response::Response::ListRange {
            status: Status::ok(),
            ranges: None,
        };
        if let Some(ref buf) = frame.header {
            if let Ok(list_ranges) = flatbuffers::root::<ListRangesResponse>(&buf) {
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
                                range.next_offset as u64,
                                Some(range.end_offset as u64),
                            )
                        } else {
                            StreamRange::new(
                                range.stream_id,
                                range.range_index,
                                range.start_offset as u64,
                                range.next_offset as u64,
                                None,
                            )
                        };

                        range
                            .replica_nodes
                            .iter()
                            .flat_map(|nodes| nodes.iter())
                            .for_each(|node| {
                                if let Some(ref n) = node.data_node {
                                    if node.is_primary {
                                        stream_range.set_primary(n.node_id);
                                    }

                                    if let Some(ref addr) = n.advertise_addr {
                                        let data_node = DataNode::new(n.node_id, addr.clone());
                                        stream_range.replica_mut().push(data_node);
                                    } else {
                                        warn!(log, "Invalid replica node: {:?}", node)
                                    }
                                } else {
                                    warn!(log, "Invalid replica node: {:?}", node)
                                }
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

    fn handle_allocate_id_response(log: &Logger, frame: &Frame) -> response::Response {
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
                            log,
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
                    } else {
                        if let response::Response::AllocateId {
                            ref mut id,
                            ref mut status,
                        } = resp
                        {
                            *id = response.id;
                            *status = response.status.as_ref().into();
                        }
                    }
                }
                Err(e) => {
                    // Deserialize error
                    warn!(log, "Failed to decode `IdAllocation` response header using FlatBuffers. Cause: {}", e);
                }
            }
        }

        resp
    }

    fn handle_describe_placement_manager_response(
        log: &Logger,
        frame: &Frame,
    ) -> response::Response {
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
                            log,
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
                Err(e) => {}
            }
        }
        resp
    }

    fn handle_response(
        inflight: &mut HashMap<u32, oneshot::Sender<response::Response>>,
        frame: Frame,
        log: &Logger,
    ) {
        let stream_id = frame.stream_id;
        trace!(
            log,
            "Received {} response for stream-id={}",
            frame.operation_code,
            stream_id
        );

        match inflight.remove(&stream_id) {
            Some(sender) => {
                let response = match frame.operation_code {
                    OperationCode::Heartbeat => Self::handle_heartbeat_response(&log, &frame),
                    OperationCode::ListRanges => Self::handle_list_ranges_response(&log, &frame),
                    OperationCode::Unknown => {
                        warn!(log, "Received an unknown operation code");
                        return;
                    }
                    OperationCode::Ping => todo!(),
                    OperationCode::GoAway => todo!(),
                    OperationCode::AllocateId => Self::handle_allocate_id_response(&log, &frame),
                    OperationCode::Append => {
                        warn!(log, "Received an unexpected `Append` response");
                        return;
                    }
                    OperationCode::Fetch => {
                        warn!(log, "Received an unexpected `Fetch` response");
                        return;
                    }
                    OperationCode::SealRanges => {
                        warn!(log, "Received an unexpected `SealRanges` response");
                        return;
                    }
                    OperationCode::SyncRanges => {
                        warn!(log, "Received an unexpected `SyncRanges` response");
                        return;
                    }
                    OperationCode::DescribeRanges => {
                        warn!(log, "Received an unexpected `DescribeRanges` response");
                        return;
                    }
                    OperationCode::CreateStreams => {
                        warn!(log, "Received an unexpected `CreateStreams` response");
                        return;
                    }
                    OperationCode::DeleteStreams => {
                        warn!(log, "Received an unexpected `DeleteStreams` response");
                        return;
                    }
                    OperationCode::UpdateStreams => {
                        warn!(log, "Received an unexpected `UpdateStreams` response");
                        return;
                    }
                    OperationCode::DescribeStreams => {
                        warn!(log, "Received an unexpected `DescribeStreams` response");
                        return;
                    }
                    OperationCode::TrimStreams => todo!(),
                    OperationCode::ReportMetrics => todo!(),
                    OperationCode::DescribePlacementManager => {
                        Self::handle_describe_placement_manager_response(&log, &frame)
                    }
                };
                sender.send(response).unwrap_or_else(|response| {
                    warn!(log, "Failed to forward response to client: {:?}", response);
                });
            }
            None => {
                warn!(
                    log,
                    "Expected inflight request[stream-id={}] is missing", frame.stream_id
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
                warn!(self.log, "Failed to notify connection reset");
            });
        });
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use protocol::rpc::header::ErrorCode;
    use std::{error::Error, time::Duration};
    use test_util::{run_listener, terminal_logger};
    use tokio::time::timeout;

    /// Verify it's OK to create a new session.
    #[test]
    fn test_new() -> Result<(), Box<dyn Error>> {
        tokio_uring::start(async {
            let logger = terminal_logger();
            let port = run_listener(logger.clone()).await;
            let target = format!("127.0.0.1:{}", port);
            let stream = TcpStream::connect(target.parse()?).await?;
            let config = Arc::new(config::Configuration::default());
            let session = Session::new(target.parse()?, stream, &target, &config, &logger);

            assert_eq!(SessionState::Active, session.state());

            assert_eq!(false, session.need_heartbeat(&Duration::from_secs(1)));

            Ok(())
        })
    }

    #[test]
    fn test_heartbeat() -> Result<(), Box<dyn Error>> {
        tokio_uring::start(async {
            let logger = terminal_logger();

            #[allow(unused_variables)]
            let port = 2378;

            let port = run_listener(logger.clone()).await;
            let target = format!("127.0.0.1:{}", port);
            let stream = TcpStream::connect(target.parse()?).await?;
            let mut config = config::Configuration::default();
            config.server.node_id = 1;
            config.server.host = "localhost".to_owned();
            config.server.port = 1234;
            let config = Arc::new(config);
            let mut session = Session::new(target.parse()?, stream, &target, &config, &logger);

            let result = session.heartbeat().await;
            let response = result.unwrap().await?;
            trace!(logger, "Heartbeat response: {:?}", response);
            if let response::Response::Heartbeat { ref status } = response {
                assert_eq!(ErrorCode::OK, status.code);
            } else {
                panic!("Unexpected response type");
            }
            Ok(())
        })
    }

    #[test]
    #[ignore]
    fn test_heartbeat_timeout() -> Result<(), Box<dyn Error>> {
        tokio_uring::start(async {
            let logger = terminal_logger();
            let port = run_listener(logger.clone()).await;
            let target = format!("127.0.0.1:{}", port);
            let stream = TcpStream::connect(target.parse()?).await?;
            let mut config = config::Configuration::default();
            config.server.node_id = 1;
            let config = Arc::new(config);
            let mut session = Session::new(target.parse()?, stream, &target, &config, &logger);

            if let Some(rx) = session.heartbeat().await {
                let start = Instant::now();
                let result = timeout(Duration::from_millis(200), rx).await;
                let duration = start.elapsed();
                assert!(duration.as_millis() >= 200, "Timeout should work");
                if let Err(ref _elapsed) = result {
                } else {
                    panic!("Should get timeout");
                }
            }
            Ok(())
        })
    }
}
