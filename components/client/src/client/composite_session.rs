use super::{lb_policy::LbPolicy, response, session::Session};
use crate::{error::ClientError, Response};
use itertools::Itertools;
use model::{
    client_role::ClientRole, range::StreamRange, range_criteria::RangeCriteria, request::Request,
    PlacementManagerNode,
};
use protocol::rpc::header::ErrorCode;
use slog::{debug, error, info, trace, warn, Logger};
use std::{
    cell::RefCell,
    collections::HashMap,
    io::ErrorKind,
    net::{SocketAddr, ToSocketAddrs},
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{sync::oneshot, time::timeout};
use tokio_uring::net::TcpStream;

pub(crate) struct CompositeSession {
    target: String,
    config: Arc<config::Configuration>,
    lb_policy: LbPolicy,
    sessions: RefCell<HashMap<SocketAddr, Session>>,
    log: Logger,
    refresh_cluster_instant: RefCell<Instant>,
    heartbeat_instant: RefCell<Instant>,
}

impl CompositeSession {
    pub(crate) async fn new<T>(
        target: T,
        config: Arc<config::Configuration>,
        lb_policy: LbPolicy,
        log: Logger,
    ) -> Result<Self, ClientError>
    where
        T: ToSocketAddrs + ToString,
    {
        let sessions = RefCell::new(HashMap::new());
        // For now, we just resolve one session out of the target.
        // In the future, we would support multiple internal connection and load balancing among them.
        for socket_addr in target
            .to_socket_addrs()
            .map_err(|_e| ClientError::BadAddress)?
        {
            let res = Self::connect(
                socket_addr.clone(),
                config.client_connect_timeout(),
                &config,
                &log,
            )
            .await;
            match res {
                Ok(session) => {
                    sessions.borrow_mut().insert(socket_addr, session);
                    info!(log, "Connection to {} established", target.to_string());
                    break;
                }
                Err(_e) => {
                    info!(log, "Failed to connect to {}", socket_addr);
                }
            }
        }

        Ok(Self {
            target: target.to_string(),
            config,
            lb_policy,
            sessions,
            log,
            refresh_cluster_instant: RefCell::new(Instant::now()),
            heartbeat_instant: RefCell::new(Instant::now()),
        })
    }

    pub(crate) fn need_heartbeat(&self) -> bool {
        let now = Instant::now();
        let last = self.heartbeat_instant.borrow();
        last.clone() + self.config.client_heartbeat_interval() >= now
    }

    fn need_refresh_cluster(&self) -> bool {
        let cluster_size = self.sessions.borrow().len();
        if cluster_size <= 1 {
            debug!(self.log, "Placement Manager Cluster size is {} which is rare in production, flag refresh-cluster true", cluster_size);
            return true;
        }

        let now = Instant::now();
        self.refresh_cluster_instant.borrow().clone()
            + self
                .config
                .client_refresh_placement_manager_cluster_interval()
            <= now
    }

    /// Broadcast heartbeat requests to all nested sessions.
    pub(crate) async fn heartbeat(&self) -> Result<(), ClientError> {
        if !self.need_heartbeat() {
            trace!(self.log, "No need to broadcast heartbeat yet");
            return Ok(());
        }

        if self.need_refresh_cluster() {
            if let Ok(Some(nodes)) = self.describe_placement_manager_cluster().await {
                if !nodes.is_empty() {
                    *self.refresh_cluster_instant.borrow_mut() = Instant::now();

                    let mut addrs = nodes
                        .into_iter()
                        .map(|node| node.advertise_addr.to_socket_addrs().into_iter())
                        .flatten()
                        .flatten()
                        .filter(|socket_addr| socket_addr.is_ipv4())
                        .dedup()
                        .collect::<Vec<_>>();

                    // Remove sessions that are no longer valid
                    self.sessions
                        .borrow_mut()
                        .drain_filter(|k, _v| !addrs.contains(k))
                        .for_each(|(k, _v)| {
                            info!(self.log, "Session to {} will be disconnected because latest Placement Manager Cluster does not contain it any more", k);
                        });

                    addrs.drain_filter(|addr| self.sessions.borrow().contains_key(addr));

                    addrs.iter().for_each(|addr| {
                        trace!(
                            self.log,
                            "Create a new session for new Placement Manager Cluster member: {}",
                            addr
                        );
                    });

                    let futures = addrs.into_iter().map(|addr| {
                        Self::connect(
                            addr,
                            self.config.client_connect_timeout(),
                            &self.config,
                            &self.log,
                        )
                    });

                    let res: Vec<Result<Session, ClientError>> =
                        futures::future::join_all(futures).await;
                    for item in res {
                        match item {
                            Ok(session) => {
                                info!(
                                    self.log,
                                    "Insert a session to composite-session {} using socket: {}",
                                    self.target,
                                    session.target
                                );
                                self.sessions
                                    .borrow_mut()
                                    .insert(session.target.clone(), session);
                            }
                            Err(e) => {
                                error!(self.log, "Failed to connect. {:?}", e);
                            }
                        }
                    }
                }
            }
        }

        let now = Instant::now();
        *self.heartbeat_instant.borrow_mut() = now;

        let request = Request::Heartbeat {
            client_id: self.config.client.client_id.clone(),
            role: ClientRole::DataNode,
            data_node: Some(self.config.server.data_node()),
        };

        let mut receivers = vec![];
        {
            let sessions = self.sessions.borrow();
            let futures = sessions
                .iter()
                .map(|(_addr, session)| {
                    let (tx, rx) = oneshot::channel();
                    receivers.push(rx);
                    session.write(&request, tx)
                })
                .collect::<Vec<_>>();
            let _res: Vec<Result<(), oneshot::Sender<Response>>> =
                futures::future::join_all(futures).await;
        }

        let res: Vec<Result<Response, oneshot::error::RecvError>> =
            futures::future::join_all(receivers).await;

        for item in res {
            match item {
                Ok(response) => {
                    if let Response::Heartbeat { status } = response {
                        if status.code != ErrorCode::OK {
                            error!(
                                self.log,
                                "Failed to maintain heartbeat to {}. Status-Message: `{}`",
                                self.target,
                                status.message
                            );
                            // TODO: refine error handling
                            return Err(ClientError::ServerInternal);
                        }
                    }
                }
                Err(_e) => {}
            }
        }
        Ok(())
    }

    pub(crate) async fn allocate_id(
        &self,
        host: &str,
        timeout: Duration,
    ) -> Result<i32, ClientError> {
        if let Some((_, session)) = self.sessions.borrow().iter().next() {
            let request = Request::AllocateId {
                timeout: self.config.client_io_timeout(),
                host: host.to_owned(),
            };
            let (tx, rx) = oneshot::channel();
            if let Err(e) = session.write(&request, tx).await {
                error!(
                    self.log,
                    "Failed to send ID-allocation-request to {}. Cause: {:?}", self.target, e
                );
                return Err(ClientError::ConnectionRefused(self.target.to_owned()));
            }

            if let Response::AllocateId { status, id } = rx.await.map_err(|e| {
                error!(
                    self.log,
                    "Internal error while allocating ID from {}. Cause: {:?}", self.target, e
                );
                ClientError::ClientInternal
            })? {
                if status.code != ErrorCode::OK {
                    error!(
                        self.log,
                        "Failed to allocate ID from {}. Status-Message: `{}`",
                        self.target,
                        status.message
                    );
                    // TODO: refine error handling
                    return Err(ClientError::ServerInternal);
                }

                return Ok(id);
            }
        }

        Err(ClientError::ClientInternal)
    }

    pub(crate) async fn list_range(
        &self,
        criteria: RangeCriteria,
    ) -> Result<Vec<StreamRange>, ClientError> {
        // TODO: apply load-balancing among `self.sessions`.
        if let Some((_, session)) = self.sessions.borrow().iter().next() {
            let request = Request::ListRanges {
                timeout: self.config.client_io_timeout(),
                criteria: vec![criteria],
            };
            let (tx, rx) = oneshot::channel();
            if let Err(e) = session.write(&request, tx).await {
                error!(
                    self.log,
                    "Failed to send list-range request to {}. Cause: {:?}", self.target, e
                );
                return Err(ClientError::ClientInternal);
            }

            if let Response::ListRange { status, ranges } = rx.await.map_err(|e| {
                error!(
                    self.log,
                    "Internal client error when listing ranges from {}. Cause: {:?}",
                    self.target,
                    e
                );
                ClientError::ClientInternal
            })? {
                if status.code != ErrorCode::OK {
                    error!(
                        self.log,
                        "Failed to list-ranges from {}. Status-Message: `{}`",
                        self.target,
                        status.message
                    );
                    // TODO: refine error handling
                    return Err(ClientError::ServerInternal);
                }

                return Ok(ranges.unwrap_or_default());
            }
        }
        Err(ClientError::ClientInternal)
    }

    /// Describe current placement manager cluster membership.
    ///
    /// There are multiple rationales for this RPC.
    /// 1. Placement manager is built on top of RAFT consensus algorithm and election happens in case of leader outage. Some RPCs
    ///    should steer to the leader node and need to refresh the leadership on failure;
    /// 2. Heartbeat, metrics-reporting RPC requests should broadcast to all placement manager nodes, so that when leader changes,
    ///    data-node liveness and load evaluation are not impacted.
    ///
    /// # Implementation walkthrough
    /// Step 1: If placement manager access URL uses domain name, resolve it;
    ///  1.1 If the result `SocketAddress` has an existing `Session`, re-use it and go to step 2;
    ///  1.2 If the result `SocketAddress` is completely new, connect and build a new `Session`
    /// Step 2: Send DescribePlacementManagerRequest to the `Session` discovered in step 1;
    /// Step 3: Once response is received from placement manager server, update the aggregated `Session` table, including leadership
    async fn describe_placement_manager_cluster(
        &self,
    ) -> Result<Option<Vec<PlacementManagerNode>>, ClientError> {
        let addrs = self
            .target
            .to_socket_addrs()
            .map_err(|e| {
                error!(
                    self.log,
                    "Failed to parse {} into SocketAddr: {:?}", self.target, e
                );
                ClientError::BadAddress
            })?
            .collect::<Vec<_>>();

        let (mut tx, rx) = oneshot::channel();
        let data_node = self.config.server.data_node();
        let request = Request::DescribePlacementManager { data_node };

        let mut request_sent = false;
        'outer: loop {
            for addr in &addrs {
                if let Some(session) = self.sessions.borrow().get(addr) {
                    if let Err(tx_) = session.write(&request, tx).await {
                        tx = tx_;
                        error!(self.log, "Failed to send request to {}", addr);
                        continue;
                    }
                    request_sent = true;
                    break 'outer;
                }
            }

            if !request_sent {
                if let Some(addr) = addrs.first() {
                    let session = Self::connect(
                        addr.clone(),
                        self.config.client_connect_timeout(),
                        &self.config,
                        &self.log,
                    )
                    .await?;
                    self.sessions.borrow_mut().insert(addr.clone(), session);
                } else {
                    break;
                }
            }
        }

        if !request_sent {
            return Err(ClientError::ClientInternal);
        }

        match rx.await {
            Ok(response) => {
                if let response::Response::DescribePlacementManager { status, nodes } = response {
                    if ErrorCode::OK == status.code {
                        trace!(self.log, "Received placement manager cluster {:?}", nodes);
                        Ok(nodes)
                    } else {
                        warn!(
                            self.log,
                            "Failed to describe placement manager cluster: {:?}", status
                        );
                        Err(ClientError::ServerInternal)
                    }
                } else {
                    Err(ClientError::ClientInternal)
                }
            }
            Err(e) => Err(ClientError::ClientInternal),
        }
    }

    async fn connect(
        addr: SocketAddr,
        duration: Duration,
        config: &Arc<config::Configuration>,
        log: &Logger,
    ) -> Result<Session, ClientError> {
        trace!(log, "Establishing connection to {:?}", addr);
        let endpoint = addr.to_string();
        let connect = TcpStream::connect(addr);
        let stream = match timeout(duration, connect).await {
            Ok(res) => match res {
                Ok(connection) => {
                    trace!(log, "Connection to {:?} established", addr);
                    connection.set_nodelay(true).map_err(|e| {
                        error!(log, "Failed to disable Nagle's algorithm. Cause: {:?}", e);
                        ClientError::DisableNagleAlgorithm
                    })?;
                    connection
                }
                Err(e) => match e.kind() {
                    ErrorKind::ConnectionRefused => {
                        error!(log, "Connection to {} is refused", endpoint);
                        return Err(ClientError::ConnectionRefused(format!("{:?}", endpoint)));
                    }
                    _ => {
                        return Err(ClientError::ConnectFailure(format!("{:?}", e)));
                    }
                },
            },
            Err(e) => {
                let description = format!("Timeout when connecting {}, elapsed: {}", endpoint, e);
                error!(log, "{}", description);
                return Err(ClientError::ConnectTimeout(description));
            }
        };
        Ok(Session::new(addr, stream, &endpoint, config, log))
    }
}

