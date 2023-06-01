use bytes::Bytes;
use moka::sync::Cache;
use std::fmt;
use std::sync::Arc;

pub(crate) struct RecordBatchCache {
    cache: Cache<CacheKey, Arc<CacheValue>>,
}

impl RecordBatchCache {
    pub(crate) fn new() -> Self {
        // TODO: make cache configurable
        let cache: Cache<CacheKey, Arc<CacheValue>> = Cache::builder()
            .weigher(|_k, v: &Arc<CacheValue>| v.data.len() as u32)
            .max_capacity(1024 * 1024 * 1024)
            .build();
        Self { cache }
    }

    pub(crate) fn insert(
        &self,
        stream_id: u64,
        range_index: u32,
        base_offset: u64,
        count: u32,
        data: Vec<Bytes>,
    ) {
        self.cache.insert(
            CacheKey::new(stream_id, range_index, base_offset),
            Arc::new(CacheValue::new(count, data)),
        );
    }

    pub(crate) fn get(
        &self,
        stream_id: u64,
        range_index: u32,
        base_offset: u64,
    ) -> Option<Arc<CacheValue>> {
        self.cache
            .get(&CacheKey::new(stream_id, range_index, base_offset))
    }
}

impl fmt::Debug for RecordBatchCache {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let cache_size = self.cache.weighted_size();
        write!(f, "RecordBatchCache cache_size={cache_size}")
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
    pub count: u32,
    pub data: Vec<Bytes>,
}

impl CacheValue {
    pub(crate) fn new(count: u32, data: Vec<Bytes>) -> Self {
        Self { count, data }
    }
}
