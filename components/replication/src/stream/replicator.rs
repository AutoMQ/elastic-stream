use std::{
    cell::RefCell,
    rc::{Rc, Weak},
    time::Duration,
};

use super::replication_range::ReplicationRange;
use crate::ReplicationError;
use bytes::Bytes;
use log::{error, info, warn};
use model::fetch::FetchRequestEntry;
use model::DataNode;
use protocol::rpc::header::ErrorCode;
use protocol::rpc::header::SealKind;
use tokio::time::sleep;

/// Replicator is responsible for replicating data to a data-node of range replica.
///
/// It is created by ReplicationRange and is dropped when the range is sealed.
#[derive(Debug)]
pub(crate) struct Replicator {
    log_ident: String,
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
        let metadata = range.metadata().clone();
        let confirm_offset = metadata.start();
        Self {
            log_ident: format!(
                "Replica[{}#{}-{}#{}]",
                metadata.stream_id(),
                metadata.index(),
                data_node.node_id,
                data_node.advertise_address
            ),
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

    pub(crate) fn append(&self, flat_record_batch_bytes: Vec<Bytes>, last_offset: u64) {
        let client = if let Some(range) = self.range.upgrade() {
            if let Some(client) = range.client() {
                client
            } else {
                warn!(target: &self.log_ident, "Client was dropped, aborting replication");
                return;
            }
        } else {
            warn!(target: &self.log_ident, "ReplicationRange was dropped, aborting replication");
            return;
        };
        let offset = Rc::clone(&self.confirm_offset);
        let target = self.data_node.advertise_address.clone();
        let range = self.range.clone();
        let corrupted = self.corrupted.clone();

        // Spawn a task to replicate data to the target data-node.
        let log_ident = self.log_ident.clone();
        tokio_uring::spawn(async move {
            let mut attempts = 1;
            loop {
                if let Some(range) = range.upgrade() {
                    if range.is_sealed() {
                        info!(target: &log_ident, "Range is sealed, aborting replication");
                        break;
                    }
                    if *corrupted.borrow() {
                        range.try_ack();
                        break;
                    }

                    if attempts > 3 {
                        warn!(target: &log_ident, "Failed to append entries after 3 attempts, aborting replication");
                        // TODO: Mark replication range as failing and incur seal immediately.
                        corrupted.replace(true);
                        break;
                    }
                } else {
                    warn!(target: &log_ident, "ReplicationRange was dropped, aborting replication");
                    return;
                }

                let result = client
                    .append(&target, flat_record_batch_bytes.clone())
                    .await;
                match result {
                    Ok(append_result_entries) => {
                        if append_result_entries.len() != 1 {
                            error!(target: &log_ident, "Failed to append entries: unexpected number of entries returned. Retry...");
                            attempts += 1;
                            sleep(Duration::from_millis(10)).await;
                            continue;
                        }
                        let status = &(append_result_entries[0].status);
                        if status.code != ErrorCode::OK {
                            warn!(target: &log_ident, "Failed to append entries: status code {status:?} is not OK. Retry...");
                            attempts += 1;
                            sleep(Duration::from_millis(10)).await;
                            continue;
                        }
                        let mut confirm_offset = offset.borrow_mut();
                        if *confirm_offset < last_offset {
                            *confirm_offset = last_offset;
                        }
                        break;
                    }
                    Err(e) => {
                        // TODO: inspect error and retry only if it's a network error.
                        // If the error is a protocol error, we should abort replication.
                        // If the range is sealed on data-node, we should abort replication and fire replication seal immediately.
                        warn!(target: &log_ident, "Failed to append entries: {e}. Retry...");
                        attempts += 1;
                        // TODO: Retry immediately?
                        sleep(Duration::from_millis(10)).await;
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
                            warn!(target: &self.log_ident, "Failed to fetch entries: status {:?}", status);
                            Err(ReplicationError::Internal)
                        } else {
                            Ok(result.data.unwrap_or_default())
                        }
                    }
                    Err(_) => Err(ReplicationError::Internal),
                }
            } else {
                warn!(target: &self.log_ident, "Client was dropped, aborting replication");
                Err(ReplicationError::Internal)
            }
        } else {
            warn!(target: &self.log_ident, "ReplicationRange was dropped, aborting fetch");
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
                        warn!(target: &self.log_ident, "Seal replica success with end_offset {end_offset}");
                        *self.confirm_offset.borrow_mut() = end_offset;
                        Ok(end_offset)
                    }
                    Err(e) => {
                        error!(target: &self.log_ident, "Seal replica fail, err: {e}");
                        Err(ReplicationError::Internal)
                    }
                };
            } else {
                warn!(target: &self.log_ident, "Client was dropped, aborting seal");
                Err(ReplicationError::AlreadyClosed)
            }
        } else {
            warn!(target: &self.log_ident, "Range was dropped, aborting seal");
            Err(ReplicationError::AlreadyClosed)
        }
    }

    pub fn corrupted(&self) -> bool {
        *self.corrupted.borrow()
    }
}
