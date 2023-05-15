use crate::{append_entry::AppendEntry, Status};

use chrono::{DateTime, TimeZone, Utc};
use protocol::rpc::header::AppendResultEntryT;

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
