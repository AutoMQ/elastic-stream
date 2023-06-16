use crate::error::{AppendError, StoreError};
use crate::index::driver::IndexDriver;
use crate::index::record_handle::{HandleExt, RecordHandle};
use crate::io::buf::{AlignedBufReader, AlignedBufWriter};
use crate::io::context::Context;
use crate::io::task::IoTask;
use crate::io::task::WriteTask;
use crate::io::wal::Wal;
use crate::io::write_window::WriteWindow;
use crate::AppendResult;
use crate::BufSlice;
use minstant::Instant;
use observation::metrics::uring_metrics::{
    UringStatistics, COMPLETED_READ_IO, COMPLETED_WRITE_IO, INFLIGHT_IO, IO_DEPTH, PENDING_TASK,
    READ_BYTES_TOTAL, READ_IO_LATENCY, WRITE_BYTES_TOTAL, WRITE_IO_LATENCY,
};
use std::io;

use crossbeam::channel::{Receiver, Sender, TryRecvError};
use io_uring::{opcode, squeue, types};
use io_uring::{register, Parameters};
use log::{debug, error, info, trace, warn};
use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;
use std::{
    cell::{RefCell, UnsafeCell},
    collections::{HashMap, VecDeque},
    os::fd::AsRawFd,
};
use tokio::sync::oneshot;

use super::block_cache::{EntryRange, MergeRange};
use super::buf::AlignedBuf;
use super::segment::FOOTER_LENGTH;
use super::task::SingleFetchResult;
use super::ReadTask;

pub(crate) struct IO {
    options: Arc<config::Configuration>,

    /// Full fledged I/O Uring instance with setup of `SQPOLL` and `IOPOLL` features
    ///
    /// This io_uring instance is supposed to take up two CPU processors/cores. Namely, both the kernel and user-land are performing
    /// busy polling, submitting and reaping requests.
    ///
    /// With `IOPOLL`, the kernel thread is polling block device drivers for completed tasks, thus no interrupts are required any more on
    /// IO completion.
    ///
    /// With `SQPOLL`, once application thread submits the IO request to `SQ` and compare-and-swap queue head, kernel thread would `see`
    /// and start to process them without `io_uring_enter` syscall.
    ///
    /// At the time of writing(kernel 5.15 and 5.19), `io_uring` instance with `IOPOLL` feature is restricted to file descriptor opened
    /// with `O_DIRECT`. That is, `Currently, this feature is usable only  on  a file  descriptor opened using the O_DIRECT flag.`
    ///
    /// As a result, Opcode `OpenAt` and`OpenAt2` are not compatible with this `io_uring` instance.
    /// `Fallocate64`, for some unknown reason, is not working either.
    data_ring: io_uring::IoUring,

    /// Sender of the IO task channel.
    ///
    /// Assumed to be taken by wrapping structs, which will offer APIs with semantics of choice.
    pub(crate) sender: Option<Sender<IoTask>>,

    /// Receiver of the IO task channel.
    ///
    /// According to our design, there is only one instance.
    receiver: Receiver<IoTask>,

    /// A WAL instance that manages the lifecycle of write-ahead-log segments.
    ///
    /// Note new segment are appended to the back; while oldest segments are popped from the front.
    wal: Wal,

    // Following fields are runtime data structure
    /// Flag indicating if the IO channel is disconnected, aka, all senders are dropped.
    channel_disconnected: bool,

    /// Number of inflight data tasks that are submitted to `data_uring` and not yet reaped.
    inflight: usize,

    /// Pending IO tasks received from IO channel.
    ///
    /// Before converting `IoTask`s into io_uring SQEs, we need to ensure these tasks are bearing valid offset and
    /// length if they are read; In case the tasks are write, we need to ensure targeting log segment file has enough
    /// space for the incoming buffers.
    ///
    /// If there is no writable log segment files available or the write is so fast that preallocated ones are depleted
    /// before a new segment file is ready, writes, though very unlikely, will stall.
    pending_data_tasks: VecDeque<IoTask>,

    buf_writer: UnsafeCell<AlignedBufWriter>,

    /// Tracks write requests that are dispatched to underlying storage device and
    /// the completed ones;
    ///
    /// Advances continuous boundary if possible.
    write_window: WriteWindow,

    /// Inflight write tasks that are not yet acknowledged
    ///
    /// Assume the target record of the `WriteTask` is [n, n + len) in WAL, we use `n + len` as key.
    inflight_write_tasks: BTreeMap<u64, WriteTask>,

    /// Inflight read tasks that are not yet acknowledged
    ///
    /// The key is the start wal offset of segment that contains the read tasks.
    inflight_read_tasks: BTreeMap<u64, VecDeque<ReadTask>>,

    /// Offsets of blocks that are partially filled with data and are still inflight.
    barrier: RefCell<HashSet<u64>>,

    /// Block the concurrent write IOs to the same page.
    /// The uring instance doesn't provide the ordering guarantee for the IOs,
    /// so we use this mechanism to avoid memory corruption.
    blocked: HashMap<u64, (*mut Context, squeue::Entry)>,

    /// Collects the SQEs that need to be re-submitted.
    resubmit_sqes: VecDeque<squeue::Entry>,

    /// Provide index service for building read index, shared with the upper store layer.
    indexer: Arc<IndexDriver>,
}

/// Check if required opcodes are supported by the host operation system.
///
/// # Arguments
/// * `probe` - Probe result, which contains all features that are supported.
///
fn check_io_uring(probe: &register::Probe, params: &Parameters) -> Result<(), StoreError> {
    if !params.is_feature_sqpoll_nonfixed() {
        error!("io_uring feature: IORING_FEAT_SQPOLL_NONFIXED is required. Current kernel version is too old");
        return Err(StoreError::IoUring);
    }
    info!("io_uring has feature IORING_FEAT_SQPOLL_NONFIXED");

    // io_uring should support never dropping completion events.
    if !params.is_feature_nodrop() {
        error!("io_uring setup: IORING_SETUP_CQ_NODROP is required.");
        return Err(StoreError::IoUring);
    }
    info!("io_uring has feature IORING_SETUP_CQ_NODROP");

    let codes = [
        opcode::OpenAt::CODE,
        opcode::Fallocate64::CODE,
        opcode::Write::CODE,
        opcode::Read::CODE,
        opcode::Close::CODE,
        opcode::UnlinkAt::CODE,
    ];
    for code in &codes {
        if !probe.is_supported(*code) {
            return Err(StoreError::OpCodeNotSupported(*code));
        }
    }
    Ok(())
}

/// A simple macro_rule to log amount of time used to await completion of submitted IO tasks.
macro_rules! log_disk_perf {
    ($level:tt, $elapsed:expr) => {
        $level!(
            "io_uring_enter waited {}ms to reap completed data CQE(s)",
            $elapsed
        );
    };
}

impl IO {
    /// Create new `IO` instance.
    ///
    /// Behavior of the IO instance can be tuned through `Options`.
    pub(crate) fn new(
        config: &Arc<config::Configuration>,
        indexer: Arc<IndexDriver>,
    ) -> Result<Self, StoreError> {
        let control_ring = io_uring::IoUring::builder().dontfork().build(32).map_err(|e| {
            error!("Failed to build I/O Uring instance for write-ahead-log segment file management: {:?}", e);
            StoreError::IoUring
        })?;

        let data_ring = io_uring::IoUring::builder()
            .dontfork()
            .setup_iopoll()
            .setup_sqpoll(config.store.uring.sqpoll_idle_ms)
            .setup_sqpoll_cpu(config.store.uring.sqpoll_cpu)
            .setup_r_disabled()
            .build(config.store.uring.queue_depth)
            .map_err(|e| {
                error!("Failed to build polling I/O Uring instance: {:?}", e);
                StoreError::IoUring
            })?;

        let mut probe = register::Probe::new();

        let submitter = data_ring.submitter();
        submitter.register_iowq_max_workers(&mut [
            config.store.uring.max_bounded_worker,
            config.store.uring.max_unbounded_worker,
        ])?;
        submitter.register_probe(&mut probe)?;
        submitter.register_enable_rings()?;

        check_io_uring(&probe, data_ring.params())?;

        trace!("Polling I/O Uring instance created");

        let (sender, receiver) = crossbeam::channel::unbounded();

        Ok(Self {
            options: config.clone(),
            data_ring,
            sender: Some(sender),
            receiver,
            write_window: WriteWindow::new(0),
            buf_writer: UnsafeCell::new(AlignedBufWriter::new(0, config.store.alignment)),
            wal: Wal::new(control_ring, config),
            channel_disconnected: false,
            inflight: 0,
            pending_data_tasks: VecDeque::new(),
            inflight_write_tasks: BTreeMap::new(),
            inflight_read_tasks: BTreeMap::new(),
            barrier: RefCell::new(HashSet::new()),
            blocked: HashMap::new(),
            resubmit_sqes: VecDeque::new(),
            indexer,
        })
    }

