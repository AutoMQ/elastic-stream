use crate::stream::replication_range::RangeAppendContext;
use crate::ReplicationError;

use super::cache::RecordBatchCache;
use super::object_reader::ObjectReader;
use super::replication_range::ReplicationRange;
use bytes::Bytes;
use client::Client;
use itertools::Itertools;
use local_sync::{mpsc, oneshot};
use log::{error, info, trace, warn};
use model::RecordBatch;
use std::cell::OnceCell;
use std::cell::RefCell;
use std::cmp::{max, min};
use std::collections::BTreeMap;
use std::ops::Bound::Included;
use std::rc::{Rc, Weak};
use std::time::Instant;
use tokio::sync::broadcast;
use tokio::time::{sleep, Duration};

pub(crate) struct ReplicationStream {
    log_ident: String,
    weak_self: RefCell<Weak<Self>>,
    id: i64,
    epoch: u64,
    ranges: RefCell<BTreeMap<u64, Rc<ReplicationRange>>>,
    client: Weak<Client>,
    next_offset: RefCell<u64>,
    last_range: RefCell<Option<Rc<ReplicationRange>>>,
    /// #append send StreamAppendRequest to tx.
    append_requests_tx: mpsc::bounded::Tx<StreamAppendRequest>,
    /// send by range ack / delay retry to trigger append task loop next round.
    append_tasks_tx: mpsc::unbounded::Tx<()>,
    // send when stream close.
    shutdown_signal_tx: broadcast::Sender<()>,
    // stream closed mark.
    closed: Rc<RefCell<bool>>,

    cache: Rc<RecordBatchCache>,
    object_reader: Rc<ObjectReader>,
}

impl ReplicationStream {
    pub(crate) fn new(
        id: i64,
        epoch: u64,
        client: Weak<Client>,
        cache: Rc<RecordBatchCache>,
        object_reader: Rc<ObjectReader>,
    ) -> Rc<Self> {
        let (append_requests_tx, append_requests_rx) = mpsc::bounded::channel(1024);
        let (append_tasks_tx, append_tasks_rx) = mpsc::unbounded::channel();
        let (shutdown_signal_tx, shutdown_signal_rx) = broadcast::channel(1);
        let this = Rc::new(Self {
            log_ident: format!("Stream[{id}] "),
            weak_self: RefCell::new(Weak::new()),
            id,
            epoch,
            ranges: RefCell::new(BTreeMap::new()),
            client,
            next_offset: RefCell::new(0),
            last_range: RefCell::new(Option::None),
            append_requests_tx,
            append_tasks_tx,
            shutdown_signal_tx,
            closed: Rc::new(RefCell::new(false)),
            cache,
            object_reader,
        });

        *(this.weak_self.borrow_mut()) = Rc::downgrade(&this);

        let weak_this = this.weak_self.borrow().clone();
        let closed = this.closed.clone();
        tokio_uring::spawn(async move {
            Self::append_task(
                weak_this,
                append_requests_rx,
                append_tasks_rx,
                shutdown_signal_rx,
                closed,
            )
            .await
        });

        this
    }

    pub(crate) async fn open(&self) -> Result<(), ReplicationError> {
        info!("{}Opening...", self.log_ident);
        let client = self.client.upgrade().ok_or(ReplicationError::Internal)?;
        // 1. load all ranges
        client
            .list_ranges(model::ListRangeCriteria::new(None, Some(self.id as u64)))
            .await
            .map_err(|e| {
                error!(
                    "{}Failed to list ranges from placement-driver: {e}",
                    self.log_ident
                );
                ReplicationError::Internal
            })?
            .into_iter()
            // skip old empty range when two range has the same start offset
            .sorted_by(|a, b| Ord::cmp(&a.index(), &b.index()))
            .for_each(|range| {
                self.ranges.borrow_mut().insert(
                    range.start(),
                    ReplicationRange::new(
                        range,
                        false,
                        self.weak_self.borrow().clone(),
                        self.client.clone(),
                        self.cache.clone(),
                        self.object_reader.clone(),
                    ),
                );
            });
        // 2. seal the last range
        let last_range = self
            .ranges
            .borrow_mut()
            .last_entry()
            .map(|e| e.get().clone());
        if let Some(last_range) = last_range {
            *(self.last_range.borrow_mut()) = Option::Some(last_range.clone());
            let range_index = last_range.metadata().index();
            match last_range.seal().await {
                Ok(confirm_offset) => {
                    // 3. set stream next offset to the exclusive end of the last range
                    *self.next_offset.borrow_mut() = confirm_offset;
                }
                Err(e) => {
                    error!("{}Failed to seal range[{range_index}], {e}", self.log_ident);
                    return Err(ReplicationError::Internal);
                }
            }
        }
        let range_count = self.ranges.borrow().len();
        let start_offset = self.start_offset();
        let next_offset = self.next_offset();
        info!("{}Opened with range_count={range_count} start_offset={start_offset} next_offset={next_offset}", self.log_ident);
        Ok(())
    }

