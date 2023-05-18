use std::{
    cell::RefCell,
    rc::{Rc, Weak},
};

use bytes::Bytes;
use client::Client;
use itertools::Itertools;
use model::range::{self, RangeMetadata};
use tokio::sync::broadcast;

use crate::ReplicationError;

use super::{replication_stream::ReplicationStream, replicator::Replicator};

const CORRUPTED_FLAG: u32 = 1 << 0;
const SEALING_FLAG: u32 = 1 << 1;
const SEALED_FLAG: u32 = 1 << 2;

#[derive(Debug)]
pub(crate) struct ReplicationRange {
    metadata: RangeMetadata,

    stream: Weak<ReplicationStream>,

    client: Weak<Client>,

    replicators: Rc<Vec<Rc<Replicator>>>,

    /// Exclusive confirm offset.
    confirm_offset: RefCell<u64>,
    /// If range is created by current stream, then open_for_write is true.
    open_for_write: bool,
    /// Range status.
    status: RefCell<u32>,
    seal_task_tx: Rc<broadcast::Sender<Result<u64, ReplicationError>>>,
}

impl ReplicationRange {
    pub(crate) fn new(
        metadata: RangeMetadata,
        open_for_write: bool,
        stream: Weak<ReplicationStream>,
        client: Weak<Client>,
    ) -> Rc<Self> {
        let confirm_offset = metadata.end().unwrap_or_else(|| metadata.start());
        let status = if metadata.end().is_some() {
            SEALED_FLAG
        } else {
            0
        };

        let (seal_task_tx, _) = broadcast::channel::<Result<u64, ReplicationError>>(1);

        let this = Self {
            metadata,
            open_for_write,
            stream,
            client,
            replicators: Rc::new(vec![]),
            confirm_offset: RefCell::new(confirm_offset),
            status: RefCell::new(status),
            seal_task_tx: Rc::new(seal_task_tx),
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
    pub(crate) fn try_ack(&self) {
        if !self.is_writable() {
            return;
        }
        match self.calculate_confirm_offset() {
            Ok(confirm_offset) => {
                if confirm_offset == *self.confirm_offset.borrow() {
                    return;
                } else {
                    *(self.confirm_offset.borrow_mut()) = confirm_offset;
                }
                if let Some(stream) = self.stream.upgrade() {
                    stream.try_ack();
                }
            }
            Err(_) => {
                self.mark_corrupted();
                if let Some(stream) = self.stream.upgrade() {
                    stream.try_ack();
                }
            }
        }
    }

    pub(crate) async fn seal(&self) -> Result<u64, ReplicationError> {
        if self.is_sealed() {
            // if range is already sealed, return confirm offset.
            return Ok(*(self.confirm_offset.borrow()));
        }
        if self.is_sealing() {
            // if range is sealing, wait for seal task to complete.
            match self.seal_task_tx.subscribe().recv().await {
                Ok(result) => {
                    return result;
                }
                Err(_) => {
                    return Err(ReplicationError::Internal);
                }
            }
        } else {
            self.mark_sealing();
            if (self.open_for_write) {
                // the range is open for write, it's ok to dirrectly use memory confirm offset as range end offset.
                let end_offset = self.confirm_offset();
                // 1. call placement manager to seal range
                match self.placement_manager_seal(end_offset).await {
                    Ok(_) => {
                        self.mark_sealed();
                        let _ = self.seal_task_tx.send(Ok(end_offset));
                        // 2. spawn task to async seal range replicas
                        let replicas = self.replicators.clone();
                        let replica_count = self.metadata.replica_count();
                        let ack_count = self.metadata.ack_count();
                        tokio_uring::spawn(async move {
                            let _ = Self::replicas_seal(
                                replicas,
                                replica_count,
                                ack_count,
                                Some(end_offset),
                            );
                        });

                        return Ok(end_offset);
                    }
                    Err(_) => {
                        self.unmark_sealing();
                        return Err(ReplicationError::Internal);
                    }
                }
            } else {
                // the range is created by old stream, it need to calculate end offset from replicas.
                let replicas = self.replicators.clone();
                // 1. seal range replicas and calculate end offset.
                match Self::replicas_seal(
                    replicas,
                    self.metadata.replica_count(),
                    self.metadata.ack_count(),
                    None,
                )
                .await
                {
                    Ok(end_offset) => {
                        // 2. call placement manager to seal range.
                        match self.placement_manager_seal(end_offset).await {
                            Ok(_) => {
                                self.mark_sealed();
                                *self.confirm_offset.borrow_mut() = end_offset;
                                let _ = self.seal_task_tx.send(Ok(end_offset));
                                return Ok(end_offset);
                            }
                            Err(_) => {
                                self.unmark_sealing();
                                return Err(ReplicationError::Internal);
                            }
                        }
                    }
                    Err(_) => {
                        self.unmark_sealing();
                        return Err(ReplicationError::Internal);
                    }
                }
            }
        }
    }

    async fn placement_manager_seal(&self, end_offset: u64) -> Result<(), ReplicationError> {
        todo!()
    }

    async fn replicas_seal(
        replicas: Rc<Vec<Rc<Replicator>>>,
        replica_count: u32,
        ack_count: u32,
        end_offset: Option<u64>,
    ) -> Result<u64, ReplicationError> {
        let end_offsets = Rc::new(RefCell::new(Vec::<u64>::new()));
        let mut seal_tasks = vec![];
        for replica in replicas.iter() {
            let end_offsets = end_offsets.clone();
            let replica = replica.clone();
            let end_offset = end_offset.clone();
            seal_tasks.push(tokio_uring::spawn(async move {
                if let Ok(replica_end_offset) = replica.seal(end_offset).await {
                    end_offsets.borrow_mut().push(replica_end_offset);
                }
            }));
        }
        for task in seal_tasks {
            let _ = task.await;
        }
        // Example1: replicas confirmOffset = [1, 2, 3]
        // - when replica_count=3 and ack_count = 1, must seal 3 replica success, the result end offset = 3.
        // - when replica_count=3 and ack_count = 2, must seal 2 replica success, the result end offset = 2.
        // - when replica_count=3 and ack_count = 3, must seal 1 replica success, the result end offset = 1.
        // Example2: replicas confirmOffset = [1, corrupted, 3]
        // - when replica_count=3 and ack_count = 1, must seal 3 replica success, the result is seal fail Err.
        // - when replica_count=3 and ack_count = 2, must seal 2 replica success, the result end offset = 3.
        // - when replica_count=3 and ack_count = 3, must seal 1 replica success, the result end offset = 1.
        // assume the corrupted replica with the largest end offset.
        let end_offset = end_offsets
            .borrow_mut()
            .iter()
            .sorted()
            .nth((replica_count - ack_count) as usize)
            .map(|offset| *offset)
            .ok_or(ReplicationError::SealReplicaNotEnough);
        end_offset
    }

    pub(crate) fn is_sealed(&self) -> bool {
        *self.status.borrow() & SEALED_FLAG != 0
    }

    pub(crate) fn mark_sealed(&self) {
        *self.status.borrow_mut() |= SEALED_FLAG;
        self.unmark_sealing();
    }

    pub(crate) fn is_sealing(&self) -> bool {
        *self.status.borrow() & SEALING_FLAG != 0
    }

    pub(crate) fn mark_sealing(&self) {
        *self.status.borrow_mut() |= SEALING_FLAG;
    }

    pub(crate) fn unmark_sealing(&self) {
        *self.status.borrow_mut() &= !SEALING_FLAG;
    }

    pub(crate) fn mark_corrupted(&self) {
        *self.status.borrow_mut() |= CORRUPTED_FLAG;
    }

    pub(crate) fn is_writable(&self) -> bool {
        *self.status.borrow() == 0 && self.open_for_write
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
