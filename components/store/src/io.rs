use crate::error::StoreError;
use crossbeam::channel::{Receiver, Sender};
use slog::{error, trace, Logger};
use std::os::fd::AsRawFd;

const DEFAULT_MAX_IO_DEPTH: u32 = 4096;
const DEFAULT_SQPOLL_IDLE_MS: u32 = 2000;
const DEFAULT_SQPOLL_CPU: u32 = 1;
const DEFAULT_MAX_BOUNDED_URING_WORKER_COUNT: u32 = 2;
const DEFAULT_MAX_UNBOUNDED_URING_WORKER_COUNT: u32 = 2;

pub(crate) struct Options {
    io_depth: u32,

    sqpoll_idle_ms: u32,

    /// Bind the kernel's poll thread to the specified cpu.
    sqpoll_cpu: u32,

    max_workers: [u32; 2],
}

impl Default for Options {
    fn default() -> Self {
        Self {
            io_depth: DEFAULT_MAX_IO_DEPTH,
            sqpoll_idle_ms: DEFAULT_SQPOLL_IDLE_MS,
            sqpoll_cpu: DEFAULT_SQPOLL_CPU,
            max_workers: [
                DEFAULT_MAX_BOUNDED_URING_WORKER_COUNT,
                DEFAULT_MAX_UNBOUNDED_URING_WORKER_COUNT,
            ],
        }
    }
}

pub(crate) struct IO {
    uring: io_uring::IoUring,
    pub(crate) sender: Sender<()>,
    receiver: Receiver<()>,
    log: Logger,
}

impl IO {
    pub(crate) fn new(options: &mut Options, log: Logger) -> Result<Self, StoreError> {
        let uring = io_uring::IoUring::builder()
            .dontfork()
            .setup_iopoll()
            .setup_sqpoll(options.sqpoll_idle_ms)
            .setup_sqpoll_cpu(options.sqpoll_cpu)
            .setup_r_disabled()
            .build(options.io_depth)
            .map_err(|e| {
                error!(log, "Failed to build I/O Uring instance: {:#?}", e);
                StoreError::IoUring
            })?;

        let submitter = uring.submitter();
        submitter.register_iowq_max_workers(&mut options.max_workers)?;
        submitter.register_enable_rings()?;
        trace!(log, "I/O Uring instance created");

        let (sender, receiver) = crossbeam::channel::unbounded();

        Ok(Self {
            uring,
            sender,
            receiver,
            log,
        })
    }

    pub(crate) fn run(&mut self) {
        let mut in_flight_requests = 0;
        loop {
            loop {
                if self.uring.params().sq_entries() <= in_flight_requests {
                    break;
                }

                if let Ok(_) = self.receiver.try_recv() {
                    in_flight_requests += 1;
                    todo!("Convert item to IO task");
                }
            }

            if let Ok(_reaped) = self.uring.submit_and_wait(1) {
                let mut completion = self.uring.completion();
                while let Some(_cqe) = completion.next() {
                    in_flight_requests -= 1;
                }
                completion.sync();
            } else {
                break;
            }
        }
    }
}

impl AsRawFd for IO {
    fn as_raw_fd(&self) -> std::os::fd::RawFd {
        self.uring.as_raw_fd()
    }
}
