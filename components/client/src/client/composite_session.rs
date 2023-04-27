use super::{lb_policy::LbPolicy, response, session::Session};
use crate::{error::ClientError, Response};
use itertools::Itertools;
use log::{debug, error, info, trace, warn};
use model::{
    client_role::ClientRole,
    range::StreamRange,
    range_criteria::RangeCriteria,
    request::{seal::SealRangeRequest, Request},
    PlacementManagerNode,
};
use protocol::rpc::header::ErrorCode;
use std::{
    cell::RefCell,
    collections::HashMap,
    io::ErrorKind,
    net::{SocketAddr, ToSocketAddrs},
    rc::Rc,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{
    sync::{broadcast, oneshot},
    time::{self, timeout},
};
use tokio_uring::net::TcpStream;

pub(crate) struct CompositeSession {
    target: String,
    config: Arc<config::Configuration>,
    lb_policy: LbPolicy,
    endpoints: RefCell<Vec<SocketAddr>>,
    sessions: Rc<RefCell<HashMap<SocketAddr, Session>>>,
    shutdown: broadcast::Sender<()>,
    refresh_cluster_instant: RefCell<Instant>,
    heartbeat_instant: RefCell<Instant>,
}

impl CompositeSession {
    pub(crate) async fn new<T>(
        target: T,
        config: Arc<config::Configuration>,
        lb_policy: LbPolicy,
        shutdown: broadcast::Sender<()>,
    ) -> Result<Self, ClientError>
    where
        T: ToSocketAddrs + ToString,
    {
        let endpoints = RefCell::new(Vec::new());
        let sessions = Rc::new(RefCell::new(HashMap::new()));
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
                &sessions,
                shutdown.subscribe(),
            )
            .await;
            match res {
                Ok(session) => {
                    endpoints.borrow_mut().push(socket_addr.clone());
                    sessions.borrow_mut().insert(socket_addr, session);
                    info!("Connection to {} established", target.to_string());
                    break;
                }
                Err(_e) => {
                    info!("Failed to connect to {}", socket_addr);
                }
            }
        }

        Ok(Self {
            target: target.to_string(),
            config,
            lb_policy,
            endpoints,
            sessions,
            shutdown,
            refresh_cluster_instant: RefCell::new(Instant::now()),
            heartbeat_instant: RefCell::new(Instant::now()),
        })
    }

    fn need_refresh_cluster(&self) -> bool {
        let cluster_size = self.sessions.borrow().len();
        if cluster_size <= 1 {
            debug!("Placement Manager Cluster size is {} which is rare in production, flag refresh-cluster true", cluster_size);
            return true;
        }

        let now = Instant::now();
        *self.refresh_cluster_instant.borrow()
            + self
                .config
                .client_refresh_placement_manager_cluster_interval()
            <= now
    }

    /// Broadcast heartbeat requests to all nested sessions.
    pub(crate) async fn heartbeat(&self) -> Result<(), ClientError> {
        self.try_reconnect().await;

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
                            self.endpoints.borrow_mut().drain_filter(|e| {
                                e == &k
                            });
                            info!("Session to {} will be disconnected because latest Placement Manager Cluster does not contain it any more", k);
                        });

                    addrs.drain_filter(|addr| self.sessions.borrow().contains_key(addr));

                    addrs.iter().for_each(|addr| {
                        trace!(
                            "Create a new session for new Placement Manager Cluster member: {}",
                            addr
                        );
                    });

                    let futures = addrs.into_iter().map(|addr| {
                        Self::connect(
                            addr,
                            self.config.client_connect_timeout(),
                            &self.config,
                            &self.sessions,
                            self.shutdown.subscribe(),
                        )
                    });

                    let res: Vec<Result<Session, ClientError>> =
                        futures::future::join_all(futures).await;
                    for item in res {
                        match item {
                            Ok(session) => {
                                info!(
                                    "Insert a session to composite-session {} using socket: {}",
                                    self.target, session.target
                                );
                                self.endpoints.borrow_mut().push(session.target);
                                self.sessions.borrow_mut().insert(session.target, session);
                            }
                            Err(e) => {
                                error!("Failed to connect. {:?}", e);
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
                                "Failed to maintain heartbeat to {}. Status-Message: `{}`",
                                self.target, status.message
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
        self.try_reconnect().await;
        if let Some((_, session)) = self.sessions.borrow().iter().next() {
            let request = Request::AllocateId {
                timeout: self.config.client_io_timeout(),
                host: host.to_owned(),
            };
            let (tx, rx) = oneshot::channel();
            if let Err(e) = session.write(&request, tx).await {
                error!(
                    "Failed to send ID-allocation-request to {}. Cause: {:?}",
                    self.target, e
                );
                return Err(ClientError::ConnectionRefused(self.target.to_owned()));
            }

            if let Response::AllocateId { status, id } = rx.await.map_err(|e| {
                error!(
                    "Internal error while allocating ID from {}. Cause: {:?}",
                    self.target, e
                );
                ClientError::ClientInternal
            })? {
                if status.code != ErrorCode::OK {
                    error!(
                        "Failed to allocate ID from {}. Status-Message: `{}`",
                        self.target, status.message
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
        self.try_reconnect().await;
        // TODO: apply load-balancing among `self.sessions`.
        if let Some((_, session)) = self.sessions.borrow().iter().next() {
            let request = Request::ListRanges {
                timeout: self.config.client_io_timeout(),
                criteria: vec![criteria],
            };
            let (tx, rx) = oneshot::channel();
            if let Err(e) = session.write(&request, tx).await {
                error!(
                    "Failed to send list-range request to {}. Cause: {:?}",
                    self.target, e
                );
                return Err(ClientError::ClientInternal);
            }

            if let Response::ListRange { status, ranges } = rx.await.map_err(|e| {
                error!(
                    "Internal client error when listing ranges from {}. Cause: {:?}",
                    self.target, e
                );
                ClientError::ClientInternal
            })? {
                if status.code != ErrorCode::OK {
                    error!(
                        "Failed to list-ranges from {}. Status-Message: `{}`",
                        self.target, status.message
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
        self.try_reconnect().await;
        let addrs = self
            .target
            .to_socket_addrs()
            .map_err(|e| {
                error!("Failed to parse {} into SocketAddr: {:?}", self.target, e);
                ClientError::BadAddress
            })?
            .collect::<Vec<_>>();

        let (mut tx, rx) = oneshot::channel();
        let data_node = self.config.server.data_node();
        let request = Request::DescribePlacementManager { data_node };

        let mut request_sent = false;
        'outer: loop {
            for addr in &addrs {
                // TODO: RefCell is held across await, which should be avoided. Drop it before await.
                if let Some(session) = self.sessions.borrow().get(addr) {
                    if let Err(tx_) = session.write(&request, tx).await {
                        tx = tx_;
                        error!("Failed to send request to {}", addr);
                        continue;
                    }
                    trace!("Describe placement manager cluster via {}", addr);
                    request_sent = true;
                    break 'outer;
                }
            }

            if !request_sent {
                warn!(
                    "Failed to describe placement manager cluster via existing sessions. Try to re-connect..."
                );
                if let Some(addr) = addrs.first() {
                    let session = Self::connect(
                        *addr,
                        self.config.client_connect_timeout(),
                        &self.config,
                        &self.sessions,
                        self.shutdown.subscribe(),
                    )
                    .await?;
                    self.sessions.borrow_mut().insert(*addr, session);
                } else {
                    break;
                }
            }
        }

        if !request_sent {
            return Err(ClientError::ClientInternal);
        }

        match time::timeout(self.config.client_connect_timeout(), rx).await {
            Ok(response) => match response {
                Ok(response) => {
                    if let response::Response::DescribePlacementManager { status, nodes } = response
                    {
                        if ErrorCode::OK == status.code {
                            trace!("Received placement manager cluster {:?}", nodes);
                            Ok(nodes)
                        } else {
                            warn!("Failed to describe placement manager cluster: {:?}", status);
                            Err(ClientError::ServerInternal)
                        }
                    } else {
                        Err(ClientError::ClientInternal)
                    }
                }
                Err(_e) => Err(ClientError::ClientInternal),
            },
            Err(_e) => Err(ClientError::RpcTimeout {
                timeout: self.config.client_connect_timeout(),
            }),
        }
    }

    /// Seal range on data-node or placement manager.
    ///
    /// # Implementation Walkthrough
    /// 1. If the seal kind is placement manager, find the session to the leader node;
    /// 2. Send the request to the session and await response;
    ///
    /// # Returns
    /// If seal kind is seal-placement-manager and renew, returns the newly created mutable range;
    /// Otherwise, return the range that is being sealed with the end properly filled.
    ///
    /// If the seal kind is seal-data-node, the returning `StreamRange` is the result of the
    /// data-node only. Final end value of the range will be resolved after MinCopy of data nodes responded.
    pub(crate) async fn seal(&self, request: SealRangeRequest) -> Result<StreamRange, ClientError> {
        todo!()
    }

    /// Try to reconnect to the endpoints that is absent from sessions.
    ///
    ///
    /// # Implementation walkthrough
    /// 1. filter endpoints that is not found in sessions
    /// 2. connect to the target and then create a session
    async fn try_reconnect(&self) {
        if self.endpoints.borrow().is_empty() {
            return;
        }

        if self.endpoints.borrow().len() > self.sessions.borrow().len() {
            let futures = self
                .endpoints
                .borrow()
                .iter()
                .filter(|target| !self.sessions.borrow().contains_key(target))
                .map(|target| {
                    Self::connect(
                        *target,
                        self.config.client_connect_timeout(),
                        &self.config,
                        &self.sessions,
                        self.shutdown.subscribe(),
                    )
                })
                .collect::<Vec<_>>();

            if futures.is_empty() {
                return;
            }
            trace!("Reconnecting {} sessions", futures.len());

            futures::future::join_all(futures)
                .await
                .into_iter()
                .for_each(|res| match res {
                    Ok(session) => {
                        let target = session.target;
                        self.sessions.borrow_mut().insert(target, session);
                    }
                    Err(e) => {
                        error!("Failed to connect. Cause: {:?}", e);
                    }
                });
        }
    }

    async fn connect(
        addr: SocketAddr,
        duration: Duration,
        config: &Arc<config::Configuration>,
        sessions: &Rc<RefCell<HashMap<SocketAddr, Session>>>,
        shutdown: broadcast::Receiver<()>,
    ) -> Result<Session, ClientError> {
        trace!("Establishing connection to {:?}", addr);
        let endpoint = addr.to_string();
        let connect = TcpStream::connect(addr);
        let stream = match timeout(duration, connect).await {
            Ok(res) => match res {
                Ok(connection) => {
                    trace!("Connection to {:?} established", addr);
                    connection.set_nodelay(true).map_err(|e| {
                        error!("Failed to disable Nagle's algorithm. Cause: {:?}", e);
                        ClientError::DisableNagleAlgorithm
                    })?;
                    connection
                }
                Err(e) => match e.kind() {
                    ErrorKind::ConnectionRefused => {
                        error!("Connection to {} is refused", endpoint);
                        return Err(ClientError::ConnectionRefused(format!("{:?}", endpoint)));
                    }
                    _ => {
                        return Err(ClientError::ConnectFailure(format!("{:?}", e)));
                    }
                },
            },
            Err(e) => {
                let description = format!("Timeout when connecting {}, elapsed: {}", endpoint, e);
                error!("{}", description);
                return Err(ClientError::ConnectTimeout(description));
            }
        };
        Ok(Session::new(
            addr,
            stream,
            &endpoint,
            config,
            Rc::downgrade(sessions),
            shutdown,
        ))
    }
}

#[cfg(test)]
mod tests {
    use tokio::sync::broadcast;

    use super::CompositeSession;
    use crate::client::lb_policy::LbPolicy;
    use std::{error::Error, sync::Arc};

    #[test]
    fn test_new() -> Result<(), Box<dyn Error>> {
        let config = Arc::new(config::Configuration::default());
        tokio_uring::start(async {
            let port = test_util::run_listener().await;
            let target = format!("{}:{}", "localhost", port);
            let (shutdown_tx, _shutdown_rx) = broadcast::channel(1);
            let _session =
                CompositeSession::new(&target, config, LbPolicy::PickFirst, shutdown_tx).await?;

            Ok(())
        })
    }

    #[test]
    fn test_describe_placement_manager_cluster() -> Result<(), Box<dyn Error>> {
        let mut config = config::Configuration::default();
        config.server.node_id = 1;
        let config = Arc::new(config);
        tokio_uring::start(async {
            let port = test_util::run_listener().await;
            let target = format!("{}:{}", "localhost", port);
            let (shutdown_tx, _shutdown_rx) = broadcast::channel(1);
            let composite_session =
                CompositeSession::new(&target, config, LbPolicy::PickFirst, shutdown_tx).await?;
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
