use std::cmp::Ordering;

use model::Batch;

#[derive(Clone, Debug)]
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

impl PartialEq for AppendRecordRequest {
    fn eq(&self, other: &Self) -> bool {
        self.stream_id == other.stream_id
            && self.range == other.range
            && self.offset == other.offset
    }
}

impl PartialOrd for AppendRecordRequest {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        match other.stream_id.partial_cmp(&self.stream_id) {
            Some(Ordering::Equal) => {}
            ord => return ord,
        }

        match other.range.partial_cmp(&self.range) {
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

        match other.range.cmp(&self.range) {
            Ordering::Equal => {}
            Ordering::Greater => return Ordering::Greater,
            Ordering::Less => return Ordering::Less,
        }

        other.offset.cmp(&self.offset)
    }
}

#[cfg(test)]
mod tests {

    use bytes::Bytes;
    use std::{collections::BinaryHeap, error::Error};

    use super::AppendRecordRequest;

    #[test]
    fn test_order() -> Result<(), Box<dyn Error>> {
        let mut requests = BinaryHeap::new();

        let buffer = Bytes::from_static(b"test");

        let req1 = AppendRecordRequest {
            stream_id: 0,
            range: 0,
            offset: 0,
            len: 2,
            buffer: buffer.clone(),
        };

        let req2 = AppendRecordRequest {
            stream_id: 0,
            range: 0,
            offset: 2,
            len: 2,
            buffer: buffer.clone(),
        };

        requests.push(req2);
        requests.push(req1.clone());

        assert_eq!(Some(req1), requests.pop());
        Ok(())
    }
}