    fn load(&mut self) -> Result<(), StoreError> {
        self.wal.load_from_paths()?;
        Ok(())
    }

    fn recover(&mut self, offset: u64) -> Result<(), StoreError> {
        let indexer = Arc::clone(&self.indexer);
        let pos = self.wal.recover(offset, indexer)?;

        // Reset offset of write buffer
        self.buf_writer.get_mut().reset_cursor(pos);

        // Rebase `buf_writer` to include the last partially written buffer `page`.
        {
            if let Some(segment) = self.wal.segment_file_of(pos) {
                let buf = segment.block_cache.buf_of_last_cache_entry();
                if let Some(buf) = buf {
                    // Only the last cache entry is possibly partially filled.
                    if buf.remaining() > 0 {
                        trace!(
                            "Rebase `BufWriter` to include the last partially written buffer: {}",
                            buf
                        );
                        debug_assert_eq!(pos, buf.wal_offset + buf.limit() as u64);
                        self.buf_writer.get_mut().rebase_buf(buf);
                    }
                }
            }
        }

        // Reset committed WAL offset
        self.write_window.reset_committed(pos);

        Ok(())
    }

    fn validate_io_task(io_task: &mut IoTask) -> bool {
        if let IoTask::Write(ref mut task) = io_task {
            if task.buffer.is_empty() {
                warn!("WriteTask buffer length is 0");
                return false;
            }
        }
        true
    }

    fn on_bad_request(io_task: IoTask) {
        match io_task {
            IoTask::Read(_) => {
                todo!()
            }
            IoTask::Write(write_task) => {
                let _ = write_task.observer.send(Err(AppendError::Internal));
            }
        }
    }

    #[inline]
    fn add_pending_task(&mut self, mut io_task: IoTask, received: &mut usize) {
        if !IO::validate_io_task(&mut io_task) {
            IO::on_bad_request(io_task);
            return;
        }
        // Log received IO-task.
        match &io_task {
            IoTask::Write(write_task) => {
                trace!(
                    "Received a write-task from channel: stream-id={}, offset={}",
                    write_task.stream_id,
                    write_task.offset
                );
            }
            IoTask::Read(read_task) => {
                trace!(
                    "Received a read-task from channel: stream-id={}, wal-offset={}, len={}",
                    read_task.stream_id,
                    read_task.wal_offset,
                    read_task.len,
                );
            }
        }

        self.pending_data_tasks.push_back(io_task);
        PENDING_TASK.set(self.pending_data_tasks.len() as i64);
        *received += 1;
    }

    fn receive_io_tasks(&mut self) -> usize {
        let mut received = 0;
        let io_depth = self.data_ring.params().sq_entries() as usize;
        IO_DEPTH.set(io_depth as i64);
        loop {
            // TODO: Find a better estimation.
            //
            // it might be some kind of pessimistic here as read/write may be merged after grouping.
            // As we might configure a larger IO depth value, finding a more precise metric is left to
            // the next development iteration.
            //
            // A better metric is combining actual SQEs number with received tasks together, so we may
            // receive tasks according merging result of the previous iteration.
            //
            // Note cloud providers count IOPS in a complex way:
            // https://aws.amazon.com/premiumsupport/knowledge-center/ebs-calculate-optimal-io-size/
            //
            // For example, if the application is performing small I/O operations of 32 KiB:
            // 1. Amazon EBS merges sequential (physically contiguous) operations to the maximum I/O size of 256 KiB.
            //    In this scenario, Amazon EBS counts only 1 IOPS to perform 8 I/O operations submitted by the operating system.
            // 2. Amazon EBS counts random I/O operations separately. A single, random I/O operation of 32 KiB counts as 1 IOPS.
            //    In this scenario, Amazon EBS counts 8 random, 32 KiB I/O operations as 8 IOPS submitted by the OS.
            //
            // Amazon EBS splits I/O operations larger than the maximum 256 KiB into smaller operations.
            // For example, if the I/O size is 500 KiB, Amazon EBS splits the operation into 2 IOPS.
            // The first one is 256 KiB and the second one is 244 KiB.
            if self.inflight + received >= io_depth {
                break received;
            }

            // if there is not inflight data uring tasks nor inflight control tasks, we
            if self.inflight
                + received
                + self.pending_data_tasks.len()
                + self.blocked.len()
                + self.wal.inflight_control_task_num()
                == 0
            {
                debug!("Block IO thread until IO tasks are received from channel");
                // Block the thread until at least one IO task arrives
                match self.receiver.recv() {
                    Ok(io_task) => {
                        self.add_pending_task(io_task, &mut received);
                    }
                    Err(_e) => {
                        info!("Channel for submitting IO task disconnected");
                        self.channel_disconnected = true;
                        break received;
                    }
                }
            } else {
                // Poll IO-task channel in non-blocking manner for more tasks given that greater IO depth exploits potential of
                // modern storage products like NVMe SSD.
                match self.receiver.try_recv() {
                    Ok(io_task) => {
                        self.add_pending_task(io_task, &mut received);
                    }
                    Err(TryRecvError::Empty) => {
                        break received;
                    }
                    Err(TryRecvError::Disconnected) => {
                        info!("Channel for submitting IO task disconnected");
                        self.channel_disconnected = true;
                        break received;
                    }
                }
            }
        }
    }

    fn reserve_write_buffers(&self) -> Result<(), StoreError> {
        let mut requirement: VecDeque<_> = self
            .pending_data_tasks
            .iter()
            .map(|task| match task {
                IoTask::Write(task) => {
                    debug_assert!(!task.buffer.is_empty());
                    task.total_len() as usize
                }
                _ => 0,
            })
            .filter(|n| *n > 0)
            .collect();
        let buf_writer = unsafe { &mut *self.buf_writer.get() };
        let requiring = requirement.iter().sum();
        if buf_writer.remaining() >= requiring {
            // If the remaining buffer is enough to hold all pending tasks, we don't need to allocate
            // new buffers.
            return Ok(());
        }

        let file_size = self.options.store.segment_size as usize;
        debug_assert!(
            file_size % self.options.store.alignment == 0,
            "Segment file size should be multiple of alignment"
        );
        debug_assert!(
            !requirement
                .iter()
                .any(|n| *n + FOOTER_LENGTH as usize > file_size),
            "A single write task should not exceed the segment file size"
        );
        // The previously allocated buffers can be reused.
        let mut pos = buf_writer.cursor as usize % file_size;
        let mut max_allocated_wal_offset = buf_writer.max_allocated_wal_offset();
        let mut to_allocate = 0;
        {
            let mut allocated = buf_writer.remaining();
            while let Some(n) = requirement.front() {
                if allocated > 0 {
                    debug_assert!(pos + FOOTER_LENGTH as usize <= file_size);
                    if pos + *n + FOOTER_LENGTH as usize > file_size {
                        trace!(
                            "Reach the end of a segment file. Take up {} bytes to append footer",
                            file_size - pos
                        );
                        if allocated >= file_size - pos {
                            trace!("Already pre-allocated enough buffer for the footer");
                            allocated -= file_size - pos;
                        } else {
                            // Need to allocate a new buffer.
                            let extra = file_size - pos - allocated;
                            trace!(
                                "Need to allocate {} bytes more for the footer. pre-allocated={}",
                                extra,
                                allocated
                            );
                            // All pre-allocated buffers are used up.
                            allocated = 0;
                            let to = max_allocated_wal_offset + extra as u64;
                            buf_writer.reserve_to(to, file_size)?;
                            max_allocated_wal_offset = buf_writer.max_allocated_wal_offset();
                        }

                        // Allocate aligned buffers for the next segment.
                        pos = 0;
                    } else {
                        pos += *n;
                        if allocated >= *n {
                            allocated -= *n;
                            requirement.pop_front();
                        } else {
                            // Pre-allocated buffers are not large enough to hold the next task.
                            // Account it to the new allocation.
                            to_allocate += *n - allocated;
                            requirement.pop_front();
                            break;
                        }
                    }
                } else {
                    break;
                }
            }
        }

        // Allocate new buffers for the remaining tasks.
        {
            while let Some(n) = requirement.front() {
                debug_assert!(pos + FOOTER_LENGTH as usize <= file_size);
                if pos + *n + FOOTER_LENGTH as usize > file_size {
                    debug_assert!(file_size - pos < *n + FOOTER_LENGTH as usize);
                    to_allocate += file_size - pos;
                    let to = max_allocated_wal_offset + to_allocate as u64;
                    buf_writer.reserve_to(to, file_size)?;
                    max_allocated_wal_offset = buf_writer.max_allocated_wal_offset();
                    pos = 0;
                    to_allocate = 0;
                } else {
                    to_allocate += *n;
                    pos += *n;
                    requirement.pop_front();
                }
            }

            if to_allocate > 0 {
                let to = max_allocated_wal_offset + to_allocate as u64;
                buf_writer.reserve_to(to, file_size)?;
            }
        }

        Ok(())
    }

