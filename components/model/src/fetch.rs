use crate::Status;
use bytes::Bytes;
use protocol::rpc::header::FetchResultEntryT;

#[derive(Debug, Clone)]
pub struct FetchRequestEntry {
    pub stream_id: i64,
    pub index: i32,
    pub start_offset: u64,
    pub end_offset: u64,
    pub batch_max_bytes: u32,
}

#[derive(Debug, Clone)]
pub struct FetchResultEntry {
    pub status: Status,
    pub data: Option<Vec<Bytes>>,
}

impl From<FetchResultEntryT> for FetchResultEntry {
    fn from(value: FetchResultEntryT) -> Self {
        Self {
            status: (&value.status).into(),
            data: None,
        }
    }
}
