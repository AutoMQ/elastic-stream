use log::warn;
use model::range::RangeMetadata;

#[derive(Debug, Clone)]
pub(crate) struct Range {
    pub(crate) metadata: RangeMetadata,
    committed: Option<u64>,
}

impl Range {
    pub(crate) fn new(metadata: RangeMetadata) -> Self {
        Self {
            metadata,
            committed: None,
        }
    }

    pub(crate) fn committed(&self) -> Option<u64> {
        self.committed
    }

    pub(crate) fn commit(&mut self, offset: u64) {
        if let Some(ref mut committed) = self.committed {
            if offset <= *committed {
                warn!("Try to commit offset {}, which is less than current committed offset {}, range={}",
                    offset, *committed, self.metadata);
            } else {
                *committed = offset;
            }
            return;
        }

        if offset >= self.metadata.start() {
            self.committed = Some(offset);
        }
    }

    pub(crate) fn seal(&mut self, metadata: &mut RangeMetadata) {
        let end = metadata
            .end()
            .unwrap_or(self.committed.unwrap_or(self.metadata.start()));
        self.metadata.set_end(end);
        metadata.set_end(end);

        if !self.data_complete() {
            // TODO: spawn a task to replica data from peers.
        }
    }

    pub(crate) fn data_complete(&self) -> bool {
        match self.committed {
            Some(committed) => committed >= self.metadata.end().unwrap_or(self.metadata.start()),
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error;

    use model::range::RangeMetadata;

    #[test]
    fn test_new() -> Result<(), Box<dyn Error>> {
        let metadata = RangeMetadata::new(0, 0, 0, 0, None);
        let range = super::Range::new(metadata);
        assert_eq!(range.committed(), None);
        assert!(!range.data_complete());
        Ok(())
    }

    #[test]
    fn test_commit() -> Result<(), Box<dyn Error>> {
        let metadata = RangeMetadata::new(0, 0, 0, 0, None);
        let mut range = super::Range::new(metadata);
        range.commit(1);
        assert_eq!(range.committed(), Some(1));

        range.commit(0);
        assert_eq!(range.committed(), Some(1));

        range.commit(2);
        assert_eq!(range.committed(), Some(2));
        Ok(())
    }

    #[test]
    fn test_seal() -> Result<(), Box<dyn Error>> {
        let metadata = RangeMetadata::new(0, 0, 0, 0, None);
        let mut range = super::Range::new(metadata.clone());
        range.commit(1);

        let mut metadata = RangeMetadata::new(0, 0, 0, 0, Some(1));
        range.seal(&mut metadata);

        assert_eq!(range.committed(), Some(1));
        assert!(range.data_complete(), "Data should be complete");

        let mut metadata = RangeMetadata::new(0, 0, 0, 0, Some(2));
        range.seal(&mut metadata);

        assert_eq!(range.committed(), Some(1));
        assert_eq!(false, range.data_complete(), "Data should not be complete");

        Ok(())
    }
}
