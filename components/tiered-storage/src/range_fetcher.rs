use bytes::Bytes;
use store::{error::FetchError, option::ReadOptions, Store};

pub trait RangeFetcher {
    async fn fetch(
        &self,
        stream_id: u64,
        range_index: u32,
        start_offset: u64,
        end_offset: u64,
        max_size: u32,
    ) -> Result<RangeFetchResult, FetchError>;
}

pub struct RangeFetchResult {
    pub payload: Vec<Bytes>,
}

impl RangeFetchResult {
    pub fn new(payload: Vec<Bytes>) -> Self {
        Self { payload }
    }
}

struct DefaultRangeFetcher<S> {
    store: S,
}

impl<S> RangeFetcher for DefaultRangeFetcher<S>
where
    S: Store + 'static,
{
    async fn fetch(
        &self,
        stream_id: u64,
        range_index: u32,
        start_offset: u64,
        end_offset: u64,
        max_size: u32,
    ) -> Result<crate::RangeFetchResult, store::error::FetchError> {
        let read_option = ReadOptions {
            stream_id: stream_id as i64,
            range: range_index,
            offset: start_offset as i64,
            max_offset: end_offset,
            max_wait_ms: 3000,
            max_bytes: max_size as i32,
        };
        let store_result = self.store.fetch(read_option).await?;
        assert_eq!(store_result.results.len(), 1);
        let payload: Vec<Bytes> = store_result.results[0].iter().cloned().collect();
        Ok(crate::RangeFetchResult { payload })
    }
}

impl<S> DefaultRangeFetcher<S>
where
    S: Store + 'static,
{
    #[allow(dead_code)]
    pub fn new(store: S) -> Self {
        Self { store }
    }
}
