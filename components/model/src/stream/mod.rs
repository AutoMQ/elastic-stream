use std::time::Duration;

use protocol::rpc::header::StreamT;

use crate::{error::StreamError, range::Range};

/// Stream is the basic storage unit in the system that store records in an append-only fashion.
///
/// A stream is composed of ranges. Conceptually, only the last range of the stream is mutable while the rest are immutable. Ranges of a
/// stream are distributed among data-nodes.
///
/// `Stream` on a specific data-node only cares about ranges that are located on it.
#[derive(Debug, Default)]
pub struct Stream {
    /// Stream ID, unique within the cluster.
    pub stream_id: i64,

    pub replica: u8,

    pub retention_period: Duration,

    /// Ranges of the stream that are placed onto current data node.
    pub ranges: Vec<Range>,
}

impl Stream {
    pub fn push(&mut self, range: Range) {
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
                range
                    .seal(committed)
                    .map_err(|_e| StreamError::AlreadySealed)
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
            .map(|range| !range.is_sealed())
            .unwrap_or_default()
    }

    pub fn last(&self) -> Option<&Range> {
        self.ranges.last()
    }

    pub fn range(&self, index: i32) -> Option<Range> {
        self.ranges
            .iter()
            .try_find(|&range| Some(range.index() == index))
            .flatten()
            .cloned()
    }

    /// Find the range that contains the given offset.
    ///
    /// # Arguments
    /// `offset` - The offset to find.
    ///
    /// # Returns
    /// The range that contains the given offset, or None if not found.
    pub fn range_of(&self, offset: u64) -> Option<Range> {
        self.ranges
            .iter()
            .try_find(|&range| Some(range.contains(offset)))
            .flatten()
            .cloned()
    }

    pub fn refresh(&mut self, ranges: Vec<Range>) {
        let to_append = ranges
            .into_iter()
            .filter(|range| !self.ranges.iter().any(|r| range.index() == r.index()))
            .collect::<Vec<_>>();
        self.ranges.extend(to_append);
    }
}

/// Converter from `StreamT` to `Stream`.
impl From<StreamT> for Stream {
    fn from(stream: StreamT) -> Self {
        Self {
            stream_id: stream.stream_id,
            replica: stream.replica as u8,
            retention_period: Duration::from_millis(stream.retention_period_ms as u64),
            ranges: vec![],
        }
    }
}

/// Converter from `&Stream` to `StreamT`.
impl From<&Stream> for StreamT {
    fn from(stream: &Stream) -> Self {
        let mut stream_t = StreamT::default();
        stream_t.stream_id = stream.stream_id;
        stream_t.replica = stream.replica as i8;
        stream_t.retention_period_ms = stream.retention_period.as_millis() as i64;
        stream_t
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error;

    use super::*;

    #[test]
    fn test_sort_stream() {
        // Construct a stream with 3 ranges, and sort it.
        let mut stream: Stream = StreamT::default().into();
        stream.push(Range::new(0, 1, 0, 0, None));
        stream.push(Range::new(0, 0, 0, 0, None));
        stream.push(Range::new(0, 2, 0, 0, None));
        stream.sort();

        // Check the ranges are sorted by index.
        assert_eq!(stream.ranges[0].index(), 0);
        assert_eq!(stream.ranges[1].index(), 1);
        assert_eq!(stream.ranges[2].index(), 2);
    }

    #[test]
    fn test_seal_stream() -> Result<(), Box<dyn Error>> {
        // Construct a stream with 3 ranges, and seal the last range.
        let mut stream: Stream = StreamT::default().into();
        stream.push(Range::new(0, 0, 0, 0, None));
        stream.push(Range::new(0, 1, 0, 10, Some(20)));
        stream.push(Range::new(0, 2, 0, 20, None));
        stream.seal(0, 2)?;

        // Check the last range is sealed.
        assert_eq!(stream.ranges.last().unwrap().is_sealed(), true);
        Ok(())
    }

    #[test]
    fn test_query_from_stream() {
        // Construct a complete stream through a iterator.
        let mut stream: Stream = StreamT::default().into();

        (0..10)
            .map(|i| {
                let mut sr = Range::new(0, i, 0, i as u64 * 10, None);
                // Seal the range if it's not the last one.
                if i != 9 {
                    let _ = sr.seal((i as u64 + 1) * 10);
                }
                sr
            })
            .for_each(|range| stream.push(range));

        // Test the range_of method.
        assert_eq!(stream.range_of(0).unwrap().index(), 0);
        assert_eq!(stream.range_of(5).unwrap().index(), 0);
        assert_eq!(stream.range_of(10).unwrap().index(), 1);
        assert_eq!(stream.range_of(15).unwrap().index(), 1);
        assert_eq!(stream.range_of(20).unwrap().index(), 2);

        // Test the range method
        assert_eq!(stream.range(5).unwrap().start(), 50);
        assert_eq!(stream.range(5).unwrap().is_sealed(), true);
    }
}
