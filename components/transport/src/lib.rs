#![feature(extract_if)]

pub mod connection;
pub mod connection_state;
pub(crate) mod error;
mod sync;
pub(crate) mod write_task;

pub use error::ConnectionError;
pub(crate) use write_task::WriteTask;

#[cfg(test)]
mod log {
    use std::io::Write;

    pub fn try_init_log() {
        let _ = env_logger::builder()
            .is_test(true)
            .format(|buf, record| {
                writeln!(
                    buf,
                    "{}:{} {} [{}] - {}",
                    record.file().unwrap_or("unknown"),
                    record.line().unwrap_or(0),
                    chrono::Local::now().format("%Y-%m-%dT%H:%M:%S%.3f"),
                    record.level(),
                    record.args()
                )
            })
            .try_init();
    }
}
