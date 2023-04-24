use crate::error::StoreError;

use super::AlignedBuf;

pub(crate) struct AlignedBufReader {}

impl AlignedBufReader {
    /// Create a new AlignedBufReader.
    /// The specified `wal_offset` doesn't need to be aligned,
    /// but the returned AlignedBuf's `wal_offset` will be aligned.
    pub(crate) fn alloc_read_buf(
        wal_offset: u64,
        len: usize,
        alignment: usize,
    ) -> Result<AlignedBuf, StoreError> {
        // Alignment must be positive
        debug_assert_ne!(0, alignment);
        // Alignment must be power of 2.
        debug_assert_eq!(0, alignment & (alignment - 1));
        let from = wal_offset / alignment as u64 * alignment as u64;
        let to =
            (wal_offset + len as u64 + alignment as u64 - 1) / alignment as u64 * alignment as u64;
        AlignedBuf::new(from, (to - from) as usize, alignment)
    }
}
