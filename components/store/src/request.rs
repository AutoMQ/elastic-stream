use std::cmp::Ordering;

use model::Batch;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd)]
pub struct AppendRecordRequest {
    /// Stream ID
    pub stream_id: i64,

    /// Range index
    pub range: i32,

    pub offset: i64,

    pub len: usize,

    pub buffer: bytes::Bytes,
}

impl Batch for AppendRecordRequest {
    fn offset(&self) -> u64 {
        self.offset as u64
    }

    fn len(&self) -> usize {
        self.len
    }
}

impl Ord for AppendRecordRequest {
    fn cmp(&self, other: &Self) -> Ordering {
        other.offset.cmp(&self.offset)
    }
}
