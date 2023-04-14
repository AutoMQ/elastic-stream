#![feature(result_option_inspect)]

pub(crate) mod cli;
pub(crate) mod connection_tracker;
mod delegate_task;
pub mod error;
pub mod handler;
pub mod server;
pub(crate) mod stream_manager;
mod worker;
mod worker_config;
pub use crate::cli::Cli;

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
