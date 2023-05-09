#[derive(Clone)]
pub struct AppendRecordRequest {
    /// Stream ID
    pub stream_id: i64,

    /// Range index
    pub range: i32,

    pub offset: i64,
    pub buffer: bytes::Bytes,
}