    /// Close the stream.
    /// 1. send stop signal to append task.
    /// 2. await append task to stop.
    /// 3. close all ranges.
    pub async fn close(&self) {
        info!("{}Closing...", self.log_ident);
        *self.closed.borrow_mut() = true;
        let _ = self.shutdown_signal_tx.send(());
        // TODO: await append task to stop.
        let last_range = self.last_range.borrow().as_ref().cloned();
        if let Some(range) = last_range {
            let _ = range.seal().await;
        }
        info!("{}Closed...", self.log_ident);
    }

    pub fn start_offset(&self) -> u64 {
        self.ranges
            .borrow()
            .first_key_value()
            .map(|(k, _)| *k)
            .unwrap_or(0)
    }

    /// next record offset to be appended.
    pub fn next_offset(&self) -> u64 {
        *self.next_offset.borrow()
    }

    pub(crate) async fn append(&self, record_batch: RecordBatch) -> Result<u64, ReplicationError> {
        let start_timestamp = Instant::now();
        if *self.closed.borrow() {
            warn!("{}Keep append to a closed stream.", self.log_ident);
            return Err(ReplicationError::AlreadyClosed);
        }
        let base_offset = *self.next_offset.borrow();
        let count = record_batch.last_offset_delta();
        *self.next_offset.borrow_mut() = base_offset + count as u64;

        let (append_tx, append_rx) = oneshot::channel::<Result<(), ReplicationError>>();
        // trigger background append task to handle the append request.
        if self
            .append_requests_tx
            .send(StreamAppendRequest::new(
                base_offset,
                record_batch,
                append_tx,
            ))
            .await
            .is_err()
        {
            warn!("{}Send to append request channel fail.", self.log_ident);
            return Err(ReplicationError::AlreadyClosed);
        }
        // await append result.
        match append_rx.await {
            Ok(result) => {
                trace!(
                    "{}Append new record with base_offset={base_offset} count={count} cost {}us",
                    self.log_ident,
                    start_timestamp.elapsed().as_micros()
                );
                result.map(|_| base_offset)
            }
            Err(_) => Err(ReplicationError::AlreadyClosed),
        }
    }

