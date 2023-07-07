#![feature(btree_extract_if)]
#![feature(btree_cursors)]
#![feature(get_mut_unchecked)]
#![feature(async_fn_in_trait)]

pub mod error;
pub mod request;
mod stream;
pub mod stream_client;

pub use error::ReplicationError;
pub use stream_client::StreamClient;

#[derive(Debug)]
pub struct Error {
    #[allow(dead_code)]
    code: u32,
    #[allow(dead_code)]
    message: String,
    source: Option<anyhow::Error>,
}

impl Error {
    pub fn new(code: u32, message: &str) -> Self {
        Self {
            code,
            message: message.to_string(),
            source: None,
        }
    }

    pub fn set_source(mut self, src: impl Into<anyhow::Error>) -> Self {
        debug_assert!(self.source.is_none(), "the source error has been set");

        self.source = Some(src.into());
        self
    }
}
