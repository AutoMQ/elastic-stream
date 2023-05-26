use super::composite_session::CompositeSession;
use crate::error::ClientError;
use std::{cell::UnsafeCell, collections::HashMap, rc::Rc, sync::Arc};
use tokio::sync::broadcast;

pub struct SessionManager {
    /// Configuration for the transport layer.
    config: Arc<config::Configuration>,

    /// Session management
    sessions: Rc<UnsafeCell<HashMap<String, Rc<CompositeSession>>>>,

    shutdown: broadcast::Sender<()>,
}

impl SessionManager {
    pub(crate) fn new(
        config: &Arc<config::Configuration>,
        shutdown: broadcast::Sender<()>,
    ) -> Self {
        let sessions = Rc::new(UnsafeCell::new(HashMap::new()));
        Self {
            config: Arc::clone(config),
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
                let lb_policy = if target == self.config.placement_manager {
                    super::lb_policy::LbPolicy::LeaderOnly
                } else {
                    super::lb_policy::LbPolicy::PickFirst
                };

                let session = Rc::new(
                    CompositeSession::new(
                        target,
                        Arc::clone(&self.config),
                        lb_policy,
                        self.shutdown.clone(),
                    )
                    .await?,
                );

                if lb_policy == super::lb_policy::LbPolicy::LeaderOnly {
                    session.refresh_cluster().await;
                }

                sessions.insert(target.to_owned(), Rc::clone(&session));
                Ok(session)
            }
        }
    }
}