    pub(crate) async fn fetch(
        &self,
        start_offset: u64,
        end_offset: u64,
        batch_max_bytes: u32,
    ) -> Result<Vec<Bytes>, ReplicationError> {
        trace!(
            "{}Fetch [{start_offset}, {end_offset}) with batch_max_bytes={batch_max_bytes}",
            self.log_ident
        );
        if start_offset == end_offset {
            return Ok(Vec::new());
        }
        let last_range = match self.last_range.borrow().as_ref() {
            Some(range) => range.clone(),
            None => return Err(ReplicationError::FetchOutOfRange),
        };
        // Fetch range is out of stream range.
        if last_range.confirm_offset() < end_offset {
            return Err(ReplicationError::FetchOutOfRange);
        }
        // Fast path: if fetch range is in the last range, then fetch from it.
        if last_range.start_offset() <= start_offset {
            return last_range
                .fetch(start_offset, end_offset, batch_max_bytes)
                .await;
        }

        // Slow path
        // 1. find all ranges that intersect with the fetch range.
        // 2. fetch from each range.
        let mut fetch_ranges: Vec<Rc<ReplicationRange>> = Vec::new();
        {
            let ranges = self.ranges.borrow();
            let mut cursor = ranges.upper_bound(Included(&start_offset));
            while let Some(range) = cursor.value() {
                if range.start_offset() >= end_offset {
                    break;
                }
                fetch_ranges.push(range.clone());
                cursor.move_next();
            }
        }
        if fetch_ranges.is_empty() || fetch_ranges[0].start_offset() > start_offset {
            return Err(ReplicationError::FetchOutOfRange);
        }
        let mut records: Vec<Bytes> = Vec::new();
        let mut max_bytes_hint = batch_max_bytes;
        for range in fetch_ranges {
            if max_bytes_hint == 0 {
                break;
            }
            let mut range_records = range
                .fetch(
                    max(start_offset, range.start_offset()),
                    min(end_offset, range.confirm_offset()),
                    max_bytes_hint,
                )
                .await?;
            for bytes in range_records.iter() {
                max_bytes_hint -= min(max_bytes_hint, bytes.len() as u32);
            }
            // TODO: check data integrity.
            records.append(&mut range_records);
        }
        Ok(records)
    }

    async fn new_range(&self, range_index: i32, start_offset: u64) -> Result<(), ReplicationError> {
        if let Some(client) = self.client.upgrade() {
            let range_metadata =
                ReplicationRange::create(client, self.id, self.epoch, range_index, start_offset)
                    .await?;
            let range = ReplicationRange::new(
                range_metadata,
                true,
                self.weak_self.borrow().clone(),
                self.client.clone(),
                self.cache.clone(),
                self.object_reader.clone(),
            );
            info!("{}Create new range: {:?}", range.metadata(), self.log_ident);
            self.ranges.borrow_mut().insert(start_offset, range.clone());
            *self.last_range.borrow_mut() = Some(range);
            Ok(())
        } else {
            Err(ReplicationError::Internal)
        }
    }

    pub(crate) fn try_ack(&self) {
        self.trigger_append_task();
    }

    pub(crate) fn trigger_append_task(&self) {
        let _ = self.append_tasks_tx.send(());
    }

