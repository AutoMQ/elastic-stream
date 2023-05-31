use std::io::Cursor;

use bytes::Buf;

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct RecordHandle {
    /// WAL offset
    pub(crate) wal_offset: u64,

    /// Bytes of the `Record` in WAL
    pub(crate) len: u32,

    /// Extended information of the record.
    pub(crate) ext: HandleExt,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum HandleExt {
    /// Hash of the `Record` tag if it contains a single entry.
    Hash(u64),

    /// Number of the nested entries included in the pointed `Record`.
    BatchSize(u16),
}

impl From<&[u8]> for RecordHandle {
    fn from(value: &[u8]) -> Self {
        debug_assert!(
            value.len() >= 12,
            "Value of index entry should at least contain offset[8B], length-type[4B]"
        );
        let mut cursor = Cursor::new(value);
        let offset = cursor.get_u64();
        let length_type = cursor.get_u32();
        let ty = length_type & 0xFF;
        let len = length_type >> 8;
        let ext = match ty {
            0 => {
                debug_assert!(
                    cursor.remaining() >= 8,
                    "Extended field should be u64, hash of record tag"
                );
                HandleExt::Hash(cursor.get_u64())
            }
            1 => {
                debug_assert!(
                    cursor.remaining() >= 2,
                    "Extended field should be u16, number of nested entries"
                );
                HandleExt::BatchSize(cursor.get_u16())
            }
            _ => {
                unreachable!("Unknown type");
            }
        };
        Self {
            wal_offset: offset,
            len,
            ext,
        }
    }
}