    fn build_read_sqe(
        &mut self,
        entries: &mut Vec<squeue::Entry>,
        missed_entries: HashMap<u64, Vec<EntryRange>>,
    ) {
        missed_entries.into_iter().for_each(|(wal_offset, ranges)| {
            // Merge the ranges to reduce the number of IOs.
            let merged_ranges = ranges.merge();

            merged_ranges.iter().for_each(|range| {
                if let Ok(buf) = AlignedBufReader::alloc_read_buf(
                    range.wal_offset,
                    range.len as usize,
                    self.options.store.alignment,
                ) {
                    let ptr = buf.as_ptr() as *mut u8;

                    // The allocated buffer is always aligned, so use the aligned offset as the read offset.
                    let read_offset = buf.wal_offset;

                    // The length of the aligned buffer is multiple of alignment,
                    // use it as the read length to maximize the value of a single IO.
                    // Note that the read len is always larger than or equal the requested length.
                    let read_len = buf.capacity as u32;

                    let segment = match self.wal.segment_file_of(wal_offset) {
                        Some(segment) => segment,
                        None => {
                            // Consume io_task directly
                            todo!("Return error to caller directly")
                        }
                    };

                    // Calculate the relative position of the read IO in the segment file.
                    let read_from = read_offset - segment.wal_offset;

                    if let Some(sd) = segment.sd.as_ref() {
                        // The pointer will be set into user_data of uring.
                        // When the uring io completes, the pointer will be used to retrieve the `Context`.
                        let context = Context::read_ctx(
                            opcode::Read::CODE,
                            Arc::new(buf),
                            read_offset,
                            read_len,
                            Instant::now(),
                        );

                        let sqe = opcode::Read::new(types::Fd(sd.fd), ptr, read_len)
                            .offset(read_from as libc::off_t)
                            .build()
                            .user_data(context as u64);

                        entries.push(sqe);

                        // Trace the submit read IO
                        trace!(
                            "Submit read IO. offset={}, len={}, segment={}",
                            read_offset,
                            read_len,
                            segment.wal_offset,
                        );
                        // Add the ongoing entries to the block cache
                        segment.block_cache.add_loading_entry(*range);
                    }
                }
            });
        });
    }

    /// Some write SQEs are blocked by barriers. This method checks if any of these barriers is lifted.
    ///
    /// Once the barrier-write task is completed, we shall dispatch the blocked ones immediately.
    ///
    /// For example, the underlying storage device has an alignment of 4KiB and the 6KiB of data is
    /// requested to write. IO module would generate two writes to block layer: one is a full-4KiB and the
    /// other is also 4KiB, yet carrying 2KiB valid data. Given than underlying io_uring cannot guarantee
    /// ordering of the submitted write tasks, data appended to the partially-filled block-page must NOT
    /// submit to io_uring/block-layer until the prior one is completed.
    ///
    /// In this example, this method tells whether
    #[inline(always)]
    fn has_write_sqe_unblocked(&self) -> bool {
        self.blocked
            .iter()
            .any(|(offset, _entry)| !self.barrier.borrow().contains(offset))
    }

    fn build_write_sqe(&mut self, entries: &mut Vec<squeue::Entry>) {
        // Add previously blocked entries.

        self.blocked
            .extract_if(|offset, _entry| !self.barrier.borrow().contains(offset))
            .for_each(|(_, entry)| {
                // Trace log submit of previously blocked write to WAL.
                {
                    // Safety:
                    // Lifecycle of context is the same with squeue::Entry, since we have NOT yet
                    // submit the entry, its associated context is valid.
                    let ctx = unsafe { Box::from_raw(entry.0) };
                    trace!(
                        "Submit previously blocked write to WAL[{}, {})",
                        ctx.wal_offset,
                        ctx.wal_offset + ctx.len as u64
                    );

                    // Need to insert a barrier if the blocked IO itself is ALSO a partial write.
                    if ctx.is_partial_write() {
                        self.barrier.borrow_mut().insert(ctx.wal_offset);
                        trace!("Insert a barrier with wal_offset={}", ctx.wal_offset);
                    }

                    Box::into_raw(ctx);
                }
                entries.push(entry.1);
            });

        let writer = self.buf_writer.get_mut();
        writer
            .take()
            .into_iter()
            .filter(|buf| {
                // Accept the last partial buffer only if it contains uncommited data.
                buf.wal_offset + buf.limit() as u64 > self.write_window.committed
            })
            .flat_map(|buf| {
                let ptr = buf.as_ptr();
                if let Some(segment) = self.wal.segment_file_of(buf.wal_offset) {
                    if let Some(sd) = segment.sd.as_ref() {
                        debug_assert!(buf.wal_offset >= segment.wal_offset);
                        debug_assert!(
                            buf.wal_offset + buf.capacity as u64
                                <= segment.wal_offset + segment.size,
                                "buf.wal_offset: {}, buf.capacity: {}, segment.wal_offset: {}, segment.size: {}",
                                buf.wal_offset, buf.capacity, segment.wal_offset, segment.size
                        );
                        let file_offset = buf.wal_offset - segment.wal_offset;

                        let mut io_blocked = false;
                        // Check barrier
                        if self.barrier.borrow().contains(&buf.wal_offset) {
                            // Submit SQE to io_uring when the blocking IO task completed.
                            io_blocked = true;
                        } else {
                            // Insert barrier, blocking future write to this aligned block issued to `io_uring` until `sqe` is reaped.
                            if buf.partial() {
                                self.barrier.borrow_mut().insert(buf.wal_offset);
                                trace!(
                                    "Insert a barrier with wal_offset={}",
                                    buf.wal_offset
                                );
                            }
                        }

                        let buf_wal_offset = buf.wal_offset;
                        let buf_capacity = buf.capacity as u32;
                        let buf_limit = buf.limit() as u32;

                        // The pointer will be set into user_data of uring.
                        // When the uring io completes, the pointer will be used to retrieve the `Context`.
                        let context =
                            Context::write_ctx(opcode::Write::CODE, buf, buf_wal_offset, buf_limit, Instant::now());

                        // Note we have to write the whole page even if the page is partially filled.
                        let sqe = opcode::Write::new(types::Fd(sd.fd), ptr, buf_capacity)
                            .offset64(file_offset as libc::off_t)
                            .build()
                            .user_data(context as u64);

                        if io_blocked {
                            trace!("Write to WAL[{}, {}) is blocked", buf_wal_offset, buf_wal_offset + buf_limit as u64);
                            if let Some((ctx, _entry)) =
                                self.blocked.insert(buf_wal_offset, (context, sqe))
                            {
                                // Release context of the dropped squeue::Entry
                                // See https://github.com/tokio-rs/io-uring/issues/230
                                let ctx = unsafe { Box::from_raw(ctx) };
                                if ctx.len <= buf_limit {
                                    debug!("Blocked write to WAL[{}, {}) is superseded by [{}, {})",
                                        ctx.wal_offset, ctx.wal_offset + ctx.len as u64,
                                        buf_wal_offset, buf_wal_offset + buf_limit as u64);
                                } else {
                                    error!("Blocked write to WAL[{}, {}) is superseded by [{}, {})",
                                        ctx.wal_offset, ctx.wal_offset + ctx.len as u64,
                                        buf_wal_offset, buf_wal_offset + buf_limit as u64);
                                    unreachable!("An extended write is superseded by a previous small one");
                                }
                            }
                        } else {
                            entries.push(sqe);
                        }
                    } else {
                        // fatal errors
                        let msg = format!("Segment {} should be open and with valid FD", segment);
                        error!("{}", msg);
                        panic!("{}", msg);
                    }
                } else {
                    error!("");
                }
                Ok::<(), super::WriteWindowError>(())
            })
            .count();
    }

