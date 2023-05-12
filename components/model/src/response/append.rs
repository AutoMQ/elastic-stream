use chrono::{DateTime, TimeZone, Utc};
use protocol::rpc::header::AppendResultEntryT;

use crate::Status;

#[derive(Debug, Clone, Default)]
pub struct AppendEntry {
    /// Stream ID
    pub stream_id: u64,

    /// Range index
    pub index: u32,

    /// Base offset
    pub offset: u64,

    /// Quantity of nested records
    pub len: u32,
}

#[derive(Debug, Clone)]
pub struct AppendResultEntry {
    /// Record batch entry.
    pub entry: AppendEntry,

    /// Appending record batch entry result.
    pub status: Status,

    /// Timestamp at which the record batch entry was appended to the stream in the data-node.
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
