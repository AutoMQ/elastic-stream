use thiserror::Error;

#[derive(Debug, Error)]
pub enum ClientError {
    #[error("Connection to server is rejected")]
    ConnectionTimeout,

    #[error("Connection to `{0}` is reset")]
    ConnectionReset(String),

    #[error("Stream[id={0}] is not found")]
    StreamNotFound(i64),
}
