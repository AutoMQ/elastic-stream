use crate::stream_manager::replication_range::RangeAppendContext;
use crate::ReplicationError;

use super::{replication_range::ReplicationRange, window::Window};
use bytes::Bytes;
use client::Client;
use itertools::Itertools;
use log::error;
use std::cell::OnceCell;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::ops::Bound::Included;
use std::rc::{Rc, Weak};
use tokio::sync::broadcast;
use tokio::sync::mpsc;
use tokio::sync::oneshot;
use tokio::time::{sleep, Duration};

pub(crate) struct ReplicationStream {
    weak_self: RefCell<Weak<Self>>,
    id: i64,
    epoch: u64,
    window: Option<Window>,
    ranges: RefCell<BTreeMap<u64, Rc<ReplicationRange>>>,
    client: Weak<Client>,
    next_offset: RefCell<u64>,
    last_range: RefCell<Option<Rc<ReplicationRange>>>,
    /// #append send StreamAppendRequest to tx.
    append_requests_tx: mpsc::Sender<StreamAppendRequest>,
    /// send by range ack / delay retry to trigger append task loop next round.
    append_tasks_tx: mpsc::Sender<()>,
    // send when stream close.
    shutdown_signal_tx: broadcast::Sender<()>,
}

impl ReplicationStream {
    pub(crate) fn new(id: i64, epoch: u64, client: Weak<Client>) -> Rc<Self> {
        let (append_requests_tx, append_requests_rx) = mpsc::channel(1024);
        let (append_tasks_tx, append_tasks_rx) = mpsc::channel(1024);
        let (shutdown_signal_tx, shutdown_signal_rx) = broadcast::channel(1);
        let this = Rc::new(Self {
            weak_self: RefCell::new(Weak::new()),
            id,
            epoch,
            window: None,
            ranges: RefCell::new(BTreeMap::new()),
            client,
            next_offset: RefCell::new(0),
            last_range: RefCell::new(Option::None),
            append_requests_tx,
            append_tasks_tx,
            shutdown_signal_tx,
        });

        *((*this).weak_self.borrow_mut()) = Rc::downgrade(&this);

        let weak_this = this.weak_self.borrow().clone();
        tokio_uring::spawn(async move {
            Self::append_task(
                weak_this,
                append_requests_rx,
                append_tasks_rx,
                shutdown_signal_rx,
            )
            .await
        });

        this
    }

