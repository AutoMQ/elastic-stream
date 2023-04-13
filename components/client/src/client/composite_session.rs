use super::{lb_policy::LbPolicy, response, session::Session};
use crate::{error::ClientError, ClientConfig, Response};
use model::{
    client_role::ClientRole, range::StreamRange, range_criteria::RangeCriteria, request::Request,
    PlacementManagerNode,
};
use protocol::rpc::header::ErrorCode;
use slog::{error, info, trace, warn, Logger};
use std::{
    cell::RefCell,
    collections::HashMap,
    io::ErrorKind,
    net::{SocketAddr, ToSocketAddrs},
    rc::Rc,
    time::Duration,
};
use tokio::{sync::oneshot, time::timeout};
use tokio_uring::net::TcpStream;

pub(crate) struct CompositeSession {
    target: String,
    config: Rc<ClientConfig>,
    lb_policy: LbPolicy,
    sessions: RefCell<HashMap<SocketAddr, Session>>,
    log: Logger,
}

impl CompositeSession {
    pub(crate) async fn new<T>(
        target: T,
        config: Rc<ClientConfig>,
        lb_policy: LbPolicy,
        log: Logger,
    ) -> Result<Self, ClientError>
    where
        T: ToSocketAddrs + ToString,
    {
        let mut sessions = RefCell::new(HashMap::new());
        // For now, we just resolve one session out of the target.
        // In the future, we would support multiple internal connection and load balancing among them.
        for socket_addr in target
            .to_socket_addrs()
            .map_err(|_e| ClientError::BadAddress)?
        {
            let session =
                Self::connect(socket_addr.clone(), config.connect_timeout, &config, &log).await?;
            sessions.borrow_mut().insert(socket_addr, session);
        }
        info!(
            log,
            "CompositeSession to {} has {} internal session(s)",
            target.to_string(),
            sessions.borrow().len()
        );
        Ok(Self {
            target: target.to_string(),
            config,
            lb_policy,
            sessions,
            log,
        })
    }

    pub(crate) fn need_heartbeat(&self, interval: &Duration) -> bool {
        true
    }

    /// Broadcast heartbeat requests to all nested sessions.
    ///
    ///
    pub(crate) async fn heartbeat(&self) -> Result<(), ClientError> {
        if let Some((_, session)) = self.sessions.borrow().iter().next() {
            let request = Request::Heartbeat {
                client_id: self.config.client_id.clone(),
                role: ClientRole::DataNode,
                data_node: self.config.data_node.clone(),
            };
            let (tx, rx) = oneshot::channel();
            if let Err(e) = session.write(&request, tx).await {
                error!(
                    self.log,
                    "Failed to send heartbeat-request to {}. Cause: {:?}", self.target, e
                );
                return Err(ClientError::ClientInternal);
            }

            if let Response::Heartbeat { status } = rx.await.map_err(|e| {
                error!(
                    self.log,
                    "Internal error while allocating ID from {}. Cause: {:?}", self.target, e
                );
                ClientError::ClientInternal
            })? {
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
                return Ok(());
            }
        }

        Err(ClientError::ClientInternal)
    }

    pub(crate) async fn allocate_id(
        &self,
        host: &str,
        timeout: Duration,
    ) -> Result<i32, ClientError> {
        if let Some((_, session)) = self.sessions.borrow().iter().next() {
            let request = Request::AllocateId {
                timeout: self.config.io_timeout,
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
                timeout: self.config.io_timeout,
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
            .into_iter()
            .collect::<Vec<_>>();

        let (mut tx, rx) = oneshot::channel();
        let data_node = self
            .config
            .data_node
            .clone()
            .ok_or(ClientError::ClientInternal)?;
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
                        self.config.connect_timeout,
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
        config: &Rc<ClientConfig>,
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
    use model::DataNode;

    use super::CompositeSession;
    use crate::{client::lb_policy::LbPolicy, ClientConfig};
    use std::{error::Error, rc::Rc};

    #[test]
    fn test_new() -> Result<(), Box<dyn Error>> {
        let log = test_util::terminal_logger();
        let client_config = Rc::new(ClientConfig::default());
        tokio_uring::start(async {
            let port = test_util::run_listener(log.clone()).await;
            let target = format!("{}:{}", "localhost", port);
            let _session =
                CompositeSession::new(&target, client_config, LbPolicy::PickFirst, log.clone())
                    .await?;

            Ok(())
        })
    }

    #[test]
    fn test_describe_placement_manager_cluster() -> Result<(), Box<dyn Error>> {
        let log = test_util::terminal_logger();
        let mut client_config = ClientConfig::default();
        let data_node = DataNode {
            node_id: 1,
            advertise_address: "localhost:1234".to_owned(),
        };
        client_config.with_data_node(data_node);
        let client_config = Rc::new(client_config);
        tokio_uring::start(async {
            let port = test_util::run_listener(log.clone()).await;
            let target = format!("{}:{}", "localhost", port);
            let composite_session =
                CompositeSession::new(&target, client_config, LbPolicy::PickFirst, log.clone())
                    .await?;
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
