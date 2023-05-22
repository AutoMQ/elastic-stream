use super::{lb_policy::LbPolicy, session::Session};
use crate::{error::ClientError, invocation_context::InvocationContext};
use bytes::Bytes;
use codec::frame::OperationCode;
use itertools::Itertools;
use log::{debug, error, info, trace, warn};
use model::{
    client_role::ClientRole, fetch::FetchRequestEntry, fetch::FetchResultEntry, payload::Payload,
    range::RangeMetadata, range_criteria::RangeCriteria, stream::StreamMetadata, AppendResultEntry,
    PlacementManagerNode,
};
use observation::metrics::{
    store_metrics::DataNodeStatistics,
    sys_metrics::{DiskStatistics, MemoryStatistics},
    uring_metrics::UringStatistics,
};
use protocol::rpc::header::SealKind;
use std::{
    cell::RefCell,
    collections::HashMap,
    f32::consts::E,
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

use crate::{request, response, NodeState};

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
                Arc::clone(&config),
                Rc::clone(&sessions),
                shutdown.clone(),
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

    fn refresh_leadership(&self, nodes: &Vec<PlacementManagerNode>) {
        debug!("Refresh placement manager cluster leadership");
        // Sync leader/follower state
        for node in nodes.iter() {
            node.advertise_addr
                .to_socket_addrs()
                .into_iter()
                .flatten()
                .for_each(|addr| {
                    if let Some((_, session)) = self
                        .sessions
                        .borrow_mut()
                        .iter()
                        .find(|&entry| *entry.0 == addr)
                    {
                        session.set_state(if node.leader {
                            NodeState::Leader
                        } else {
                            NodeState::Follower
                        })
                    }
                });
        }
    }

    async fn refresh_sessions(&self, nodes: &Vec<PlacementManagerNode>) {
        if nodes.is_empty() {
            trace!("Placement Manager Cluster is empty, no need to refresh sessions");
            return;
        }

        // Update last-refresh-cluster-instant.
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
                Arc::clone(&self.config),
                Rc::clone(&self.sessions),
                self.shutdown.clone(),
            )
        });

        let res: Vec<Result<Session, ClientError>> = futures::future::join_all(futures).await;
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
        self.refresh_leadership(nodes);
    }

    pub(crate) async fn refresh_cluster(&self) {
        if let Ok(Some(nodes)) = self.describe_placement_manager_cluster().await {
            self.refresh_sessions(&nodes).await;
        }
    }

    fn pick_session(&self, lb_policy: LbPolicy) -> Option<Session> {
        match lb_policy {
            LbPolicy::LeaderOnly => self
                .sessions
                .borrow()
                .iter()
                .filter(|&(_, session)| session.state() == NodeState::Leader)
                .map(|(_, session)| session.clone())
                .next(),
            LbPolicy::PickFirst => self
                .sessions
                .borrow()
                .iter()
                .map(|(_, session)| session.clone())
                .next(),
            LbPolicy::RoundRobin => unimplemented!(),
        }
    }

    /// Broadcast heartbeat requests to all nested sessions.
    pub(crate) async fn heartbeat(&self) -> Result<(), ClientError> {
        self.try_reconnect().await;

        if self.need_refresh_cluster() {
            self.refresh_cluster().await;
        }

        let now = Instant::now();
        *self.heartbeat_instant.borrow_mut() = now;

        let request = request::Request {
            timeout: self.config.client_io_timeout(),
            headers: request::Headers::Heartbeat {
                client_id: self.config.client.client_id.clone(),
                role: ClientRole::DataNode,
                data_node: Some(self.config.server.data_node()),
            },
        };

        let mut receivers = vec![];
        {
            let sessions = self.sessions.borrow();
            let futures = sessions
                .iter()
                .map(|(_addr, session)| {
                    let (tx, rx) = oneshot::channel();
                    receivers.push(rx);
                    session.write(request.clone(), tx)
                })
                .collect::<Vec<_>>();
            let _res: Vec<Result<(), InvocationContext>> = futures::future::join_all(futures).await;
        }

        let res: Vec<Result<response::Response, oneshot::error::RecvError>> =
            futures::future::join_all(receivers).await;

        for item in res {
            match item {
                Ok(response) => {
                    if !response.ok() {
                        error!(
                            "Failed to maintain heartbeat to {}. Status-Message: `{}`",
                            self.target, response.status.message
                        );
                        // TODO: refine error handling
                        return Err(ClientError::ServerInternal);
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
        timeout: Option<Duration>,
    ) -> Result<i32, ClientError> {
        self.try_reconnect().await;
        if let Some(session) = self.pick_session(LbPolicy::LeaderOnly) {
            let request = request::Request {
                timeout: timeout.unwrap_or(self.config.client_io_timeout()),
                headers: request::Headers::AllocateId {
                    host: host.to_owned(),
                },
            };
            let (tx, rx) = oneshot::channel();
            if let Err(e) = session.write(request, tx).await {
                error!(
                    "Failed to send ID-allocation-request to {}. Cause: {:?}",
                    self.target, e
                );
                return Err(ClientError::ConnectionRefused(self.target.to_owned()));
            }

            let response = rx.await.map_err(|e| {
                error!(
                    "Internal error while allocating ID from {}. Cause: {:?}",
                    self.target, e
                );
                ClientError::ClientInternal
            })?;

            if !response.ok() {
                error!(
                    "Failed to allocate ID from {}. Status-Message: `{}`",
                    self.target, response.status.message
                );
                // TODO: refine error handling
                return Err(ClientError::ServerInternal);
            }

            if let Some(response::Headers::AllocateId { id }) = response.headers {
                return Ok(id);
            } else {
                unreachable!();
            }
        }

        Err(ClientError::ClientInternal)
    }

    pub(crate) async fn create_stream(
        &self,
        stream_metadata: StreamMetadata,
    ) -> Result<StreamMetadata, ClientError> {
        self.try_reconnect().await;
        let session = self
            .pick_session(LbPolicy::LeaderOnly)
            .ok_or(ClientError::ConnectFailure(self.target.clone()))?;
        let (tx, rx) = oneshot::channel();
        let request = request::Request {
            timeout: self.config.client_io_timeout(),
            headers: request::Headers::CreateStream { stream_metadata },
        };

        if let Err(ctx) = session.write(request, tx).await {
            error!(
                "Failed to send create-stream request to {}. Cause: {:?}",
                self.target, ctx
            );
            return Err(ClientError::ConnectionRefused(self.target.to_owned()));
        }

        let response = rx.await.map_err(|e| {
            error!(
                "Internal error while creating stream from {}. Cause: {:?}",
                self.target, e
            );
            ClientError::ClientInternal
        })?;

        if !response.ok() {
            error!(
                "Failed to create stream from {}. Status: `{:#?}`",
                self.target, response.status
            );
            // TODO: refine error handling according to status code
            return Err(ClientError::ClientInternal);
        }

        if let Some(response::Headers::CreateStream { stream }) = response.headers {
            Ok(stream)
        } else {
            unreachable!();
        }
    }

    pub(crate) async fn describe_stream(
        &self,
        stream_id: u64,
    ) -> Result<StreamMetadata, ClientError> {
        self.try_reconnect().await;
        let session = self
            .pick_session(LbPolicy::PickFirst)
            .ok_or(ClientError::ClientInternal)?;

        let request = request::Request {
            timeout: self.config.client_io_timeout(),
            headers: request::Headers::DescribeStream { stream_id },
        };

        let (tx, rx) = oneshot::channel();
        if let Err(ctx) = session.write(request, tx).await {
            error!(
                "Failed to send describe-stream request to {}. Cause: {:?}",
                self.target, ctx
            );
            return Err(ClientError::ConnectionRefused(self.target.to_owned()));
        }

        let response = rx.await.map_err(|e| {
            error!(
                "Internal error while describing stream from {}. Cause: {:?}",
                self.target, e
            );
            ClientError::ClientInternal
        })?;

        if !response.ok() {
            error!(
                "Failed to describe stream[stream-id={stream_id}] from {}. Status: `{:#?}`",
                self.target, response.status
            );
            // TODO: refine error handling according to status code
            return Err(ClientError::ServerInternal);
        }

        if let Some(response::Headers::DescribeStream { stream }) = response.headers {
            return Ok(stream);
        } else {
            unreachable!();
        }
    }

    /// Create the specified range to the target: placement manager or data node.
    ///
    /// If the target is placement manager, we need to select the session to the primary node;
    pub(crate) async fn create_range(
        &self,
        range: RangeMetadata,
    ) -> Result<RangeMetadata, ClientError> {
        self.try_reconnect().await;
        if let Some(session) = self.pick_session(LbPolicy::LeaderOnly) {
            let (tx, rx) = oneshot::channel();

            let request = request::Request {
                timeout: self.config.client_io_timeout(),
                headers: request::Headers::CreateRange { range },
            };

            if let Err(ctx) = session.write(request, tx).await {
                error!(
                    "Failed to send create-range request to {}. Cause: {:?}",
                    self.target, ctx
                );
                return Err(ClientError::ConnectionRefused(self.target.to_owned()));
            }

            let response = rx.await.map_err(|e| {
                error!(
                    "Internal client error when creating range on {}. Cause: {:?}",
                    self.target, e
                );
                ClientError::ClientInternal
            })?;

            if !response.ok() {
                error!(
                    "Failed to create range on {}. Status: `{:?}`",
                    self.target, response.status
                );
                return Err(ClientError::ServerInternal);
            }
            if let Some(response::Headers::CreateRange { range }) = response.headers {
                return Ok(range);
            } else {
                unreachable!();
            }
        } else {
            Err(ClientError::ClientInternal)
        }
    }

    pub(crate) async fn list_range(
        &self,
        criteria: RangeCriteria,
    ) -> Result<Vec<RangeMetadata>, ClientError> {
        self.try_reconnect().await;
        if let Some(session) = self.pick_session(LbPolicy::LeaderOnly) {
            let request = request::Request {
                timeout: self.config.client_io_timeout(),
                headers: request::Headers::ListRange { criteria },
            };
            let (tx, rx) = oneshot::channel();
            if let Err(_ctx) = session.write(request, tx).await {
                error!("Failed to send list-range request to {}.", self.target);
                return Err(ClientError::ClientInternal);
            }

            let response = rx.await.map_err(|e| {
                error!(
                    "Internal client error when listing ranges from {}. Cause: {:?}",
                    self.target, e
                );
                ClientError::ClientInternal
            })?;

            if !response.ok() {
                error!(
                    "Failed to list-ranges from {}. Status-Message: `{}`",
                    self.target, response.status.message
                );
                // TODO: refine error handling
                return Err(ClientError::ServerInternal);
            }

            if let Some(response::Headers::ListRange { ranges }) = response.headers {
                return ranges.ok_or(ClientError::ClientInternal);
            } else {
                unreachable!();
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

        // Get latest `A` records for access point domain name
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
        let request = request::Request {
            timeout: self.config.client_io_timeout(),
            headers: request::Headers::DescribePlacementManager { data_node },
        };

        let mut request_sent = false;
        'outer: loop {
            for addr in &addrs {
                if let Some(session) = self.sessions.borrow().get(addr).cloned() {
                    if let Err(mut ctx) = session.write(request.clone(), tx).await {
                        tx = ctx
                            .response_observer()
                            .expect("Response observer should NOT be consumed");
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
                        Arc::clone(&self.config),
                        Rc::clone(&self.sessions),
                        self.shutdown.clone(),
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

        match time::timeout(self.config.client_io_timeout(), rx).await {
            Ok(response) => match response {
                Ok(response) => {
                    debug_assert_eq!(
                        response.operation_code,
                        OperationCode::DescribePlacementManager,
                        "Unexpected operation code"
                    );
                    if !response.ok() {
                        warn!(
                            "Failed to describe placement manager cluster: {:?}",
                            response.status
                        );
                        Err(ClientError::ServerInternal)
                    } else {
                        if let Some(response::Headers::DescribePlacementManager { nodes }) =
                            response.headers
                        {
                            trace!("Received placement manager cluster {:?}", nodes);
                            Ok(nodes)
                        } else {
                            Err(ClientError::ClientInternal)
                        }
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
    /// If the seal kind is seal-data-node, resulting `end` of `StreamRange` is data-node specific only.
    /// Final end value of the range will be resolved after MinCopy of data nodes responded.
    pub(crate) async fn seal(
        &self,
        kind: SealKind,
        range: RangeMetadata,
    ) -> Result<RangeMetadata, ClientError> {
        self.try_reconnect().await;
        let session = self
            .pick_session(LbPolicy::LeaderOnly)
            .ok_or(ClientError::ConnectFailure(self.target.clone()))?;
        let request = request::Request {
            timeout: self.config.client_io_timeout(),
            headers: request::Headers::SealRange { kind, range },
        };
        let (tx, rx) = oneshot::channel();
        if let Err(ctx) = session.write(request, tx).await {
            let request = ctx.request();
            error!("Failed to seal range {request:?}");
            return Err(ClientError::ClientInternal);
        }

        let response = rx.await.map_err(|_e| ClientError::ClientInternal)?;
        if !response.ok() {
            warn!("Failed to seal range: {:?}", response.status);
            return Err(ClientError::ServerInternal);
        }

        if let Some(response::Headers::SealRange { range }) = response.headers {
            trace!("Sealed range {:?}", range);
            range.ok_or(ClientError::ClientInternal)
        } else {
            Err(ClientError::ClientInternal)
        }
    }

    /// Try to reconnect to the endpoints that is absent from sessions.
    ///
    ///
    /// # Implementation walkthrough
    /// 1. filter endpoints that is not found in sessions
    /// 2. connect to the target and then create a session
    /// 3. if there is no existing sessions, await till connect attempts completed or failed.
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
                        Arc::clone(&self.config),
                        Rc::clone(&self.sessions),
                        self.shutdown.clone(),
                    )
                })
                .collect::<Vec<_>>();

            if futures.is_empty() {
                return;
            }
            trace!("Reconnecting {} sessions", futures.len());
            let need_await = self.sessions.borrow().is_empty();
            let sessions = Rc::clone(&self.sessions);
            let handle = tokio_uring::spawn(async move {
                futures::future::join_all(futures)
                    .await
                    .into_iter()
                    .for_each(|res| match res {
                        Ok(session) => {
                            let target = session.target;
                            sessions.borrow_mut().insert(target, session);
                        }
                        Err(e) => {
                            error!("Failed to connect. Cause: {:?}", e);
                        }
                    });
            });

            if need_await {
                let _ = handle.await;
            }
        }
    }

    async fn connect(
        addr: SocketAddr,
        duration: Duration,
        config: Arc<config::Configuration>,
        sessions: Rc<RefCell<HashMap<SocketAddr, Session>>>,
        shutdown: broadcast::Sender<()>,
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
            &config,
            Rc::downgrade(&sessions),
            shutdown.clone(),
        ))
    }

    pub(crate) async fn append(
        &self,
        buf: Vec<Bytes>,
    ) -> Result<Vec<AppendResultEntry>, ClientError> {
        self.try_reconnect().await;
        let session = self
            .pick_session(self.lb_policy)
            .ok_or(ClientError::ConnectFailure(self.target.clone()))?;

        let request = request::Request {
            timeout: self.config.client_io_timeout(),
            headers: request::Headers::Append { buf },
        };

        let (tx, rx) = oneshot::channel();
        if let Err(ctx) = session.write(request, tx).await {
            let request = ctx.request();
            // TODO: decode append info from flat_record_batch_bytes to provide readable information.
            error!("Failed to append {request:?}");
            return Err(ClientError::ClientInternal);
        }

        let response = rx.await.map_err(|_e| ClientError::ClientInternal)?;
        if !response.ok() {
            warn!("Failed to append: {:?}", response.status);
            return Err(ClientError::ServerInternal);
        }

        if let Some(response::Headers::Append { entries }) = response.headers {
            trace!("Append entries {:?}", entries);
            Ok(entries)
        } else {
            Err(ClientError::ClientInternal)
        }
    }

    pub(crate) async fn fetch(
        &self,
        request: FetchRequestEntry,
    ) -> Result<FetchResultEntry, ClientError> {
        // TODO: support fetch request group in session level.
        self.try_reconnect().await;
        let session = self
            .pick_session(self.lb_policy)
            .ok_or(ClientError::ConnectFailure(self.target.clone()))?;

        let request = request::Request {
            timeout: self.config.client_io_timeout(),
            headers: request::Headers::Fetch {
                entries: vec![request],
            },
        };
        let (tx, rx) = oneshot::channel();
        if let Err(ctx) = session.write(request, tx).await {
            let request = ctx.request();
            error!("Failed to fetch {request:?} to {}", self.target);
            return Err(ClientError::ClientInternal);
        }

        let response = rx.await.map_err(|_e| ClientError::ClientInternal)?;
        if !response.ok() {
            warn!("Failed to fetch: {:?}", response.status);
            return Err(ClientError::ServerInternal);
        }

        if let Some(response::Headers::Fetch { entries }) = response.headers {
            trace!("Fetch entries {:?}", entries);
            if entries.len() != 1 {
                error!("Expect exactly one entry in fetch response");
                Err(ClientError::ClientInternal)
            } else {
                let entry = FetchResultEntry {
                    status: entries[0].status.clone(),
                    data: response.payload,
                };
                Ok(entry)
            }
        } else {
            Err(ClientError::ClientInternal)
        }
    }

    pub(crate) async fn report_metrics(
        &self,
        uring_statistics: &UringStatistics,
        data_node_statistics: &DataNodeStatistics,
        disk_statistics: &DiskStatistics,
        memory_statistics: &MemoryStatistics,
    ) -> Result<(), ClientError> {
        self.try_reconnect().await;
        if self.need_refresh_cluster() {
            self.refresh_cluster().await;
        }
        // TODO: add disk_unindexed_data_size, range_missing_replica_cnt, range_active_cnt
        let extension = request::Headers::ReportMetrics {
            data_node: self.config.server.data_node(),
            disk_in_rate: disk_statistics.get_disk_in_rate(),
            disk_out_rate: disk_statistics.get_disk_out_rate(),
            disk_free_space: disk_statistics.get_disk_free_space(),
            disk_unindexed_data_size: 0,
            memory_used: memory_statistics.get_memory_used(),
            uring_task_rate: uring_statistics.get_uring_task_rate(),
            uring_inflight_task_cnt: uring_statistics.get_uring_inflight_task_cnt(),
            uring_pending_task_cnt: uring_statistics.get_uring_pending_task_cnt(),
            uring_task_avg_latency: uring_statistics.get_uring_task_avg_latency(),
            network_append_rate: data_node_statistics.get_network_append_rate(),
            network_fetch_rate: data_node_statistics.get_network_fetch_rate(),
            network_failed_append_rate: data_node_statistics.get_network_failed_append_rate(),
            network_failed_fetch_rate: data_node_statistics.get_network_failed_fetch_rate(),
            network_append_avg_latency: data_node_statistics.get_network_append_avg_latency(),
            network_fetch_avg_latency: data_node_statistics.get_network_fetch_avg_latency(),
            range_missing_replica_cnt: 0,
            range_active_cnt: 0,
        };
        let request = request::Request {
            timeout: self.config.client_io_timeout(),
            headers: extension,
        };

        let mut receivers = vec![];
        {
            let sessions = self.sessions.borrow();
            let futures = sessions
                .iter()
                .map(|(_addr, session)| {
                    let (tx, rx) = oneshot::channel();
                    receivers.push(rx);
                    session.write(request.clone(), tx)
                })
                .collect::<Vec<_>>();
            let _res: Vec<Result<(), InvocationContext>> = futures::future::join_all(futures).await;
        }
        let res: Vec<Result<response::Response, oneshot::error::RecvError>> =
            futures::future::join_all(receivers).await;
        for item in res {
            match item {
                Ok(response) => {
                    if !response.ok() {
                        error!(
                            "Failed to report metrics to {}. Status-Message: `{}`",
                            self.target, response.status.message
                        );
                        return Err(ClientError::ServerInternal);
                    }
                }
                Err(_e) => {}
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use tokio::sync::broadcast;

    use super::CompositeSession;
    use crate::lb_policy::LbPolicy;
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
        test_util::try_init_log();
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
            assert_eq!(1, nodes.len());
            Ok(())
        })
    }
}