    pub(crate) async fn open(&self) -> Result<(), ReplicationError> {
        let client = self.client.upgrade().ok_or(ReplicationError::Internal)?;
        // 1. load all ranges
        client
            .list_range(Some(self.id))
            .await
            .map_err(|e| {
                error!("Failed to list ranges from placement-manager: {e}");
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
                    ),
                );
            });
        // 2. seal the last range
        if let Some(entry) = self.ranges.borrow_mut().last_entry() {
            *(self.last_range.borrow_mut()) = Option::Some(entry.get().clone());
            match entry.get().seal().await {
                Ok(confirm_offset) => {
                    // 3. set stream next offset to the exclusive end of the last range
                    *self.next_offset.borrow_mut() = confirm_offset;
                }
                Err(e) => {
                    error!("Failed to seal range: {e}");
                    return Err(ReplicationError::Internal);
                }
            }
        }
        Ok(())
    }

    /// Close the stream.
    /// 1. send stop signal to append task.
    /// 2. await append task to stop.
    /// 3. close all ranges.
    pub async fn close(&self) {
        let _ = self.shutdown_signal_tx.send(());
        // TODO: await append task to stop.
        if let Some(range) = self.last_range.borrow().as_ref() {
            let _ = range.seal().await;
        }
    }

    pub(crate) async fn append(
        &self,
        payload: Bytes,
        context: StreamAppendContext,
    ) -> Result<(), ReplicationError> {
        let base_offset = *self.next_offset.borrow();
        *self.next_offset.borrow_mut() = base_offset + context.count as u64;

        let (append_tx, append_rx) = oneshot::channel::<Result<(), ReplicationError>>();
        // trigger background append task to handle the append request.
        if let Err(_) = self
            .append_requests_tx
            .send(StreamAppendRequest::new(
                base_offset,
                payload,
                context,
                append_tx,
            ))
            .await
        {
            return Err(ReplicationError::AlreadyClosed);
        }
        // await append result.
        match append_rx.await {
            Ok(result) => result,
            Err(_) => Err(ReplicationError::AlreadyClosed),
        }
    }

    async fn new_range(&self, range_index: i32, start_offset: u64) -> Result<(), ReplicationError> {
        let range_metadata =
            ReplicationRange::create(self.id, self.epoch, range_index, start_offset).await?;
        let range = ReplicationRange::new(
            range_metadata,
            true,
            self.weak_self.borrow().clone(),
            self.client.clone(),
        );
        self.ranges.borrow_mut().insert(start_offset, range.clone());
        *self.last_range.borrow_mut() = Some(range.clone());
        Ok(())
    }

    pub(crate) fn try_ack(&self) {
        self.trigger_append_task();
    }

    pub(crate) fn trigger_append_task(&self) {
        let _ = self.append_tasks_tx.try_send(());
    }

    async fn append_task(
        stream: Weak<ReplicationStream>,
        mut append_requests_rx: mpsc::Receiver<StreamAppendRequest>,
        mut append_tasks_rx: mpsc::Receiver<()>,
        mut shutdown_signal_rx: broadcast::Receiver<()>,
    ) {
        let stream_option = stream.upgrade();
        if stream_option.is_none() {
            return;
        }
        let stream = stream_option.unwrap();
        let mut inflight: BTreeMap<u64, Rc<StreamAppendRequest>> = BTreeMap::new();
        let mut next_append_start_offset: u64 = 0;

        loop {
            tokio::select! {
                Some(append_request) = append_requests_rx.recv() => {
                    inflight.insert(append_request.base_offset, Rc::new(append_request));
                }
                Some(_) = append_tasks_rx.recv() => {
                    // usaully send by range ack / delay retry
                }
                _ = shutdown_signal_rx.recv() => {
                    for (_, append_request) in inflight.iter() {
                        let _ = append_request.fail(ReplicationError::AlreadyClosed);
                    }
                    break;
                }
            }

            // 1. get writable range.
            let last_range_ref = (*stream).last_range.borrow();
            let last_writable_range = match last_range_ref.as_ref() {
                Some(last_range) => {
                    if !last_range.is_writable() {
                        // if last range is not writable, try to seal it and create a new range and retry append in next round.
                        match last_range.seal().await {
                            Ok(end_offset) => {
                                // rewind back next append start offset and try append to new writable range in next round.
                                next_append_start_offset = last_range.confirm_offset();
                                if let Err(_) = stream
                                    .new_range(last_range.metadata().index() + 1, end_offset)
                                    .await
                                {
                                    // delay retry to avoid busy loop
                                    sleep(Duration::from_millis(1000)).await;
                                }
                            }
                            Err(_) => {
                                // delay retry to avoid busy loop
                                sleep(Duration::from_millis(100)).await;
                            }
                        }
                        stream.trigger_append_task();
                        continue;
                    }
                    last_range
                }
                None => {
                    if let Err(_) = stream.new_range(0, 0).await {
                        // delay retry to avoid busy loop
                        sleep(Duration::from_millis(1000)).await;
                        stream.trigger_append_task();
                    }
                    continue;
                }
            };
            if !inflight.is_empty() {
                // 2. ack success append request, and remove them from inflight.
                let confirm_offset = last_writable_range.confirm_offset();
                let mut ack_count = 0;
                for (base_offset, append_request) in inflight.iter() {
                    if *base_offset < confirm_offset {
                        // if base offset is less than confirm offset, it means append request is already success.
                        append_request.success();
                        ack_count += 1;
                    }
                }
                for _ in 0..ack_count {
                    inflight.pop_first();
                }

                // 3. try append request which base_offset >= next_append_start_offset.
                let cursor = inflight.lower_bound(Included(&next_append_start_offset));
                while let Some((base_offset, append_request)) = cursor.key_value() {
                    last_writable_range.append(
                        append_request.payload.clone(),
                        RangeAppendContext::new(*base_offset, append_request.context.count),
                    );
                    next_append_start_offset = base_offset + append_request.context.count as u64;
                }
            }
        }
    }
}

pub(crate) struct StreamAppendContext {
    count: u32,
}

impl StreamAppendContext {
    pub(crate) fn new(count: u32) -> StreamAppendContext {
        StreamAppendContext { count }
    }
}

struct StreamAppendRequest {
    base_offset: u64,
    payload: Rc<Bytes>,
    context: StreamAppendContext,
    append_tx: RefCell<OnceCell<oneshot::Sender<Result<(), ReplicationError>>>>,
}

impl StreamAppendRequest {
    pub fn new(
        base_offset: u64,
        payload: Bytes,
        context: StreamAppendContext,
        append_tx: oneshot::Sender<Result<(), ReplicationError>>,
    ) -> Self {
        let append_tx_cell = OnceCell::new();
        let _ = append_tx_cell.set(append_tx);
        Self {
            base_offset,
            payload: Rc::new(payload),
            context,
            append_tx: RefCell::new(append_tx_cell),
        }
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
