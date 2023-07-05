#![feature(io_error_more)]
#![feature(io_slice_advance)]
#![feature(slice_first_last_chunk)]
#![feature(slice_pattern)]

pub mod bytes;
pub mod crc32;
pub mod fs;
pub(crate) mod handle_joiner;
pub mod metrics;

/// Create directories recursively if missing
pub use crate::fs::mkdirs_if_missing;
pub use crate::handle_joiner::HandleJoiner;
