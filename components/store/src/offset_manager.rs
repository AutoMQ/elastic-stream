use std::sync::atomic::{AtomicU64, Ordering};

use crate::index::MinOffset;

pub(crate) struct WalOffsetManager {
    min: AtomicU64,
}

impl WalOffsetManager {
    pub(crate) fn new() -> Self {
        Self {
            min: AtomicU64::new(u64::MAX),
        }
    }

    /// Advance WAL min-offset once a segment file is deleted.
    #[allow(dead_code)]
    fn set_min_offset(&self, min: u64) {
        self.min.store(min, Ordering::Relaxed);
    }
}

impl MinOffset for WalOffsetManager {
    fn min_offset(&self) -> u64 {
        self.min.load(Ordering::Relaxed)
    }
}
