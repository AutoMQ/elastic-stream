use bytes::Bytes;
use derivative::Derivative;
use tokio::sync::oneshot;

use crate::{
    error::{AppendError, FetchError},
    AppendResult,
};

#[derive(Debug)]
pub(crate) struct ReadTask {
    /// Stream ID of the record.
    pub(crate) stream_id: i64,

    /// Offset, in term of WAL, of the record to read.
    pub(crate) wal_offset: u64,

    /// Number of bytes to read.
    pub(crate) len: u32,

    /// Oneshot sender, used to return `FetchResult` or propagate error.
    pub(crate) observer: oneshot::Sender<Result<SingleFetchResult, FetchError>>,
}

// Each fetch operation may split into multiple `ReadTask`s.
// Each `ReadTask` returns a single fetch result.
#[derive(Debug)]
pub struct SingleFetchResult {
    pub(crate) stream_id: i64,
    pub(crate) wal_offset: i64,
    /// The payload of a SingleFetchResult may be splitted into multiple `Bytes`s.
    pub(crate) payload: Vec<Bytes>,
}

impl SingleFetchResult {
    /// The total length of the payload.
    pub fn total_len(&self) -> usize {
        self.payload.iter().map(|buf| buf.len()).sum()
    }

    /// The iterator of the payload.
    /// It's a convenience method to iterate over the payload.
    pub fn iter(&self) -> impl Iterator<Item = &Bytes> {
        self.payload.iter()
    }
}

#[derive(Derivative)]
#[derivative(Debug)]
pub(crate) struct WriteTask {
    /// Stream ID of the record.
    pub(crate) stream_id: i64,

    /// Range Index
    pub(crate) range: u32,

    /// Logical primary index offset
    pub(crate) offset: i64,

    /// Number of nested record entries included in `buffer`.
    pub(crate) len: u32,

    /// `Record` serialized.
    ///
    /// Note: An application `Record` may be splitted into multiple WAL blocks/records,
    /// with enhancing digest/checksum and integrity guarantee.
    #[derivative(Debug = "ignore")]
    pub(crate) buffer: Bytes,

    /// Number of bytes written to the WAL.
    /// It's different from `buffer.len()` because the buffer will be armed with a storage header, currently, it's a 8-byte header.
    /// See `components/store/src/io/record.rs`.
    pub(crate) written_len: Option<u32>,

    pub(crate) observer: oneshot::Sender<Result<AppendResult, AppendError>>,
}

impl WriteTask {
    // Fetch the total length of the record, including the header.
    pub(crate) fn total_len(&self) -> u32 {
        self.buffer.len() as u32 + 4 /* CRC */ + 3 /* Record Size */ + 1 /* Record Type */
    }
}

pub(crate) enum IoTask {
    Read(ReadTask),
    Write(WriteTask),
}