#[cfg(test)]
mod tests {
    use super::CompositeSession;
    use crate::client::lb_policy::LbPolicy;
    use std::{error::Error, sync::Arc};

    #[test]
    fn test_new() -> Result<(), Box<dyn Error>> {
        let log = test_util::terminal_logger();
        let config = Arc::new(config::Configuration::default());
        tokio_uring::start(async {
            let port = test_util::run_listener(log.clone()).await;
            let target = format!("{}:{}", "localhost", port);
            let _session =
                CompositeSession::new(&target, config, LbPolicy::PickFirst, log.clone()).await?;

            Ok(())
        })
    }

    #[test]
    fn test_describe_placement_manager_cluster() -> Result<(), Box<dyn Error>> {
        let log = test_util::terminal_logger();
        let mut config = config::Configuration::default();
        config.server.node_id = 1;
        let config = Arc::new(config);
        tokio_uring::start(async {
            let port = test_util::run_listener(log.clone()).await;
            let target = format!("{}:{}", "localhost", port);
            let composite_session =
                CompositeSession::new(&target, config, LbPolicy::PickFirst, log.clone()).await?;
            let nodes = composite_session
                .describe_placement_manager_cluster()
                .await
                .unwrap()
                .unwrap();
            assert_eq!(3, nodes.len());
            Ok(())
        })
    }
}
