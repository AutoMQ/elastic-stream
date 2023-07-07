use protocol::rpc::header::OperationCode;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ServiceError {
    #[error("Unsupported operation `{0:?}`")]
    Unsupported(OperationCode),

    #[error("Failed to describe stream")]
    DescribeStream,

    #[error("Failed to acquire stream range from placement drivers")]
    AcquireRange,

    #[error("Failed to seal a stream range")]
    Seal,

    #[error("The range is already sealed")]
    AlreadySealed,

    #[error("The range already existed")]
    AlreadyExisted,

    #[error("Resource `{0}` is not found")]
    NotFound(String),

    #[error("The offset of the append request is already committed")]
    OffsetCommitted,

    #[error("The offset of the append request is already in flight")]
    OffsetInFlight,

    #[error("The offset of the append request is out of order")]
    OffsetOutOfOrder,

    #[error("Internal error: `{0}`")]
    Internal(String),
}

#[derive(Debug, Error)]
pub enum LaunchError {
    #[error("No cores available to bind")]
    NoCoresAvailable,
}
