use super::composite_session::CompositeSession;
use crate::error::ClientError;
use slog::{info, trace, Logger};
use std::{cell::UnsafeCell, collections::HashMap, rc::Rc, sync::Arc};
use tokio::time::{timeout, Instant};

pub struct SessionManager {
    /// Configuration for the transport layer.
    config: Arc<config::Configuration>,

    log: Logger,

    /// Session management
    sessions: Rc<UnsafeCell<HashMap<String, Rc<CompositeSession>>>>,

    stop_rx: Option<async_channel::Receiver<()>>,
}

impl SessionManager {
    pub(crate) fn new(
        config: &Arc<config::Configuration>,
        stop_rx: async_channel::Receiver<()>,
        log: &Logger,
    ) -> Self {
        let sessions = Rc::new(UnsafeCell::new(HashMap::new()));
        Self {
            config: Arc::clone(config),
            log: log.clone(),
            sessions,
            stop_rx: Some(stop_rx),
        }
    }

    fn start_heartbeat_loop(
        logger: Logger,
        config: Arc<config::Configuration>,
        stop_rx: async_channel::Receiver<()>,
        sessions: Rc<UnsafeCell<HashMap<String, Rc<CompositeSession>>>>,
    ) {
        let heartbeat_interval = config.client_heartbeat_interval();
        let io_timeout = config.client_io_timeout();

        tokio_uring::spawn(async move {
            tokio::pin! {
                let stop_fut = stop_rx.recv();

                // Interval to check if a session needs to send a heartbeat request.
                let sleep = tokio::time::sleep(heartbeat_interval);
            }

            loop {
                tokio::select! {
                    _ = &mut stop_fut => {
                        info!(logger, "Got notified to stop");
                        break;
                    }

                    _hb = &mut sleep => {
                        sleep.as_mut().reset(Instant::now() + heartbeat_interval);
                        let sessions_ = unsafe {&*sessions.get()};
                        let mut futures = Vec::with_capacity(sessions_.len());
                        for (_addr, session) in sessions_.iter() {
                            if session.need_heartbeat() {
                                trace!(logger, "Heartbeat to {:?}", _addr);
                                futures.push(timeout(io_timeout, session.heartbeat()));
                            }
                        }
                         futures::future::join_all(futures).await;
                    }
                }
            }
        });
    }

    pub(super) async fn start(&self) -> Result<(), ClientError> {
        trace!(self.log, "Start session manager");

        if let Some(ref stop_rx) = self.stop_rx {
            Self::start_heartbeat_loop(
                self.log.clone(),
                Arc::clone(&self.config),
                stop_rx.clone(),
                Rc::clone(&self.sessions),
            );

            // Start other task loops
        }

        Ok(())
    }

    pub(crate) async fn get_session(
        &mut self,
        target: &str,
    ) -> Result<Rc<CompositeSession>, ClientError> {
        let sessions = unsafe { &mut *self.sessions.get() };
        match sessions.get(target) {
            Some(session) => Ok(Rc::clone(session)),
            None => {
                let session = Rc::new(
                    CompositeSession::new(
                        target,
                        Arc::clone(&self.config),
                        super::lb_policy::LbPolicy::PickFirst,
                        self.log.clone(),
                    )
                    .await?,
                );
                sessions.insert(target.to_owned(), Rc::clone(&session));
                Ok(session)
            }
        }
    }
}
