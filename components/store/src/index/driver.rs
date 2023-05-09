use std::{
    ops::Deref,
    sync::Arc,
    thread::{sleep, Builder, JoinHandle},
};

use crate::{error::StoreError, index::LocalRangeManager};
use crossbeam::channel::{self, Receiver, Select, Sender, TryRecvError};
use log::{error, info};
use model::range::StreamRange;
use tokio::sync::{mpsc, oneshot};

use super::{indexer::Indexer, record_handle::RecordHandle, MinOffset};

pub(crate) struct IndexDriver {
    tx: Sender<IndexCommand>,
    // Issue a shutdown signal to the driver thread.
    shutdown_tx: Sender<()>,
    indexer: Arc<Indexer>,
    handles: Vec<JoinHandle<()>>,
}

pub(crate) enum IndexCommand {
    Index {
        stream_id: i64,
        range: u32,
        offset: u64,
        handle: RecordHandle,
    },
    /// Used to retrieve a batch of record handles from a given offset.
    ScanRecord {
        stream_id: i64,
        range: u32,
        offset: u64,
        max_offset: Option<u64>,
        max_bytes: u32,
        observer: oneshot::Sender<Result<Option<Vec<RecordHandle>>, StoreError>>,
    },

    ListRange {
        tx: mpsc::UnboundedSender<StreamRange>,
    },

    ListRangeByStream {
        stream_id: i64,
        tx: mpsc::UnboundedSender<StreamRange>,
    },

    SealRange {
        range: StreamRange,
        tx: oneshot::Sender<Result<(), StoreError>>,
    },

    CreateRange {
        range: StreamRange,
        tx: oneshot::Sender<Result<(), StoreError>>,
    },
}

impl IndexDriver {
    pub(crate) fn new(
        path: &str,
        min_offset: Arc<dyn MinOffset>,
        flush_threshold: usize,
    ) -> Result<Self, StoreError> {
        let (tx, rx) = channel::unbounded();
        let (shutdown_tx, shutdown_rx) = channel::bounded(1);
        let indexer = Arc::new(Indexer::new(path, min_offset, flush_threshold)?);
        let runner = IndexDriverRunner::new(rx, shutdown_rx, Arc::clone(&indexer));
        let handle = Builder::new()
            .name("IndexDriver".to_owned())
            .spawn(move || {
                runner.run();
            })
            .map_err(|e| StoreError::Internal(e.to_string()))?;
        info!("IndexDriver thread started");
        Ok(Self {
            tx,
            shutdown_tx,
            indexer,
            handles: vec![handle],
        })
    }

    pub(crate) fn index(&self, stream_id: i64, range: u32, offset: u64, handle: RecordHandle) {
        if let Err(_e) = self.tx.send(IndexCommand::Index {
            stream_id,
            range,
            offset,
            handle,
        }) {
            error!("Failed to send index entry to internal indexer");
        }
    }

    pub(crate) fn scan_record_handles(
        &self,
        stream_id: i64,
        range: u32,
        offset: u64,
        max_offset: Option<u64>,
        max_bytes: u32,
        observer: oneshot::Sender<Result<Option<Vec<RecordHandle>>, StoreError>>,
    ) {
        if let Err(_e) = self.tx.send(IndexCommand::ScanRecord {
            stream_id,
            range,
            offset,
            max_offset,
            max_bytes,
            observer,
        }) {
            error!("Failed to send scan record handles command to internal indexer");
        }
    }

    pub(crate) fn list_ranges(&self, tx: mpsc::UnboundedSender<StreamRange>) {
        if let Err(_e) = self.tx.send(IndexCommand::ListRange { tx }) {
            error!("Failed to send list range command");
        }
    }

    pub(crate) fn list_ranges_by_stream(
        &self,
        stream_id: i64,
        tx: mpsc::UnboundedSender<StreamRange>,
    ) {
        if let Err(_e) = self
            .tx
            .send(IndexCommand::ListRangeByStream { stream_id, tx })
        {
            error!("Failed to send list range by stream command");
        }
    }

    pub(crate) fn create_range(
        &self,
        range: StreamRange,
        tx: oneshot::Sender<Result<(), StoreError>>,
    ) {
        if let Err(e) = self.tx.send(IndexCommand::CreateRange { range, tx }) {
            error!("Failed to submit create range command");
            if let IndexCommand::CreateRange { tx, .. } = e.0 {
                let _ = tx.send(Err(StoreError::Internal(
                    "Submit create range failed".to_owned(),
                )));
            }
        }
    }

