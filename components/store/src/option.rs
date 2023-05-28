//! Options of various kinds to modify their action behaviors
//!

/// Options that control write operations
#[derive(Debug)]
pub struct WriteOptions {
    /// If true, the write will be flushed from operating system buffer cache(through fsync or fdatasync)
    /// and replicated to replica-group peers before the write is considered complete.
    ///
    /// If this flag is true, writes will require relatively more amount of time.
    ///
    /// If this flag is false, some recent writes may be lost on machine/process crashes, or failing-over.
    ///
    /// Default: true
    pub sync: bool,
}

impl Default for WriteOptions {
    fn default() -> Self {
        Self { sync: true }
    }
}

#[derive(Debug, Default)]
pub struct ReadOptions {
    /// Target stream
    pub stream_id: i64,

    pub range: u32,

    /// Logical offset, from which to read records
    pub offset: i64,

    /// Logical offset, the exclusive upper boundary of the read range, specified by client.
    pub max_offset: u64,

    /// The maximum time in milliseconds to wait for the read response.
    pub max_wait_ms: i32,

    /// The maximum bytes of the read response.
    pub max_bytes: i32,
}
