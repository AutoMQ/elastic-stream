use flatbuffers::InvalidFlatbuffer;
use thiserror::Error;

#[derive(Debug, Error, PartialEq)]
pub enum RangeError {
    #[error("The range has already been sealed")]
    AlreadySealed(u64),
}

#[derive(Debug, Error)]
pub enum StreamError {
    #[error("Last range is already sealed")]
    AlreadySealed,

    #[error("Range index does not match")]
    RangeIndexMismatch { target: i32, actual: i32 },

    #[error("Bad offset")]
    SealBadOffset,

    #[error("Current data-node does not have the stream/range to seal")]
    SealWrongNode,
}

#[derive(Debug, Error)]
pub enum RecordError {
    #[error("Required record field is missing")]
    RequiredFieldMissing,

    #[error("The stream id of the record does not match the stream id of the record batch")]
    StreamIdMismatch,

    #[error("Parse header for record error")]
    ParseHeader,
}

#[derive(Debug, Error)]
pub enum WriterError {
    #[error("Network IO timeout when sending records")]
    Timeout,
}

#[derive(Debug, Error)]
pub enum EncodeError {
    #[error("The record batch is empty")]
    EmptyBatch,
}

#[derive(Debug, Error)]
pub enum DecodeError {
    #[error("Build record error")]
    BuildRecord(#[from] RecordError),

    #[error("The total length of record batch is not equal to the length of the data")]
    DataLengthMismatch,

    #[error("The magic number of the record batch is invalid")]
    InvalidMagic,

    #[error("The format of the record batch is invalid")]
    InvalidDataFormat,

    #[error("Failed to parse append request payload")]
    Flatbuffer(#[from] InvalidFlatbuffer),
}
