use opendal::services::S3;
use opendal::Operator;
use std::env;
use std::error::Error;
use std::rc::Rc;
use std::time::Duration;
use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
};
use tokio::time::sleep;

use crate::range_accumulator::{DefaultRangeAccumulator, RangeAccumulator};
use crate::TieredStorage;
use crate::{RangeFetcher, RangeKey};

pub struct ObjectTieredStorage<F: RangeFetcher> {
    config: ObjectTieredStorageConfig,
    ranges: RefCell<HashMap<RangeKey, Rc<DefaultRangeAccumulator>>>,
    part_full_ranges: RefCell<HashSet<RangeKey>>,
    cache_size: RefCell<i64>,
    op: Operator,
    range_fetcher: Rc<F>,
}

impl<F> TieredStorage for ObjectTieredStorage<F>
where
    F: RangeFetcher + 'static,
{
    fn add_range(&self, stream_id: u64, range_index: u32, start_offset: u64, end_offset: u64) {
        let range = RangeKey::new(stream_id, range_index);
        self.ranges.borrow_mut().insert(
            range,
            Rc::new(DefaultRangeAccumulator::new(
                range,
                start_offset,
                end_offset,
                self.op.clone(),
                self.range_fetcher.clone(),
                self.config.object_size,
                self.config.part_size,
            )),
        );
    }

    fn new_record_arrived(
        &self,
        stream_id: u64,
        range_index: u32,
        end_offset: u64,
        record_size: u32,
    ) {
        let range_key = RangeKey::new(stream_id, range_index);
        if let Some(range) = self.ranges.borrow().get(&range_key) {
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

impl<F> ObjectTieredStorage<F>
where
    F: RangeFetcher + 'static,
{
    pub fn new(
        config: ObjectTieredStorageConfig,
        range_fetcher: Rc<F>,
    ) -> Result<Rc<Self>, Box<dyn Error>> {
        // construct opendal operator
        let mut s3_builder = S3::default();
        s3_builder.root("/");
        s3_builder.bucket(&config.bucket);
        s3_builder.region(&config.region);
        s3_builder.endpoint(&config.endpoint);
        s3_builder.access_key_id(
            &env::var("ES_S3_ACCESS_KEY_ID")
                .map_err(|_| "ES_S3_ACCESS_KEY_ID cannot find in env")?,
        );
        s3_builder.secret_access_key(
            &env::var("ES_S3_SECRET_ACCESS_KEY")
                .map_err(|_| "ES_S3_SECRET_ACCESS_KEY cannot find in env")?,
        );
        let op = Operator::new(s3_builder)?.finish();

        let force_flush_interval = config.force_flush_interval;
        let this = Rc::new(ObjectTieredStorage {
            config,
            ranges: RefCell::new(HashMap::new()),
            part_full_ranges: RefCell::new(HashSet::new()),
            cache_size: RefCell::new(0),
            op,
            range_fetcher,
        });

        Self::run_force_flush_task(this.clone(), force_flush_interval);

        Ok(this)
    }

    pub fn run_force_flush_task(storage: Rc<ObjectTieredStorage<F>>, max_duration: Duration) {
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

pub struct ObjectTieredStorageConfig {
    pub endpoint: String,
    pub bucket: String,
    pub region: String,
    pub object_size: u32,
    pub part_size: u32,
    pub max_cache_size: u64,
    pub cache_low_watermark: u64,
    pub force_flush_interval: Duration,
}

#[cfg(test)]
mod test {}
