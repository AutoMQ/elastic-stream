use std::{
    cell::RefCell,
    os::fd::{AsRawFd, RawFd},
    sync::Arc,
    thread::{Builder, JoinHandle},
};

use super::lock::Lock;
use crate::{
    error::{AppendError, FetchError, StoreError},
    index::{driver::IndexDriver, MinOffset},
    io::{
        self,
        task::{
            IoTask::{self, Read, Write},
            WriteTask,
        },
        ReadTask,
    },
    offset_manager::WalOffsetManager,
    option::{ReadOptions, WriteOptions},
    AppendRecordRequest, AppendResult, FetchResult, Store,
};
use client::PlacementDriverIdGenerator;
use crossbeam::channel::Sender;
use futures::future::join_all;
use log::{error, trace};
use model::range::RangeMetadata;
use observation::metrics::store_metrics::{
    RangeServerStatistics, STORE_APPEND_BYTES_COUNT, STORE_APPEND_COUNT,
    STORE_APPEND_LATENCY_HISTOGRAM, STORE_FAILED_APPEND_COUNT, STORE_FAILED_FETCH_COUNT,
    STORE_FETCH_BYTES_COUNT, STORE_FETCH_COUNT, STORE_FETCH_LATENCY_HISTOGRAM,
};
use tokio::sync::{mpsc, oneshot};

#[derive(Clone)]
pub struct ElasticStore {
    config: Arc<config::Configuration>,

    lock: Arc<Lock>,

    /// The channel for server layer to communicate with io module.
    ///
    /// For sending a message from async to sync, you should use the standard library unbounded channel or crossbeam.
    /// https://docs.rs/tokio/latest/tokio/sync/mpsc/index.html#communicating-between-sync-and-async-code
    io_tx: Sender<IoTask>,

    /// The reference to index driver, which is used to communicate with index module.
    indexer: Arc<IndexDriver>,

    #[allow(dead_code)]
    wal_offset_manager: Arc<WalOffsetManager>,

    /// Expose underlying I/O Uring FD so that its worker pool may be shared with
    /// server layer I/O Uring instances.
    sharing_uring: RawFd,

    #[allow(dead_code)]
    join_handles: Arc<util::HandleJoiner>,
}

impl ElasticStore {
    pub fn new(
        mut config: config::Configuration,
        recovery_completion_tx: oneshot::Sender<()>,
    ) -> Result<Self, StoreError> {
        let id_generator = Box::new(PlacementDriverIdGenerator::new(&config));

        let lock = Arc::new(Lock::new(&config, id_generator)?);

        // Fill server_id
        config.server.server_id = lock.id();
        let config = Arc::new(config);

        // Build wal offset manager
        let wal_offset_manager = Arc::new(WalOffsetManager::new());
        // Build index driver
        let indexer = Arc::new(IndexDriver::new(
            &config,
            Arc::clone(&wal_offset_manager) as Arc<dyn MinOffset>,
            config.store.rocksdb.flush_threshold,
        )?);

        let (sender, receiver) = oneshot::channel();

        // IO thread will be left in detached state.
        // Copy a indexer
        let indexer_cp = Arc::clone(&indexer);
        let cfg = Arc::clone(&config);
        let _io_thread_handle = Self::with_thread("IO", move || {
            if !core_affinity::set_for_current(core_affinity::CoreId {
                id: cfg.store.io_cpu,
            }) {
                error!("Failed to set affinity for IO thread");
            }
            let mut io = io::IO::new(&cfg, indexer_cp)?;
            let sharing_uring = io.as_raw_fd();
            let tx = io
                .sender
                .take()
                .ok_or(StoreError::Configuration("IO channel".to_owned()))?;

            let io = RefCell::new(io);
            if let Err(_e) = sender.send((tx, sharing_uring)) {
                error!("Failed to expose sharing_uring and task channel sender");
            }
            io::IO::run(io, recovery_completion_tx)
        })?;
        let (tx, sharing_uring) = receiver
            .blocking_recv()
            .map_err(|_e| StoreError::Internal("Start".to_owned()))?;

        let mut handle_joiner = util::HandleJoiner::new();
        handle_joiner.push(_io_thread_handle);

        let store = Self {
            config,
            lock,
            io_tx: tx,
            indexer,
            wal_offset_manager,
            sharing_uring,
            join_handles: Arc::new(handle_joiner),
        };
        trace!("ElasticStore launched");
        Ok(store)
    }

