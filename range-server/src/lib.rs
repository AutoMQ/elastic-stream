#![feature(result_option_inspect)]
#![feature(try_find)]
#![feature(hash_extract_if)]
#![feature(btree_extract_if)]
#![feature(async_fn_in_trait)]

pub(crate) mod cli;
pub(crate) mod connection_tracker;
mod delegate_task;
pub mod error;
pub mod handler;
pub(crate) mod range_manager;
pub mod server;
mod worker;
mod worker_config;
pub use crate::cli::Cli;
pub(crate) mod connection_handler;
mod metrics;
pub(crate) mod profiling;
pub(crate) mod session;

pub mod built_info {
    include!(concat!(env!("OUT_DIR"), "/built.rs"));
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_thread_affinity() {
        let core_ids = core_affinity::get_core_ids().unwrap();
        core_ids.into_iter().for_each(|id| {
            println!("{}", id.id);
        });
    }
}