    pub(crate) fn seal_range(
        &self,
        range: StreamRange,
        tx: oneshot::Sender<Result<(), StoreError>>,
    ) {
        if let Err(e) = self.tx.send(IndexCommand::SealRange { range, tx }) {
            error!("Failed to submit create range command");
            if let IndexCommand::SealRange { tx, .. } = e.0 {
                let _ = tx.send(Err(StoreError::Internal(
                    "Submit seal range failed".to_owned(),
                )));
            }
        }
    }

    pub(crate) fn shutdown_indexer(&self) {
        if let Err(e) = self.indexer.flush(true) {
            error!("Failed to flush primary index. Cause: {:?}", e);
        }

        if self.shutdown_tx.send(()).is_err() {
            error!("Failed to send shutdown signal to indexer");
        }
    }

    pub(crate) fn join(&mut self) {
        for handle in self.handles.drain(..) {
            let _ = handle.join();
        }
    }
}

impl Drop for IndexDriver {
    fn drop(&mut self) {
        self.join();
    }
}

impl Deref for IndexDriver {
    type Target = Arc<Indexer>;

    fn deref(&self) -> &Self::Target {
        &self.indexer
    }
}

struct IndexDriverRunner {
    rx: Receiver<IndexCommand>,
    shutdown_rx: Receiver<()>,
    indexer: Arc<Indexer>,
}

impl IndexDriverRunner {
    fn new(rx: Receiver<IndexCommand>, shutdown_rx: Receiver<()>, indexer: Arc<Indexer>) -> Self {
        Self {
            rx,
            shutdown_rx,
            indexer,
        }
    }

    fn run(&self) {
        let mut selector = Select::new();
        selector.recv(&self.rx);
        selector.recv(&self.shutdown_rx);
        loop {
            let index = selector.ready();
            if 0 == index {
                match self.rx.try_recv() {
                    Ok(index_command) => match index_command {
                        IndexCommand::Index {
                            stream_id,
                            range,
                            offset,
                            handle,
                        } => {
                            while let Err(e) = self.indexer.index(stream_id, range, offset, &handle)
                            {
                                error!("Failed to index: stream_id={}, offset={}, record_handle={:?}, cause: {}", 
                                stream_id, offset, handle, e);
                                sleep(std::time::Duration::from_millis(100));
                            }
                        }
                        IndexCommand::ScanRecord {
                            stream_id,
                            range,
                            offset,
                            max_offset,
                            max_bytes,
                            observer,
                        } => {
                            observer
                                .send(self.indexer.scan_record_handles_left_shift(
                                    stream_id, range, offset, max_offset, max_bytes,
                                ))
                                .unwrap_or_else(|_e| {
                                    error!(
                                        "Failed to send scan result of {}/{} to observer.",
                                        stream_id, offset
                                    );
                                });
                        }

                        IndexCommand::ListRange { tx } => {
                            self.indexer.list(tx);
                        }

                        IndexCommand::ListRangeByStream { stream_id, tx } => {
                            self.indexer.list_by_stream(stream_id, tx);
                        }

                        IndexCommand::CreateRange { range, tx } => {
                            match self.indexer.add(range.stream_id(), &range) {
                                Ok(()) => {
                                    let _ = tx.send(Ok(()));
                                }
                                Err(e) => {
                                    error!("Failed to add stream range: {}", range);
                                    let _ = tx.send(Err(e));
                                }
                            }
                        }

                        IndexCommand::SealRange { range, tx } => {
                            match self.indexer.seal(range.stream_id(), &range) {
                                Ok(()) => {
                                    let _ = tx.send(Ok(()));
                                }
                                Err(e) => {
                                    error!("Failed to seal range: {}", range);
                                    let _ = tx.send(Err(e));
                                }
                            }
                        }
                    },
                    Err(TryRecvError::Empty) => {
                        continue;
                    }
                    Err(TryRecvError::Disconnected) => {
                        info!("IndexChannel disconnected");
                        break;
                    }
                }
            } else if 1 == index {
                info!("Got a command to quit IndexDriverRunner thread");
                break;
            }
        }
        info!("IndexDriverRunner thread completed");
    }
}

#[cfg(test)]
mod tests {
    use crate::index::MinOffset;
    use std::{error::Error, sync::Arc};

    struct TestMinOffset {}

    impl MinOffset for TestMinOffset {
        fn min_offset(&self) -> u64 {
            0
        }
    }

    #[test]
    fn test_index_driver() -> Result<(), Box<dyn Error>> {
        let db_path = test_util::create_random_path()?;
        let _dir_guard = test_util::DirectoryRemovalGuard::new(db_path.as_path());
        let min_offset = Arc::new(TestMinOffset {});
        let index_driver =
            super::IndexDriver::new(db_path.as_os_str().to_str().unwrap(), min_offset, 128)?;
        assert_eq!(0, index_driver.get_wal_checkpoint()?);

        index_driver.shutdown_indexer();
        Ok(())
    }
}