    fn build_sqe(&mut self, entries: &mut Vec<squeue::Entry>) {
        let alignment = self.options.store.alignment;
        if let Err(e) = self.reserve_write_buffers() {
            error!("Failed to reserve write buffers: {}", e);
            return;
        }

        let mut need_write = false;

        // Try reclaim the cache space for the upcoming entries.
        let (_, free_bytes) = try_reclaim_from_wal(&mut self.wal, 0);
        let mut free_bytes = free_bytes as i64;

        // The missed entries that are not in the cache, group by the start offset of segments.
        let mut missed_entries: HashMap<u64, Vec<EntryRange>> = HashMap::new();
        let mut strong_referenced_entries: HashMap<u64, Vec<EntryRange>> = HashMap::new();

        'task_loop: while let Some(io_task) = self.pending_data_tasks.pop_front() {
            match io_task {
                IoTask::Read(task) => {
                    let segment = match self.wal.segment_file_of(task.wal_offset) {
                        Some(segment) => segment,
                        None => {
                            // Consume io_task directly
                            todo!("Return error to caller directly")
                        }
                    };

                    if let Some(_sd) = segment.sd.as_ref() {
                        let range_to_read = EntryRange::new(task.wal_offset, task.len, alignment);

                        // Try to read from the buffer first.
                        let buf_res = segment.block_cache.try_get_entries(range_to_read);
                        let seg_wal_offset = segment.wal_offset;

                        match buf_res {
                            Ok(Some(buf_v)) => {
                                // Complete the task directly.
                                self.complete_read_task(task, buf_v);
                                continue;
                            }
                            Ok(None) => {
                                // The dependent entries are inflight, move to inflight list directly.
                            }
                            Err(mut entries) => {
                                // Cache miss, there are some entries should be read from disk.
                                // Calculate the total length of the entries.
                                let mut total_len = 0;
                                for entry in entries.iter() {
                                    total_len += entry.len;
                                }

                                free_bytes -= total_len as i64;

                                if free_bytes >= 0 {
                                    missed_entries
                                        .entry(seg_wal_offset)
                                        .or_default()
                                        .append(&mut entries);
                                } else {
                                    // There is no enough space to read the entries, the task should be blocked.
                                    // Trace the pending task
                                    trace!("Pending the read task");
                                    self.pending_data_tasks.push_front(IoTask::Read(task));
                                    break 'task_loop;
                                }
                            }
                        }

                        strong_referenced_entries
                            .entry(seg_wal_offset)
                            .or_default()
                            .push(range_to_read);

                        // Move the task to inflight list.
                        self.inflight_read_tasks
                            .entry(seg_wal_offset)
                            .or_default()
                            .push_back(task);
                    } else {
                        self.pending_data_tasks.push_front(IoTask::Read(task));
                    }
                }
                IoTask::Write(mut task) => {
                    let writer = unsafe { &mut *self.buf_writer.get() };
                    loop {
                        let payload_length = task.buffer.len();

                        free_bytes -= payload_length as i64;

                        if free_bytes < 0 {
                            // Pending the task and wait for the cache space to be reclaimed.
                            self.pending_data_tasks.push_front(IoTask::Write(task));
                            break 'task_loop;
                        }

                        if let Some(segment) = self.wal.segment_file_of(writer.cursor) {
                            if !segment.writable() {
                                trace!(
                                    "WAL Segment {}. Yield dispatching IO to block layer",
                                    segment
                                );
                                self.pending_data_tasks.push_front(IoTask::Write(task));
                                break 'task_loop;
                            }

                            if let Some(_sd) = segment.sd.as_ref() {
                                if !segment.can_hold(payload_length as u64) {
                                    if let Ok(cursor) = segment.append_footer(writer) {
                                        trace!(
                                            "Write cursor of WAL after padding segment footer is: {}",
                                            cursor
                                        );
                                        need_write = true;
                                    }
                                    // Switch to a new log segment
                                    continue;
                                }

                                let pre_written = segment.written;
                                if let Ok(cursor) = segment.append_record(writer, &task.buffer[..])
                                {
                                    trace!(
                                        "Write cursor of WAL after appending record is: {}",
                                        cursor
                                    );
                                    // Set the written len of the task
                                    task.written_len = Some((segment.written - pre_written) as u32);
                                    self.inflight_write_tasks.insert(cursor, task);
                                    need_write = true;
                                }
                                break;
                            } else {
                                error!("LogSegmentFile {} with read_write status does not have valid FD", segment);
                                unreachable!("LogSegmentFile {} should have been with a valid FD if its status is read_write", segment);
                            }
                        } else {
                            self.pending_data_tasks.push_front(IoTask::Write(task));
                            break 'task_loop;
                        }
                    }
                }
            }
        }
        PENDING_TASK.set(self.pending_data_tasks.len() as i64);
        self.build_read_sqe(entries, missed_entries);

        // Increase the strong reference count of the entries.
        // Note: we do this task after the read task is built, because the build step will add loading entries to the cache.
        for (seg_wal_offset, entries) in strong_referenced_entries.into_iter() {
            if let Some(segment) = self.wal.segment_file_of(seg_wal_offset) {
                for range_to_read in entries {
                    // The read task is blocked by inflight entries, the involved cache entries should be strong referenced.
                    segment
                        .block_cache
                        .strong_reference_entries(range_to_read, 1);
                }
            }
        }

        if self.has_write_sqe_unblocked() || need_write {
            self.build_write_sqe(entries);
        }

