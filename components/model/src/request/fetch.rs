use protocol::rpc::header::{FetchRequestT, RangeT};

use crate::range::RangeMetadata;

#[derive(Debug, Clone, PartialEq)]
pub struct FetchRequest {
    pub max_wait: std::time::Duration,

    pub range: RangeMetadata,

    /// Position to read from
    pub offset: u64,

    /// Records within [range.start_offset, limit) are valid and safe to read.
    pub limit: u64,

    pub min_bytes: Option<usize>,

    pub max_bytes: Option<usize>,
}

impl From<&FetchRequest> for FetchRequestT {
    fn from(value: &FetchRequest) -> Self {
        let mut res = FetchRequestT::default();
        res.max_wait_ms = value.max_wait.as_millis() as i32;
        let mut range = RangeT::default();
        range.stream_id = value.range.stream_id();
        range.index = value.range.index();
        range.start = value.range.start() as i64;
        if let Some(end) = value.range.end() {
            range.end = end as i64;
        }
        res.range = Box::new(range);
        res.offset = value.offset as i64;
        res.limit = value.limit as i64;
        if let Some(min) = value.min_bytes {
            res.min_bytes = min as i32;
        }

        if let Some(max) = value.max_bytes {
            res.max_bytes = max as i32;
        }
        res
    }
}
