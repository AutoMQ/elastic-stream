use std::{
    cell::RefCell,
    rc::{Rc, Weak},
};

use crate::ReplicationError;
use bytes::Bytes;
use log::{info, warn};
use model::{payload::Payload, DataNode};

use super::replication_range::ReplicationRange;

/// Replicator is responsible for replicating data to a data-node of range replica.
///
/// It is created by ReplicationRange and is dropped when the range is sealed.
#[derive(Debug)]
pub(crate) struct Replicator {
    range: Weak<ReplicationRange>,
    confirm_offset: Rc<RefCell<u64>>,
    data_node: DataNode,
    corrupted: Rc<RefCell<bool>>,
}

impl Replicator {
    /// Create a new replicator.
    ///
    /// # Arguments
    /// `range` - The replication range.
    /// `confirm_offset` - The start offset of the last mutable range.
    /// `data_node` - The target data-node to replicate data to.
    pub(crate) fn new(
        range: Rc<ReplicationRange>,
        confirm_offset: u64,
        data_node: DataNode,
    ) -> Self {
        Self {
            range: Rc::downgrade(&range),
            confirm_offset: Rc::new(RefCell::new(confirm_offset)),
            data_node,
            corrupted: Rc::new(RefCell::new(false)),
        }
    }

    pub(crate) fn confirm_offset(&self) -> u64 {
        // only sealed range replica has confirm offset.
        *self.confirm_offset.borrow()
    }

    pub(crate) fn append(&self, payload: Bytes) {
        let client = if let Some(range) = self.range.upgrade() {
            if let Some(client) = range.client() {
                client
            } else {
                warn!("Client was dropped, aborting replication");
                return;
            }
        } else {
            warn!("ReplicationRange was dropped, aborting replication");
            return;
        };
        let offset = Rc::clone(&self.confirm_offset);
        let target = self.data_node.advertise_address.clone();
        let range = self.range.clone();
        let corrupted = self.corrupted.clone();

        // Spawn a task to replicate data to the target data-node.
        tokio_uring::spawn(async move {
            let mut attempts = 1;
            loop {
                if let Some(range) = range.upgrade() {
                    if range.is_sealed() {
                        info!("Range is sealed, aborting replication");
                        break;
                    }

                    if attempts > 3 {
                        warn!("Failed to append entries after 3 attempts, aborting replication");
                        // TODO: Mark replication range as failing and incur seal immediately.
                        corrupted.replace(true);
                        break;
                    }
                } else {
                    warn!("ReplicationRange was dropped, aborting replication");
                    return;
                }

                let result = client.append(&target, payload.clone()).await;
                if let Err(e) = result {
                    // TODO: inspect error and retry only if it's a network error.
                    // If the error is a protocol error, we should abort replication.
                    // If the range is sealed on data-node, we should abort replication and fire replication seal immediately.
                    warn!("Failed to append entries: {}. Retry...", e);
                    attempts += 1;
                    // TODO: Retry immediately?
                    continue;
                }

                *offset.borrow_mut() = Payload::max_offset(&payload);
                break;
            }

            if let Some(range) = range.upgrade() {
                range.try_ack();
            }
        });
    }

    /// Seal the range replica.
    /// - When range is open for write, then end_offset is Some(end_offset).
    /// - When range is created by old stream, then end_offset is None.
    pub(crate) async fn seal(&self, end_offset: Option<u64>) -> Result<u64, ReplicationError> {
        todo!()
    }

    pub fn corrupted(&self) -> bool {
        *self.corrupted.borrow()
    }
}
