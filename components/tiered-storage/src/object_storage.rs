use config::ObjectStorageConfig;
use model::object::ObjectMetadata;
use opendal::services::{Fs, S3};
use opendal::Operator;
use std::rc::Rc;
use std::time::Duration;
use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
};
use std::{env, thread};
use store::{ElasticStore, Store};
use tokio::sync::{mpsc, oneshot};
use tokio::time::sleep;

use crate::object_manager::MemoryObjectManager;
use crate::range_accumulator::{DefaultRangeAccumulator, RangeAccumulator};
use crate::range_fetcher::{DefaultRangeFetcher, RangeFetcher};
use crate::range_offload::RangeOffload;
use crate::{ObjectManager, ObjectStorage};
use crate::{Owner, RangeKey};

#[derive(Clone)]
pub struct AsyncObjectStorage {
    tx: mpsc::UnboundedSender<Task>,
}

impl AsyncObjectStorage {
    pub fn new(config: &ObjectStorageConfig, store: ElasticStore) -> Self {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let config = config.clone();
        let store = store;
        let _ = thread::Builder::new()
            .name("ObjectStorage".to_owned())
            .spawn(move || {
                tokio_uring::start(async move {
                    let range_fetcher = DefaultRangeFetcher::new(Rc::new(store));
                    let object_manager = MemoryObjectManager::default();
                    let object_storage =
                        DefaultObjectStorage::new(&config, range_fetcher, object_manager);
                    while let Some(task) = rx.recv().await {
                        match task {
                            Task::NewCommit(stream_id, range_index, record_size) => {
                                object_storage.new_commit(stream_id, range_index, record_size);
                            }
                            Task::GetObjects(task) => {
                                let object_storage = object_storage.clone();
                                tokio_uring::spawn(async move {
                                    let rst = object_storage
                                        .get_objects(
                                            task.stream_id,
                                            task.range_index,
                                            task.start_offset,
                                            task.end_offset,
                                            task.size_hint,
                                        )
                                        .await;
                                    let _ = task.tx.send(rst);
                                });
                            }
                        }
                    }
                });
            });
        Self { tx }
    }
}

impl ObjectStorage for AsyncObjectStorage {
    fn new_commit(&self, stream_id: u64, range_index: u32, record_size: u32) {
        let _ = self
            .tx
            .send(Task::NewCommit(stream_id, range_index, record_size));
    }

    async fn get_objects(
        &self,
        stream_id: u64,
        range_index: u32,
        start_offset: u64,
        end_offset: u64,
        size_hint: u32,
    ) -> Vec<ObjectMetadata> {
        let (tx, rx) = oneshot::channel();
        let _ = self.tx.send(Task::GetObjects(GetObjectsTask {
            stream_id,
            range_index,
            start_offset,
            end_offset,
            size_hint,
            tx,
        }));
        rx.await.unwrap()
    }
}

enum Task {
    NewCommit(u64, u32, u32),
    GetObjects(GetObjectsTask),
}

struct GetObjectsTask {
    stream_id: u64,
    range_index: u32,
    start_offset: u64,
    end_offset: u64,
    size_hint: u32,
    tx: oneshot::Sender<Vec<ObjectMetadata>>,
}

pub struct DefaultObjectStorage<F: RangeFetcher, M: ObjectManager> {
    config: ObjectStorageConfig,
    ranges: RefCell<HashMap<RangeKey, Rc<DefaultRangeAccumulator>>>,
    part_full_ranges: RefCell<HashSet<RangeKey>>,
    cache_size: RefCell<i64>,
    op: Option<Operator>,
    object_manager: Rc<M>,
    range_fetcher: Rc<F>,
}

impl<F, M> DefaultObjectStorage<F, M>
where
    F: RangeFetcher + 'static,
    M: ObjectManager + 'static,
{
    fn add_range(&self, stream_id: u64, range_index: u32, owner: Owner) {
        let op = if let Some(op) = self.op.as_ref() {
            op.clone()
        } else {
            return;
        };
        let range = RangeKey::new(stream_id, range_index);
        let range_offload = Rc::new(RangeOffload::new(
            stream_id,
            range_index,
            op,
            self.object_manager.clone(),
            self.config.object_size,
        ));
        self.ranges.borrow_mut().insert(
            range,
            Rc::new(DefaultRangeAccumulator::new(
                range,
                owner.start_offset,
                self.range_fetcher.clone(),
                self.config.clone(),
                range_offload,
            )),
        );
    }

    fn remove_range(&self, stream_id: u64, range_index: u32) {
        if self.op.is_none() {
            return;
        }
        let range = RangeKey::new(stream_id, range_index);
        let _ = self.ranges.borrow_mut().remove(&range);
    }
}

