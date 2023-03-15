use std::{
    cell::RefCell,
    os::fd::{AsRawFd, RawFd},
    rc::Rc,
    sync::Arc,
    thread::{Builder, JoinHandle},
};

use crate::{
    error::{AppendError, FetchError, StoreError},
    index::{driver::IndexDriver, record_handle::RecordHandle, MinOffset},
    io::{
        self,
        task::{
            IoTask::{self, Read, Write},
            WriteTask,
        },
        ReadTask,
    },
    offset_manager::WalOffsetManager,
    ops::{append::AppendResult, fetch::FetchResult, Append, Fetch, Scan},
    option::{ReadOptions, WalPath, WriteOptions},
    AppendRecordRequest, Store,
};
use core_affinity::CoreId;
use crossbeam::channel::Sender;
use futures::{future::join_all, Future};
use slog::{error, trace, Logger};
use tokio::sync::oneshot;

#[derive(Clone)]
pub struct ElasticStore {
    /// The channel for server layer to communicate with io module.
    io_tx: Sender<IoTask>,

    /// The reference to index driver, which is used to communicate with index module.
    indexer: Arc<IndexDriver>,

    wal_offset_manager: Arc<WalOffsetManager>,

    /// Expose underlying I/O Uring FD so that its worker pool may be shared with
    /// server layer I/O Uring instances.
    sharing_uring: RawFd,

    log: Logger,
}

impl ElasticStore {
    pub fn new(log: Logger) -> Result<Self, StoreError> {
        let logger = log.clone();
        let mut opt = io::Options::default();

        // Customize IO options from store options.
        let size_10g = 10u64 * (1 << 30);
        opt.add_wal_path(WalPath::new("/data/store", size_10g)?);

        // Build wal offset manager
        let wal_offset_manager = Arc::new(WalOffsetManager::new());

        // Build index driver
        let indexer = Arc::new(IndexDriver::new(
            log.clone(),
            &opt.metadata_path,
            Arc::clone(&wal_offset_manager) as Arc<dyn MinOffset>,
        )?);

        let (sender, receiver) = oneshot::channel();

        // IO thread will be left in detached state.
        // Copy a indexer
        let indexer_cp = Arc::clone(&indexer);
        let _io_thread_handle = Self::with_thread(
            "IO",
            move || {
                let log = log.clone();
                let mut io = io::IO::new(&mut opt, indexer_cp, log.clone())?;
                let sharing_uring = io.as_raw_fd();
                let tx = io
                    .sender
                    .take()
                    .ok_or(StoreError::Configuration("IO channel".to_owned()))?;

                let io = RefCell::new(io);
                if let Err(e) = sender.send((tx, sharing_uring)) {
                    error!(
                        log,
                        "Failed to expose sharing_uring and task channel sender"
                    );
                }
                io::IO::run(io)
            },
            None,
        )?;
        let (tx, sharing_uring) = receiver
            .blocking_recv()
            .map_err(|e| StoreError::Internal("Start".to_owned()))?;

        let store = Self {
            io_tx: tx,
            indexer,
            wal_offset_manager,
            sharing_uring,
            log: logger,
        };
        trace!(store.log, "ElasticStore launched");
        Ok(store)
    }

    fn with_thread<F>(
        name: &str,
        task: F,
        affinity: Option<CoreId>,
    ) -> Result<JoinHandle<()>, StoreError>
    where
        F: FnOnce() -> Result<(), StoreError> + Send + 'static,
    {
        if let Some(core) = affinity {
            let closure = move || {
                if !core_affinity::set_for_current(core) {
                    todo!("Log error when setting core affinity");
                }
                if let Err(_e) = task() {
                    todo!("Log internal store error");
                }
            };
            let handle = Builder::new()
                .name(name.to_owned())
                .spawn(closure)
                .map_err(|_e| StoreError::IoUring)?;
            Ok(handle)
        } else {
            let closure = move || {
                if let Err(_e) = task() {
                    todo!("Log internal store error");
                }
            };
            let handle = Builder::new()
                .name(name.to_owned())
                .spawn(closure)
                .map_err(|_e| StoreError::IoUring)?;
            Ok(handle)
        }
    }

