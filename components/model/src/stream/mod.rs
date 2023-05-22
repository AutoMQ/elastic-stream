use std::time::Duration;

use protocol::rpc::header::StreamT;

/// Stream is the basic storage unit in the system that store records in an append-only fashion.
///
/// A stream is composed of ranges. Conceptually, only the last range of the stream is mutable while the rest are immutable. Ranges of a
/// stream are distributed among data-nodes.
///
/// `Stream` on a specific data-node only cares about ranges that are located on it.
#[derive(Debug, Default, Clone)]
pub struct StreamMetadata {
    /// Stream ID, unique within the cluster.
    pub stream_id: u64,

    pub replica: u8,

    pub ack_count: u8,

    pub retention_period: Duration,
}

/// Converter from `StreamT` to `Stream`.
impl From<StreamT> for StreamMetadata {
    fn from(stream: StreamT) -> Self {
        Self {
            stream_id: stream.stream_id as u64,
            replica: stream.replica as u8,
            ack_count: stream.ack_count as u8,
            retention_period: Duration::from_millis(stream.retention_period_ms as u64),
        }
    }
}

/// Converter from `&Stream` to `StreamT`.
impl From<&StreamMetadata> for StreamT {
    fn from(stream: &StreamMetadata) -> Self {
        let mut stream_t = StreamT::default();
        stream_t.stream_id = stream.stream_id as i64;
        stream_t.replica = stream.replica as i8;
        stream_t.retention_period_ms = stream.retention_period.as_millis() as i64;
        stream_t.ack_count = stream.ack_count as i8;
        stream_t
    }
}
