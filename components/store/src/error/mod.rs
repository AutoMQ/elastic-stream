//! Define various error types for this crate.
//!
//! Though some developers prefer to have their errors in each module, this crate takes the strategy of
//! defining errors centrally. Namely, all errors live in this module with the hope of having a consistent
//! and coherent hierarchy of errors.
//!

use thiserror::Error;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("Configuration invalid: `{0}`")]
    Configuration(String),

    #[error("Disk of `{0}` is full")]
    DiskFull(String),

    #[error("Request path `{0}` is invalid")]
    InvalidPath(String),

    #[error("Invalid log segment file name")]
    InvalidLogSegmentFileName,

    #[error("Failed to allocate log segment")]
    AllocLogSegment,

    #[error("Write window")]
    WriteWindow,

    #[error("Request offset `{0}` is out of range")]
    OffsetOutOfRange(u64),

    #[error("`{0}`")]
    NotFound(String),

    #[error("Internal IO error")]
    IO(#[from] std::io::Error),

    #[error("System error with errno: `{0}`")]
    System(i32),

    #[error("Create to create I/O Uring instance")]
    IoUring,

    #[error("Required io_uring opcode `{0}` is not supported")]
    OpCodeNotSupported(u8),

    #[error("Memory alignment issue")]
    MemoryAlignment,

    #[error("Internal error: `{0}`")]
    Internal(String),

    #[error("Data corrupted")]
    DataCorrupted,

    #[error("Unsupported record type")]
    UnsupportedRecordType,

    #[error("Insufficient data")]
    InsufficientData,

    #[error("RocksDB error: {0}")]
    RocksDB(String),

    #[error("Log segemnt is not opened")]
    NotOpened,
}

#[derive(Debug, Error)]
pub enum ReadError {}

#[derive(Debug, Error)]
pub enum AppendError {
    #[error("Failed to submit AppendRecordRequest")]
    SubmissionQueue,

    #[error("Recv from oneshot channel failed")]
    ChannelRecv,

    #[error("System error with errno: `{0}`")]
    System(i32),

    #[error("The request is illegal")]
    BadRequest,

    #[error("Internal error")]
    Internal,
}
