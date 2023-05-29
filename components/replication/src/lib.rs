#![feature(btree_drain_filter)]
#![feature(btree_cursors)]
#![feature(get_mut_unchecked)]

pub mod error;
pub mod request;
pub mod stream_client;
mod stream_manager;

pub use error::ReplicationError;
pub use stream_client::StreamClient;
