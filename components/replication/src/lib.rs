#![feature(btree_extract_if)]
#![feature(btree_cursors)]
#![feature(get_mut_unchecked)]

pub mod error;
pub mod request;
mod stream;
pub mod stream_client;

pub use error::ReplicationError;
pub use stream_client::StreamClient;