    /// Send append request to IO module.
    ///
    /// * `request` - Append record request, which includes target stream_id, logical offset and serialized `Record` data.
    /// * `observer` - Oneshot sender, used to return `AppendResult` or propagate error.
    fn do_append(
        &self,
        request: AppendRecordRequest,
        observer: oneshot::Sender<Result<AppendResult, AppendError>>,
    ) {
        let task = WriteTask {
            stream_id: request.stream_id,
            offset: request.offset,
            buffer: request.buffer,
            observer,
            written_len: None,
        };
        let io_task = Write(task);
        if let Err(e) = self.io_tx.send(io_task) {
            if let Write(task) = e.0 {
                if let Err(e) = task.observer.send(Err(AppendError::SubmissionQueue)) {
                    error!(self.log, "Failed to propagate error: {:?}", e);
                }
            }
        }
    }
}

impl Store for ElasticStore {
    type AppendOp = impl Future<Output = Result<AppendResult, AppendError>>;
    type FetchOp = impl Future<Output = Result<FetchResult, FetchError>>;

    fn append(&self, options: WriteOptions, request: AppendRecordRequest) -> Append<Self::AppendOp>
    where
        <Self as Store>::AppendOp: Future<Output = Result<AppendResult, AppendError>>,
    {
        let (sender, receiver) = oneshot::channel();

        self.do_append(request, sender);

        let inner = async {
            match receiver.await.map_err(|_e| AppendError::ChannelRecv) {
                Ok(res) => res,
                Err(e) => Err(e),
            }
        };

        Append { inner }
    }

    fn fetch(&self, options: ReadOptions) -> Fetch<<Self as Store>::FetchOp>
    where
        <Self as Store>::FetchOp: Future<Output = Result<FetchResult, FetchError>>,
    {
        let (index_tx, index_rx) = oneshot::channel();
        self.indexer.scan_record_handles(
            options.stream_id,
            options.offset as u64,
            options.max_bytes as u32,
            index_tx,
        );

        let io_tx_cp = self.io_tx.clone();
        let logger = self.log.clone();
        let inner = async move {
            let scan_res = match index_rx.await.map_err(|_e| FetchError::TranslateIndex) {
                Ok(res) => res.map_err(|_e| FetchError::TranslateIndex),
                Err(e) => Err(e),
            }?;

            if let Some(handles) = scan_res {
                let mut io_receiver = Vec::with_capacity(handles.len());
                for handle in handles {
                    let (sender, receiver) = oneshot::channel();
                    let io_task = ReadTask {
                        stream_id: options.stream_id,
                        wal_offset: handle.wal_offset,
                        len: handle.len,
                        observer: sender,
                    };

                    if let Err(e) = io_tx_cp.send(Read(io_task)) {
                        if let Read(io_task) = e.0 {
                            if let Err(e) = io_task.observer.send(Err(FetchError::SubmissionQueue))
                            {
                                error!(logger, "Failed to propagate error: {:?}", e);
                            }
                        }
                    }

                    io_receiver.push(receiver);
                }

                // Join all IO tasks.
                let io_result = join_all(io_receiver).await;

                let flattened_result: Vec<_> = io_result
                    .into_iter()
                    .map(|res| match res {
                        Ok(Ok(res)) => Ok(res),
                        Ok(Err(e)) => Err(e),
                        Err(_) => Err(FetchError::ChannelRecv), // Channel receive error branch
                    })
                    .collect();

                // Take the first error from the flattened result, and return it.
                let first_error = flattened_result.iter().find(|res| res.is_err());
                if let Some(Err(e)) = first_error {
                    return Err(e.clone());
                }

                // Collect all successful IO results, and sort it by the wal offset
                let mut result: Vec<_> = flattened_result.into_iter().flatten().collect();

                // Sort the result
                result.sort_by(|a, b| a.wal_offset.cmp(&b.wal_offset));

                // Extract the payload from the result, and assemble the final result.
                let final_result: Vec<_> = result.into_iter().map(|res| res.payload).collect();

                return Ok(FetchResult {
                    stream_id: options.stream_id,
                    offset: options.offset,
                    payload: final_result,
                });
            }

            Err(FetchError::NoRecord)
        };

        Fetch { inner }
    }

    fn scan(&self, options: ReadOptions) -> Scan {
        todo!()
    }
}

impl AsRawFd for ElasticStore {
    /// FD of the underlying I/O Uring instance, for the purpose of sharing worker pool with other I/O Uring instances.
    fn as_raw_fd(&self) -> RawFd {
        self.sharing_uring
    }
}
