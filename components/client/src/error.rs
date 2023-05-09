use std::time::Duration;

use thiserror::Error;

#[derive(Debug, Error, PartialEq)]
pub enum CommandError {}

#[derive(Debug, Error, PartialEq)]
pub enum ClientError {
    #[error("Bad address")]
    BadAddress,

    #[error("Bad request")]
    BadRequest,

    #[error("Connection to `{0}` is refused")]
    ConnectionRefused(String),

    #[error("Timeout on connecting `{0}`")]
    ConnectTimeout(String),

    #[error("Failed to establish TCP connection. Cause: `{0}`")]
    ConnectFailure(String),

    #[error("Failed to disable Nagle's algorithm")]
    DisableNagleAlgorithm,

    #[error("Channel `{0}` is half closed")]
    ChannelClosing(String),

    #[error("Server internal error")]
    ServerInternal,

    #[error("Client internal error")]
    ClientInternal,

    #[error("Client fails to receive response from server within {timeout:#?}")]
    RpcTimeout { timeout: Duration },
}

#[derive(Debug, Error)]
pub enum ListRangeError {
    #[error("Bad arguments: {0}")]
    BadArguments(String),

    #[error("Internal client error")]
    Internal,

    #[error("Request Timeout")]
    Timeout,
}