    fn with_thread<F>(name: &str, task: F) -> Result<JoinHandle<()>, StoreError>
    where
        F: FnOnce() -> Result<(), StoreError> + Send + 'static,
    {
        let closure = move || {
            if let Err(e) = task() {
                eprintln!("{}", e);
                todo!("Log internal store error");
            }
        };
        let handle = Builder::new()
            .name(name.to_owned())
            .spawn(closure)
            .map_err(|_e| StoreError::IoUring)?;
        Ok(handle)
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
            range: request.range_index as u32,
            offset: request.offset,
            len: request.len,
            buffer: request.buffer,
            observer,
            written_len: None,
        };
        let io_task = Write(task);
        if let Err(e) = self.io_tx.send(io_task) {
            if let Write(task) = e.0 {
                if let Err(e) = task.observer.send(Err(AppendError::SubmissionQueue)) {
                    error!("Failed to propagate error: {:?}", e);
                }
            }
        }
    }
}

impl Store for ElasticStore {
    async fn append(
        &self,
        _options: WriteOptions,
        request: AppendRecordRequest,
    ) -> Result<AppendResult, AppendError> {
        let now = std::time::Instant::now();
        let (sender, receiver) = oneshot::channel();
        let len = request.buffer.len();
        self.do_append(request, sender);
        match receiver.await.map_err(|_e| AppendError::ChannelRecv) {
            Ok(res) => {
                let latency = now.elapsed();
                STORE_APPEND_LATENCY_HISTOGRAM.observe(latency.as_micros() as f64);
                RangeServerStatistics::observe_append_latency(latency.as_millis() as i16);
                STORE_APPEND_COUNT.inc();
                STORE_APPEND_BYTES_COUNT.inc_by(len as u64);
                res
            }
            Err(e) => {
                STORE_FAILED_APPEND_COUNT.inc();
                Err(e)
            }
        }
    }

    async fn fetch(&self, options: ReadOptions) -> Result<FetchResult, FetchError> {
        let now = std::time::Instant::now();
        let (index_tx, index_rx) = oneshot::channel();
        self.indexer.scan_record_handles(
            options.stream_id,
            options.range,
            options.offset as u64,
            options.max_offset,
            options.max_bytes as u32,
            index_tx,
        );

        let io_tx_cp = self.io_tx.clone();
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
                        if let Err(e) = io_task.observer.send(Err(FetchError::SubmissionQueue)) {
                            error!("Failed to propagate error: {:?}", e);
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
            let mut final_result: Vec<_> = flattened_result.into_iter().flatten().collect();

            // Sort the result
            final_result.sort_by(|a, b| a.wal_offset.cmp(&b.wal_offset));

            STORE_FETCH_COUNT.inc();
            let latency = now.elapsed();
            STORE_FETCH_LATENCY_HISTOGRAM.observe(latency.as_micros() as f64);
            RangeServerStatistics::observe_fetch_latency(latency.as_millis() as i16);
            STORE_FETCH_BYTES_COUNT
                .inc_by(final_result.iter().map(|re| re.total_len()).sum::<usize>() as u64);
            return Ok(FetchResult {
                stream_id: options.stream_id,
                offset: options.offset,
                results: final_result,
            });
        }
        STORE_FAILED_FETCH_COUNT.inc();
        Err(FetchError::NoRecord)
    }

