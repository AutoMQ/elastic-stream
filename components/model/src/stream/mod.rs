use crate::{
    error::StreamError,
    range::{Range, StreamRange},
};

/// Stream is the basic storage unit in the system that store records in an append-only fashion.
pub struct Stream {
    id: i64,
    ranges: Vec<StreamRange>,
}

impl Stream {
    pub fn new(id: i64, ranges: Vec<StreamRange>) -> Self {
        Self { id, ranges }
    }

    pub fn with_id(id: i64) -> Self {
        Self { id, ranges: vec![] }
    }

    pub fn push(&mut self, range: StreamRange) {
        self.ranges.push(range);
    }

    // Sort ranges
    pub fn sort(&mut self) {
        self.ranges.sort_by(|a, b| a.index().cmp(&b.index()));
    }

    pub fn seal(&mut self, committed: u64, range_index: i32) -> Result<u64, StreamError> {
        if let Some(range) = self.ranges.last_mut() {
            if range.index() == range_index {
                if range.is_sealed() {
                    return Err(StreamError::AlreadySealed);
                }
                range.set_limit(committed);
                Ok(range.seal().map_err(|_e| StreamError::SealBadOffset)?)
            } else {
                Err(StreamError::RangeIndexMismatch {
                    target: range_index,
                    actual: range.index(),
                })
            }
        } else {
            Err(StreamError::SealWrongNode)
        }
    }

    /// A stream is mutable iff its last range is not sealed.
    pub fn is_mut(&self) -> bool {
        self.ranges
            .last()
            .and_then(|range| Some(!range.is_sealed()))
            .unwrap_or(false)
    }

    pub fn last(&self) -> Option<&StreamRange> {
        self.ranges.last()
    }

    pub fn range(&self, index: i32) -> Option<StreamRange> {
        self.ranges
            .iter()
            .try_find(|&range| Some(range.index() == index))
            .flatten()
            .map(|range| range.clone())
    }

    pub fn refresh(&mut self, ranges: Vec<StreamRange>) {
        let to_append = ranges
            .into_iter()
            .filter(|range| {
                self.ranges
                    .iter()
                    .find(|r| range.index() == r.index())
                    .is_none()
            })
            .collect::<Vec<_>>();
        self.ranges.extend(to_append);
    }
}
