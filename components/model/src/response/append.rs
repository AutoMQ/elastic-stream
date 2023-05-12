use chrono::{DateTime, TimeZone, Utc};
use protocol::rpc::header::AppendResultEntryT;

use crate::Status;

#[derive(Debug, Clone, Default)]
pub struct AppendEntry {
    stream_id: u64,
    index: u32,
    offset: u64,
    len: u32,
}

#[derive(Debug, Clone)]
pub struct AppendResultEntry {
    pub entry: AppendEntry,
    pub status: Status,
    pub timestamp: DateTime<Utc>,
}

impl From<AppendResultEntryT> for AppendResultEntry {
    fn from(value: AppendResultEntryT) -> Self {
        Self {
            entry: AppendEntry::default(),
            status: (&value.status).into(),
            timestamp: Utc.timestamp(
                value.timestamp_ms / 1000,
                (value.timestamp_ms % 1000 * 1_000_000) as u32,
            ),
        }
    }
}
