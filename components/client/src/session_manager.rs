use super::composite_session::CompositeSession;
use crate::error::ClientError;
use log::error;
use model::client_role::ClientRole;
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

    /// Broadcast keep-alive heartbeat to all composite sessions managed by current manager.
    pub(crate) async fn broadcast_heartbeat(&self, role: ClientRole) {
        let map = unsafe { &mut *self.sessions.get() };
        let composite_sessions = map.iter().map(|(_k, v)| Rc::clone(v)).collect::<Vec<_>>();

        for composite_session in composite_sessions {
            composite_session.heartbeat(role).await;
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
                    if session.refresh_placement_manager_cluster().await.is_err() {
                        error!("Failed to refresh placement manager cluster for {target}");
                    }
                }

                sessions.insert(target.to_owned(), Rc::clone(&session));
                Ok(session)
            }
        }
    }
}
