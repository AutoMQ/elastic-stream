use bytes::Bytes;
use model::RecordBatch;
use std::time::Duration;
use tokio::sync::oneshot;

use crate::ReplicationError;

#[derive(Debug)]
pub struct AppendRequest {
    pub stream_id: u64,
    pub record_batch: RecordBatch,
}

#[derive(Debug)]
pub struct AppendResponse {
    pub offset: u64,
}

#[derive(Debug)]
pub struct ReadRequest {
    pub stream_id: u64,
    pub start_offset: u64,
    pub end_offset: u64,
    pub batch_max_bytes: u32,
}

#[derive(Debug)]
pub struct ReadResponse {
    pub data: Vec<Bytes>,
}

#[derive(Debug)]
pub struct CreateStreamRequest {
    pub replica: u8,
    pub ack_count: u8,
    pub retention_period: Duration,
}

#[derive(Debug)]
pub struct CreateStreamResponse {
    pub stream_id: u64,
}

#[derive(Debug)]
pub struct OpenStreamRequest {
    pub stream_id: u64,
    pub epoch: u64,
}

#[derive(Debug)]
pub struct OpenStreamResponse {}

#[derive(Debug)]
pub struct CloseStreamRequest {
    pub stream_id: u64,
}

#[derive(Debug)]
pub struct TrimRequest {
    pub stream_id: u64,
    pub new_start_offset: u64,
}

#[derive(Debug)]
pub(crate) enum Request {
    Append {
        request: AppendRequest,
        tx: oneshot::Sender<Result<AppendResponse, ReplicationError>>,
    },
    Read {
        request: ReadRequest,
        tx: oneshot::Sender<Result<ReadResponse, ReplicationError>>,
    },
    CreateStream {
        request: CreateStreamRequest,
        tx: oneshot::Sender<Result<CreateStreamResponse, ReplicationError>>,
    },
    OpenStream {
        request: OpenStreamRequest,
        tx: oneshot::Sender<Result<OpenStreamResponse, ReplicationError>>,
    },
    CloseStream {
        request: CloseStreamRequest,
        tx: oneshot::Sender<Result<(), ReplicationError>>,
    },
    StartOffset {
        // stream id
        request: u64,
        tx: oneshot::Sender<Result<u64, ReplicationError>>,
    },
    NextOffset {
        // stream id
        request: u64,
        tx: oneshot::Sender<Result<u64, ReplicationError>>,
    },
    Trim {
        request: TrimRequest,
        tx: oneshot::Sender<Result<(), ReplicationError>>,
    },
}
