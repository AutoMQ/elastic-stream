use bytes::Bytes;
use std::cell::RefCell;
use std::cmp::min;
use std::collections::{HashMap, VecDeque};
use std::fmt;
use std::rc::Rc;

use super::records_block::{BlockRecord, RecordsBlock};

pub(crate) struct HotCache {
    max_cache_size: u64,
    occupied_size: RefCell<u64>,
    map: RefCell<HashMap<Rc<CacheKey>, Rc<BlockRecord>>>,
    fifo_deque: RefCell<VecDeque<Rc<CacheKey>>>,
}

impl HotCache {
    pub(crate) fn new(max_cache_size: u64) -> Self {
        if max_cache_size < 4 * 1024 {
            panic!("max_cache_size must be greater than 4KB");
        }
        let cache_entry_size = 4 * 1024;
        Self {
            max_cache_size,
            occupied_size: RefCell::new(0),
            map: RefCell::new(HashMap::with_capacity(
                (max_cache_size / cache_entry_size) as usize,
            )),
            fifo_deque: RefCell::new(VecDeque::with_capacity(
                (max_cache_size / cache_entry_size) as usize,
            )),
        }
    }

    pub(crate) fn insert(
        &self,
        stream_id: u64,
        start_offset: u64,
        end_offset_delta: u32,
        data: Vec<Bytes>,
    ) {
        let mut occupied_size = self.occupied_size.borrow_mut();
        let mut fifo_deque = self.fifo_deque.borrow_mut();
        let mut map = self.map.borrow_mut();
        while *occupied_size >= self.max_cache_size {
            if let Some(cache_key) = fifo_deque.pop_front() {
                if let Some(cache_value) = map.remove(&cache_key) {
                    *occupied_size -= cache_value.len() as u64;
                }
            } else {
                break;
            }
        }
        let cache_key = Rc::new(CacheKey::new(stream_id, start_offset));
        let cache_value = Rc::new(BlockRecord {
            start_offset,
            end_offset_delta,
            data,
        });
        *occupied_size += cache_value.len() as u64;
        fifo_deque.push_back(cache_key.clone());
        map.insert(cache_key, cache_value);
    }

    pub(crate) fn get_block(
        &self,
        stream_id: u64,
        mut start_offset: u64,
        end_offset: u64,
        mut size_hint: u32,
    ) -> RecordsBlock {
        let mut records = vec![];
        let map = self.map.borrow();
        loop {
            if start_offset >= end_offset || size_hint == 0 {
                break;
            }
            let cache_key = Rc::new(CacheKey::new(stream_id, start_offset));
            let record = map.get(&cache_key).cloned();
            if let Some(record) = record {
                let record = record.as_ref().clone();
                start_offset = record.end_offset();
                size_hint -= min(record.len(), size_hint);
                records.push(record);
            } else {
                break;
            }
        }
        if records.is_empty() {
            RecordsBlock::empty_block(start_offset)
        } else {
            RecordsBlock::new(records)
        }
    }
}

impl fmt::Debug for HotCache {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "RecordBatchCache[occupied/total={}/{} cache_count={}]",
            self.occupied_size.borrow(),
            self.max_cache_size,
            self.fifo_deque.borrow().len()
        )
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct CacheKey {
    stream_id: u64,
    base_offset: u64,
}

impl CacheKey {
    pub(crate) fn new(stream_id: u64, base_offset: u64) -> Self {
        Self {
            stream_id,
            base_offset,
        }
    }
}

#[cfg(test)]
mod test {
    use crate::stream::cache::HotCache;
    use bytes::Bytes;
    use std::error::Error;

    #[test]
    fn test_cache_insert_get() -> Result<(), Box<dyn Error>> {
        let cache = HotCache::new(4096);
        let stream_id = 1;
        let count = 10;
        for i in 0..100 {
            let base_offset = i * 10;
            let data = vec![Bytes::from(format!("data{}", i))];
            cache.insert(stream_id, base_offset, count, data.clone());
            let cache_value =
                cache.get_block(stream_id, base_offset, base_offset + count as u64, 1024);
            let records = cache_value.records;
            assert_eq!(1, records.len());
            assert_eq!(count, records[0].end_offset_delta);
            assert_eq!(data, records[0].data);
        }
        for i in 0..100 {
            let base_offset = i * 10;
            let cache_value =
                cache.get_block(stream_id, base_offset, base_offset + count as u64, 1024);
            let records = cache_value.records;
            assert_eq!(1, records.len());
            assert_eq!(count, records[0].end_offset_delta);
            assert_eq!(vec![Bytes::from(format!("data{}", i))], records[0].data);
        }
        Ok(())
    }

    #[test]
    fn test_evict() -> Result<(), Box<dyn Error>> {
        let cache = HotCache::new(4096);
        cache.insert(1, 0, 1, vec![Bytes::from(vec![0; 4096])]);
        assert_eq!(4096, cache.get_block(1, 0, 1, 4096).len());
        // expect evict the last record
        cache.insert(1, 1, 2, vec![Bytes::from(vec![0; 1024])]);
        assert_eq!(0, cache.get_block(1, 0, 1, 4096).len());
        assert_eq!(1024, cache.get_block(1, 1, 2, 4096).len());

        cache.insert(1, 3, 3, vec![Bytes::from(vec![0; 1024])]);
        assert_eq!(2048, cache.get_block(1, 1, 5, 4096).len());
        Ok(())
    }
}
