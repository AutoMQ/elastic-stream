//! Data node uses placement clients to talk to placement managers.
//!
//! As a result, placement clients shall comply with `thread-per-core` threading model. Further,
//! placement clients reuse the same `tokio-uring` network library stack to initiate requests and
//! reap completed responses.
//!  
//! For applications that need to talk to `PlacementManager` and `DataNode`, please use crate `front-end-sdk`.

#![feature(try_find)]
#![feature(iterator_try_collect)]
#![feature(hash_drain_filter)]
#![feature(drain_filter)]
#![feature(once_cell)]

pub mod client;
pub mod error;
pub mod id_generator;
pub mod invocation_context;
pub mod request;
pub mod response;

pub use crate::client::client::Client;
pub use crate::id_generator::IdGenerator;
pub use crate::id_generator::PlacementManagerIdGenerator;
