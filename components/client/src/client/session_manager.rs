use super::composite_session::CompositeSession;
use crate::error::ClientError;
use slog::Logger;
use std::{cell::UnsafeCell, collections::HashMap, rc::Rc, sync::Arc};

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

    pub(crate) async fn get_composite_session(
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
