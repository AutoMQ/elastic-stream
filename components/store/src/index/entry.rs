use std::io::Cursor;

use bytes::{Buf, BufMut, Bytes, BytesMut};

use super::record_handle::{HandleExt, RecordHandle};

/// Index entry in RocksDB
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct IndexEntry {
    pub(crate) stream_id: u64,
    pub(crate) range: u32,
    pub(crate) offset: u64,
    pub(crate) handle: RecordHandle,
}

impl IndexEntry {
    pub(crate) fn new(k: &[u8], v: &[u8]) -> Self {
        debug_assert_eq!(
            8 + 4 + 8,
            k.len(),
            "Key of index entry should contain 20 bytes: stream_id[u64], range[u32], offset[u64]"
        );
        let mut cursor = Cursor::new(k);
        Self {
            stream_id: cursor.get_u64(),
            range: cursor.get_u32(),
            offset: cursor.get_u64(),
            handle: Into::<RecordHandle>::into(v),
        }
    }

    pub(crate) fn key(&self) -> Bytes {
        let mut buf = BytesMut::with_capacity(20);
        buf.put_u64(self.stream_id);
        buf.put_u32(self.range);
        buf.put_u64(self.offset);
        buf.freeze()
    }

    /// Max offset of nested entries in the pointed `Record`
    pub(crate) fn max_offset(&self) -> u64 {
        match self.handle.ext {
            HandleExt::Hash(..) => self.offset,
            HandleExt::BatchSize(len) => self.offset + len as u64,
        }
    }
}
