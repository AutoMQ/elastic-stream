use crate::error::{AppendError, StoreError};
use crate::index::driver::IndexDriver;
use crate::index::record_handle::RecordHandle;
use crate::io::buf::{AlignedBufReader, AlignedBufWriter};
use crate::io::context::Context;
use crate::io::options::Options;
use crate::io::segment::Status;
use crate::io::task::IoTask;
use crate::io::task::WriteTask;
use crate::io::wal::Wal;
use crate::io::write_window::WriteWindow;
use crate::ops::append::AppendResult;
use crate::BufSlice;

use crossbeam::channel::{Receiver, Sender, TryRecvError};
use io_uring::register;
use io_uring::{opcode, squeue, types};
use slog::{error, info, trace, warn, Logger};
use std::borrow::{Borrow, BorrowMut};
use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;
use std::{
    cell::{RefCell, UnsafeCell},
    collections::{HashMap, VecDeque},
    os::fd::AsRawFd,
};

use super::block_cache::{EntryRange, MergeRange};
use super::buf::AlignedBuf;
use super::task::SingleFetchResult;
use super::ReadTask;

pub(crate) struct IO {
    options: Options,

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

    /// Logger instance.
    log: Logger,

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
    barrier: HashSet<u64>,

    /// Block the concurrent write IOs to the same page.
    /// The uring instance doesn't provide the ordering guarantee for the IOs,
    /// so we use this mechanism to avoid memory corruption.
    blocked: HashMap<u64, squeue::Entry>,

    /// Provide index service for building read index, shared with the upper store layer.
    indexer: Arc<IndexDriver>,
}