    /// List ranges of all streams that are served by this range server.
    ///
    /// Note this job is delegated to RocksDB threads.
    ///
    /// True that RocksDB may internally use asynchronous IO, but its public API is blocking and synchronous.
    /// Given that we do NOT accept any blocking code in range-server and store crates, we have to delegate these
    /// tasks to RocksDB threads and asynchronously await in tokio::sync::mpsc::unbounded channel.
    ///
    /// This method involves communication between sync and async code, remember to read
    /// https://docs.rs/tokio/latest/tokio/sync/mpsc/index.html#communicating-between-sync-and-async-code
    async fn list<F>(&self, filter: F) -> Result<Vec<RangeMetadata>, StoreError>
    where
        F: Fn(&RangeMetadata) -> bool,
    {
        let (tx, mut rx) = mpsc::unbounded_channel();
        self.indexer.list_ranges(tx);

        let mut ranges = vec![];
        while let Some(range) = rx.recv().await {
            if filter(&range) {
                ranges.push(range);
            }
        }
        Ok(ranges)
    }

    /// List ranges of the specified stream.
    ///
    /// Rationale of delegating this job to RocksDB threads is exactly same to `list` ranges of all streams served by
    /// this range server.
    async fn list_by_stream<F>(
        &self,
        stream_id: i64,
        filter: F,
    ) -> Result<Vec<RangeMetadata>, StoreError>
    where
        F: Fn(&RangeMetadata) -> bool,
    {
        let (tx, mut rx) = mpsc::unbounded_channel();
        self.indexer.list_ranges_by_stream(stream_id, tx);
        let mut ranges = vec![];
        while let Some(range) = rx.recv().await {
            debug_assert_eq!(stream_id, range.stream_id());
            if filter(&range) {
                ranges.push(range);
            }
        }
        Ok(ranges)
    }

    async fn seal(&self, range: RangeMetadata) -> Result<(), StoreError> {
        let (tx, rx) = oneshot::channel();
        self.indexer.seal_range(range, tx);
        rx.await
            .map_err(|_e| StoreError::Internal("Channel error".to_owned()))
            .flatten()
    }

    async fn create(&self, range: RangeMetadata) -> Result<(), StoreError> {
        let (tx, rx) = oneshot::channel();
        self.indexer.create_range(range, tx);
        rx.await
            .map_err(|_e| StoreError::Internal("Channel error".to_owned()))
            .flatten()
    }

    /// Find max record offset of the specified stream and range.
    ///
    /// # Returns
    /// `StoreError` - If something is wrong when accessing RocksDB;
    /// `Some(u64)` - If the max record offset is found;
    /// `None` - If there is no record of the given stream;
    fn max_record_offset(&self, stream_id: i64, range: u32) -> Result<Option<u64>, StoreError> {
        self.indexer
            .retrieve_max_key(stream_id, range)
            .map(|buf| buf.map(|entry| entry.max_offset()))
    }

    fn id(&self) -> i32 {
        self.lock.id()
    }

    fn config(&self) -> Arc<config::Configuration> {
        Arc::clone(&self.config)
    }
}

impl AsRawFd for ElasticStore {
    /// FD of the underlying I/O Uring instance, for the purpose of sharing worker pool with other I/O Uring instances.
    fn as_raw_fd(&self) -> RawFd {
        self.sharing_uring
    }
}

/// Some tests for ElasticStore.
#[cfg(test)]
mod tests {
    use bytes::{Bytes, BytesMut};
    use futures::future::join_all;
    use log::trace;
    use std::error::Error;
    use tokio::sync::oneshot;

    use crate::{
        error::{AppendError, FetchError},
        io::task::SingleFetchResult,
        option::{ReadOptions, WriteOptions},
        store::{append_result::AppendResult, fetch_result::FetchResult},
        AppendRecordRequest, ElasticStore, Store,
    };

    fn build_store(pd_address: &str, store_path: &str) -> ElasticStore {
        let mut config = config::Configuration::default();
        config.placement_driver = pd_address.to_owned();
        config.store.path.set_base(store_path);
        config.check_and_apply().expect("Configuration is invalid");
        let (tx, rx) = oneshot::channel();
        let store = match ElasticStore::new(config, tx) {
            Ok(store) => store,
            Err(e) => {
                panic!("Failed to launch ElasticStore: {:?}", e);
            }
        };
        rx.blocking_recv().expect("Await recovery completion");
        store
    }