impl<F, M> ObjectStorage for DefaultObjectStorage<F, M>
where
    F: RangeFetcher + 'static,
    M: ObjectManager + 'static,
{
    fn new_commit(&self, stream_id: u64, range_index: u32, record_size: u32) {
        if self.op.is_none() {
            return;
        }
        let owner = self.object_manager.is_owner(stream_id, range_index);
        let range_key = RangeKey::new(stream_id, range_index);
        let mut range = self.ranges.borrow().get(&range_key).cloned();
        if let Some(owner) = owner {
            if range.is_none() {
                self.add_range(stream_id, range_index, owner);
                range = self.ranges.borrow().get(&range_key).cloned();
            }
        } else if range.is_some() {
            self.remove_range(stream_id, range_index);
            return;
        }
        if let Some(range) = range {
            let (size_change, is_part_full) = range.accumulate(record_size);
            let mut cache_size = self.cache_size.borrow_mut();
            *cache_size += size_change as i64;
            if (*cache_size as u64) < self.config.max_cache_size {
                if is_part_full {
                    // if range accumulate size is large than a part, then add it to part_full_ranges.
                    // when cache_size is large than max_cache_size, we will offload ranges in part_full_ranges
                    // util cache_size under cache_low_watermark.
                    self.part_full_ranges.borrow_mut().insert(range_key);
                }
            } else {
                if is_part_full {
                    *cache_size += range.try_offload_part() as i64;
                }
                // try offload ranges in part_full_ranges util cache_size under cache_low_watermark.
                let mut part_full_ranges = self.part_full_ranges.borrow_mut();
                let part_full_ranges_length = part_full_ranges.len();
                if part_full_ranges_length > 0 {
                    let mut remove_keys = Vec::with_capacity(part_full_ranges_length);
                    for range_key in part_full_ranges.iter() {
                        if let Some(range) = self.ranges.borrow().get(range_key) {
                            *cache_size += range.try_offload_part() as i64;
                            remove_keys.push(*range_key);
                            if *cache_size < self.config.cache_low_watermark as i64 {
                                break;
                            }
                        }
                    }
                    for range_key in remove_keys {
                        part_full_ranges.remove(&range_key);
                    }
                }
            }
        }
    }

    async fn get_objects(
        &self,
        stream_id: u64,
        range_index: u32,
        start_offset: u64,
        end_offset: u64,
        size_hint: u32,
    ) -> Vec<ObjectMetadata> {
        self.object_manager
            .get_objects(stream_id, range_index, start_offset, end_offset, size_hint)
    }
}

impl<F, M> DefaultObjectStorage<F, M>
where
    F: RangeFetcher + 'static,
    M: ObjectManager + 'static,
{
    pub fn new(config: &ObjectStorageConfig, range_fetcher: F, object_manager: M) -> Rc<Self> {
        let op = if config.endpoint.starts_with("fs://") {
            let mut builder = Fs::default();
            builder.root("/tmp/");
            Some(Operator::new(builder).unwrap().finish())
        } else if !config.endpoint.is_empty() {
            let mut s3_builder = S3::default();
            s3_builder.root("/");
            s3_builder.bucket(&config.bucket);
            s3_builder.region(&config.region);
            s3_builder.endpoint(&config.endpoint);
            s3_builder.access_key_id(
                &env::var("ES_S3_ACCESS_KEY_ID")
                    .map_err(|_| "ES_S3_ACCESS_KEY_ID cannot find in env")
                    .unwrap(),
            );
            s3_builder.secret_access_key(
                &env::var("ES_S3_SECRET_ACCESS_KEY")
                    .map_err(|_| "ES_S3_SECRET_ACCESS_KEY cannot find in env")
                    .unwrap(),
            );
            Some(Operator::new(s3_builder).unwrap().finish())
        } else {
            None
        };

        let force_flush_secs = config.force_flush_secs;
        let this = Rc::new(DefaultObjectStorage {
            config: config.clone(),
            ranges: RefCell::new(HashMap::new()),
            part_full_ranges: RefCell::new(HashSet::new()),
            cache_size: RefCell::new(0),
            op,
            object_manager: Rc::new(object_manager),
            range_fetcher: Rc::new(range_fetcher),
        });
        Self::run_force_flush_task(this.clone(), Duration::from_secs(force_flush_secs));
        this
    }

    pub fn object_manager() -> Rc<MemoryObjectManager> {
        Rc::new(MemoryObjectManager::default())
    }

    pub fn range_fetcher<S: Store + 'static>(store: Rc<S>) -> Rc<DefaultRangeFetcher<S>> {
        Rc::new(DefaultRangeFetcher::<S>::new(store))
    }

    pub fn run_force_flush_task(storage: Rc<DefaultObjectStorage<F, M>>, max_duration: Duration) {
        tokio_uring::spawn(async move {
            loop {
                storage.ranges.borrow().iter().for_each(|(_, range)| {
                    range.try_flush(max_duration);
                });
                sleep(Duration::from_secs(1)).await;
            }
        });
    }
}

#[cfg(test)]
mod test {}