        // Drain the SQEs that need to be re-submitted.
        self.resubmit_sqes.drain(..).for_each(|sqe| {
            entries.push(sqe);
        });
    }

    fn await_data_task_completion(&self, mut wanted: usize) {
        if self.inflight == 0 {
            trace!("No inflight data task. Skip `await_data_task_completion`");
            return;
        }

        trace!(
            "Waiting for at least {}/{} data CQE(s) to reap",
            wanted,
            self.inflight
        );

        if wanted > self.inflight {
            wanted = self.inflight;
        }

        let now = Instant::now();
        loop {
            match self.data_ring.submit_and_wait(wanted) {
                Ok(_reaped) => {
                    let elapsed = now.elapsed().as_millis();
                    if elapsed <= 1 {
                        log_disk_perf!(trace, elapsed);
                    } else if elapsed <= 5 {
                        log_disk_perf!(debug, elapsed);
                    } else if elapsed <= 10 {
                        log_disk_perf!(info, elapsed);
                    } else {
                        log_disk_perf!(warn, elapsed);
                    }
                    break;
                }
                Err(e) => {
                    match e.kind() {
                        io::ErrorKind::Interrupted => {
                            // io_uring_enter just propagates underlying EINTR signal up to application, allowing to handle this signal.
                            // For our usage, we just ignore this signal and retry.
                            //
                            // See https://www.spinics.net/lists/io-uring/msg01823.html
                            // Lots of system calls return -EINTR if interrupted by a signal, don't
                            // think there's anything worth fixing there. For the wait part, the
                            // application may want to handle the signal before we can wait again.
                            // We can't go to sleep with a pending signal.
                            warn!("io_uring_enter got an error: {:?}", e);
                            continue;
                        }
                        io::ErrorKind::WouldBlock => {
                            // The kernel was unable to allocate memory for the request, or otherwise ran out of resources to handle it.
                            // The application should wait for some completions and try again.
                            warn!("io_uring_enter got an error: {:?}", e);
                            continue;
                        }
                        io::ErrorKind::ResourceBusy => {
                            // If the IORING_FEAT_NODROP feature flag is set, then EBUSY will be returned if there were overflow entries,
                            // IORING_ENTER_GETEVENTS flag is set and not all of the overflow entries were able to be flushed to the CQ ring.
                            // Without IORING_FEAT_NODROP the application is attempting to overcommit the number of requests it can have pending.
                            // The application should wait for some completions and try again. May occur if the application tries to queue more
                            // requests than we have room for in the CQ ring, or if the application attempts to wait for more events without
                            // having reaped the ones already present in the CQ ring.
                        }
                        _ => {
                            // Fatal errors, crash the process and let watchdog to restart.
                            error!("io_uring_enter got an error: {:?}", e);
                            panic!("io_uring_enter returns error {:?}", e);
                        }
                    }
                }
            }
        }
    }

    fn reap_data_tasks(&mut self) {
        if 0 == self.inflight {
            return;
        }
        let committed = self.write_window.committed;
        let mut cache_entries = vec![];
        let mut cache_bytes = 0u32;
        let mut affected_segments = HashSet::new();
        {
            let mut count = 0;
            let mut completion = self.data_ring.completion();
            loop {
                if completion.is_empty() {
                    break;
                }

                #[allow(clippy::while_let_on_iterator)]
                while let Some(cqe) = completion.next() {
                    count += 1;
                    let tag = cqe.user_data();

                    let ptr = tag as *mut Context;

                    // Safety:
                    // It's safe to convert tag ptr back to Box<Context> as the memory pointed by ptr
                    // is allocated by Box itself, hence, there will no alignment issue at all.
                    let mut context = unsafe { Box::from_raw(ptr) };
                    let latency = context.start_time.elapsed();
                    match context.opcode {
                        opcode::Read::CODE => {
                            READ_IO_LATENCY.observe(latency.as_micros() as f64);
                            READ_BYTES_TOTAL.inc_by(context.buf.capacity as u64);
                            COMPLETED_READ_IO.inc();
                            UringStatistics::observe_latency(latency.as_millis() as i16);
                        }
                        opcode::Write::CODE => {
                            WRITE_IO_LATENCY.observe(latency.as_micros() as f64);
                            WRITE_BYTES_TOTAL.inc_by(context.buf.capacity as u64);
                            COMPLETED_WRITE_IO.inc();
                            UringStatistics::observe_latency(latency.as_millis() as i16);
                        }
                        _ => {}
                    }

                    // Remove write barrier
                    if (context.opcode == opcode::Write::CODE
                        || context.opcode == opcode::Writev::CODE
                        || context.opcode == opcode::WriteFixed::CODE)
                        && self.barrier.borrow_mut().remove(&context.buf.wal_offset)
                    {
                        trace!("Remove the barrier with wal_offset={}", context.wal_offset);
                    }

                    if let Err(e) = on_complete(&mut self.write_window, &mut context, cqe.result())
                    {
                        match e {
                            StoreError::System(errno) => {
                                error!(
                                    "io_uring opcode `{}` failed. errno: `{}`",
                                    context.opcode, errno
                                );

                                let error = io::Error::from_raw_os_error(errno);
                                // Some errors are not fatal, we should retry the task.
                                // TODO: Add more error kinds to retry.
                                if error.kind() == io::ErrorKind::Interrupted
                                    || error.kind() == io::ErrorKind::WouldBlock
                                {
                                    if self.blocked.contains_key(&context.wal_offset) {
                                        // There is a pending write task for this block, skip this task.
                                        trace!(
                                            "Skip retrying the failed write task for wal offset {} as there is a pending write task for this block",
                                            context.wal_offset
                                        );
                                        continue;
                                    }

                                    let wal_offset = context.wal_offset;
                                    let buf = context.buf.clone();

                                    if buf.partial() {
                                        // Partial write, set the barrier.
                                        self.barrier.borrow_mut().insert(buf.wal_offset);
                                        trace!(
                                            "Insert a barrier with wal_offset={}",
                                            buf.wal_offset
                                        );
                                    }

                                    let sg = match self.wal.segment_file_of(wal_offset) {
                                        Some(sg) => sg,
                                        None => {
                                            error!(
                                                "Log segment not found for wal offset {}",
                                                wal_offset
                                            );
                                            continue;
                                        }
                                    };

                                    let sd = match &sg.sd {
                                        Some(sd) => sd,
                                        None => {
                                            error!(
                                                "Log segment {} does not have a valid descriptor",
                                                sg.wal_offset
                                            );
                                            continue;
                                        }
                                    };

                                    let file_offset = wal_offset - sg.wal_offset;
                                    let buf_ptr = buf.as_ptr() as *mut u8;

                                    match context.opcode {
                                        opcode::Read::CODE => {
                                            let sqe = opcode::Read::new(
                                                types::Fd(sd.fd),
                                                buf_ptr,
                                                context.len,
                                            )
                                            .offset(file_offset as libc::off_t)
                                            .build()
                                            .user_data(Box::into_raw(context) as u64);

                                            self.resubmit_sqes.push_back(sqe);
                                        }
                                        opcode::Write::CODE => {
                                            let sqe = opcode::Write::new(
                                                types::Fd(sd.fd),
                                                buf_ptr,
                                                buf.capacity as u32,
                                            )
                                            .offset64(file_offset as libc::off_t)
                                            .build()
                                            .user_data(Box::into_raw(context) as u64);

                                            self.resubmit_sqes.push_back(sqe);
                                        }
                                        _ => (),
                                    };

                                    continue;
                                }
                                // TODO: handle the error that can't be recovered.
                            }

                            StoreError::WriteWindow => {
                                panic!("Invalid write is found. Write ordering is compromised");
                            }

                            _ => {
                                panic!("Unrecoverable error found when reaping CQEs");
                            }
                        }
                    } else {
                        // Add block cache
                        cache_entries.push(Arc::clone(&context.buf));
                        cache_bytes += context.buf.capacity as u32;

                        // Cache the completed read context
                        if opcode::Read::CODE == context.opcode {
                            if let Some(segment) = self.wal.segment_file_of(context.wal_offset) {
                                affected_segments.insert(segment.wal_offset);
                                trace!("Affected segment {}", segment.wal_offset);
                            }
                        }
                    }
                }
                // This will flush any entries consumed in this iterator and will make available new entries in the queue
                // if the kernel has produced some entries in the meantime.
                completion.sync();
            }
            debug_assert!(self.inflight >= count);
            self.inflight -= count;
            INFLIGHT_IO.sub(count as i64);
            trace!("Reaped {} data CQE(s)", count);
        }

        let (_, free_bytes) = try_reclaim_from_wal(&mut self.wal, cache_bytes);

        if free_bytes < cache_bytes as u64 {
            // There is no enough space to cache the returned blocks.
            // It's unlikely to happen, because the uring will pend the io task if there is no enough space.
            // So we just warn this case, and ingest the blocks into the cache.
            warn!(
                "No enough space to cache the returned blocks. Cache size: {}, free space: {}",
                cache_bytes, free_bytes
            );
        }

        // Add to block cache
        for buf in cache_entries {
            if let Some(segment) = self.wal.segment_file_of(buf.wal_offset) {
                segment.block_cache.add_entry(buf);
            }
        }

        self.complete_read_tasks(affected_segments);

        if self.write_window.committed > committed {
            self.complete_write_tasks();
        }
    }

    fn acknowledge_to_observer(&mut self, wal_offset: u64, task: WriteTask) {
        let append_result = AppendResult {
            stream_id: task.stream_id,
            range_index: task.range,
            offset: task.offset,
            wal_offset,
        };
        trace!(
            "Ack `WriteTask` {{ stream-id: {}, offset: {} }}",
            task.stream_id,
            task.offset
        );
        if let Err(e) = task.observer.send(Ok(append_result)) {
            error!("Failed to propagate AppendResult `{:?}`", e);
        }
    }

    fn build_read_index(&mut self, wal_offset: u64, written_len: u32, task: &WriteTask) {
        let handle = RecordHandle {
            wal_offset,
            len: written_len,
            ext: HandleExt::BatchSize(task.len),
        };
        self.indexer
            .index(task.stream_id, task.range, task.offset as u64, handle);
    }

    fn complete_read_tasks(&mut self, affected_segments: HashSet<u64>) {
        affected_segments.into_iter().for_each(|wal_offset| {
            let inflight_read_tasks = self.inflight_read_tasks.remove(&wal_offset);
            if let Some(mut inflight_read_tasks) = inflight_read_tasks {
                // If the inflight read task is completed, we need to complete it and remove it from the inflight list
                // Pop the completed task from the inflight list and send back if it's completed
                let mut send_back = vec![];
                while let Some(task) = inflight_read_tasks.pop_front() {
                    if let Some(segment) = self.wal.segment_file_of(wal_offset) {
                        let range = EntryRange::new(
                            task.wal_offset,
                            task.len,
                            self.options.store.alignment,
                        );

                        match segment.block_cache.try_get_entries(range) {
                            Ok(Some(buf)) => {
                                // The read task is blocked by inflight entries, the involved cache entries should be dereferenced.
                                segment.block_cache.strong_reference_entries(range, -1);
                                self.complete_read_task(task, buf);
                                continue;
                            }
                            Ok(None) => {
                                // Wait for the next IO to complete this task
                                // Trace the detail of uncompleted task
                                trace!(
                                    "Inflight read task {{ wal_offset: {}, len: {} }}",
                                    task.wal_offset,
                                    task.len
                                );
                            }
                            Err(_) => {
                                // The error means we need issue some read IOs for this task,
                                // but it's impossible for inflight read task since we already handle this situation in `submit_read_task`
                                error!("Unexpected error when get entries from block cache");
                            }
                        }
                    }
                    // Send back the task since it's not completed
                    send_back.push(task);
                }
                inflight_read_tasks.extend(send_back);
                if !inflight_read_tasks.is_empty() {
                    self.inflight_read_tasks
                        .insert(wal_offset, inflight_read_tasks);
                }
            }
        });

        // Trace the inflight_read_tasks after a round of completion
        self.inflight_read_tasks
            .iter()
            .for_each(|(wal_offset, tasks)| {
                trace!(
                    "Inflight read tasks for wal offset {}: {}",
                    wal_offset,
                    tasks.len()
                );
            });
    }

    fn complete_read_task(&mut self, read_task: ReadTask, read_buf_v: Vec<Arc<AlignedBuf>>) {
        // Completes the read task
        // Construct the SingleFetchResult from the read buffer
        // We need narrow the first and the last AlignedBuf to the actual read range
        let mut slice_v = vec![];
        read_buf_v.into_iter().for_each(|buf| {
            let mut start_pos = 0;

            if buf.wal_offset < read_task.wal_offset {
                start_pos = (read_task.wal_offset - buf.wal_offset) as u32;
            }

            let mut limit = buf.limit();
            let limit_len = (buf.wal_offset + buf.limit() as u64)
                .checked_sub(read_task.wal_offset + read_task.len as u64);
            if let Some(limit_len) = limit_len {
                limit -= limit_len as usize;
            }

            slice_v.push(BufSlice::new(buf, start_pos, limit as u32));
        });

        let fetch_result = SingleFetchResult {
            stream_id: read_task.stream_id,
            wal_offset: read_task.wal_offset as i64,
            payload: slice_v,
        };

        trace!(
            "Completes read task for stream {} at WAL offset {}, return {} bytes",
            fetch_result.stream_id,
            fetch_result.wal_offset,
            read_task.len,
        );

        if let Err(e) = read_task.observer.send(Ok(fetch_result)) {
            error!("Failed to send read result to observer: {:?}", e);
        }
    }

    fn complete_write_tasks(&mut self) {
        let committed = self.write_window.committed;
        while let Some((offset, _)) = self.inflight_write_tasks.first_key_value() {
            if *offset > committed {
                break;
            }

            if let Some((written_pos, task)) = self.inflight_write_tasks.pop_first() {
                // TODO: A better way to build read index is needed.
                if let Some(written_len) = task.written_len {
                    let wal_offset = written_pos - written_len as u64;
                    self.build_read_index(wal_offset, written_len, &task);
                    self.acknowledge_to_observer(wal_offset, task);
                } else {
                    error!("No written length for `WriteTask`");
                }
            }
        }
    }

    fn should_quit(&self) -> bool {
        0 == self.inflight
            && self.pending_data_tasks.is_empty()
            && self.inflight_write_tasks.is_empty()
            && self.inflight_read_tasks.is_empty()
            && self.blocked.is_empty()
            && self.wal.control_task_num() == 0
            && self.channel_disconnected
    }

    fn submit_data_tasks(&mut self, entries: &Vec<squeue::Entry>) -> Result<(), StoreError> {
        // Submit io_uring entries into submission queue.
        trace!(
            "Get {} incoming SQE(s) to submit, inflight: {}",
            entries.len(),
            self.inflight
        );
        if !entries.is_empty() {
            let cnt = entries.len();
            unsafe {
                self.data_ring
                    .submission()
                    .push_multiple(entries)
                    .map_err(|e| {
                        info!("Failed to push SQE entries into submission queue: {:?}", e);
                        StoreError::IoUring
                    })?
            };
            self.inflight += cnt;
            INFLIGHT_IO.add(cnt as i64);
            trace!("Pushed {} SQEs into submission queue", cnt);
        }
        Ok(())
    }

    pub(crate) fn run(
        io: RefCell<IO>,
        recovery_completion_tx: oneshot::Sender<()>,
    ) -> Result<(), StoreError> {
        io.borrow_mut().load()?;
        let pos = io.borrow().indexer.get_wal_checkpoint()?;
        io.borrow_mut().recover(pos)?;

        if recovery_completion_tx.send(()).is_err() {
            error!("Failed to notify completion of recovery");
        }

        let min_preallocated_segment_files =
            io.borrow().options.store.pre_allocate_segment_file_number;

        let cqe_wanted = 1;

        // Main loop
        loop {
            // Check if we need to create a new log segment
            loop {
                if io.borrow().wal.writable_segment_count() > min_preallocated_segment_files {
                    break;
                }
                io.borrow_mut().wal.try_open_segment()?;
            }

            // check if we have expired segment files to close and delete
            {
                io.borrow_mut().wal.try_close_segment()?;
            }

            let mut entries = vec![];
            {
                let mut io_mut = io.borrow_mut();

                // Receive IO tasks from channel
                let cnt = io_mut.receive_io_tasks();
                trace!("Received {} IO requests from channel", cnt);

                // Convert IO tasks into io_uring entries
                io_mut.build_sqe(&mut entries);
            }

            if !entries.is_empty() {
                io.borrow_mut().submit_data_tasks(&entries)?;
            } else {
                let io_borrow = io.borrow();
                if !io_borrow.should_quit() {
                    if io_borrow.wal.inflight_control_task_num() > 0 {
                        io_borrow.wal.await_control_task_completion();
                    }
                } else {
                    info!(
                        "Now that all IO requests are served and channel disconnects, stop main loop"
                    );
                    break;
                }
            }

            // Wait complete asynchronous IO
            io.borrow().await_data_task_completion(cqe_wanted);

            {
                let mut io_mut = io.borrow_mut();
                // Reap data CQE(s)
                io_mut.reap_data_tasks();
                // Perform file operation
                io_mut.wal.reap_control_tasks()?;
            }
        }
        info!("Main loop quit");

        // Flush index and metadata
        io.borrow().indexer.flush(true)?;

        Ok(())
    }
}