    /// Test the basic append and fetch operations.
    #[tokio::test]
    async fn test_run_store() -> Result<(), Box<dyn Error>> {
        let store_dir = tempfile::tempdir()?;
        let store_path = store_dir.path().to_str().unwrap().to_owned();

        let (tx, rx) = oneshot::channel();

        let (stop_tx, stop_rx) = oneshot::channel();
        let (port_tx, port_rx) = oneshot::channel();

        let handle = std::thread::spawn(move || {
            tokio_uring::start(async {
                let port = test_util::run_listener().await;
                let _ = port_tx.send(port);
                let _ = stop_rx.await;
            });
        });
        let port = port_rx.await.unwrap();
        let pd_address = format!("localhost:{}", port);
        let _ = std::thread::spawn(move || {
            let store = build_store(&pd_address, store_path.as_str());
            let send_r = tx.send(store);
            if let Err(_) = send_r {
                panic!("Failed to send store");
            }
        });

        let store = rx.await.unwrap();

        let mut append_fs = vec![];
        (0..1024)
            .into_iter()
            .map(|i| AppendRecordRequest {
                stream_id: 1,
                range_index: 0,
                offset: i,
                len: 1,
                buffer: Bytes::from(format!("{}-{}", "hello, world", i)),
            })
            .for_each(|req| {
                let options = WriteOptions::default();
                let append_f = store.append(options, req);
                append_fs.push(append_f)
            });

        let append_rs: Vec<Result<AppendResult, AppendError>> = join_all(append_fs).await;

        // Case One: Fetch all records one by one.
        {
            let mut fetch_fs = vec![];
            append_rs.iter().for_each(|res| {
                // Log the append result
                match res {
                    Ok(res) => {
                        trace!("Append result: {:?}", res);
                        let options = ReadOptions {
                            stream_id: 1,
                            range: 0,
                            offset: 0,
                            max_offset: 1024,
                            max_bytes: 1,
                            max_wait_ms: 1000,
                        };
                        let fetch_f = store.fetch(options);
                        fetch_fs.push(fetch_f);
                    }
                    Err(e) => {
                        panic!("Append error: {:?}", e);
                    }
                }
            });

            let fetch_rs: Vec<Result<FetchResult, FetchError>> = join_all(fetch_fs).await;
            assert!(fetch_rs.len() == append_rs.len());
            fetch_rs.iter().flatten().for_each(|res| {
                let mut res_payload = BytesMut::new();
                res.results.iter().for_each(|r| {
                    res_payload.extend_from_slice(&copy_single_fetch_result(r));
                });

                assert_eq!(
                    Bytes::copy_from_slice(&res_payload[(crate::RECORD_PREFIX_LENGTH as usize)..]),
                    Bytes::from(format!("{}-{}", "hello, world", res.offset))
                );
            });
        }

        // Case Two: Fetch all records through a single call
        {
            let mut fetch_fs = vec![];
            let options = ReadOptions {
                stream_id: 1,
                range: 0,
                offset: 0,
                max_offset: 1024,
                max_bytes: 1024 * 1024 * 1024,
                max_wait_ms: 1000,
            };
            let fetch_f = store.fetch(options);
            fetch_fs.push(fetch_f);
            let fetch_rs: Vec<Result<FetchResult, FetchError>> = join_all(fetch_fs).await;
            assert_eq!(1, fetch_rs.len());
            let fetch_res = fetch_rs[0].as_ref().unwrap();
            fetch_res.results.iter().enumerate().for_each(|(i, r)| {
                let mut res_payload = BytesMut::new();
                res_payload.extend_from_slice(&copy_single_fetch_result(r));
                assert_eq!(
                    Bytes::copy_from_slice(&res_payload[(crate::RECORD_PREFIX_LENGTH as usize)..]),
                    Bytes::from(format!("{}-{}", "hello, world", i))
                );
            });
        }

        let _ = stop_tx.send(());
        let _ = handle.join();
        drop(store);
        Ok(())
    }

    /// Copy all buffers from a SingleFetchResult.
    fn copy_single_fetch_result(res: &SingleFetchResult) -> Bytes {
        let mut res_payload = BytesMut::new();
        res.iter().for_each(|r| {
            res_payload.extend_from_slice(&r[..]);
        });
        res_payload.freeze()
    }
}
