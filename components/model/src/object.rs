use bytes::Bytes;
use protocol::rpc::header::ObjectMetadataT;

pub const BLOCK_DELIMITER: u8 = 0x66;
pub const FOOTER_MAGIC: u64 = 0x88e241b785f4cff7;

#[derive(Debug, Clone)]
pub struct ObjectMetadata {
    pub stream_id: u64,
    pub range_index: u32,
    pub start_offset: u64,
    pub end_offset_delta: u32,
    pub sparse_index: Bytes,
}

impl ObjectMetadata {
    pub fn new(stream_id: u64, range_index: u32, start_offset: u64) -> Self {
        Self {
            stream_id,
            range_index,
            start_offset,
            end_offset_delta: 0,
            sparse_index: Bytes::new(),
        }
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
            end_offset_delta: (t.end_offset - t.start_offset) as u32,
            sparse_index,
        }
    }
}
