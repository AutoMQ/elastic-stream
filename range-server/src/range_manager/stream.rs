use log::{error, info, trace, warn};
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
        if self
            .metadata
            .stream_id
            .expect("Stream-id should be present")
            != metadata.stream_id() as u64
        {
            error!(
                "Stream-id mismatch, stream_id={:?}, metadata={}",
                self.metadata.stream_id, metadata
            );
            return Err(ServiceError::Internal("Stream-id mismatch".to_owned()));
        }
        Ok(())
    }

    pub(crate) fn reset_commit(&mut self, range_index: i32, offset: u64) {
        self.ranges
            .iter_mut()
            .filter(|range| range.metadata.index() == range_index)
            .for_each(|range| range.reset(offset));
    }

    pub(crate) fn create_range(&mut self, metadata: RangeMetadata) {
        if self.verify_stream_id(&metadata).is_err() {
            return;
        }

        let res = self
            .ranges
            .iter()
            .find(|range| range.metadata.index() == metadata.index());

        if let Some(range) = res {
            if range.metadata == metadata {
                trace!("No-op, when creating range with the same metadata");
                return;
            } else {
                error!(
                    "Attempting to create inconsistent range. Prior range-metadata={}, attempted range-metadata={:?}",
                    range.metadata, metadata
                );
                return;
            }
        }

        if let Some(range) = self.ranges.iter().next_back() {
            if range.metadata.index() > metadata.index() {
                warn!(
                    "Attempting to create a range which should have been sealed: {:?}. Last range on current server: {:?}",
                    metadata, range.metadata
                );
            }
        }

        self.ranges.push(Range::new(metadata));
        self.sort();
    }

    // Sort ranges
    fn sort(&mut self) {
        self.ranges
            .sort_by(|a, b| a.metadata.index().cmp(&b.metadata.index()));
    }

    pub(crate) fn seal(&mut self, metadata: &mut RangeMetadata) -> Result<(), ServiceError> {
        self.verify_stream_id(metadata)?;
        if let Some(range) = self.get_range(metadata.index()) {
            range.seal(metadata);
            Ok(())
        } else {
            info!("Range does not exist, metadata={:?}. Create the sealed range on range-server directly", metadata);
            Err(ServiceError::NotFound(format!(
                "Range[{}#{}] is not found",
                metadata.stream_id(),
                metadata.index()
            )))
        }
    }

    pub(crate) fn get_range(&mut self, index: i32) -> Option<&mut Range> {
        self.ranges
            .iter_mut()
            .find(|range| range.metadata.index() == index)
    }

    pub(crate) fn has_range(&self, index: i32) -> bool {
        self.ranges
            .iter()
            .any(|range| range.metadata.index() == index)
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error;

    use model::{range::RangeMetadata, stream::StreamMetadata, Batch};
    use protocol::rpc::header::StreamT;

    #[test]
    fn test_new() -> Result<(), Box<dyn Error>> {
        let mut stream = StreamT::default();
        stream.stream_id = 1;
        stream.replica = 1;
        stream.retention_period_ms = 1000;
        let stream = StreamMetadata::from(stream);
        assert_eq!(stream.stream_id, Some(1));
        assert_eq!(stream.replica, 1);
        assert_eq!(stream.retention_period.as_millis(), 1000);
        Ok(())
    }

    #[test]
    fn test_create_range() -> Result<(), Box<dyn Error>> {
        let mut stream = StreamT::default();
        stream.stream_id = 1;
        stream.replica = 1;
        stream.retention_period_ms = 1000;
        let stream_metadata = StreamMetadata::from(stream);
        let mut stream = super::Stream::new(stream_metadata);

        let range = RangeMetadata::new(1, 0, 0, 0, None);
        let range = RangeMetadata::from(range);
        stream.create_range(range);

        assert_eq!(stream.ranges.len(), 1);

        Ok(())
    }

    #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
    struct Foo {
        offset: u64,
        len: u32,
    }

    impl Batch for Foo {
        fn len(&self) -> u32 {
            self.len
        }

        fn offset(&self) -> u64 {
            self.offset
        }
    }

    #[test]
    fn test_seal() -> Result<(), Box<dyn Error>> {
        let mut stream = StreamT::default();
        stream.stream_id = 1;
        stream.replica = 1;
        stream.retention_period_ms = 1000;
        let stream_metadata = StreamMetadata::from(stream);
        let mut stream = super::Stream::new(stream_metadata);

        let range = RangeMetadata::new(1, 0, 0, 0, None);
        let range = RangeMetadata::from(range);
        stream.create_range(range);

        let range = stream.get_range(0).expect("The first range should exist");
        range.window_mut().and_then(|window| {
            window
                .check_barrier(&Foo {
                    offset: 0,
                    len: 100,
                })
                .ok()
        });
        range.commit(0);
        let mut metadata = RangeMetadata::new(1, 0, 0, 0, Some(50));
        stream.seal(&mut metadata)?;

        let committed = stream
            .ranges
            .iter()
            .nth(0)
            .expect("The first range should exist")
            .committed();
        assert_eq!(committed, Some(100));
        Ok(())
    }
}
