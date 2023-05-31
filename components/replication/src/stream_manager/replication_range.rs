use std::{
    cell::RefCell,
    rc::{Rc, Weak},
};

use crate::ReplicationError;
use bytes::Bytes;
use client::Client;
use itertools::Itertools;
use log::{debug, error, info, trace, warn};
use model::range::RangeMetadata;
use model::record::{flat_record::FlatRecordBatch, RecordBatch};
use std::cmp::min;
use tokio::sync::broadcast;

use super::{
    cache::RecordBatchCache, replication_stream::ReplicationStream, replicator::Replicator,
};
use protocol::rpc::header::SealKind;

const CORRUPTED_FLAG: u32 = 1 << 0;
const SEALING_FLAG: u32 = 1 << 1;
const SEALED_FLAG: u32 = 1 << 2;

#[derive(Debug)]
pub(crate) struct ReplicationRange {
    log_ident: String,

    weak_self: Weak<Self>,

    metadata: RangeMetadata,

    stream: Weak<ReplicationStream>,

    client: Weak<Client>,

    cache: Rc<RecordBatchCache>,

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
        cache: Rc<RecordBatchCache>,
    ) -> Rc<Self> {
        let confirm_offset = metadata.end().unwrap_or_else(|| metadata.start());
        let status = if metadata.end().is_some() {
            SEALED_FLAG
        } else {
            0
        };

        let (seal_task_tx, _) = broadcast::channel::<Result<u64, ReplicationError>>(1);

        let log_ident = format!("Range[{}#{}]", metadata.stream_id(), metadata.index());
        let mut this = Rc::new(Self {
            log_ident,
            weak_self: Weak::new(),
            metadata,
            open_for_write,
            stream,
            client,
            cache,
            replicators: Rc::new(vec![]),
            confirm_offset: RefCell::new(confirm_offset),
            status: RefCell::new(status),
            seal_task_tx: Rc::new(seal_task_tx),
        });

        let mut replicators = Vec::with_capacity(this.metadata.replica().len());
        for replica_node in this.metadata.replica().iter() {
            replicators.push(Rc::new(Replicator::new(this.clone(), replica_node.clone())));
        }
        // #Safety: the weak_self/replicators only changed(init) in range new.
        unsafe {
            Rc::get_mut_unchecked(&mut this).weak_self = Rc::downgrade(&this);
            Rc::get_mut_unchecked(&mut this).replicators = Rc::new(replicators);
        }
        info!(
            "Load range with metadata: {:?} open_for_write={open_for_write}",
            this.metadata
        );
        this
    }

    pub(crate) async fn create(
        client: Rc<Client>,
        stream_id: i64,
        epoch: u64,
        index: i32,
        start_offset: u64,
    ) -> Result<RangeMetadata, ReplicationError> {
        // 1. request placement manager to create range and get the range metadata.
        let mut metadata = RangeMetadata::new(stream_id, index, epoch, start_offset, None);
        metadata = client.create_range(metadata).await.map_err(|e| {
            error!("Create range[{stream_id}#{index}] to pm failed, err: {e}");
            ReplicationError::Internal
        })?;
        // 2. request data node to create range replica.
        let mut create_replica_tasks = vec![];
        for node in metadata.replica().iter() {
            let address = node.advertise_address.clone();
            let metadata = metadata.clone();
            let client = client.clone();
            create_replica_tasks.push(tokio_uring::spawn(async move {
                client
                    .create_range_replica(&address, metadata)
                    .await
                    .map_err(|e| {
                        error!("Create range[{stream_id}#{index}] to data node[{address}] failed, err: {e}");
                        ReplicationError::Internal
                    })
            }));
        }
        for task in create_replica_tasks {
            // if success replica is less than ack count, the stream append task will create new range triggered by append error.
            let _ = task.await;
        }
        // 3. return metadata
        Ok(metadata)
    }

    pub(crate) fn metadata(&self) -> &RangeMetadata {
        &self.metadata
    }

    pub(crate) fn client(&self) -> Option<Rc<Client>> {
        self.client.upgrade()
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

    pub(crate) fn append(&self, payload: Bytes, context: RangeAppendContext) {
        let record_batch = RecordBatch::new_builder()
            .with_stream_id(self.metadata.stream_id())
            .with_range_index(self.metadata.index())
            .with_base_offset(context.base_offset as i64)
            .with_last_offset_delta(context.count as i32)
            .with_payload(payload)
            .build()
            .expect("valid record batch");
        let flat_record_batch: FlatRecordBatch = Into::into(record_batch);
        let (flat_record_batch_bytes, _) = flat_record_batch.encode();
        self.cache.insert(
            self.metadata.stream_id() as u64,
            self.metadata().index() as u32,
            context.base_offset,
            context.count,
            flat_record_batch_bytes.clone(),
        );
        for replica in (*self.replicators).iter() {
            replica.append(
                flat_record_batch_bytes.clone(),
                context.base_offset + context.count as u64,
            );
        }
    }

    pub(crate) async fn fetch(
        &self,
        start_offset: u64,
        end_offset: u64,
        batch_max_bytes: u32,
    ) -> Result<Vec<Bytes>, ReplicationError> {
        // try fetch from cache first, if cache cannot fulfill the request, then fetch from remote.
        let stream_id = self.metadata.stream_id() as u64;
        let range_index = self.metadata.index() as u32;
        let mut next_start_offset = start_offset;
        let end_offset = end_offset;
        let mut next_batch_max_bytes = batch_max_bytes;
        let mut fetch_data = vec![];
        loop {
            // cache hit the fetch range, return data from cache.
            if next_start_offset >= end_offset || next_batch_max_bytes == 0 {
                trace!(target: &self.log_ident,"Fetch [{}, {}) with batch_max_bytes[{}] fulfilled by cache", start_offset, end_offset, batch_max_bytes);
                return Ok(fetch_data);
            }
            if let Some(cache_data) = self.cache.get(stream_id, range_index, next_start_offset) {
                let mut data = cache_data.data.clone();
                next_batch_max_bytes -= min(next_batch_max_bytes, data.len() as u32);
                next_start_offset += cache_data.count as u64;
                fetch_data.append(&mut data);
            } else {
                break;
            }
        }

        // TODO: select replica strategy.
        // - balance the read traffic.
        // - isolate unreadable (data less than expected, unaccessible) replica.
        for replicator in self.replicators.iter() {
            if replicator.corrupted() {
                continue;
            }
            let result = replicator
                .fetch(next_start_offset, end_offset, next_batch_max_bytes)
                .await;
            match result {
                Ok(mut payloads) => {
                    fetch_data.append(&mut payloads);
                    return Ok(fetch_data);
                }
                Err(e) => {
                    warn!(target: &self.log_ident, "Fetch [{next_start_offset}, {end_offset}) with batch_max_bytes={next_batch_max_bytes} fail, err: {e}");
                    continue;
                }
            }
        }
        Err(ReplicationError::Internal)
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
            Err(err) => {
                warn!(target: &self.log_ident, "Calculate confirm offset fail, current confirm_offset=[{}], err: {err}", self.confirm_offset());
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
            self.seal_task_tx.subscribe().recv().await.map_err(|_| {
                error!(target: &self.log_ident, "Seal task channel closed");
                ReplicationError::Internal
            })?
        } else {
            self.mark_sealing();
            if self.open_for_write {
                // the range is open for write, it's ok to directly use memory confirm offset as range end offset.
                let end_offset = self.confirm_offset();
                // 1. call placement manager to seal range
                match self.placement_manager_seal(end_offset).await {
                    Ok(_) => {
                        info!(target: &self.log_ident, "The range is created by current stream, then directly seal with memory confirm_offset=[{}]", end_offset);
                        self.mark_sealed();
                        let _ = self.seal_task_tx.send(Ok(end_offset));
                        // 2. spawn task to async seal range replicas
                        let replicas = self.replicators.clone();
                        let replica_count = self.metadata.replica_count();
                        let ack_count = self.metadata.ack_count();
                        let log_ident = self.log_ident.clone();
                        let range = self.weak_self.upgrade().clone();
                        tokio_uring::spawn(async move {
                            if Self::replicas_seal(
                                &log_ident,
                                replicas,
                                replica_count,
                                ack_count,
                                Some(end_offset),
                            )
                            .await
                            .is_err()
                            {
                                debug!("Failed to seal data-node after sealing placement-manager");
                            }
                            // keep range alive until seal task complete.
                            drop(range);
                        });
                        Ok(end_offset)
                    }
                    Err(e) => {
                        error!(target: &self.log_ident, "Request pm seal fail, err: {e}");
                        self.erase_sealing();
                        Err(ReplicationError::Internal)
                    }
                }
            } else {
                // the range is created by old stream, it need to calculate end offset from replicas.
                let replicas = self.replicators.clone();
                // 1. seal range replicas and calculate end offset.
                match Self::replicas_seal(
                    &self.log_ident,
                    replicas,
                    self.metadata.replica_count(),
                    self.metadata.ack_count(),
                    None,
                )
                .await
                {
                    Ok(end_offset) => {
                        // 2. call placement manager to seal range.
                        info!(target: &self.log_ident, "The range is created by other stream, then seal replicas to calculate end_offset=[{}]", end_offset);
                        match self.placement_manager_seal(end_offset).await {
                            Ok(_) => {
                                self.mark_sealed();
                                *self.confirm_offset.borrow_mut() = end_offset;
                                let _ = self.seal_task_tx.send(Ok(end_offset));
                                Ok(end_offset)
                            }
                            Err(e) => {
                                error!(target: &self.log_ident, "Request pm seal fail, err: {e}");
                                self.erase_sealing();
                                Err(ReplicationError::Internal)
                            }
                        }
                    }
                    Err(_) => {
                        self.erase_sealing();
                        Err(ReplicationError::Internal)
                    }
                }
            }
        }
    }

    async fn placement_manager_seal(&self, end_offset: u64) -> Result<(), ReplicationError> {
        if let Some(client) = self.client.upgrade() {
            let mut metadata = self.metadata.clone();
            metadata.set_end(end_offset);
            match client
                .seal(None, SealKind::PLACEMENT_MANAGER, metadata)
                .await
            {
                Ok(_) => Ok(()),
                Err(e) => {
                    error!(target: &self.log_ident, "Request pm seal with end_offset[{end_offset}] fail, err: {e}");
                    Err(ReplicationError::Internal)
                }
            }
        } else {
            Err(ReplicationError::AlreadyClosed)
        }
    }

    async fn replicas_seal(
        log_ident: &String,
        replicas: Rc<Vec<Rc<Replicator>>>,
        replica_count: u8,
        ack_count: u8,
        end_offset: Option<u64>,
    ) -> Result<u64, ReplicationError> {
        let end_offsets = Rc::new(RefCell::new(Vec::<u64>::new()));
        let mut seal_tasks = vec![];
        let replicas = replicas.clone();
        for replica in replicas.iter() {
            let end_offsets = end_offsets.clone();
            let replica = replica.clone();
            seal_tasks.push(tokio_uring::spawn(async move {
                if let Ok(replica_end_offset) = replica.seal(end_offset).await {
                    (*end_offsets).borrow_mut().push(replica_end_offset);
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
            .borrow()
            .iter()
            .sorted()
            .nth((replica_count - ack_count) as usize)
            .copied()
            .ok_or(ReplicationError::SealReplicaNotEnough);
        info!(target: log_ident, "Replicas seal with end_offsets={end_offsets:#?} and final end_offset={end_offset:#?}");
        end_offset
    }

    pub(crate) fn is_sealed(&self) -> bool {
        *self.status.borrow() & SEALED_FLAG != 0
    }

    pub(crate) fn mark_sealed(&self) {
        *self.status.borrow_mut() |= SEALED_FLAG;
        self.erase_sealing();
    }

    pub(crate) fn is_sealing(&self) -> bool {
        *self.status.borrow() & SEALING_FLAG != 0
    }

    pub(crate) fn mark_sealing(&self) {
        *self.status.borrow_mut() |= SEALING_FLAG;
    }

    pub(crate) fn erase_sealing(&self) {
        *self.status.borrow_mut() &= !SEALING_FLAG;
    }

    pub(crate) fn mark_corrupted(&self) {
        *self.status.borrow_mut() |= CORRUPTED_FLAG;
    }

    pub(crate) fn is_writable(&self) -> bool {
        *self.status.borrow() == 0 && self.open_for_write
    }

    pub(crate) fn start_offset(&self) -> u64 {
        self.metadata.start()
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
