use bytes::Bytes;

pub struct Payload {}

impl Payload {
    /// Decode max offset contained in the request payload.
    pub fn max_offset(payload: &Bytes) -> u64 {
        unimplemented!()
    }
}
