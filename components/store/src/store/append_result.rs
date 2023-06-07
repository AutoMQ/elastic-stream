#[derive(Debug)]
pub struct AppendResult {
    pub stream_id: i64,
    pub range_index: u32,
    pub offset: i64,
    pub wal_offset: u64,
}
