use std::{
    cell::RefCell,
    rc::{Rc, Weak},
};

use bytes::Bytes;
use log::warn;
use model::{payload::Payload, DataNode};

use super::replication_range::ReplicationRange;

#[derive(Debug)]
pub(crate) struct Replicator {
    range: Weak<ReplicationRange>,
    confirm_offset: Rc<RefCell<u64>>,
    data_node: DataNode,
}

impl Replicator {
    pub(crate) fn new(
        range: Rc<ReplicationRange>,
        confirm_offset: u64,
        data_node: DataNode,
    ) -> Self {
        Self {
            range: Rc::downgrade(&range),
            confirm_offset: Rc::new(RefCell::new(confirm_offset)),
            data_node,
        }
    }

    pub(crate) fn confirm_offset(&self) -> u64 {
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
        tokio_uring::spawn(async move {
            let result = client.append(&target, payload.clone()).await;
            if let Err(e) = result {
                warn!("Failed to append entries: {}", e);
                return;
            }
            *offset.borrow_mut() = Payload::max_offset(&payload);

            if let Some(range) = range.upgrade() {
                if let Err(e) = range.try_ack() {
                    warn!("Failed to ack: {}", e);
                }
            }
        });
    }
}
