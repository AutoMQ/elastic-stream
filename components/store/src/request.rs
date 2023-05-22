use std::cmp::Ordering;

use model::Batch;

#[derive(Clone, Debug)]
pub struct AppendRecordRequest {
    /// Stream ID
    pub stream_id: i64,

    /// Range index
    pub range_index: i32,

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

impl PartialEq for AppendRecordRequest {
    fn eq(&self, other: &Self) -> bool {
        self.stream_id == other.stream_id
            && self.range_index == other.range_index
            && self.offset == other.offset
    }
}

impl PartialOrd for AppendRecordRequest {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        match other.stream_id.partial_cmp(&self.stream_id) {
            Some(Ordering::Equal) => {}
            ord => return ord,
        }

        match other.range_index.partial_cmp(&self.range_index) {
            Some(Ordering::Equal) => {}
            ord => return ord,
        }

        other.offset.partial_cmp(&self.offset)
    }
}

impl Eq for AppendRecordRequest {}

impl Ord for AppendRecordRequest {
    fn cmp(&self, other: &Self) -> Ordering {
        match other.stream_id.cmp(&self.stream_id) {
            Ordering::Equal => {}
            Ordering::Greater => return Ordering::Greater,
            Ordering::Less => return Ordering::Less,
        }

        match other.range_index.cmp(&self.range_index) {
            Ordering::Equal => {}
            Ordering::Greater => return Ordering::Greater,
            Ordering::Less => return Ordering::Less,
        }

        other.offset.cmp(&self.offset)
    }
}
