use bytes::Bytes;
use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::fmt;
use std::rc::Rc;

pub(crate) struct RecordBatchCache {
    max_cache_size: usize,
    occupied_size: RefCell<usize>,
    map: RefCell<HashMap<Rc<CacheKey>, Rc<CacheValue>>>,
    fifo_deque: RefCell<VecDeque<Rc<CacheKey>>>,
}

impl RecordBatchCache {
    pub(crate) fn new(max_cache_size: usize) -> Self {
        if max_cache_size < 4 * 1024 {
            panic!("max_cache_size must be greater than 4KB");
        }
        let cache_entry_size = 4 * 1024;
        Self {
            max_cache_size,
            occupied_size: RefCell::new(0),
            map: RefCell::new(HashMap::with_capacity(max_cache_size / cache_entry_size)),
            fifo_deque: RefCell::new(VecDeque::with_capacity(max_cache_size / cache_entry_size)),
        }
    }

    #[allow(dead_code)]
    pub(crate) fn insert(
        &self,
        stream_id: u64,
        range_index: u32,
        base_offset: u64,
        count: u32,
        data: Vec<Bytes>,
    ) {
        let mut occupied_size = self.occupied_size.borrow_mut();
        let mut fifo_deque = self.fifo_deque.borrow_mut();
        let mut map = self.map.borrow_mut();
        while *occupied_size >= self.max_cache_size {
            if let Some(cache_key) = fifo_deque.pop_front() {
                if let Some(cache_value) = map.remove(&cache_key) {
                    *occupied_size -= cache_value.size();
                }
            } else {
                break;
            }
        }
        let cache_key = Rc::new(CacheKey::new(stream_id, range_index, base_offset));
        let cache_value = Rc::new(CacheValue::new(count, data));
        *occupied_size += cache_value.size();
        fifo_deque.push_back(cache_key.clone());
        map.insert(cache_key, cache_value);
    }

    #[allow(dead_code)]
    pub(crate) fn get(
        &self,
        stream_id: u64,
        range_index: u32,
        base_offset: u64,
    ) -> Option<Rc<CacheValue>> {
        self.map
            .borrow()
            .get(&CacheKey::new(stream_id, range_index, base_offset))
            .cloned()
    }
}

impl fmt::Debug for RecordBatchCache {
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
    range_index: u32,
    base_offset: u64,
}

impl CacheKey {
    pub(crate) fn new(stream_id: u64, range_index: u32, base_offset: u64) -> Self {
        Self {
            stream_id,
            range_index,
            base_offset,
        }
    }
}

pub(crate) struct CacheValue {
    #[allow(dead_code)]
    pub count: u32,
    pub data: Vec<Bytes>,
}

impl CacheValue {
    #[allow(dead_code)]
    pub(crate) fn new(count: u32, data: Vec<Bytes>) -> Self {
        Self { count, data }
    }

    pub fn size(&self) -> usize {
        self.data.iter().map(|b| b.len()).sum()
    }
}

#[cfg(test)]
mod test {
    use crate::stream::cache::RecordBatchCache;
    use bytes::Bytes;
    use std::error::Error;

    #[test]
    fn test_cache_insert_get() -> Result<(), Box<dyn Error>> {
        let cache = RecordBatchCache::new(4096);
        let stream_id = 1;
        let range_index = 0;
        for i in 0..100 {
            let base_offset = i * 10;
            let count = 10;
            let data = vec![Bytes::from(format!("data{}", i))];
            cache.insert(stream_id, range_index, base_offset, count, data.clone());
            let cache_value = cache.get(stream_id, range_index, base_offset).unwrap();
            assert_eq!(cache_value.count, count);
            assert_eq!(cache_value.data, data);
        }
        for i in 0..100 {
            let base_offset = i * 10;
            let cache_value = cache.get(stream_id, range_index, base_offset).unwrap();
            assert_eq!(cache_value.count, 10);
            assert_eq!(cache_value.data, vec![Bytes::from(format!("data{}", i))]);
        }
        Ok(())
    }

    #[test]
    fn test_evict() -> Result<(), Box<dyn Error>> {
        let cache = RecordBatchCache::new(4096);
        cache.insert(1, 1, 0, 1, vec![Bytes::from(vec![0; 4096])]);
        assert_eq!((*cache.get(1, 1, 0).unwrap()).count, 1);
        // expect evict the last record
        cache.insert(1, 1, 1, 2, vec![Bytes::from(vec![0; 1024])]);
        assert!(cache.get(1, 1, 0).is_none());
        assert_eq!((*cache.get(1, 1, 1).unwrap()).count, 2);

        cache.insert(1, 1, 2, 3, vec![Bytes::from(vec![0; 1024])]);
        assert_eq!((*cache.get(1, 1, 1).unwrap()).count, 2);
        assert_eq!((*cache.get(1, 1, 2).unwrap()).count, 3);
        Ok(())
    }
}
