use std::fmt;

use log::{trace, warn};
use model::range::RangeMetadata;

use super::window::Window;

#[derive(Debug)]
pub(crate) struct Range {
    pub(crate) metadata: RangeMetadata,

    committed: Option<u64>,

    window: Option<Window>,
}

impl Range {
    pub(crate) fn new(metadata: RangeMetadata) -> Self {
        Self {
            window: if metadata.is_sealed() {
                None
            } else {
                Some(Window::new(metadata.start()))
            },
            metadata,
            committed: None,
        }
    }

    pub(crate) fn committed(&self) -> Option<u64> {
        self.committed
    }

    pub(crate) fn reset(&mut self, offset: u64) {
        if let Some(window) = self.window_mut() {
            window.reset(offset);
        }
    }

    pub(crate) fn commit(&mut self, offset: u64) {
        let offset = self
            .window_mut()
            .and_then(|window| Some(window.commit(offset)));

        if let Some(offset) = offset {
            if let Some(ref mut committed) = self.committed {
                if offset <= *committed {
                    warn!("Try to commit offset {}, which is less than current committed offset {}, range={}",
                        offset, *committed, self.metadata);
                } else {
                    trace!(
                        "Committed offset of stream[id={}] changed from {} to {}",
                        self.metadata.stream_id(),
                        *committed,
                        offset
                    );
                    *committed = offset;
                }
                return;
            }

            if offset >= self.metadata.start() {
                self.committed = Some(offset);
            }
        }
    }

    pub(crate) fn seal(&mut self, metadata: &mut RangeMetadata) {
        let end = metadata
            .end()
            .unwrap_or(self.committed.unwrap_or(self.metadata.start()));
        self.metadata.set_end(end);
        metadata.set_end(end);

        // Drop window once the range is sealed.
        self.window.take();

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

    pub(crate) fn window_mut(&mut self) -> Option<&mut Window> {
        self.window.as_mut()
    }
}

impl fmt::Display for Range {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Range[stream-id={}, range-index={}], committed={:?}, next={}",
            self.metadata.stream_id(),
            self.metadata.index(),
            self.window
                .as_ref()
                .map_or(self.committed.unwrap_or(0), |win| { win.committed() }),
            self.window.as_ref().map_or(0, |window| window.next())
        )
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error;

    use model::{range::RangeMetadata, Batch};

    #[test]
    fn test_new() -> Result<(), Box<dyn Error>> {
        let metadata = RangeMetadata::new(0, 0, 0, 0, None);
        let range = super::Range::new(metadata);
        assert_eq!(range.committed(), None);
        assert!(!range.data_complete());
        Ok(())
    }

    #[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
    struct Foo {
        offset: u64,
        len: u32,
    }

    impl Batch for Foo {
        fn offset(&self) -> u64 {
            self.offset
        }

        fn len(&self) -> u32 {
            self.len
        }
    }

    #[test]
    fn test_commit() -> Result<(), Box<dyn Error>> {
        let metadata = RangeMetadata::new(0, 0, 0, 0, None);
        let mut range = super::Range::new(metadata);
        range
            .window_mut()
            .and_then(|window| window.check_barrier(&Foo { offset: 0, len: 1 }).ok());
        range.commit(0);
        assert_eq!(range.committed(), Some(1));

        Ok(())
    }

    #[test]
    fn test_seal() -> Result<(), Box<dyn Error>> {
        let metadata = RangeMetadata::new(0, 0, 0, 0, None);
        let mut range = super::Range::new(metadata.clone());
        range
            .window_mut()
            .and_then(|window| window.check_barrier(&Foo { offset: 0, len: 1 }).ok());
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
