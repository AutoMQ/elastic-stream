use thiserror::Error;

#[derive(Debug, Error)]
pub enum ClientError {
    #[error("Connection to server is rejected")]
    ConnectionTimeout,

    #[error("Connection to `{0}` is reset")]
    ConnectionReset(String),

    #[error("Stream[id={0}] is not found")]
    StreamNotFound(i64),

    #[error("MPSC to submit command is broken: `{0}`")]
    BrokenChannel(String),

    #[error("RpcClientError")]
    RpcClientError(#[from] client::error::ClientError),

    #[error("Unexpected internal client error")]
    Internal(String),
}
