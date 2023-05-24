use std::{
    cell::RefCell,
    rc::{Rc, Weak},
};

use super::replication_range::ReplicationRange;
use crate::ReplicationError;
use bytes::Bytes;
use log::{info, warn};
use model::fetch::FetchRequestEntry;
use model::DataNode;
use protocol::rpc::header::ErrorCode;
use protocol::rpc::header::SealKind;

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
    /// `data_node` - The target data-node to replicate data to.
    pub(crate) fn new(range: Rc<ReplicationRange>, data_node: DataNode) -> Self {
        Self {
            range: Rc::downgrade(&range),
            confirm_offset: Rc::new(RefCell::new(0)),
            data_node,
            corrupted: Rc::new(RefCell::new(false)),
        }
    }

    pub(crate) fn confirm_offset(&self) -> u64 {
        // only sealed range replica has confirm offset.
        *self.confirm_offset.borrow()
    }

    pub(crate) fn append(&self, flat_record_batch_bytes: Vec<Bytes>, last_offset: u64) {
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

                let result = client
                    .append(&target, flat_record_batch_bytes.clone())
                    .await;
                match result {
                    Ok(append_result_entries) => {
                        if append_result_entries.len() != 1 {
                            warn!("Failed to append entries: unexpected number of entries returned. Retry...");
                            attempts += 1;
                            continue;
                        }
                        let status = &(append_result_entries[0].status);
                        if status.code != ErrorCode::OK {
                            warn!(
                                "Failed to append entries: status code {:?} is not OK. Retry...",
                                status
                            );
                            attempts += 1;
                            continue;
                        }
                        *offset.borrow_mut() = last_offset;
                        break;
                    }
                    Err(e) => {
                        // TODO: inspect error and retry only if it's a network error.
                        // If the error is a protocol error, we should abort replication.
                        // If the range is sealed on data-node, we should abort replication and fire replication seal immediately.
                        warn!("Failed to append entries: {}. Retry...", e);
                        attempts += 1;
                        // TODO: Retry immediately?
                        continue;
                    }
                }
            }

            if let Some(range) = range.upgrade() {
                range.try_ack();
            }
        });
    }

    pub(crate) async fn fetch(
        &self,
        start_offset: u64,
        end_offset: u64,
        batch_max_bytes: u32,
    ) -> Result<Vec<Bytes>, ReplicationError> {
        if let Some(range) = self.range.upgrade() {
            if let Some(client) = range.client() {
                let result = client
                    .fetch(
                        &self.data_node.advertise_address,
                        FetchRequestEntry {
                            stream_id: range.metadata().stream_id(),
                            index: range.metadata().index(),
                            start_offset,
                            end_offset,
                            batch_max_bytes,
                        },
                    )
                    .await;
                match result {
                    Ok(result) => {
                        let status = result.status;
                        if status.code != ErrorCode::OK {
                            warn!("Failed to fetch entries: status {:?}", status);
                            Err(ReplicationError::Internal)
                        } else {
                            Ok(result.data.unwrap_or_default())
                        }
                    }
                    Err(_) => Err(ReplicationError::Internal),
                }
            } else {
                warn!("Client was dropped, aborting replication");
                Err(ReplicationError::Internal)
            }
        } else {
            warn!("ReplicationRange was dropped, aborting fetch");
            Err(ReplicationError::Internal)
        }
    }

    /// Seal the range replica.
    /// - When range is open for write, then end_offset is Some(end_offset).
    /// - When range is created by old stream, then end_offset is None.
    pub(crate) async fn seal(&self, end_offset: Option<u64>) -> Result<u64, ReplicationError> {
        if let Some(range) = self.range.upgrade() {
            let mut metadata = range.metadata().clone();
            if let Some(end_offset) = end_offset {
                metadata.set_end(end_offset);
            }
            if let Some(client) = range.client() {
                return match client
                    .seal(
                        Some(&self.data_node.advertise_address),
                        SealKind::DATA_NODE,
                        metadata,
                    )
                    .await
                {
                    Ok(metadata) => {
                        let end_offset = metadata.end().ok_or(ReplicationError::Internal)?;
                        *self.confirm_offset.borrow_mut() = end_offset;
                        Ok(end_offset)
                    }
                    Err(_) => Err(ReplicationError::Internal),
                };
            } else {
                Err(ReplicationError::AlreadyClosed)
            }
        } else {
            Err(ReplicationError::AlreadyClosed)
        }
    }

    pub fn corrupted(&self) -> bool {
        *self.corrupted.borrow()
    }
}
