use std::{
    cell::RefCell,
    collections::BTreeMap,
    rc::{Rc, Weak},
};

use bytes::Bytes;
use client::Client;
use itertools::Itertools;
use model::range::{self, RangeMetadata};

use crate::ReplicationError;

use super::{replication_context::ReplicationContext, replicator::Replicator};

#[derive(Debug)]
pub(crate) struct ReplicationRange {
    metadata: RangeMetadata,

    client: Weak<Client>,

    replicators: Vec<Replicator>,

    /// Inflight append entry requests.
    inflight: RefCell<BTreeMap<u64, ReplicationContext>>,

    /// exclusive confirm offset.
    confirm_offset: RefCell<u64>,
}

impl ReplicationRange {
    pub(crate) fn new(metadata: RangeMetadata, client: Weak<Client>) -> Rc<Self> {
        let confirm_offset = metadata.end().unwrap_or_else(|| metadata.start());
        let this = Self {
            metadata,
            client,
            replicators: vec![],
            inflight: RefCell::new(BTreeMap::new()),
            confirm_offset: RefCell::new(confirm_offset),
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

    fn calculate_confirm_offset(&self) -> Result<u64, ReplicationError> {
        if self.replicators.is_empty() {
            return Err(ReplicationError::Internal);
        }

        // Example1: replicas confirmOffset = [1, 2, 3]
        // - when replica_count=3 and ack_count = 1, then result confirm offset = 3.
        // - when replica_count=3 and ack_count = 2, then result confirm offset = 2.
        // - when replica_count=3 and ack_count = 3, then result confirm offset = 1.
        // Example2: replicas confirmOffset = [1, corrupted, 3]
        // - when replica_count=3 and ack_count = 1, then result confirm offset = 3.
        // - when replica_count=3 and ack_count = 2, then result confirm offset = 1.
        // - when replica_count=3 and ack_count = 3, then result is ReplicationError.
        let confirm_offset_index = self.metadata.ack_count() - 1;
        self.replicators
            .iter()
            .filter(|r| !r.corrupted())
            .map(|r| r.confirm_offset())
            .sorted()
            .rev() // Descending order
            .nth(confirm_offset_index as usize)
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
        let confirm_offset = self.calculate_confirm_offset()?;
        if confirm_offset == *self.confirm_offset.borrow() {
            return Ok(());
        } else {
            *(self.confirm_offset.borrow_mut()) = confirm_offset;
        }
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
