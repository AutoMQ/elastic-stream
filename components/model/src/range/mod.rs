use std::fmt::{self, Display, Formatter};

use derivative::Derivative;

use crate::data_node::DataNode;

pub trait Range {
    fn is_sealed(&self) -> bool;

    fn seal(&mut self) -> u64;
}

/// Representation of a stream range in form of `[start, end)` in which `start` is inclusive and `end` is exclusive.
/// If `start` == `end`, there will be no valid records in the range.
///
/// At the beginning, `end` will be `None` and it would grow as more slots are taken from the range.
/// Once the range is sealed, it becomes immutable and its right boundary becomes fixed.
#[derive(Derivative)]
#[derivative(Debug, PartialEq, Clone)]
pub struct StreamRange {
    stream_id: i64,

    index: i32,

    /// The start slot index, inclusive.
    start: u64,

    /// TODO: remove this field.
    limit: u64,

    /// The end of the range, exclusive
    end: Option<u64>,

    /// List of data nodes, that all have identical records within the range.
    #[derivative(PartialEq = "ignore")]
    replica: Vec<DataNode>,

    /// Node ID of the primary replica
    primary: Option<i32>,
}

impl StreamRange {
    pub fn new(stream_id: i64, index: i32, start: u64, limit: u64, end: Option<u64>) -> Self {
        Self {
            stream_id,
            index,
            start,
            limit,
            end,
            replica: vec![],
            primary: None,
        }
    }

    pub fn set_primary(&mut self, node_id: i32) {
        self.primary = Some(node_id);
    }

    pub fn primary(&self) -> Option<i32> {
        self.primary
    }

    pub fn replica(&self) -> &Vec<DataNode> {
        &self.replica
    }

    pub fn replica_mut(&mut self) -> &mut Vec<DataNode> {
        &mut self.replica
    }

    /// Test if the given offset is within the range.
    pub fn contains(&self, offset: u64) -> bool {
        if self.start > offset {
            return false;
        }

        match self.end {
            None => true,
            Some(end) => offset < end,
        }
    }

    /// Length of the range.
    /// That is, number of records in the stream range.
    pub fn len(&self) -> u64 {
        self.limit - self.start
    }

    pub fn stream_id(&self) -> i64 {
        self.stream_id
    }

    pub fn index(&self) -> i32 {
        self.index
    }

    pub fn start(&self) -> u64 {
        self.start
    }

    pub fn end(&self) -> Option<u64> {
        self.end
    }

    pub fn limit(&self) -> u64 {
        self.limit
    }

    /// Update `limit` of the range. Slots within `[start, limit)` shall hold valid records.
    ///
    /// if a range is immutable, it's not allowed to change its `limit`.
    pub fn set_limit(&mut self, limit: u64) {
        if self.end().is_none() {
            self.limit = limit;
        }
    }
}

impl Range for StreamRange {
    fn is_sealed(&self) -> bool {
        self.end.is_some()
    }

    fn seal(&mut self) -> u64 {
        match self.end {
            None => {
                self.end = Some(self.limit);
                self.limit
            }
            Some(offset) => offset,
        }
    }
}

impl Display for StreamRange {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{{stream-id={}, index={}}}=[{}, {}, {:?})",
            self.stream_id, self.index, self.start, self.limit, self.end
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_take_slot() {
        let mut range = StreamRange::new(0, 0, 0, 0, None);
        assert_eq!(range.is_sealed(), false);
        assert_eq!(range.len(), 0);
        assert_eq!(range.primary(), None);

        range.set_limit(100);
        assert_eq!(100, range.len());
        assert_eq!(100, range.seal());

        // Double seal should return the same offset.
        assert_eq!(100, range.seal());

        assert_eq!(range.is_sealed(), true);
    }

    #[test]
    fn test_contains() {
        // Test a sealed range that contains a given offset.
        let range = StreamRange::new(0, 0, 0, 10, Some(10));
        assert_eq!(range.contains(0), true);
        assert_eq!(range.contains(1), true);
        assert_eq!(range.contains(11), false);

        // Test a open range that contains a given offset.
        let range = StreamRange::new(0, 0, 10, 20, None);
        assert_eq!(range.contains(0), false);
        assert_eq!(range.contains(11), true);
        assert_eq!(range.contains(21), true);
    }
}
