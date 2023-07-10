use config::ObjectStorageConfig;
use opendal::services::S3;
use opendal::Operator;
use std::rc::Rc;
use std::time::Duration;
use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
};
use std::{env, thread};
use store::{ElasticStore, Store};
use tokio::sync::mpsc;
use tokio::time::sleep;

use crate::object_manager::MemoryObjectManager;
use crate::range_accumulator::{DefaultRangeAccumulator, RangeAccumulator};
use crate::range_fetcher::{DefaultRangeFetcher, RangeFetcher};
use crate::range_offload::RangeOffload;
use crate::{ObjectManager, TieredStorage};
use crate::{Owner, RangeKey};

#[derive(Clone)]
pub struct AsyncObjectTieredStorage {
    tx: mpsc::UnboundedSender<Task>,
}

impl AsyncObjectTieredStorage {
    pub fn new(config: &ObjectStorageConfig, store: ElasticStore) -> Self {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let config = config.clone();
        let store = store.clone();
        let _ = thread::Builder::new()
            .name("ObjectStorage".to_owned())
            .spawn(move || {
                let _ = tokio_uring::start(async move {
                    let range_fetcher = DefaultRangeFetcher::new(Rc::new(store));
                    let object_manager = MemoryObjectManager::default();
                    let object_storage =
                        ObjectTieredStorage::new(&config, range_fetcher, object_manager);
                    while let Some(task) = rx.recv().await {
                        match task {
                            Task::NewRecordArrived(
                                stream_id,
                                range_index,
                                end_offset,
                                record_size,
                            ) => {
                                object_storage.new_record_arrived(
                                    stream_id,
                                    range_index,
                                    end_offset,
                                    record_size,
                                );
                            }
                        }
                    }
                });
            });
        Self { tx }
    }
}

impl TieredStorage for AsyncObjectTieredStorage {
    fn new_record_arrived(
        &self,
        stream_id: u64,
        range_index: u32,
        end_offset: u64,
        record_size: u32,
    ) {
        let _ = self.tx.send(Task::NewRecordArrived(
            stream_id,
            range_index,
            end_offset,
            record_size,
        ));
    }
}

enum Task {
    NewRecordArrived(u64, u32, u64, u32),
}

pub struct ObjectTieredStorage<F: RangeFetcher, M: ObjectManager> {
    config: ObjectStorageConfig,
    ranges: RefCell<HashMap<RangeKey, Rc<DefaultRangeAccumulator>>>,
    part_full_ranges: RefCell<HashSet<RangeKey>>,
    cache_size: RefCell<i64>,
    op: Operator,
    object_manager: Rc<M>,
    range_fetcher: Rc<F>,
}

impl<F, M> ObjectTieredStorage<F, M>
where
    F: RangeFetcher + 'static,
    M: ObjectManager + 'static,
{
    fn add_range(&self, stream_id: u64, range_index: u32, owner: Owner) {
        let range = RangeKey::new(stream_id, range_index);
        let range_offload = Rc::new(RangeOffload::new(
            stream_id,
            range_index,
            self.op.clone(),
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
}

impl<F, M> TieredStorage for ObjectTieredStorage<F, M>
where
    F: RangeFetcher + 'static,
    M: ObjectManager + 'static,
{
    fn new_record_arrived(
        &self,
        stream_id: u64,
        range_index: u32,
        end_offset: u64,
        record_size: u32,
    ) {
        let owner = self.object_manager.is_owner(stream_id, range_index);
        let range_key = RangeKey::new(stream_id, range_index);
        let mut range = self.ranges.borrow().get(&range_key).cloned();
        if owner.is_some() && range.is_none() {
            self.add_range(stream_id, range_index, owner.unwrap());
            range = self.ranges.borrow().get(&range_key).cloned();
        }
        if let Some(range) = range {
            let (size_change, is_part_full) = range.accumulate(end_offset, record_size);
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
}

impl<F, M> ObjectTieredStorage<F, M>
where
    F: RangeFetcher + 'static,
    M: ObjectManager + 'static,
{
    pub fn new(config: &ObjectStorageConfig, range_fetcher: F, object_manager: M) -> Rc<Self> {
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
        let op = Operator::new(s3_builder).unwrap().finish();

        let force_flush_interval = config.force_flush_interval;
        let this = Rc::new(ObjectTieredStorage {
            config: config.clone(),
            ranges: RefCell::new(HashMap::new()),
            part_full_ranges: RefCell::new(HashSet::new()),
            cache_size: RefCell::new(0),
            op,
            object_manager: Rc::new(object_manager),
            range_fetcher: Rc::new(range_fetcher),
        });
        Self::run_force_flush_task(this.clone(), force_flush_interval);
        this
    }

    pub fn object_manager() -> Rc<MemoryObjectManager> {
        Rc::new(MemoryObjectManager::default())
    }

    pub fn range_fetcher<S: Store + 'static>(store: Rc<S>) -> Rc<DefaultRangeFetcher<S>> {
        Rc::new(DefaultRangeFetcher::<S>::new(store))
    }

    pub fn run_force_flush_task(storage: Rc<ObjectTieredStorage<F, M>>, max_duration: Duration) {
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
