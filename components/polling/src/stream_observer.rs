use codec::frame::Frame;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum StreamError {
    #[error("Underlying network might have been closed")]
    Network,

    #[error("StreamObserver should have been completed")]
    IllegalState,
}

/// Response observer is an abstract handle to write one or multiple response frames(in case server streaming) to
/// clients.
///
/// Once `on_complete` is called, similar to HTTP2, the underlying streaming shall be completed. Further
/// invocation of `on_next` will receive illegal-state error.
///
/// Duplicated call of `on_complete` should be no-op.
pub trait StreamObserver {
    fn on_next(&self, response: &Frame) -> Result<(), StreamError>;

    fn on_complete(&self);
}
