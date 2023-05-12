use std::io::Cursor;

use bytes::{Buf, Bytes};
use protocol::flat_model::records::RecordBatchMeta;

use crate::{error::DecodeError, record::flat_record::RecordMagic, response::append::AppendEntry};

pub struct Payload {}

impl Payload {
    /// Decode max offset contained in the request payload.
    pub fn max_offset(payload: &Bytes) -> u64 {
        unimplemented!()
    }

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
        let mut cursor = Cursor::new(&payload[..]);
        let mut entries = Vec::new();
        loop {
            if !cursor.has_remaining() {
                break;
            }

            if cursor.remaining() < 9 {
                return Err(DecodeError::DataLengthMismatch);
            }

            let magic_code = cursor.get_i8();
            if magic_code != RecordMagic::Magic0 as i8 {
                return Err(DecodeError::InvalidMagic);
            }

            let metadata_len = cursor.get_i32();
            if cursor.remaining() <= metadata_len as usize {
                return Err(DecodeError::DataLengthMismatch);
            }

            let remaining = cursor.remaining_slice();
            // Partial decode via Flatbuffers
            let metadata =
                flatbuffers::root::<RecordBatchMeta>(&remaining[..metadata_len as usize])?;

            let mut entry = AppendEntry::default();
            entry.stream_id = metadata.stream_id() as u64;
            entry.index = metadata.range_index() as u32;
            entry.offset = metadata.base_offset() as u64;
            entry.len = metadata.last_offset_delta() as u32;
            entries.push(entry);

            // Advance record batch metadata
            cursor.advance(metadata_len as usize);

            let payload_len = cursor.get_i32();
            if cursor.remaining() < payload_len as usize {
                return Err(DecodeError::DataLengthMismatch);
            }

            // Skip record batch payload
            cursor.advance(payload_len as usize);
        }

        Ok(entries)
    }
}