/// Check if required opcodes are supported by the host operation system.
///
/// # Arguments
/// * `probe` - Probe result, which contains all features that are supported.
///
fn check_io_uring(probe: &register::Probe) -> Result<(), StoreError> {
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

impl IO {
    /// Create new `IO` instance.
    ///
    /// Behavior of the IO instance can be tuned through `Options`.
    pub(crate) fn new(
        options: &mut Options,
        indexer: Arc<IndexDriver>,
        log: Logger,
    ) -> Result<Self, StoreError> {
        if options.wal_paths.is_empty() {
            return Err(StoreError::Configuration("WAL path required".to_owned()));
        }

        if options.metadata_path.is_empty() {
            return Err(StoreError::Configuration(
                "Metadata path required".to_owned(),
            ));
        }

        // Ensure WAL directories exists.
        for dir in &options.wal_paths {
            util::mkdirs_if_missing(&dir.path)?;
        }

        let control_ring = io_uring::IoUring::builder().dontfork().build(32).map_err(|e| {
            error!(log, "Failed to build I/O Uring instance for write-ahead-log segment file management: {:#?}", e);
            StoreError::IoUring
        })?;

        let data_ring = io_uring::IoUring::builder()
            .dontfork()
            .setup_iopoll()
            .setup_sqpoll(options.sqpoll_idle_ms)
            .setup_sqpoll_cpu(options.sqpoll_cpu)
            .setup_r_disabled()
            .build(options.io_depth)
            .map_err(|e| {
                error!(log, "Failed to build polling I/O Uring instance: {:#?}", e);
                StoreError::IoUring
            })?;

        let mut probe = register::Probe::new();

        let submitter = data_ring.submitter();
        submitter.register_iowq_max_workers(&mut options.max_workers)?;
        submitter.register_probe(&mut probe)?;
        submitter.register_enable_rings()?;

        check_io_uring(&probe)?;

        trace!(log, "Polling I/O Uring instance created");

        let (sender, receiver) = crossbeam::channel::unbounded();

        Ok(Self {
            options: options.clone(),
            data_ring,
            sender: Some(sender),
            receiver,
            write_window: WriteWindow::new(log.clone(), 0),
            buf_writer: UnsafeCell::new(AlignedBufWriter::new(log.clone(), 0, options.alignment)),
            wal: Wal::new(
                options.clone().wal_paths,
                control_ring,
                options.file_size,
                log.clone(),
            ),
            log,
            channel_disconnected: false,
            inflight: 0,
            pending_data_tasks: VecDeque::new(),
            inflight_write_tasks: BTreeMap::new(),
            inflight_read_tasks: BTreeMap::new(),
            barrier: HashSet::new(),
            blocked: HashMap::new(),
            indexer,
        })
    }

    fn load(&mut self) -> Result<(), StoreError> {
        self.wal.load_from_paths()?;
        Ok(())
    }

    fn recover(&mut self, offset: u64) -> Result<(), StoreError> {
        let pos = self.wal.recover(offset)?;

        // Reset offset of write buffer
        self.buf_writer.get_mut().wal_offset(pos);

        // Reset committed WAL offset
        self.write_window.reset_committed(pos);

        Ok(())
    }

    fn validate_io_task(io_task: &mut IoTask, log: &Logger) -> bool {
        if let IoTask::Write(ref mut task) = io_task {
            if task.buffer.is_empty() {
                warn!(log, "WriteTask buffer length is 0");
                return false;
            }
        }
        return true;
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

    fn receive_io_tasks(&mut self) -> usize {
        let mut received = 0;
        let io_depth = self.data_ring.params().sq_entries() as usize;
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

            // if the log segment file is full, break loop.

            if self.inflight + received + self.pending_data_tasks.len() == 0 {
                // Block the thread until at least one IO task arrives
                match self.receiver.recv() {
                    Ok(mut io_task) => {
                        if !IO::validate_io_task(&mut io_task, &self.log) {
                            IO::on_bad_request(io_task);
                            continue;
                        }
                        self.pending_data_tasks.push_back(io_task);
                        received += 1;
                    }
                    Err(_e) => {
                        info!(self.log, "Channel for submitting IO task disconnected");
                        self.channel_disconnected = true;
                        break received;
                    }
                }
            } else {
                match self.receiver.try_recv() {
                    Ok(mut io_task) => {
                        if !IO::validate_io_task(&mut io_task, &self.log) {
                            IO::on_bad_request(io_task);
                            continue;
                        }
                        self.pending_data_tasks.push_back(io_task);
                        received += 1;
                    }
                    Err(TryRecvError::Empty) => {
                        break received;
                    }
                    Err(TryRecvError::Disconnected) => {
                        info!(self.log, "Channel for submitting IO task disconnected");
                        self.channel_disconnected = true;
                        break received;
                    }
                }
            }
        }
    }

    fn calculate_write_buffers(&self) -> Vec<usize> {
        let mut requirement: VecDeque<_> = self
            .pending_data_tasks
            .iter()
            .map(|task| match task {
                IoTask::Write(task) => {
                    debug_assert!(task.buffer.len() > 0);
                    task.total_len() as usize
                }
                _ => 0,
            })
            .filter(|n| *n > 0)
            .collect();
        self.wal.calculate_write_buffers(&mut requirement)
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
                    self.log.clone(),
                    range.wal_offset,
                    range.len as usize,
                    self.options.alignment as u64,
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

                    if let Some(sd) = segment.sd.as_ref() {
                        // The pointer will be set into user_data of uring.
                        // When the uring io completes, the pointer will be used to retrieve the `Context`.
                        let context = Context::read_ctx(
                            opcode::Read::CODE,
                            Arc::new(buf),
                            read_offset,
                            read_len,
                        );

                        let sqe = opcode::Read::new(types::Fd(sd.fd), ptr, read_len)
                            .offset(read_offset as i64)
                            .build()
                            .user_data(context as u64);

                        entries.push(sqe);

                        // Trace the submit read IO
                        trace!(self.log, "Submit read IO";
                            "offset" => read_offset,
                            "len" => read_len,
                            "segment" => segment.wal_offset,
                        );
                        // Add the ongoing entries to the block cache
                        segment.block_cache.add_loading_entry(*range);
                    }
                }
            });
        });
    }

    fn build_write_sqe(&mut self, entries: &mut Vec<squeue::Entry>) {
        // Add previously blocked entries.
        self.blocked
            .drain_filter(|offset, _entry| !self.barrier.contains(offset))
            .for_each(|(_, entry)| {
                entries.push(entry);
            });

        let writer = self.buf_writer.get_mut();
        writer
            .take()
            .into_iter()
            .flat_map(|buf| {
                let ptr = buf.as_ptr();
                if let Some(segment) = self.wal.segment_file_of(buf.wal_offset) {
                    debug_assert_eq!(Status::ReadWrite, segment.status);
                    if let Some(sd) = segment.sd.as_ref() {
                        debug_assert!(buf.wal_offset >= segment.wal_offset);
                        debug_assert!(
                            buf.wal_offset + buf.capacity as u64
                                <= segment.wal_offset + segment.size
                        );
                        let file_offset = buf.wal_offset - segment.wal_offset;

                        // Track write requests
                        self.write_window.add(buf.wal_offset, buf.limit() as u32)?;

                        let mut io_blocked = false;
                        // Check barrier
                        if self.barrier.contains(&buf.wal_offset) {
                            // Submit SQE to io_uring when the blocking IO task completed.
                            io_blocked = true;
                        } else {
                            // Insert barrier, blocking future write to this aligned block issued to `io_uring` until `sqe` is reaped.
                            if buf.partial() {
                                self.barrier.insert(buf.wal_offset);
                            }
                        }

                        let buf_offset = buf.wal_offset;
                        let buf_len = buf.capacity as u32;

                        // The pointer will be set into user_data of uring.
                        // When the uring io completes, the pointer will be used to retrieve the `Context`.
                        let context = Context::write_ctx(opcode::Write::CODE, buf);

                        // Note we have to write the whole page even if the page is partially filled.
                        let sqe = opcode::Write::new(types::Fd(sd.fd), ptr, buf_len)
                            .offset64(file_offset as libc::off_t)
                            .build()
                            .user_data(context as u64);

                        if io_blocked {
                            self.blocked.insert(buf_offset, sqe);
                        } else {
                            entries.push(sqe);
                        }
                    } else {
                        // fatal errors
                        let msg = format!("Segment {} should be open and with valid FD", segment);
                        error!(self.log, "{}", msg);
                        panic!("{}", msg);
                    }
                } else {
                    error!(self.log, "");
                }
                Ok::<(), super::WriteWindowError>(())
            })
            .count();
    }

    fn build_sqe(&mut self, entries: &mut Vec<squeue::Entry>) {
        let log = self.log.clone();
        let alignment = self.options.alignment;
        let buf_list = self.calculate_write_buffers();
        let left = self.buf_writer.get_mut().remaining();

        buf_list
            .iter()
            .enumerate()
            .flat_map(|(idx, n)| {
                if 0 == idx {
                    if *n > left {
                        self.buf_writer.get_mut().reserve(*n - left)
                    } else {
                        Ok(())
                    }
                } else {
                    self.buf_writer.get_mut().reserve(*n)
                }
            })
            .count();

        let mut need_write = false;

        // The missed entries that are not in the cache, group by the start offset of segments.
        let mut missed_entries: HashMap<u64, Vec<EntryRange>> = HashMap::new();

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

                    if let Some(sd) = segment.sd.as_ref() {
                        let range_to_read =
                            EntryRange::new(task.wal_offset, task.len, alignment as u64);

                        // Try to read from the buffer first.
                        let buf_res = segment.block_cache.try_get_entries(range_to_read);
                        let mut move_to_inflight = false;
                        match buf_res {
                            Ok(Some(buf_v)) => {
                                // Complete the task directly.
                                self.complete_read_task(task, buf_v);
                                continue;
                            }
                            Ok(None) => {
                                // The dependent entries are inflight, move to inflight list directly.
                                move_to_inflight = true;
                            }
                            Err(mut entries) => {
                                // Cache miss, there are some entries should be read from disk.
                                missed_entries
                                    .entry(segment.wal_offset)
                                    .or_default()
                                    .append(&mut entries);

                                move_to_inflight = true;
                            }
                        }
                        if move_to_inflight {
                            self.inflight_read_tasks
                                .entry(segment.wal_offset)
                                .or_default()
                                .push_back(task);
                        }
                    } else {
                        self.pending_data_tasks.push_front(IoTask::Read(task));
                    }
                }
                IoTask::Write(mut task) => {
                    let writer = unsafe { &mut *self.buf_writer.get() };
                    loop {
                        if let Some(segment) = self.wal.segment_file_of(writer.wal_offset) {
                            if !segment.writable() {
                                trace!(
                                    log,
                                    "WAL Segment {}. Yield dispatching IO to block layer",
                                    segment
                                );
                                self.pending_data_tasks.push_front(IoTask::Write(task));
                                break 'task_loop;
                            }

                            if let Some(_sd) = segment.sd.as_ref() {
                                let payload_length = task.buffer.len();
                                if !segment.can_hold(payload_length as u64) {
                                    if let Ok(pos) = segment.append_footer(writer) {
                                        trace!(self.log, "Write position of WAL after padding segment footer: {}", pos);
                                        need_write = true;
                                    }
                                    // Switch to a new log segment
                                    continue;
                                }
                                let pre_written = segment.written;
                                if let Ok(pos) = segment.append_record(writer, &task.buffer[..]) {
                                    trace!(
                                        self.log,
                                        "Write position of WAL after appending record: {}",
                                        pos
                                    );
                                    // Set the written len of the task
                                    task.written_len = Some((segment.written - pre_written) as u32);
                                    self.inflight_write_tasks.insert(pos, task);
                                    need_write = true;
                                }
                                break;
                            } else {
                                error!(log, "LogSegmentFile {} with read_write status does not have valid FD", segment);
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

        self.build_read_sqe(entries, missed_entries);

        if need_write {
            self.build_write_sqe(entries);
        }
    }

    fn await_data_task_completion(&self, mut wanted: usize) {
        if self.inflight == 0 {
            trace!(
                self.log,
                "No inflight data task. Skip `await_data_task_completion`"
            );
            return;
        }

        trace!(
            self.log,
            "Waiting for at least {}/{} CQE(s) to reap",
            wanted,
            self.inflight
        );

        if wanted > self.inflight {
            wanted = self.inflight;
        }

        let now = std::time::Instant::now();
        match self.data_ring.submit_and_wait(wanted) {
            Ok(_reaped) => {
                trace!(
                    self.log,
                    "io_uring_enter waited {}us to reap completed data CQE(s)",
                    now.elapsed().as_micros()
                );
            }
            Err(e) => {
                error!(self.log, "io_uring_enter got an error: {:?}", e);

                // Fatal errors, crash the process and let watchdog to restart.
                panic!("io_uring_enter returns error {:?}", e);
            }
        }
    }

    fn reap_data_tasks(&mut self) {
        if 0 == self.inflight {
            return;
        }
        let committed = self.write_window.committed;
        let mut cache_entries = vec![];
        let mut effected_segments = HashSet::new();
        {
            let mut completion = self.data_ring.completion();
            let mut count = 0;
            loop {
                for cqe in completion.by_ref() {
                    count += 1;
                    let tag = cqe.user_data();

                    let ptr = tag as *mut Context;

                    // Safety:
                    // It's safe to convert tag ptr back to Box<Context> as the memory pointed by ptr
                    // is allocated by Box itself, hence, there will no alignment issue at all.
                    let mut context = unsafe { Box::from_raw(ptr) };

                    // Remove barrier
                    self.barrier.remove(&context.buf.wal_offset);

                    if let Err(e) = on_complete(
                        &mut self.write_window,
                        &mut context,
                        cqe.result(),
                        &self.log,
                    ) {
                        if let StoreError::System(errno) = e {
                            error!(
                                self.log,
                                "io_uring opcode `{}` failed. errno: `{}`", context.opcode, errno
                            );

                            // TODO: Check if the errno is recoverable...
                        }
                    } else {
                        // Add block cache
                        cache_entries.push(Arc::clone(&context.buf));

                        // Cache the completed read context
                        if opcode::Read::CODE == context.opcode {
                            match context.wal_offset {
                                Some(wal_offset) => {
                                    if let Some(segment) = self.wal.segment_file_of(wal_offset) {
                                        effected_segments.insert(wal_offset);
                                    }
                                }
                                None => {
                                    error!(self.log, "Read context without wal_offset");
                                }
                            }
                        }
                    }
                }
                // This will flush any entries consumed in this iterator and will make available new entries in the queue
                // if the kernel has produced some entries in the meantime.
                completion.sync();

                if completion.is_empty() {
                    break;
                }
            }
            debug_assert!(self.inflight >= count);
            self.inflight -= count;
            trace!(self.log, "Reaped {} data CQE(s)", count);
        }

        // Add to block cache
        for buf in cache_entries {
            if let Some(segment) = self.wal.segment_file_of(buf.wal_offset) {
                segment.block_cache.add_entry(buf);
            }
        }

        self.complete_read_tasks(effected_segments);

        if self.write_window.committed > committed {
            self.complete_write_tasks();
        }
    }

    fn acknowledge_to_observer(&mut self, wal_offset: u64, task: WriteTask) {
        let append_result = AppendResult {
            stream_id: task.stream_id,
            offset: task.offset,
            wal_offset,
        };
        trace!(
            self.log,
            "Ack `WriteTask` {{ stream-id: {}, offset: {} }}",
            task.stream_id,
            task.offset
        );
        if let Err(e) = task.observer.send(Ok(append_result)) {
            error!(self.log, "Failed to propagate AppendResult `{:?}`", e);
        }
    }
    fn build_read_index(&mut self, wal_offset: u64, written_len: u32, task: &WriteTask) {
        let handle = RecordHandle {
            hash: 0, // TODO: set hash for record handle
            len: written_len,
            wal_offset,
        };
        self.indexer
            .index(task.stream_id, task.offset as u64, handle);
    }

    fn complete_read_tasks(&mut self, effected_segments: HashSet<u64>) {
        effected_segments.into_iter().for_each(|wal_offset| {
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
                            self.options.alignment as u64,
                        );

                        match segment.block_cache.try_get_entries(range) {
                            Ok(Some(buf)) => {
                                self.complete_read_task(task, buf);
                                continue;
                            }
                            Ok(None) => {
                                // Wait for the next IO to complete this task
                            }
                            Err(_) => {
                                // The error means we need issue some read IOs for this task,
                                // but it's impossible for inflight read task since we already handle this situation in `submit_read_task`
                                error!(
                                    self.log,
                                    "Unexpected error when get entries from block cache"
                                );
                            }
                        }
                    }
                    // Send back the task since it's not completed
                    send_back.push(task);
                }
                inflight_read_tasks.extend(send_back);

                self.inflight_read_tasks
                    .insert(wal_offset, inflight_read_tasks);
            }
        });
    }

    fn complete_read_task(&mut self, read_task: ReadTask, read_buf_v: Vec<Arc<AlignedBuf>>) {
        // Completes the read task
        // Construct the SingleFetchResult from the read buffer
        // We need narrow the first and the last AlignedBuf to the actual read range
        let mut slice_v = vec![];
        read_buf_v.into_iter().for_each(|buf| {
            trace!(
                self.log,
                "Read buffer: offset: {}, len: {}",
                buf.wal_offset,
                buf.limit()
            );

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
            self.log,
            "Completes read task for stream {} at WAL offset {}, return {} bytes",
            fetch_result.stream_id,
            fetch_result.wal_offset,
            read_task.len,
        );

        if let Err(e) = read_task.observer.send(Ok(fetch_result)) {
            error!(self.log, "Failed to send read result to observer: {:?}", e);
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
                    error!(self.log, "No written length for `WriteTask`");
                }
            }
        }
    }

    fn should_quit(&self) -> bool {
        0 == self.inflight
            && self.pending_data_tasks.is_empty()
            && self.inflight_write_tasks.is_empty()
            && self.inflight_read_tasks.is_empty()
            && self.wal.control_task_num() == 0
            && self.channel_disconnected
    }

    fn submit_data_tasks(&mut self, entries: &Vec<squeue::Entry>) -> Result<(), StoreError> {
        // Submit io_uring entries into submission queue.
        trace!(
            self.log,
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
                        info!(
                            self.log,
                            "Failed to push SQE entries into submission queue: {:?}", e
                        );
                        StoreError::IoUring
                    })?
            };
            self.inflight += cnt;
            trace!(self.log, "Pushed {} SQEs into submission queue", cnt);
        }
        Ok(())
    }

    pub(crate) fn run(io: RefCell<IO>) -> Result<(), StoreError> {
        let log = io.borrow().log.clone();
        io.borrow_mut().load()?;
        let pos = io.borrow().indexer.get_wal_checkpoint()?;
        io.borrow_mut().recover(pos)?;

        let min_preallocated_segment_files = io.borrow().options.min_preallocated_segment_files;

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
                trace!(log, "Received {} IO requests from channel", cnt);

                // Convert IO tasks into io_uring entries
                io_mut.build_sqe(&mut entries);
            }

            if !entries.is_empty() {
                io.borrow_mut().submit_data_tasks(&entries)?;
            } else {
                let io_borrow = io.borrow();
                if !io_borrow.should_quit() {
                    if !io_borrow.pending_data_tasks.is_empty() {
                        io_borrow.wal.await_control_task_completion();
                    }
                } else {
                    info!(
                        log,
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
        info!(log, "Main loop quit");
        Ok(())
    }
}

/// Process reaped IO completion.
///
/// # Arguments
///
/// * `state` - Operation state, including original IO request, buffer and response observer.
/// * `result` - Result code, exactly same to system call return value.
/// * `log` - Logger instance.
fn on_complete(
    write_window: &mut WriteWindow,
    context: &mut Context,
    result: i32,
    log: &Logger,
) -> Result<(), StoreError> {
    match context.opcode {
        opcode::Write::CODE => {
            if result < 0 {
                error!(
                    log,
                    "Write to WAL range `[{}, {})` failed",
                    context.buf.wal_offset,
                    context.buf.capacity
                );
                return Err(StoreError::System(-result));
            } else {
                write_window
                    .commit(context.buf.wal_offset, context.buf.limit() as u32)
                    .map_err(|_e| StoreError::WriteWindow)?;
            }
            Ok(())
        }
        opcode::Read::CODE => {
            if result < 0 {
                error!(
                    log,
                    "Read from WAL range `[{}, {})` failed",
                    context.wal_offset.unwrap_or_default(),
                    context.len.unwrap_or_default()
                );
                return Err(StoreError::System(-result));
            } else {
                if let (Some(offset), Some(len)) = (context.wal_offset, context.len) {
                    if result != len as i32 {
                        error!(
                            log,
                            "Read {} bytes from WAL range `[{}, {})`, but {} bytes expected",
                            result,
                            offset,
                            len,
                            len
                        );
                        return Err(StoreError::InsufficientData);
                    }

                    context.buf.increase_written(result as usize);
                    return Ok(());
                }
            }
            // This should never happen, because we have checked the request before.
            return Err(StoreError::Internal("Invalid read request".to_string()));
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
    use bytes::{Bytes, BytesMut};
    use slog::trace;
    use std::cell::RefCell;
    use std::error::Error;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::Arc;
    use tokio::sync::oneshot;

    use crate::option::WalPath;

    fn create_io(store_dir: &Path) -> Result<super::IO, StoreError> {
        let mut options = super::Options::default();
        let logger = test_util::terminal_logger();
        let store_path = store_dir.join("rocksdb");
        if !store_path.exists() {
            fs::create_dir_all(store_path.as_path()).map_err(|e| StoreError::IO(e))?;
        }

        options.metadata_path = store_path
            .into_os_string()
            .into_string()
            .map_err(|_e| StoreError::Configuration("Bad path".to_owned()))?;

        let wal_dir = store_dir.join("wal");
        if !wal_dir.exists() {
            fs::create_dir_all(wal_dir.as_path()).map_err(|e| StoreError::IO(e))?;
        }
        let wal_path = WalPath::new(wal_dir.to_str().unwrap(), 1234)?;
        options.add_wal_path(wal_path);

        // Build wal offset manager
        let wal_offset_manager = Arc::new(WalOffsetManager::new());

        // Build index driver
        let indexer = Arc::new(IndexDriver::new(
            logger.clone(),
            &options.metadata_path,
            Arc::clone(&wal_offset_manager) as Arc<dyn MinOffset>,
        )?);

        super::IO::new(&mut options, indexer, logger.clone())
    }

    fn random_store_dir() -> Result<PathBuf, StoreError> {
        test_util::create_random_path().map_err(|e| StoreError::IO(e))
    }

    #[test]
    fn test_receive_io_tasks() -> Result<(), StoreError> {
        let log = test_util::terminal_logger();
        let store_dir = random_store_dir()?;
        let store_dir = store_dir.as_path();
        let _store_dir_guard = test_util::DirectoryRemovalGuard::new(log, store_dir);
        let mut io = create_io(store_dir)?;
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
                    offset: 0,
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

    #[test]
    fn test_build_sqe() -> Result<(), StoreError> {
        let log = test_util::terminal_logger();
        let store_dir = random_store_dir()?;
        let store_dir = store_dir.as_path();
        let _store_dir_guard = test_util::DirectoryRemovalGuard::new(log, store_dir);
        let mut io = create_io(store_dir)?;

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
                    offset: n,
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
    fn test_run() -> Result<(), Box<dyn Error>> {
        let log = test_util::terminal_logger();

        let (tx, rx) = oneshot::channel();
        let logger = log.clone();
        let handle = std::thread::spawn(move || {
            let store_dir = random_store_dir().unwrap();
            let store_dir = store_dir.as_path();
            let _store_dir_guard = test_util::DirectoryRemovalGuard::new(logger, store_dir);
            let mut io = create_io(store_dir).unwrap();

            let sender = io
                .sender
                .take()
                .ok_or(StoreError::Configuration("IO channel".to_owned()))
                .unwrap();
            let _ = tx.send(sender);
            let io = RefCell::new(io);

            let _ = super::IO::run(io);
            println!("Module io stopped");
        });

        let sender = rx
            .blocking_recv()
            .map_err(|_| StoreError::Internal("Internal error".to_owned()))?;

        let mut buffer = BytesMut::with_capacity(4096);
        buffer.resize(4096, 65);
        let buffer = buffer.freeze();

        let mut receivers = vec![];

        (0..16)
            .into_iter()
            .map(|i| {
                let (tx, rx) = oneshot::channel();
                receivers.push(rx);
                IoTask::Write(WriteTask {
                    stream_id: 0,
                    offset: i as i64,
                    buffer: buffer.clone(),
                    observer: tx,
                    written_len: None,
                })
            })
            .for_each(|task| {
                sender.send(task).unwrap();
            });

        let mut results = Vec::new();
        for receiver in receivers {
            let res = receiver.blocking_recv()??;
            trace!(
                log,
                "{{ stream-id: {}, offset: {} , wal_offset: {}}}",
                res.stream_id,
                res.offset,
                res.wal_offset
            );
            results.push(res);
        }

        // Read the data from store
        let mut receivers = vec![];

        results
            .iter()
            .map(|res| {
                let (tx, rx) = oneshot::channel();
                receivers.push(rx);
                IoTask::Read(ReadTask {
                    stream_id: res.stream_id,
                    wal_offset: res.wal_offset,
                    len: 4096 + 8, // 4096 is the write buffer size, 8 is the prefix added by the store
                    observer: tx,
                })
            })
            .for_each(|task| {
                sender.send(task).unwrap();
            });

        for receiver in receivers {
            let res = receiver.blocking_recv()??;
            trace!(
                log,
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

            assert_eq!(buffer, res_payload.freeze()[8..]);
        }

        drop(sender);
        handle.join().map_err(|_| StoreError::AllocLogSegment)?;
        Ok(())
    }
}
