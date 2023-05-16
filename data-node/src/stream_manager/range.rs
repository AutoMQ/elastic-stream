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
