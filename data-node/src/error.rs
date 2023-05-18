use codec::frame::OperationCode;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ServiceError {
    #[error("Unsupported operation `{0}`")]
    Unsupported(OperationCode),

    #[error("Failed to describe stream")]
    DescribeStream,

    #[error("Failed to acquire stream range from placement managers")]
    AcquireRange,

    #[error("Failed to seal a stream range")]
    Seal,

    #[error("The range is already sealed")]
    AlreadySealed,

    #[error("The range is already existed")]
    AlreadyExisted,

    #[error("Resource `{0}` is not found")]
    NotFound(String),

    #[error("Internal error: `{0}`")]
    Internal(String),
}

#[derive(Debug, Error)]
pub enum LaunchError {
    #[error("No cores available to bind")]
    NoCoresAvailable,
}
