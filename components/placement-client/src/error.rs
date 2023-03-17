use thiserror::Error;

use crate::client::session_state::SessionState;

#[derive(Debug, Error, PartialEq)]
pub enum CommandError {}

#[derive(Debug, Error, PartialEq)]
pub enum ClientError {
    #[error("Bad address")]
    BadAddress,

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
}

#[derive(Debug, Error)]
pub enum SessionError {
    #[error("Session state is `{actual:?}`, expecting `{expected:?}`")]
    IllegalState {
        actual: SessionState,
        expected: SessionState,
    },
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
