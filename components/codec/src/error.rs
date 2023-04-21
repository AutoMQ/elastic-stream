use thiserror::Error;

#[derive(Debug, Error, PartialEq)]
pub enum FrameError {
    /// Not enough data is available to parse a message
    #[error("Not enough data for a frame")]
    Incomplete,

    #[error("The incoming frame is invalid. Reason: {0}")]
    BadFrame(String),

    #[error("Invalid frame length(max: {max:?}, found: {found:?})")]
    TooLongFrame { found: u32, max: u32 },

    #[error("Frame magic code mismatch(expect: {expected:?}, found: {found:?})")]
    MagicCodeMismatch { found: u8, expected: u8 },

    #[error("Invalid frame header length(expect: {expected:?}, found: {found:?})")]
    TooLongFrameHeader { found: u32, expected: u32 },

    #[error("Payload checksum mismatch detected(expect: {expected:?}, actual: {actual:?})")]
    PayloadChecksumMismatch { expected: u32, actual: u32 },

    #[error("Underlying connection reset by peer")]
    ConnectionReset,
}
