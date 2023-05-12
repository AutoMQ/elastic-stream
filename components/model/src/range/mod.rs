use std::fmt::{self, Display, Formatter};

use derivative::Derivative;
use protocol::rpc::header::{DataNodeT, RangeT};

use crate::{data_node::DataNode, error::RangeError};

/// Representation of a stream range in form of `[start, end)` in which `start` is inclusive and `end` is exclusive.
/// If `start` == `end`, there will be no valid records in the range.
///
/// At the beginning, `end` will be `None` and it would grow as more slots are taken from the range.
/// Once the range is sealed, it becomes immutable and its right boundary becomes fixed.
#[derive(Derivative)]
#[derivative(Debug, PartialEq, Clone)]
pub struct Range {
    stream_id: i64,

    epoch: u64,

    /// Range index
    index: i32,

    /// The start slot index, inclusive.
    start: u64,

    /// The end of the range, exclusive
    end: Option<u64>,

    /// List of data nodes, that all have identical records within the range.
    #[derivative(PartialEq = "ignore")]
    replica: Vec<DataNode>,
}

impl Range {
    pub fn new(stream_id: i64, index: i32, epoch: u64, start: u64, end: Option<u64>) -> Self {
        Self {
            stream_id,
            index,
            epoch,
            start,
            end,
            replica: vec![],
        }
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

    pub fn stream_id(&self) -> i64 {
        self.stream_id
    }

    pub fn index(&self) -> i32 {
        self.index
    }

    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    pub fn start(&self) -> u64 {
        self.start
    }

    pub fn end(&self) -> Option<u64> {
        self.end
    }

    pub fn is_sealed(&self) -> bool {
        self.end.is_some()
    }

    pub fn seal(&mut self, end: u64) -> Result<u64, RangeError> {
        match self.end {
            None => {
                self.end = Some(end);
                Ok(end)
            }
            Some(offset) => Err(RangeError::AlreadySealed(offset)),
        }
    }
}

impl Display for Range {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{{stream-id={}, index={}}}=[{}, {:?})",
            self.stream_id, self.index, self.start, self.end
        )
    }
}

impl From<&Range> for RangeT {
    fn from(value: &Range) -> Self {
        let mut range = RangeT::default();
        range.stream_id = value.stream_id;
        range.epoch = value.epoch as i64;
        range.index = value.index;
        range.start = value.start as i64;
        range.end = match value.end {
            None => -1,
            Some(offset) => offset as i64,
        };
        let mut replica: Vec<DataNodeT> = vec![];
        for node in &value.replica {
            replica.push(node.into());
        }
        if replica.is_empty() {
            range.nodes = None;
        } else {
            range.nodes = Some(replica);
        }
        range
    }
}

impl From<&RangeT> for Range {
    fn from(value: &RangeT) -> Self {
        let mut replica: Vec<DataNode> = vec![];
        if let Some(nodes) = &value.nodes {
            for node in nodes {
                replica.push(node.into());
            }
        }
        Self {
            stream_id: value.stream_id,
            epoch: value.epoch as u64,
            index: value.index,
            start: value.start as u64,
            end: match value.end {
                -1 => None,
                offset => Some(offset as u64),
            },
            replica,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_take_slot() {
        let mut range = Range::new(0, 0, 0, 0, None);
        assert_eq!(range.is_sealed(), false);

        // Double seal should return the same offset.
        assert_eq!(Ok(100), range.seal(100));
        assert_eq!(range.is_sealed(), true);

        assert_eq!(Err(RangeError::AlreadySealed(100)), range.seal(101));

        assert_eq!(range.is_sealed(), true);
    }

    #[test]
    fn test_contains() {
        // Test a sealed range that contains a given offset.
        let range = Range::new(0, 0, 0, 0, Some(10));
        assert_eq!(range.contains(0), true);
        assert_eq!(range.contains(1), true);
        assert_eq!(range.contains(11), false);

        // Test a open range that contains a given offset.
        let range = Range::new(0, 0, 0, 10, None);
        assert_eq!(range.contains(0), false);
        assert_eq!(range.contains(11), true);
        assert_eq!(range.contains(21), true);
    }
}