/// Try reclaim some bytes from the wal
///
fn try_reclaim_from_wal(wal: &mut Wal, reclaim_bytes: u32) -> (u64, u64) {
    // Try reclaim the cache space for the upcoming entries.
    let (reclaimed_bytes, free_bytes) = wal.try_reclaim(reclaim_bytes);
    trace!(
        "Need to reclaim {} bytes, actually {} bytes are reclaimed from WAL, now {} bytes free",
        reclaim_bytes,
        reclaimed_bytes,
        free_bytes
    );

    (reclaimed_bytes, free_bytes)
}

/// Process reaped IO completion.
///
/// # Arguments
///
/// * `state` - Operation state, including original IO request, buffer and response observer.
/// * `result` - Result code, exactly same to system call return value.
fn on_complete(
    write_window: &mut WriteWindow,
    context: &mut Context,
    result: i32,
) -> Result<(), StoreError> {
    match context.opcode {
        opcode::Write::CODE => {
            if result < 0 {
                error!(
                    "Write to WAL range `[{}, {})` failed",
                    context.buf.wal_offset,
                    context.buf.wal_offset + context.buf.capacity as u64
                );
                return Err(StoreError::System(-result));
            } else {
                if result as usize != context.buf.capacity {
                    error!(
                        "Only {} of {} bytes are written to disk",
                        result, context.buf.capacity
                    );
                    todo!("Implement write_all");
                }
                write_window
                    .commit(context.wal_offset, context.len)
                    .map_err(|e| {
                        error!(
                            "Failed to commit `write`[{}, {}) into WriteWindow: {:?}",
                            context.wal_offset,
                            context.wal_offset + context.len as u64,
                            e
                        );
                        StoreError::WriteWindow
                    })?;
            }
            Ok(())
        }
        opcode::Read::CODE => {
            if result < 0 {
                error!(
                    "Read from WAL range `[{}, {})` failed",
                    context.wal_offset, context.len
                );
                Err(StoreError::System(-result))
            } else {
                if result != context.len as i32 {
                    error!(
                        "Read {} bytes from WAL range `[{}, {})`, but {} bytes expected",
                        result,
                        context.wal_offset,
                        context.wal_offset + context.len as u64,
                        context.len
                    );
                    todo!("Implement read_all");
                }
                context.buf.increase_written(result as usize);
                Ok(())
            }
        }
        _ => Ok(()),
    }
}

impl AsRawFd for IO {
    fn as_raw_fd(&self) -> std::os::fd::RawFd {
        self.data_ring.as_raw_fd()
    }
}

impl Drop for IO {
    fn drop(&mut self) {
        self.indexer.shutdown_indexer();
    }
}

#[cfg(test)]
mod tests {
    use super::{IoTask, WriteTask};
    use crate::error::StoreError;
    use crate::index::driver::IndexDriver;
    use crate::index::MinOffset;
    use crate::io::ReadTask;
    use crate::offset_manager::WalOffsetManager;
    use bytes::BytesMut;
    use crossbeam::channel::Sender;
    use log::trace;
    use model::record::flat_record::FlatRecordBatch;
    use model::record::RecordBatchBuilder;
    use std::cell::RefCell;
    use std::error::Error;
    use std::io;
    use std::path::{Path, PathBuf};
    use std::sync::Arc;
    use std::thread::JoinHandle;
    use tokio::sync::oneshot;

    struct IOBuilder {
        cfg: config::Configuration,
    }

    impl IOBuilder {
        fn new(store_dir: PathBuf) -> Self {
            let mut cfg = config::Configuration::default();
            cfg.store
                .path
                .set_base(store_dir.as_path().to_str().unwrap());
            cfg.check_and_apply()
                .expect("Failed to check-and-apply configuration");
            Self { cfg }
        }

