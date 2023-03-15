use super::session_state::SessionState;
use super::{config, response};
use crate::{client::response::Status, notifier::Notifier};
use codec::frame::{Frame, OperationCode};
use model::{client_role::ClientRole, request::Request};
use slog::{error, trace, warn, Logger};
use std::{
    cell::UnsafeCell,
    collections::HashMap,
    rc::Rc,
    time::{Duration, Instant},
};
use tokio::sync::oneshot;
use tokio_uring::net::TcpStream;
use transport::channel::Channel;

pub(crate) struct Session {
    config: Rc<config::ClientConfig>,

    log: Logger,

    state: SessionState,

    channel: Rc<Channel>,

    /// In-flight requests.
    inflight_requests: Rc<UnsafeCell<HashMap<u32, oneshot::Sender<response::Response>>>>,

    idle_since: Instant,
}

impl Session {
    pub(crate) fn new(
        stream: TcpStream,
        endpoint: &str,
        config: &Rc<config::ClientConfig>,
        notifier: Rc<dyn Notifier>,
        log: &Logger,
    ) -> Self {
        let channel = Rc::new(Channel::new(stream, endpoint, log.clone()));
        let inflight = Rc::new(UnsafeCell::new(HashMap::new()));

        {
            let _inflight = Rc::clone(&inflight);
            let _log = log.clone();
            let _channel = Rc::clone(&channel);
            tokio_uring::spawn(async move {
                let inflight_requests = _inflight;
                let channel = _channel;
                let log = _log;
                loop {
                    match channel.read_frame().await {
                        Err(e) => {
                            // Handle connection reset
                            todo!()
                        }
                        Ok(Some(frame)) => {
                            let inflight = unsafe { &mut *inflight_requests.get() };
                            if frame.is_response() {
                                Session::on_response(inflight, frame, &log);
                            } else {
                                let response = notifier.on_notification(frame);
                                channel.write_frame(&response).await.unwrap_or_else(|e| {
                                    warn!(
                                        log,
                                        "Failed to write response to server. Cause: {:?}", e
                                    );
                                });
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

        Self {
            config: Rc::clone(config),
            log: log.clone(),
            state: SessionState::Active,
            channel,
            inflight_requests: inflight,
            idle_since: Instant::now(),
        }
    }

    pub(crate) async fn write(
        &mut self,
        request: &Request,
        response_observer: oneshot::Sender<response::Response>,
    ) -> Result<(), oneshot::Sender<response::Response>> {
        trace!(self.log, "Sending request {:?}", request);

        // Update last read/write instant.
        self.idle_since = Instant::now();

        match request {
            Request::Heartbeat { .. } => {
                let mut frame = Frame::new(OperationCode::Heartbeat);
                let header = request.into();
                frame.header = Some(header);
                match self.channel.write_frame(&frame).await {
                    Ok(_) => {
                        let inflight_requests = unsafe { &mut *self.inflight_requests.get() };
                        inflight_requests.insert(frame.stream_id, response_observer);
                        trace!(
                            self.log,
                            "Write `Heartbeat` request to {}",
                            self.channel.peer_address()
                        );
                    }
                    Err(e) => {
                        error!(
                            self.log,
                            "Failed to write request to network. Cause: {:?}", e
                        );
                    }
                }
            }

            Request::ListRanges { .. } => {
                let mut list_range_frame = Frame::new(OperationCode::ListRanges);
                let header = request.into();
                list_range_frame.header = Some(header);

                match self.channel.write_frame(&list_range_frame).await {
                    Ok(_) => {
                        let inflight_requests = unsafe { &mut *self.inflight_requests.get() };
                        inflight_requests.insert(list_range_frame.stream_id, response_observer);
                        trace!(
                            self.log,
                            "Write `ListRange` request to {}",
                            self.channel.peer_address()
                        );
                    }
                    Err(e) => {
                        warn!(
                            self.log,
                            "Failed to write `ListRange` request to {}. Cause: {:?}",
                            self.channel.peer_address(),
                            e
                        );
                        return Err(response_observer);
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
        self.active() && (Instant::now() - self.idle_since >= *duration)
    }

    pub(crate) async fn heartbeat(&mut self) -> Option<oneshot::Receiver<response::Response>> {
        // If the current session is being closed, heartbeat will be no-op.
        if self.state == SessionState::Closing {
            return None;
        }

        let request = Request::Heartbeat {
            client_id: self.config.client_id.clone(),
            role: ClientRole::DataNode,
            data_node: self.config.data_node.clone(),
        };
        let (response_observer, rx) = oneshot::channel();
        if self.write(&request, response_observer).await.is_ok() {
            trace!(
                self.log,
                "Heartbeat sent to {}",
                self.channel.peer_address()
            );
        }
        Some(rx)
    }

    fn on_response(
        inflight: &mut HashMap<u32, oneshot::Sender<response::Response>>,
        response: Frame,
        log: &Logger,
    ) {
        let stream_id = response.stream_id;
        trace!(
            log,
            "Received {} response for stream-id={}",
            response.operation_code,
            stream_id
        );

        match inflight.remove(&stream_id) {
            Some(sender) => {
                let res = match response.operation_code {
                    OperationCode::Heartbeat => {
                        trace!(log, "Mock parsing {} response", response.operation_code);
                        response::Response::Heartbeat { status: Status::OK }
                    }
                    OperationCode::ListRanges => {
                        trace!(log, "Mock parsing {} response", response.operation_code);
                        response::Response::ListRange {
                            status: Status::OK,
                            ranges: None,
                        }
                    }
                    _ => {
                        warn!(log, "Unsupported operation {}", response.operation_code);
                        return;
                    }
                };

                sender.send(res).unwrap_or_else(|_resp| {
                    warn!(log, "Failed to forward response to Client");
                });
            }
            None => {
                warn!(
                    log,
                    "Expected in-flight request[stream-id={}] is missing", response.stream_id
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
                status: Status::Aborted,
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

    use std::error::Error;

    use test_util::{run_listener, terminal_logger};

    use crate::notifier::UnsupportedNotifier;

    use super::*;

    /// Verify it's OK to create a new session.
    #[test]
    fn test_new() -> Result<(), Box<dyn Error>> {
        tokio_uring::start(async {
            let logger = terminal_logger();
            let port = run_listener(logger.clone()).await;
            let target = format!("127.0.0.1:{}", port);
            let stream = TcpStream::connect(target.parse()?).await?;
            let config = Rc::new(config::ClientConfig::default());
            let notifier = Rc::new(UnsupportedNotifier {});
            let session = Session::new(stream, &target, &config, notifier, &logger);

            assert_eq!(SessionState::Active, session.state());

            assert_eq!(false, session.need_heartbeat(&Duration::from_secs(1)));

            Ok(())
        })
    }

    #[test]
    fn test_heartbeat() -> Result<(), Box<dyn Error>> {
        tokio_uring::start(async {
            let logger = terminal_logger();
            let port = run_listener(logger.clone()).await;
            let target = format!("127.0.0.1:{}", port);
            let stream = TcpStream::connect(target.parse()?).await?;
            let config = Rc::new(config::ClientConfig::default());
            let notifier = Rc::new(UnsupportedNotifier {});
            let mut session = Session::new(stream, &target, &config, notifier, &logger);

            let result = session.heartbeat().await;
            let response = result.unwrap().await;
            assert_eq!(true, response.is_ok());
            Ok(())
        })
    }

    #[test]
    fn test_list_ranges() -> Result<(), Box<dyn Error>> {
        tokio_uring::start(async {
            let logger = terminal_logger();
            let port = run_listener(logger.clone()).await;
            let target = format!("127.0.0.1:{}", port);
            let stream = TcpStream::connect(target.parse()?).await?;
            let config = Rc::new(config::ClientConfig::default());
            let notifier = Rc::new(UnsupportedNotifier {});
            let mut session = Session::new(stream, &target, &config, notifier, &logger);

            let result = session.heartbeat().await;
            let response = result.unwrap().await;
            assert_eq!(true, response.is_ok());
            Ok(())
        })
    }
}