    async fn append_task(
        stream: Weak<ReplicationStream>,
        mut append_requests_rx: mpsc::bounded::Rx<StreamAppendRequest>,
        mut append_tasks_rx: mpsc::unbounded::Rx<()>,
        mut shutdown_signal_rx: broadcast::Receiver<()>,
        closed: Rc<RefCell<bool>>,
    ) {
        let stream_option = stream.upgrade();
        if stream_option.is_none() {
            warn!("Stream is already released, then directly exit append task");
            return;
        }
        let stream = stream_option.expect("stream id cannot be none");
        let log_ident = &stream.log_ident;
        let mut inflight: BTreeMap<u64, Rc<StreamAppendRequest>> = BTreeMap::new();
        let mut next_append_start_offset: u64 = 0;

        loop {
            tokio::select! {
                Some(append_request) = append_requests_rx.recv() => {
                    inflight.insert(append_request.base_offset(), Rc::new(append_request));
                }
                Some(_) = append_tasks_rx.recv() => {
                    // usually send by range ack / delay retry
                }
                _ = shutdown_signal_rx.recv() => {
                    let inflight_count = inflight.len();
                    info!("{}Receive shutdown signal, then quick fail {inflight_count} inflight requests with AlreadyClosed err.", log_ident);
                    for (_, append_request) in inflight.iter() {
                        append_request.fail(ReplicationError::AlreadyClosed);
                    }
                    break;
                }
            }
            if *closed.borrow() {
                let inflight_count = inflight.len();
                info!("{}Detect closed mark, then quick fail {inflight_count} inflight requests with AlreadyClosed err.", log_ident);
                for (_, append_request) in inflight.iter() {
                    append_request.fail(ReplicationError::AlreadyClosed);
                }
                break;
            }

            // 1. get writable range.
            let last_range = stream.last_range.borrow().as_ref().cloned();
            let last_writable_range = match last_range {
                Some(last_range) => {
                    let range_index = last_range.metadata().index();
                    if !last_range.is_writable() {
                        info!("{}The last range[{range_index}] is not writable, try create a new range.", log_ident);
                        // if last range is not writable, try to seal it and create a new range and retry append in next round.
                        match last_range.seal().await {
                            Ok(end_offset) => {
                                info!("{}Seal not writable last range[{range_index}] with end_offset={end_offset}.", log_ident);
                                // rewind back next append start offset and try append to new writable range in next round.
                                next_append_start_offset = end_offset;
                                if let Err(e) = stream.new_range(range_index + 1, end_offset).await
                                {
                                    error!(
                                        "{}Try create a new range fail, retry later, err[{e}]",
                                        log_ident
                                    );
                                    // delay retry to avoid busy loop
                                    sleep(Duration::from_millis(1000)).await;
                                }
                            }
                            Err(_) => {
                                // delay retry to avoid busy loop
                                sleep(Duration::from_millis(1000)).await;
                            }
                        }
                        stream.trigger_append_task();
                        continue;
                    }
                    last_range
                }
                None => {
                    info!(
                        "{}The stream don't have any range, then try new a range.",
                        log_ident
                    );
                    if let Err(e) = stream.new_range(0, 0).await {
                        error!(
                            "{}New a range from absent fail, retry later, err[{e}]",
                            log_ident
                        );
                        // delay retry to avoid busy loop
                        sleep(Duration::from_millis(1000)).await;
                    }
                    stream.trigger_append_task();
                    continue;
                }
            };
            if !inflight.is_empty() {
                let range_index = last_writable_range.metadata().index();
                // 2. ack success append request, and remove them from inflight.
                let confirm_offset = last_writable_range.confirm_offset();
                let mut ack_count = 0;
                for (base_offset, append_request) in inflight.iter() {
                    if *base_offset < confirm_offset {
                        // if base offset is less than confirm offset, it means append request is already success.
                        append_request.success();
                        ack_count += 1;
                        trace!("{}Ack append request with base_offset={base_offset}, confirm_offset={confirm_offset}", log_ident);
                    }
                }
                for _ in 0..ack_count {
                    inflight.pop_first();
                }

                // 3. try append request which base_offset >= next_append_start_offset.
                let mut cursor = inflight.lower_bound(Included(&next_append_start_offset));
                while let Some((base_offset, append_request)) = cursor.key_value() {
                    last_writable_range.append(
                        &append_request.record_batch,
                        RangeAppendContext::new(*base_offset),
                    );
                    trace!(
                        "{}Try append record[{base_offset}] to range[{range_index}]",
                        log_ident
                    );
                    next_append_start_offset = base_offset + append_request.count() as u64;
                    cursor.move_next();
                }
            }
        }
    }

    pub async fn trim(&self, _new_start_offset: u64) -> Result<(), ReplicationError> {
        // TODO
        Ok(())
    }
}

struct StreamAppendRequest {
    base_offset: u64,
    record_batch: RecordBatch,
    append_tx: RefCell<OnceCell<oneshot::Sender<Result<(), ReplicationError>>>>,
}

impl StreamAppendRequest {
    pub fn new(
        base_offset: u64,
        record_batch: RecordBatch,
        append_tx: oneshot::Sender<Result<(), ReplicationError>>,
    ) -> Self {
        let append_tx_cell = OnceCell::new();
        let _ = append_tx_cell.set(append_tx);
        Self {
            base_offset,
            record_batch,
            append_tx: RefCell::new(append_tx_cell),
        }
    }

    pub fn base_offset(&self) -> u64 {
        self.base_offset
    }

    pub fn count(&self) -> u32 {
        self.record_batch.last_offset_delta() as u32
    }

    pub fn success(&self) {
        if let Some(append_tx) = self.append_tx.borrow_mut().take() {
            let _ = append_tx.send(Ok(()));
        }
    }

    pub fn fail(&self, err: ReplicationError) {
        if let Some(append_tx) = self.append_tx.borrow_mut().take() {
            let _ = append_tx.send(Err(err));
        }
    }
}
