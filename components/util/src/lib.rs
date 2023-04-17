#![feature(io_error_more)]

pub mod crc32;
pub mod fs;
pub(crate) mod handle_joiner;
pub mod metrics;

/// Create directories recursively if missing
pub use crate::fs::mkdirs_if_missing;
pub use crate::handle_joiner::HandleJoiner;
