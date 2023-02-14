use std::{
    cell::UnsafeCell, collections::HashMap, io::ErrorKind, net::SocketAddr, rc::Rc, str::FromStr,
    time::Duration,
};

use slog::{debug, error, info, trace, warn, Logger};
use tokio::{
    sync::{mpsc, oneshot},
    time::{timeout, Instant},
};
use tokio_uring::net::TcpStream;

use crate::{client::response::Status, error::ClientError};

use super::{
    config,
    naming::{self, Endpoints},
    request, response,
    session::Session,
    LBPolicy,
};

pub struct SessionManager {
    /// Configuration for the transport layer.
    config: Rc<config::ClientConfig>,

    /// Receiver of SubmitRequestChannel.
    /// It is used by `Client` to submit requst to `SessionManager`. Requests are expected to be converted into `Command`s and then
    /// forwarded to transport layer.
    rx: mpsc::UnboundedReceiver<(request::Request, oneshot::Sender<response::Response>)>,

    log: Logger,

    /// Parsed endpoints from target url.
    endpoints: naming::Endpoints,

    // Session management
    lb_policy: LBPolicy,
    sessions: Rc<UnsafeCell<HashMap<SocketAddr, Session>>>,
    session_mgr_tx: mpsc::UnboundedSender<(SocketAddr, oneshot::Sender<bool>)>,

    // MPMC channel
    stop_tx: mpsc::Sender<()>,
}

impl SessionManager {
    pub(crate) fn new(
        target: &str,
        config: &Rc<config::ClientConfig>,
        rx: mpsc::UnboundedReceiver<(request::Request, oneshot::Sender<response::Response>)>,
        log: &Logger,
    ) -> Result<Self, ClientError> {
        let (session_mgr_tx, mut session_mgr_rx) =
            mpsc::unbounded_channel::<(SocketAddr, oneshot::Sender<bool>)>();
        let sessions = Rc::new(UnsafeCell::new(HashMap::new()));

        // Handle session re-connect event.
        {
            let sessions = Rc::clone(&sessions);
            let timeout = config.connect_timeout;
            let logger = log.clone();
            let _config = Rc::clone(config);
            tokio_uring::spawn(async move {
                let config = _config;
                loop {
                    match session_mgr_rx.recv().await {
                        Some((addr, tx)) => {
                            let sessions = unsafe { &mut *sessions.get() };
                            match SessionManager::connect(&addr, timeout, &config, &logger).await {
                                Ok(session) => {
                                    sessions.insert(addr.clone(), session);
                                    match tx.send(true) {
                                        Ok(_) => {
                                            trace!(logger, "Session creation is notified");
                                        }
                                        Err(res) => {
                                            debug!(
                                                logger,
                                                "Failed to notify session creation result: `{}`",
                                                res
                                            );
                                        }
                                    }
                                }
                                Err(e) => {
                                    error!(
                                        logger,
                                        "Failed to connect to `{:?}`. Cause: `{:?}`", addr, e
                                    );
                                    match tx.send(false) {
                                        Ok(_) => {}
                                        Err(res) => {
                                            debug!(
                                                logger,
                                                "Failed to notify session creation result: `{}`",
                                                res
                                            );
                                        }
                                    }
                                }
                            }
                        }
                        None => {
                            break;
                        }
                    }
                }
            });
        }

        // Heartbeat
        let (stop_tx, mut stop_rx) = mpsc::channel::<()>(1);
        {
            let sessions = Rc::clone(&sessions);
            let logger = log.clone();
            let idle_interval = config.heartbeat_interval;

            tokio_uring::spawn(async move {
                tokio::pin! {
                    let stop_fut = stop_rx.recv();

                    // Interval to check if a session needs to send a heartbeat request.
                    let sleep = tokio::time::sleep(idle_interval);
                }

                loop {
                    tokio::select! {
                        _ = &mut stop_fut => {
                            info!(logger, "Got notified to stop");
                            break;
                        }

                        hb = &mut sleep => {
                            sleep.as_mut().reset(Instant::now() + idle_interval);

                            let sessions = unsafe {&mut *sessions.get()};
                            let mut futs = Vec::with_capacity(sessions.len());
                            for (_addr, session) in sessions.iter_mut() {
                                if session.need_heartbeat(&idle_interval) {
                                    futs.push(session.heartbeat());
                                }
                            }
                            futures::future::join_all(futs).await;
                        }
                    }
                }
            });
        }

        let endpoints = Endpoints::from_str(target)?;

        Ok(Self {
            config: Rc::clone(config),
            rx,
            log: log.clone(),
            endpoints,
            lb_policy: LBPolicy::PickFirst,
            session_mgr_tx,
            sessions,
            stop_tx,
        })
    }

