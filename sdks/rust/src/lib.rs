pub mod append_result;
pub mod error;
pub mod stream;
pub mod stream_manager;
pub mod stream_options;

pub use crate::append_result::AppendResult;
pub use crate::error::ClientError;
pub use crate::stream::Stream;
pub use crate::stream_manager::StreamManager;
pub use crate::stream_options::StreamOptions;
