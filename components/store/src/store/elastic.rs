use std::{cell::UnsafeCell, fs, path::Path, rc::Rc};

use local_sync::mpsc::bounded::{self, Rx, Tx};
use monoio::fs::OpenOptions;
use slog::{debug, error, info, warn, Logger};

use crate::{
    api::{AppendResult, AsyncStore, Command, Cursor},
    error::StoreError,
};

use super::{option::StoreOptions, segment::JournalSegment};

pub struct ElasticStore {
    options: StoreOptions,
    logger: Logger,
    segments: UnsafeCell<Vec<Rc<JournalSegment>>>,
    cursor: UnsafeCell<Cursor>,
    tx: Rc<Tx<Command>>,
}

impl ElasticStore {
    pub fn new(options: &StoreOptions, logger: &Logger) -> Result<Rc<Self>, StoreError> {
        if !options.create_if_missing && !Path::new(&options.store_path.path).exists() {
            error!(
                logger,
                "Specified store path[`{}`] does not exist.", options.store_path.path
            );
            return Err(StoreError::NotFound(
                "Specified store path does not exist".to_owned(),
            ));
        }

        let (tx, mut rx) = bounded::channel(options.command_queue_depth);
        let store = Rc::new(Self {
            options: options.clone(),
            logger: logger.clone(),
            segments: UnsafeCell::new(vec![]),
            cursor: UnsafeCell::new(Cursor::new()),
            tx: Rc::new(tx),
        });

        let cloned = Rc::clone(&store);
        monoio::spawn(async move {
            let store = cloned;
            loop {
                match rx.recv().await {
                    Some(command) => {
                        let _store = Rc::clone(&store);
                        monoio::spawn(async move { _store.process(command).await });
                    }
                    None => {
                        break;
                    }
                }
            }
        });

        Ok(store)
    }

    pub async fn open(&self) -> Result<(), StoreError> {
        let file_name = format!("{}/1", self.options.store_path.path);

        let path = Path::new(&file_name);
        {
            let dir = match path.parent() {
                Some(p) => p,
                None => {
                    return Err(StoreError::InvalidPath(
                        self.options.store_path.path.clone(),
                    ))
                }
            };

            if !dir.exists() {
                info!(self.logger, "Create directory: {}", dir.display());
                fs::create_dir_all(dir).map_err(|e| StoreError::IO(e))?
            }
        }

        let f = OpenOptions::new()
            .read(true)
            .append(true)
            .create(true)
            .open(&path)
            .await?;
        let segment = JournalSegment { file: f, file_name };
        let segments = unsafe { &mut *self.segments.get() };
        segments.push(Rc::new(segment));
        Ok(())
    }

    /// Return the last segment of the store journal.
    fn last_segment(&self) -> Option<Rc<JournalSegment>> {
        let segments = unsafe { &mut *self.segments.get() };
        match segments.last() {
            Some(f) => Some(Rc::clone(f)),
            None => None,
        }
    }

    async fn process(&self, command: Command) {
        match command {
            Command::Append(req) => match req.sender.send(AppendResult {}) {
                Ok(_) => {
                    // Allocate buffer to append data
                    let len = 1;
                    let pos = self.cursor_alloc(len);

                    // file.write.await

                    // Assume we have written `len` bytes.
                    if !self.cursor_commit(pos, len) {
                        // TODO put [pos, pos + len] to the binary search tree
                    }
                }
                Err(_e) => {}
            },
        }
    }

    fn cursor_alloc(&self, len: u64) -> u64 {
        let cursor = unsafe { &mut *self.cursor.get() };
        cursor.alloc(len)
    }

    fn cursor_commit(&self, pos: u64, len: u64) -> bool {
        let cursor = unsafe { &mut *self.cursor.get() };
        cursor.commit(pos, len)
    }

    /// Delete all journal segment files.
    pub fn destroy(&self) -> Result<(), StoreError> {
        let segments = unsafe { &*self.segments.get() };
        segments.iter().for_each(|s| {
            let file_name = &s.file_name;
            debug!(self.logger, "Delete file[`{}`]", file_name);
            fs::remove_file(Path::new(file_name)).unwrap_or_else(|e| {
                warn!(
                    self.logger,
                    "Failed to delete file[`{}`]. Cause: {:?}", file_name, e
                );
            });
        });

        Ok(())
    }
}

impl AsyncStore for ElasticStore {
    fn submission_queue(&self) -> Rc<Tx<Command>> {
        Rc::clone(&self.tx)
    }
}

impl Drop for ElasticStore {
    fn drop(&mut self) {
        if self.options.destroy_on_exit {
            warn!(
                self.logger,
                "Start to destroy ElasticStore[`{}`]", self.options.store_path.path
            );
            self.destroy().unwrap_or_else(|e| {
                warn!(
                    self.logger,
                    "Failed to destroy ElasticStore[{}]. Cause: {:?}",
                    self.options.store_path.path,
                    e
                );
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use slog::{o, Drain, Logger};
    use uuid::Uuid;

    use crate::store::option::StorePath;

    fn get_logger() -> Logger {
        let decorator = slog_term::TermDecorator::new().build();
        let drain = slog_term::CompactFormat::new(decorator).build().fuse();
        let drain = slog_async::Async::new(drain).build().fuse();
        let log = slog::Logger::root(drain, o!());
        log
    }

    #[test]
    fn test_elastic_store_new_with_non_existing_path() {
        let uuid = Uuid::new_v4();
        dbg!(uuid);
        let store_path = StorePath {
            path: format!(
                "{}/{}",
                std::env::temp_dir().as_path().display(),
                uuid.hyphenated().to_string()
            ),
            target_size: 1024 * 1024,
        };

        let mut options = StoreOptions::new(&store_path);
        options.create_if_missing = false;
        let logger = get_logger();

        let store = ElasticStore::new(&options, &logger);
        assert_eq!(true, store.is_err());
    }

    async fn new_elastic_store() -> Result<Rc<ElasticStore>, StoreError> {
        let store_path = StorePath {
            path: format!("{}/store", std::env::temp_dir().as_path().display()),
            target_size: 1024 * 1024,
        };

        let mut options = StoreOptions::new(&store_path);
        options.destroy_on_exit = true;
        let logger = get_logger();

        Ok(ElasticStore::new(&options, &logger)?)
    }

    #[monoio::test]
    async fn test_elastic_store_open() -> Result<(), StoreError> {
        let store = new_elastic_store().await.unwrap();
        store.open().await?;
        let segments = unsafe { &*store.segments.get() };
        assert_eq!(false, segments.is_empty());
        Ok(())
    }

    #[monoio::test]
    async fn test_elastic_store_last_segment() -> Result<(), StoreError> {
        let store = new_elastic_store().await.unwrap();
        store.open().await?;
        assert_eq!(true, store.last_segment().is_some());
        Ok(())
    }
}