    async fn poll_enqueue(&mut self) -> Result<(), ClientError> {
        trace!(self.log, "poll_enqueue"; "struct" => "SessionManager");
        match self.rx.recv().await {
            Some((req, response_observer)) => {
                trace!(self.log, "Received a request `{:?}`", req; "method" => "poll_enqueue");
                self.dispatch(req, response_observer).await;
            }
            None => {
                return Err(ClientError::ChannelClosing(
                    "SubmitRequestChannel".to_owned(),
                ));
            }
        };
        Ok(())
    }

    async fn dispatch(
        &mut self,
        request: request::Request,
        mut response_observer: oneshot::Sender<response::Response>,
    ) {
        trace!(self.log, "Received a request `{:?}`", request; "method" => "dispatch");

        let sessions = unsafe { &mut *self.sessions.get() };
        if sessions.is_empty() {
            // Create a new session
            match self.lb_policy {
                LBPolicy::PickFirst => {
                    if let Some(&socket_addr) = self.endpoints.get() {
                        let (tx, rx) = oneshot::channel();
                        self.session_mgr_tx
                            .send((socket_addr.clone(), tx))
                            .unwrap_or_else(|e| {
                                error!(self.log, "Failed to create a new session. Cause: {:?}", e);
                            });
                        match rx.await {
                            Ok(result) => {
                                if result {
                                    trace!(self.log, "Session to {:?} was created", socket_addr);
                                } else {
                                    warn!(
                                        self.log,
                                        "Failed to create session to {:?}", socket_addr
                                    );
                                }
                            }
                            Err(e) => {
                                warn!(
                                    self.log,
                                    "Got an error while creating session to {:?}. Error: {:?}",
                                    socket_addr,
                                    e
                                );
                            }
                        }
                    } else {
                        error!(self.log, "No endpoints available to connect.");
                        let response = response::Response::ListRange {
                            status: Status::Unavailable,
                            ranges: None,
                        };
                        response_observer
                            .send(response)
                            .unwrap_or_else(|_response| {
                                warn!(self.log, "Failed to write response to `Client`");
                            });
                        return;
                    }
                }
            }
        }

        let mut attempt = 0;
        for (addr, session) in sessions.iter_mut() {
            attempt += 1;
            if attempt > self.config.max_attempt {
                match request {
                    request::Request::Heartbeat { .. } => {}
                    request::Request::ListRange { .. } => {
                        let response = response::Response::ListRange {
                            status: Status::Unavailable,
                            ranges: None,
                        };
                        match response_observer.send(response) {
                            Ok(_) => {}
                            Err(e) => {
                                warn!(
                                    self.log,
                                    "Failed to propogate error response. Cause: {:?}", e
                                );
                            }
                        }
                    }
                }
                break;
            }
            trace!(
                self.log,
                "Attempt to write {} request for the {} time",
                request,
                ordinal::Ordinal(attempt)
            );
            response_observer = match session.write(&request, response_observer).await {
                Ok(_) => {
                    trace!(self.log, "Request[`{request:?}`] forwarded to {addr:?}");
                    break;
                }
                Err(observer) => {
                    error!(self.log, "Failed to forward request to {addr:?}");
                    observer
                }
            }
        }
    }

    pub(super) async fn run(&mut self) {
        trace!(self.log, "run"; "struct" => "SessionManager");
        loop {
            if let Err(ClientError::ChannelClosing(_)) = self.poll_enqueue().await {
                info!(self.log, "SubmitRequsetChannel is half closed");
                break;
            }
        }
    }

    async fn connect(
        addr: &SocketAddr,
        duration: Duration,
        config: &Rc<config::ClientConfig>,
        log: &Logger,
    ) -> Result<Session, ClientError> {
        trace!(log, "Establishing connection to {:?}", addr);
        let endpoint = addr.to_string();
        let connect = TcpStream::connect(addr.clone());
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

        Ok(Session::new(stream, &endpoint, config, log))
    }
}
