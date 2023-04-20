use super::composite_session::CompositeSession;
use crate::error::ClientError;
use slog::Logger;
use std::{cell::UnsafeCell, collections::HashMap, rc::Rc, sync::Arc};
use tokio::sync::broadcast;

pub struct SessionManager {
    /// Configuration for the transport layer.
    config: Arc<config::Configuration>,

    log: Logger,

    /// Session management
    sessions: Rc<UnsafeCell<HashMap<String, Rc<CompositeSession>>>>,

    shutdown: broadcast::Sender<()>,
}

impl SessionManager {
    pub(crate) fn new(
        config: &Arc<config::Configuration>,
        shutdown: broadcast::Sender<()>,
        log: &Logger,
    ) -> Self {
        let sessions = Rc::new(UnsafeCell::new(HashMap::new()));
        Self {
            config: Arc::clone(config),
            log: log.clone(),
            sessions,
            shutdown,
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
                        self.shutdown.clone(),
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
