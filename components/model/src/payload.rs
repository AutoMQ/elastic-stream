use bytes::Bytes;

use crate::response::append::AppendEntry;

pub struct Payload {}

impl Payload {
    /// Decode max offset contained in the request payload.
    pub fn max_offset(payload: &Bytes) -> u64 {
        unimplemented!()
    }

    pub fn parse_append_entries(payload: &Bytes) -> Vec<AppendEntry> {
        unimplemented!()
    }
}
