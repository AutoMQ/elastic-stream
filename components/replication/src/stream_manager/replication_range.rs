use std::{
    cell::RefCell,
    rc::{Rc, Weak},
};

use bytes::Bytes;
use client::Client;
use itertools::Itertools;
use model::range::{self, RangeMetadata};

use crate::ReplicationError;

use super::{replication_stream::ReplicationStream, replicator::Replicator};

#[derive(Debug)]
pub(crate) struct ReplicationRange {
    metadata: RangeMetadata,

    stream: Weak<ReplicationStream>,

    client: Weak<Client>,

    replicators: Vec<Replicator>,

    /// exclusive confirm offset.
    confirm_offset: RefCell<u64>,
}

impl ReplicationRange {
    pub(crate) fn new(
        metadata: RangeMetadata,
        stream: Weak<ReplicationStream>,
        client: Weak<Client>,
    ) -> Rc<Self> {
        let confirm_offset = metadata.end().unwrap_or_else(|| metadata.start());
        let this = Self {
            metadata,
            stream,
            client,
            replicators: vec![],
            confirm_offset: RefCell::new(confirm_offset),
        };

        Rc::new(this)
    }

    pub(crate) async fn create(
        stream_id: i64,
        epoch: u64,
        index: i32,
        start_offset: u64,
    ) -> Result<RangeMetadata, ReplicationError> {
        // 1. request placement manager to create range.
        // 2. request placement manager to create range replica.
        // 3. return metadata
        todo!()
    }

    pub(crate) fn metadata(&self) -> &RangeMetadata {
        &self.metadata
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

    pub(crate) fn append(&self, payload: Rc<Bytes>, context: RangeAppendContext) {
        // FIXME: encode request payload from raw payload and context.
    }

    /// update range confirm offset and invoke stream#try_ack.
    pub(crate) fn try_ack(&self) -> Result<(), ReplicationError> {
        let confirm_offset = self.calculate_confirm_offset()?;
        if confirm_offset == *self.confirm_offset.borrow() {
            return Ok(());
        } else {
            *(self.confirm_offset.borrow_mut()) = confirm_offset;
        }
        if let Some(stream) = self.stream.upgrade() {
            stream.try_ack();
        }
        Ok(())
    }

    pub(crate) async fn seal(&self) -> Result<u64, ReplicationError> {
        Ok(*(self.confirm_offset.borrow()))
    }

    pub(crate) fn is_writable(&self) -> bool {
        true
    }

    pub(crate) fn confirm_offset(&self) -> u64 {
        *(self.confirm_offset.borrow())
    }
}

pub struct RangeAppendContext {
    base_offset: u64,
    count: u32,
}

impl RangeAppendContext {
    pub fn new(base_offset: u64, count: u32) -> Self {
        Self { base_offset, count }
    }
}
