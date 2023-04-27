use crate::ReplicationError;

use super::{replication_range::ReplicationRange, window::Window};
use client::Client;
use log::error;
use model::range::Range;
use std::rc::Weak;

pub(crate) struct ReplicationStream {
    id: i64,
    window: Option<Window>,
    ranges: Vec<ReplicationRange>,
    client: Weak<Client>,
}

impl ReplicationStream {
    pub(crate) fn new(id: i64, client: Weak<Client>) -> Self {
        Self {
            id,
            window: None,
            ranges: vec![],
            client,
        }
    }

    fn is_open(&self) -> bool {
        self.ranges
            .iter()
            .last()
            .map(|range| !range.metadata.is_sealed())
            .unwrap_or(false)
    }

    pub(crate) async fn open(&mut self) -> Result<(), ReplicationError> {
        let client = self.client.upgrade().ok_or(ReplicationError::Internal)?;
        self.ranges = client
            .list_range(Some(self.id))
            .await
            .map_err(|e| {
                error!("Failed to list ranges from placement-manager: {e}");
                ReplicationError::Internal
            })?
            .into_iter()
            .map(|stream_range| ReplicationRange {
                metadata: stream_range,
            })
            .collect();
        if self.is_open() {
            // TODO: seal data nodes that are backing up the last mutable range.

            let end = 0u64;
            // TODO: client.seal_and_create
        } else {
            // TODO: client.create_range
        }

        Ok(())
    }
}
