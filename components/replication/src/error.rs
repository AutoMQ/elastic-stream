use thiserror::Error;

#[derive(Debug, Error, Clone)]
pub enum ReplicationError {
    #[error("RPC timeout")]
    RpcTimeout,

    #[error("Internal client error")]
    Internal,

    #[error("Range is already sealed")]
    AlreadySealed,

    #[error("Precondition required")]
    PreconditionRequired,

    #[error("Stream is already closed")]
    AlreadyClosed,
}
