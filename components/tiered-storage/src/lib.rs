#![feature(async_fn_in_trait)]
#![feature(map_try_insert)]

pub mod object_storage;
mod range_accumulator;
pub mod range_offload;

use bytes::Bytes;
use store::error::FetchError;

pub trait TieredStorage {
    fn add_range(&self, stream_id: u64, range_index: u32, start_offset: u64, end_offset: u64);

    /// new record arrived notify
    fn new_record_arrived(
        &self,
        stream_id: u64,
        range_index: u32,
        end_offset: u64,
        record_size: u32,
    );
}

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
    pub end_offset: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RangeKey {
    stream_id: u64,
    range_index: u32,
}

impl RangeKey {
    pub fn new(stream_id: u64, range_index: u32) -> Self {
        Self {
            stream_id,
            range_index,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {}
}
