//! Range Server uses placement clients to talk to placement drivers.
//!
//! As a result, placement clients shall comply with `thread-per-core` threading model. Further,
//! placement clients reuse the same `tokio-uring` network library stack to initiate requests and
//! reap completed responses.
//!
//! For applications that need to talk to `PlacementDriver` and `RangeServer`, please use crate `front-end-sdk`.

#![feature(try_find)]
#![feature(iterator_try_collect)]
#![feature(hash_extract_if)]
#![feature(extract_if)]

pub mod client;
pub(crate) mod composite_session;
pub mod error;
pub mod id_generator;
pub mod invocation_context;
pub(crate) mod lb_policy;
pub(crate) mod node_state;
pub mod request;
pub mod response;
mod session;
mod session_manager;

pub use crate::client::Client;
pub use crate::id_generator::IdGenerator;
pub use crate::id_generator::PlacementDriverIdGenerator;
pub(crate) use crate::node_state::NodeState;