        fn segment_size(mut self, segment_size: u64) -> Self {
            self.cfg.store.segment_size = segment_size;
            self
        }

        fn max_cache_size(mut self, max_cache_size: u64) -> Self {
            self.cfg.store.max_cache_size = max_cache_size;
            self
        }

        fn build(self) -> Result<super::IO, StoreError> {
            let config = Arc::new(self.cfg);

            // Build wal offset manager
            let wal_offset_manager = Arc::new(WalOffsetManager::new());

            // Build index driver
            let indexer = Arc::new(IndexDriver::new(
                &config
                    .store
                    .path
                    .metadata_path()
                    .as_path()
                    .to_str()
                    .unwrap(),
                Arc::clone(&wal_offset_manager) as Arc<dyn MinOffset>,
                128,
            )?);

            super::IO::new(&config, indexer)
        }
    }

    fn create_default_io(store_dir: &Path) -> Result<super::IO, StoreError> {
        IOBuilder::new(store_dir.to_path_buf()).build()
    }

    fn create_small_io(store_dir: &Path) -> Result<super::IO, StoreError> {
        IOBuilder::new(store_dir.to_path_buf())
            .segment_size(1024 * 1024)
            .max_cache_size(1024 * 1024)
            .build()
    }

    fn create_and_run_io<IoCreator>(
        io_creator: IoCreator,
    ) -> Result<(JoinHandle<()>, Sender<IoTask>), Box<dyn Error>>
    where
        IoCreator: Fn(&Path) -> Result<super::IO, StoreError> + Send + 'static,
    {
        let (tx, rx) = oneshot::channel();
        let (recovery_completion_tx, recovery_completion_rx) = oneshot::channel();

        let handle = std::thread::spawn(move || {
            let store_dir = random_store_dir().unwrap();
            let store_dir = store_dir.as_path();
            let _store_dir_guard = test_util::DirectoryRemovalGuard::new(store_dir);
            let mut io = io_creator(store_dir).unwrap();

            let sender = io
                .sender
                .take()
                .ok_or(StoreError::Configuration("IO channel".to_owned()))
                .unwrap();
            let _ = tx.send(sender);
            let io = RefCell::new(io);

            let _ = super::IO::run(io, recovery_completion_tx);
            println!("Module io stopped");
        });

        if let Err(_) = recovery_completion_rx.blocking_recv() {
            panic!("Failed to await recovery completion");
        }
        let sender: Sender<IoTask> = rx
            .blocking_recv()
            .map_err(|_| StoreError::Internal("Internal error".to_owned()))?;

        return Ok((handle, sender));
    }

    fn random_store_dir() -> Result<PathBuf, StoreError> {
        test_util::create_random_path().map_err(|e| StoreError::IO(e))
    }

    #[test]
    fn test_receive_io_tasks() -> Result<(), StoreError> {
        test_util::try_init_log();
        let store_dir = random_store_dir()?;
        let store_dir = store_dir.as_path();
        let _store_dir_guard = test_util::DirectoryRemovalGuard::new(store_dir);
        let mut io = create_default_io(store_dir)?;
        let sender = io.sender.take().unwrap();
        let mut buffer = BytesMut::with_capacity(128);
        buffer.resize(128, 65);
        let buffer = buffer.freeze();

        // Send IoTask to channel
        (0..16)
            .into_iter()
            .flat_map(|_| {
                let (tx, _rx) = oneshot::channel();
                let io_task = IoTask::Write(WriteTask {
                    stream_id: 0,
                    range: 0,
                    offset: 0,
                    len: 1,
                    buffer: buffer.clone(),
                    observer: tx,
                    written_len: None,
                });
                sender.send(io_task)
            })
            .count();

        io.receive_io_tasks();
        assert_eq!(16, io.pending_data_tasks.len());
        io.pending_data_tasks.clear();

        drop(sender);

        // Mock that some in-flight IO tasks were reaped
        io.inflight = 0;

        io.receive_io_tasks();

        assert_eq!(true, io.pending_data_tasks.is_empty());
        assert_eq!(true, io.channel_disconnected);

        Ok(())
    }

    #[inline]
    fn align_up(n: u64, alignment: u64) -> u64 {
        (n + alignment - 1) / alignment * alignment
    }

    #[test]
    fn test_reserve_write_buffers() -> Result<(), Box<dyn Error>> {
        test_util::try_init_log();
        let store_path = test_util::create_random_path()?;
        let _guard = test_util::DirectoryRemovalGuard::new(store_path.as_path());

        let file_size = 1024 * 1024;
        let mut io = IOBuilder::new(store_path.clone())
            .segment_size(file_size)
            .max_cache_size(file_size)
            .build()?;

        let buf_writer = unsafe { &mut *io.buf_writer.get() };
        let alignment = buf_writer.alignment as u64;
        buf_writer.reserve_to(4096, file_size as usize)?;
        assert_eq!(0, buf_writer.cursor);

        let mut bytes = BytesMut::with_capacity(1016);
        bytes.resize(1016, 0);
        let bytes = bytes.freeze();

        for _ in 0..4 {
            let (tx, _rx) = oneshot::channel();
            let write_task = WriteTask {
                stream_id: 0,
                range: 0,
                offset: 0,
                len: 1,
                buffer: bytes.clone(),
                observer: tx,
                written_len: None,
            };
            let task = IoTask::Write(write_task);
            io.pending_data_tasks.push_back(task);
        }
        io.reserve_write_buffers()?;
        assert_eq!(
            align_up(4096, alignment),
            buf_writer.max_allocated_wal_offset()
        );
        assert_eq!(0, buf_writer.cursor);

        for _ in 0..4 {
            let (tx, _rx) = oneshot::channel();
            let write_task = WriteTask {
                stream_id: 0,
                range: 0,
                offset: 0,
                len: 1,
                buffer: bytes.clone(),
                observer: tx,
                written_len: None,
            };
            let task = IoTask::Write(write_task);
            io.pending_data_tasks.push_back(task);
        }
        io.reserve_write_buffers()?;
        assert_eq!(
            align_up(8192, alignment),
            buf_writer.max_allocated_wal_offset()
        );
        assert_eq!(0, buf_writer.cursor);

        for _ in 0..1024 {
            let (tx, _rx) = oneshot::channel();
            let write_task = WriteTask {
                stream_id: 0,
                range: 0,
                offset: 0,
                len: 1,
                buffer: bytes.clone(),
                observer: tx,
                written_len: None,
            };
            let task = IoTask::Write(write_task);
            io.pending_data_tasks.push_back(task);
        }
        io.reserve_write_buffers()?;

        // align((4 + 4 + 1024) * 1024 + padding(1024), 4096)
        assert_eq!(
            align_up((4 + 4 + 1024) * 1024 + 1024, alignment),
            buf_writer.max_allocated_wal_offset()
        );
        assert_eq!(0, buf_writer.cursor);

        Ok(())
    }

    #[test]
    fn test_build_sqe() -> Result<(), StoreError> {
        test_util::try_init_log();
        let store_dir = random_store_dir()?;
        let store_dir = store_dir.as_path();
        let _store_dir_guard = test_util::DirectoryRemovalGuard::new(store_dir);
        let mut io = create_default_io(store_dir)?;

        io.wal.open_segment_directly()?;

        let len = 4088;
        let mut buffer = BytesMut::with_capacity(len);
        buffer.resize(len, 65);
        let buffer = buffer.freeze();

        // Send IoTask to channel
        (0..16)
            .into_iter()
            .map(|n| {
                let (tx, _rx) = oneshot::channel();
                IoTask::Write(WriteTask {
                    stream_id: 0,
                    range: 0,
                    offset: n,
                    len: 1,
                    buffer: buffer.clone(),
                    observer: tx,
                    written_len: None,
                })
            })
            .for_each(|io_task| {
                io.pending_data_tasks.push_back(io_task);
            });

        let mut entries = Vec::new();

        io.build_sqe(&mut entries);
        assert!(!entries.is_empty());
        Ok(())
    }

    #[test]
    fn test_run_basic() -> Result<(), Box<dyn Error>> {
        test_util::try_init_log();
        let (handle, sender) = create_and_run_io(create_default_io)?;
        let records: Vec<_> = (0..16)
            .into_iter()
            .map(|_| {
                let mut rng = rand::thread_rng();
                let random_size = rand::Rng::gen_range(&mut rng, 2..128);
                create_random_bytes(random_size)
            })
            .collect();

        send_and_receive_with_records(sender.clone(), 0, 0, records);

        drop(sender);
        handle.join().map_err(|_| StoreError::AllocLogSegment)?;
        Ok(())
    }

