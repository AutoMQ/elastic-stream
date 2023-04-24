use std::collections::{btree_map::Entry, BTreeMap};

use log::{error, trace};
use thiserror::Error;

#[derive(Debug, Error, PartialEq)]
pub(crate) enum WriteWindowError {
    #[error("A previous entry(offset: {offset:?}, length: {length:?}) exists")]
    Existed { offset: u64, length: u32 },

    #[error(
        "Invalid commit attempt (offset: {offset:?}, previous_length: {previous_length:?}, attempted_length: {attempted_length:?})"
    )]
    InvalidCommit {
        offset: u64,
        previous_length: u32,
        attempted_length: u32,
    },
}

/// Window manager that manages concurrent direct IO writes.
///
/// A write would make a range of WAL data persistent. The range is represented
/// as [offset, offset + len). `io_uring` allows up to configured IO_DEPTH number of
/// writes submitted to underlying device and get them done concurrently regardless of
/// submit order, unless their SQEs are flagged with `IOSQE_IO_LINK` and within the
/// same submit batch.
///
/// To achieve maximum IOPS/throughput, we do not use linking requests and manage flush
/// progress manually.
///
/// Here is how we manage the concurrent window:
///
/// 1. On reaping a CQE, we add the range [offset, offset + len) into the completed tree map in form of offset --> len;
/// 1.1 If the completed range offset > committed, meaning the gap exists between the committed and the completed,
/// we just wait further completed ranges to advance the committed;
/// 1.2 If the completed range offset == committed, meaning the gap is closed, we advance the committed directly.
/// 1.3 If the completed range offset < committed, meaning we are expanding continuous writes, advance the committed as well.
pub(crate) struct WriteWindow {
    /// An offset in WAL, all data prior to it should have been completely written. All write requests whose
    /// ranges fall into [0, committed) can be safely acknowledged.
    pub(crate) committed: u64,

    /// Writes that are completed but disjoint with the prior.
    completed: BTreeMap<u64, u32>,
}

impl WriteWindow {
    pub(crate) fn new(committed: u64) -> Self {
        Self {
            committed,
            completed: BTreeMap::new(),
        }
    }

    /// Reset committed WAL offset, expected to be called once on initialization.
    pub(crate) fn reset_committed(&mut self, committed: u64) {
        self.committed = committed;
    }

    fn advance(&mut self) -> Result<u64, WriteWindowError> {
        while let Some((completed_offset, _completed_len)) = self.completed.first_key_value() {
            if self.committed < *completed_offset {
                // Some writes, within the gap, are NOT yet completed
                break;
            }

            // If completed_offset <= self.committed, it means we could move forward without doubt.
            if let Some((completed_offset, completed_len)) = self.completed.pop_first() {
                self.committed = u64::max(completed_offset + completed_len as u64, self.committed);
                trace!("Advance committed position of WAL to: {}", self.committed);
            }
        }

        Ok(self.committed)
    }

    pub(crate) fn commit(&mut self, offset: u64, length: u32) -> Result<u64, WriteWindowError> {
        trace!("Try to commit WAL [{}, {})", offset, offset + length as u64);
        match self.completed.entry(offset) {
            Entry::Vacant(e) => {
                e.insert(length);
            }
            Entry::Occupied(mut e) => {
                let prev_len = e.get_mut();
                if length >= *prev_len {
                    trace!(
                        "Completed WAL range expanded from [{}, {}) to [{}, {})",
                        offset,
                        offset + *prev_len as u64,
                        offset,
                        offset + length as u64
                    );
                    *prev_len = length;
                } else {
                    // Should not reach here, as [offset, e.get()) strictly includes [offset, length). This violates
                    // barrier mechanism.
                    error!(
                        "Unexpected invalid commit. Previously completed range[{}, {}), attempted range: [{}, {})",
                        offset,
                        offset + *prev_len as u64,
                        offset,
                        offset + length as u64
                    );
                    return Err(WriteWindowError::InvalidCommit {
                        offset,
                        previous_length: *prev_len,
                        attempted_length: length,
                    });
                }
            }
        };
        self.advance()
    }
}

#[cfg(test)]
mod tests {
    use super::WriteWindowError;

    #[test]
    fn test_write_window() -> Result<(), WriteWindowError> {
        let mut window = super::WriteWindow::new(0);
        window.commit(0, 10)?;
        assert_eq!(10, window.committed);

        // Test expand
        window.commit(0, 20)?;
        assert_eq!(20, window.committed);

        window.commit(20, 10)?;
        assert_eq!(30, window.committed);

        // Gap
        window.commit(40, 10)?;
        assert_eq!(30, window.committed);

        // Shrink commit should be considered as invalid
        assert_eq!(
            Err(WriteWindowError::InvalidCommit {
                offset: 40,
                previous_length: 10,
                attempted_length: 5
            }),
            window.commit(40, 5)
        );

        Ok(())
    }
}
