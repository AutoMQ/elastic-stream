use std::{
    cell::RefCell,
    collections::BTreeMap,
    rc::{Rc, Weak},
};

use bytes::Bytes;
use client::Client;
use itertools::Itertools;
use model::range::Range;

use crate::ReplicationError;

use super::{replication_context::ReplicationContext, replicator::Replicator};

#[derive(Debug)]
pub(crate) struct ReplicationRange {
    metadata: Range,

    client: Weak<Client>,

    replicators: Vec<Replicator>,

    /// Inflight append entry requests.
    inflight: RefCell<BTreeMap<u64, ReplicationContext>>,
}

impl ReplicationRange {
    pub(crate) fn new(metadata: Range, client: Weak<Client>) -> Rc<Self> {
        let this = Self {
            metadata,
            client,
            replicators: vec![],
            inflight: RefCell::new(BTreeMap::new()),
        };

        Rc::new(this)
    }

    pub(crate) fn is_sealed(&self) -> bool {
        self.metadata.end().is_some()
    }

    pub(crate) fn client(&self) -> Option<Rc<Client>> {
        self.client.upgrade()
    }

    pub(crate) fn create_replicator(
        range: Rc<ReplicationRange>,
        start_offset: u64,
    ) -> Result<(), ReplicationError> {
        Ok(())
    }

    fn confirm_offset(&self) -> Result<u64, ReplicationError> {
        if self.replicators.is_empty() {
            return Err(ReplicationError::Internal);
        }

        self.replicators
            .iter()
            .map(|r| r.confirm_offset())
            .sorted() // Ascending order
            .nth((self.replicators.len() - 1) / 2)
            .ok_or(ReplicationError::Internal)
    }

    pub(crate) fn append(&self, payload: Bytes) -> Result<(), ReplicationError> {
        if self.is_sealed() {
            return Err(ReplicationError::AlreadySealed);
        }

        for replicator in &self.replicators {
            replicator.append(payload.clone());
        }

        Ok(())
    }

    /// Ack requests to downstream clients.
    pub(crate) fn try_ack(&self) -> Result<(), ReplicationError> {
        let confirm_offset = self.confirm_offset()?;
        let mut inflight = self.inflight.borrow_mut();
        loop {
            if let Some(entry) = inflight.first_entry() {
                if entry.key() > &confirm_offset {
                    break;
                }
                let mut context = entry.remove();
                context.ack();
            } else {
                break;
            }
        }

        Ok(())
    }
}