    #[test]
    fn test_run_on_small_io() -> Result<(), Box<dyn Error>> {
        test_util::try_init_log();
        let (handle, sender) = create_and_run_io(create_small_io)?;
        // Will cost at least 4K * 1024 = 4M bytes, which means at least 4 segments will be allocated
        // And the cache reclaim will be triggered since a small io only has 1M cache
        let records: Vec<_> = (0..4096)
            .into_iter()
            .map(|_| {
                let mut rng = rand::thread_rng();
                let random_size = rand::Rng::gen_range(&mut rng, 4096..8192);
                create_random_bytes(random_size)
            })
            .collect();
        send_and_receive_with_records(sender.clone(), 0, 0, records);

        drop(sender);
        handle.join().map_err(|_| StoreError::AllocLogSegment)?;
        Ok(())
    }

    #[test]
    fn test_send_and_receive_half_page() -> Result<(), Box<dyn Error>> {
        test_util::try_init_log();
        let (handle, sender) = create_and_run_io(create_default_io)?;
        send_and_receive_with_records(sender.clone(), 0, 0, vec![create_random_bytes(199)]);
        drop(sender);
        handle.join().map_err(|_| StoreError::AllocLogSegment)?;
        Ok(())
    }

    #[test]
    fn test_recover() -> Result<(), Box<dyn Error>> {
        test_util::try_init_log();
        let (tx, rx) = oneshot::channel();
        let store_dir = random_store_dir().unwrap();
        // Delete the directory after restart and verification of `recover`.
        let _store_dir_guard = test_util::DirectoryRemovalGuard::new(store_dir.as_path());
        let store_path = store_dir.as_os_str().to_os_string();

        let (recovery_completion_tx, recovery_completion_rx) = oneshot::channel();

        let handle = std::thread::spawn(move || {
            let store_dir = Path::new(&store_path);
            let mut io = create_default_io(store_dir).unwrap();
            let sender = io
                .sender
                .take()
                .ok_or(StoreError::Configuration("IO channel".to_owned()))
                .unwrap();
            let _ = tx.send(sender);
            let io = RefCell::new(io);

            let _ = super::IO::run(io, recovery_completion_tx);
            println!("Module io stopped");
        });

        if let Err(_) = recovery_completion_rx.blocking_recv() {
            panic!("Failed to wait store recovery completion");
        }

        let sender = rx
            .blocking_recv()
            .map_err(|_| StoreError::Internal("Internal error".to_owned()))?;

        let mut payload = BytesMut::with_capacity(1024);
        payload.resize(1024, 65);

        let record_batch = RecordBatchBuilder::default()
            .with_stream_id(0)
            .with_range_index(0)
            .with_base_offset(0)
            .with_last_offset_delta(1)
            .with_payload(payload.freeze())
            .build()?;
        let record_group = Into::<FlatRecordBatch>::into(record_batch);
        let (bufs, total) = record_group.encode();
        let mut buffer = BytesMut::new();
        for buf in &bufs {
            buffer.extend_from_slice(buf);
        }
        let buffer = buffer.freeze();
        assert_eq!(buffer.len(), total as usize);

        let records: Vec<_> = (0..16).into_iter().map(|_| buffer.clone()).collect();

        send_and_receive_with_records(sender.clone(), 0, 0, records);

        drop(sender);
        handle.join().map_err(|_| StoreError::AllocLogSegment)?;

        {
            let mut io = create_default_io(store_dir.as_path()).unwrap();
            io.load()?;
            let pos = io.indexer.get_wal_checkpoint()?;
            io.recover(pos)?;
            assert!(!io.buf_writer.get_mut().take().is_empty());
        }

        Ok(())
    }

    #[test]
    fn test_multiple_run_with_random_bytes() -> Result<(), Box<dyn Error>> {
        test_util::try_init_log();
        let (handle, sender) = create_and_run_io(create_small_io)?;
        let mut records: Vec<_> = vec![];
        records.push(create_random_bytes(4096 - 8));
        records.push(create_random_bytes(4096 - 8));
        records.push(create_random_bytes(4096 - 8));
        records.push(create_random_bytes(4096));
        records.push(create_random_bytes(4096 - 8 - 8));
        records.push(create_random_bytes(4096 - 8));
        records.push(create_random_bytes(4096 - 8 - 24));
        send_and_receive_with_records(sender.clone(), 0, 0, records);
        let mut records: Vec<_> = vec![];
        records.push(create_random_bytes(4096 - 8));
        send_and_receive_with_records(sender.clone(), 0, 7, records);
        let mut records: Vec<_> = vec![];
        records.push(create_random_bytes(4096 - 8));
        records.push(create_random_bytes(4096 - 8));
        records.push(create_random_bytes(4096));
        records.push(create_random_bytes(4096 - 8 - 8));
        records.push(create_random_bytes(4096 - 8));
        records.push(create_random_bytes(4096 - 8 - 24));
        send_and_receive_with_records(sender.clone(), 0, 8, records);
        drop(sender);
        handle.join().map_err(|_| StoreError::AllocLogSegment)?;
        Ok(())
    }

    #[test]
    fn test_run_with_random_bytes() -> Result<(), Box<dyn Error>> {
        test_util::try_init_log();
        let (handle, sender) = create_and_run_io(create_small_io)?;
        let mut records: Vec<_> = vec![];
        let count = 1000;
        (0..count).into_iter().for_each(|_| {
            let mut rng = rand::thread_rng();
            let random_size = rand::Rng::gen_range(&mut rng, 1000..9000);
            records.push(create_random_bytes(random_size));
        });
        send_and_receive_with_records(sender.clone(), 0, 0, records);

        drop(sender);
        handle.join().map_err(|_| StoreError::AllocLogSegment)?;
        Ok(())
    }

    fn send_and_receive_with_records(
        sender: crossbeam::channel::Sender<IoTask>,
        stream_id: i64,
        start_offset: i64,
        records: Vec<bytes::Bytes>,
    ) {
        let mut receivers = vec![];
        records
            .iter()
            .enumerate()
            .map(|(i, buf)| {
                let (tx, rx) = oneshot::channel();
                receivers.push(rx);
                IoTask::Write(WriteTask {
                    stream_id: stream_id,
                    range: 0,
                    offset: start_offset + i as i64,
                    len: 1,
                    buffer: buf.clone(),
                    observer: tx,
                    written_len: None,
                })
            })
            .for_each(|task| {
                sender.send(task).unwrap();
            });

        let mut results = Vec::new();
        for receiver in receivers {
            let res = receiver.blocking_recv().unwrap().unwrap();
            trace!(
                "{{ stream-id: {}, offset: {} , wal_offset: {}}}",
                res.stream_id,
                res.offset,
                res.wal_offset
            );
            results.push(res);
        }

        // Read the data from store
        let mut receivers = vec![];
        let mut res_map = std::collections::HashMap::new();
        results
            .iter()
            .map(|res| {
                let (tx, rx) = oneshot::channel();
                receivers.push(rx);
                let idx = (res.offset - start_offset) as usize;
                res_map.insert(res.wal_offset, &records[idx]);
                IoTask::Read(ReadTask {
                    stream_id: res.stream_id,
                    wal_offset: res.wal_offset,
                    len: (records[idx].len() + 8) as u32,
                    observer: tx,
                })
            })
            .for_each(|task| {
                sender.send(task).unwrap();
            });

        for receiver in receivers {
            let res = receiver.blocking_recv().unwrap().unwrap();
            trace!(
                "{{Read result is stream-id: {}, wal_offset: {}, payload length: {}}}",
                res.stream_id,
                res.wal_offset,
                res.payload[0].len()
            );
            // Assert the payload is equal to the write buffer
            // trace the length

            let mut res_payload = BytesMut::new();
            res.payload.iter().for_each(|r| {
                res_payload.extend_from_slice(&r[..]);
            });
            assert_eq!(
                *res_map[&(res.wal_offset as u64)],
                res_payload.freeze()[(crate::RECORD_PREFIX_LENGTH as usize)..]
            );
        }
    }

    fn create_random_bytes(capacity: usize) -> bytes::Bytes {
        const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ\
                                abcdefghijklmnopqrstuvwxyz\
                                0123456789)(*&^%$#@!~";
        let mut rng = rand::thread_rng();

        let random_string: String = (0..capacity)
            .map(|_| {
                let idx = rand::Rng::gen_range(&mut rng, 0..CHARSET.len());
                CHARSET[idx] as char
            })
            .collect();
        let buffer = random_string.as_bytes();
        BytesMut::from(buffer).freeze()
    }

    #[test]
    fn test_err() {
        let error = io::Error::from_raw_os_error(11);
        assert_eq!(error.kind(), io::ErrorKind::WouldBlock);
    }
}
