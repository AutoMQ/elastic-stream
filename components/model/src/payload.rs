use std::io::Cursor;

use bytes::{Buf, Bytes};
use protocol::flat_model::records::RecordBatchMeta;

use crate::{error::DecodeError, record::flat_record::RecordMagic, AppendEntry};

pub struct Payload {}

impl Payload {
    /// Parse AppendRequest payload for nested append entries.
    ///
    /// # Arguments
    ///
    /// `payload` - AppendRequest payload.
    ///
    /// # Returns
    /// `Vec<AppendEntry>` - A vector of append entries.
    /// `DecodeError` - Error if the request payload is malformed.
    pub fn parse_append_entries(payload: &Bytes) -> Result<Vec<AppendEntry>, DecodeError> {
        let mut entries = Vec::new();
        let mut pos = 0;
        while let (Some(entry), len) = Self::parse_append_entry(&payload[pos..])? {
            entries.push(entry);
            pos += len;
        }
        Ok(entries)
    }

    pub fn parse_append_entry(payload: &[u8]) -> Result<(Option<AppendEntry>, usize), DecodeError> {
        if payload.is_empty() {
            return Ok((None, 0));
        }

        let mut cursor = Cursor::new(payload);
        if cursor.remaining() < 9 {
            return Err(DecodeError::DataLengthMismatch);
        }

        let magic_code = cursor.get_i8();
        if magic_code != RecordMagic::Magic0 as i8 {
            return Err(DecodeError::InvalidMagic);
        }

        let metadata_len = cursor.get_i32() as usize;
        if cursor.remaining() <= metadata_len {
            return Err(DecodeError::DataLengthMismatch);
        }

        let remaining = cursor.remaining_slice();
        // Partial decode via Flatbuffers
        let metadata = flatbuffers::root::<RecordBatchMeta>(&remaining[..metadata_len])?;

        let base_offset = metadata.base_offset();
        let offset = if base_offset >= 0 {
            Some(base_offset as u64)
        } else {
            None
        };

        let entry = AppendEntry {
            stream_id: metadata.stream_id() as u64,
            index: metadata.range_index() as u32,
            offset,
            len: metadata.last_offset_delta() as u32,
        };

        // Advance record batch metadata
        cursor.advance(metadata_len);

        let payload_len = cursor.get_i32() as usize;
        if cursor.remaining() < payload_len {
            return Err(DecodeError::DataLengthMismatch);
        }
        // Skip record batch payload
        cursor.advance(payload_len);

        Ok((Some(entry), cursor.position() as usize))
    }
}
