use codec::error::FrameError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConnectionError {
    #[error("Failed to encode frame")]
    EncodeFrame(#[from] FrameError),

    #[error("Network IO")]
    Network(#[from] std::io::Error),
}
