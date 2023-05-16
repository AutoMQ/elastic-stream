use log::{error, info};
use model::{range::RangeMetadata, stream::StreamMetadata};

use crate::error::ServiceError;

use super::range::Range;

pub(crate) struct Stream {
    metadata: StreamMetadata,

    ranges: Vec<Range>,
}

impl Stream {
    pub(crate) fn new(metadata: StreamMetadata) -> Self {
        Self {
            metadata,
            ranges: Vec::new(),
        }
    }

    fn verify_stream_id(&self, metadata: &RangeMetadata) -> Result<(), ServiceError> {
        if self.metadata.stream_id != metadata.stream_id() as u64 {
            error!(
                "Stream-id mismatch, stream_id={}, metadata={}",
                self.metadata.stream_id, metadata
            );
            return Err(ServiceError::Internal("Stream-id mismatch".to_owned()));
        }
        Ok(())
    }

    pub(crate) fn create_range(&mut self, metadata: RangeMetadata) -> Result<(), ServiceError> {
        self.verify_stream_id(&metadata)?;

        let existed = self
            .ranges
            .iter()
            .any(|range: &Range| range.metadata.index() == metadata.index());
        if existed {
            info!("Range already exists, metadata={}", metadata);
            return Err(ServiceError::AlreadyExisted);
        }

        self.ranges.push(Range::new(metadata));
        self.sort();
        Ok(())
    }

    // Sort ranges
    fn sort(&mut self) {
        self.ranges
            .sort_by(|a, b| a.metadata.index().cmp(&b.metadata.index()));
    }

    pub(crate) fn commit(&mut self, offset: u64) {
        if let Some(range) = self.ranges.last_mut() {
            range.commit(offset);
        }
    }

    pub(crate) fn seal(&mut self, metadata: &mut RangeMetadata) -> Result<(), ServiceError> {
        self.verify_stream_id(&metadata)?;

        let existed = self
            .ranges
            .iter()
            .any(|range: &Range| range.metadata.index() == metadata.index());
        if !existed {
            info!("Range does not exist, metadata={}", metadata);
            return Err(ServiceError::NotFound(format!(
                "Range does not exist, metadata={}",
                metadata
            )));
        }

        let range = self
            .ranges
            .iter_mut()
            .find(|range: &&mut Range| range.metadata.index() == metadata.index())
            .unwrap();
        range.seal(metadata);
        self.sort();
        Ok(())
    }

    pub(crate) fn range_of(&self, offset: u64) -> Option<Range> {
        self.ranges
            .iter()
            .find(|&range| range.metadata.contains(offset))
            .cloned()
    }
}

mod tests {
    // #[test]
    // fn test_sort_stream() {
    //     // Construct a stream with 3 ranges, and sort it.
    //     let mut stream: Stream = StreamT::default().into();
    //     stream.push(RangeMetadata::new(0, 1, 0, 0, None));
    //     stream.push(RangeMetadata::new(0, 0, 0, 0, None));
    //     stream.push(RangeMetadata::new(0, 2, 0, 0, None));
    //     stream.sort();

    //     // Check the ranges are sorted by index.
    //     assert_eq!(stream.ranges[0].index(), 0);
    //     assert_eq!(stream.ranges[1].index(), 1);
    //     assert_eq!(stream.ranges[2].index(), 2);
    // }

    // #[test]
    // fn test_seal_stream() -> Result<(), Box<dyn Error>> {
    //     // Construct a stream with 3 ranges, and seal the last range.
    //     let mut stream: Stream = StreamT::default().into();
    //     stream.push(RangeMetadata::new(0, 0, 0, 0, None));
    //     stream.push(RangeMetadata::new(0, 1, 0, 10, Some(20)));
    //     stream.push(RangeMetadata::new(0, 2, 0, 20, None));
    //     stream.seal(0, 2)?;

    //     // Check the last range is sealed.
    //     assert_eq!(stream.ranges.last().unwrap().is_sealed(), true);
    //     Ok(())
    // }

    // #[test]
    // fn test_query_from_stream() {
    //     // Construct a complete stream through a iterator.
    //     let mut stream: Stream = StreamT::default().into();

    //     (0..10)
    //         .map(|i| {
    //             let mut sr = RangeMetadata::new(0, i, 0, i as u64 * 10, None);
    //             // Seal the range if it's not the last one.
    //             if i != 9 {
    //                 sr.set_end((i as u64 + 1) * 10);
    //             }
    //             sr
    //         })
    //         .for_each(|range| stream.push(range));

    //     // Test the range_of method.
    //     assert_eq!(stream.range_of(0).unwrap().index(), 0);
    //     assert_eq!(stream.range_of(5).unwrap().index(), 0);
    //     assert_eq!(stream.range_of(10).unwrap().index(), 1);
    //     assert_eq!(stream.range_of(15).unwrap().index(), 1);
    //     assert_eq!(stream.range_of(20).unwrap().index(), 2);

    //     // Test the range method
    //     assert_eq!(stream.range(5).unwrap().start(), 50);
    //     assert_eq!(stream.range(5).unwrap().is_sealed(), true);
    // }
}
