use std::cmp::min;

use bytes::{Buf, Bytes};
use protocol::rpc::header::ObjectMetadataT;

pub const BLOCK_DELIMITER: u8 = 0x66;
pub const FOOTER_MAGIC: u64 = 0x88e241b785f4cff7;

#[derive(Debug, Clone)]
pub struct ObjectMetadata {
    pub stream_id: u64,
    pub range_index: u32,
    pub start_offset: u64,
    pub end_offset_delta: u32,
    pub data_len: u32,
    pub sparse_index: Bytes,
    pub key: Option<String>,
}

impl ObjectMetadata {
    pub fn new(stream_id: u64, range_index: u32, start_offset: u64) -> Self {
        Self {
            stream_id,
            range_index,
            start_offset,
            end_offset_delta: 0,
            data_len: 0,
            sparse_index: Bytes::new(),
            key: None,
        }
    }

    /// Find the position of the given offset which offset is after the position in the sparse index.
    pub fn find_bound(
        &self,
        start_offset: u64,
        end_offset: Option<u64>,
        size_hint: u32,
        position: Option<u32>,
    ) -> Option<(u32, u32)> {
        let object_end_offset = self.start_offset + self.end_offset_delta as u64;
        if start_offset >= object_end_offset {
            return None;
        }
        if let Some(end_offset) = end_offset {
            if end_offset <= self.start_offset {
                return None;
            }
        }

        // find bound start_position and end_position from sparse index
        let mut end_position = None;
        let start_position = if let Some(position) = position {
            position
        } else {
            let mut position = 0;
            let mut cursor = self.sparse_index.clone();
            loop {
                if cursor.is_empty() {
                    break;
                }
                let index_end_offset = self.start_offset + cursor.get_u32() as u64;
                let index_position = cursor.get_u32();
                if index_end_offset <= start_offset {
                    position = index_position;
                }
                if let Some(end_offset) = end_offset {
                    if index_end_offset >= end_offset {
                        end_position = Some(index_position);
                        break;
                    }
                }
            }
            position
        };
        let end_position = if let Some(end_position) = end_position {
            end_position
        } else {
            min(self.data_len, start_position + Self::normalize(size_hint))
        };
        Some((start_position, end_position))
    }

    fn normalize(length: u32) -> u32 {
        // normalize length to 1MB aligned
        (length + 1024 * 1024 - 1) / (1024 * 1024) * 1024 * 1024
    }
}

impl From<&ObjectMetadataT> for ObjectMetadata {
    fn from(t: &ObjectMetadataT) -> Self {
        let sparse_index = if let Some(sparse_index) = t.sparse_index.clone() {
            Bytes::from(sparse_index)
        } else {
            Bytes::new()
        };
        ObjectMetadata {
            stream_id: 0,
            range_index: 0,
            start_offset: t.start_offset as u64,
            end_offset_delta: t.end_offset_delta as u32,
            data_len: t.data_len as u32,
            sparse_index,
            key: Some(t.key.clone()),
        }
    }
}

impl From<ObjectMetadata> for ObjectMetadataT {
    fn from(m: ObjectMetadata) -> Self {
        let mut t = ObjectMetadataT::default();
        t.key = m.key.unwrap_or_default();
        t.start_offset = m.start_offset as i64;
        t.end_offset_delta = m.end_offset_delta as i32;
        t.sparse_index = Some(m.sparse_index.to_vec());
        t.data_len = m.data_len as i32;
        t
    }
}
