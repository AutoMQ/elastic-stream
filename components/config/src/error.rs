use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigurationError {
    #[error("Concurrency is configured too large than the available cores")]
    ConcurrencyTooLarge,

    #[error("IO concurrency is configured too large than the available cores")]
    IoCpuTooLarge(usize),

    #[error("Directory `{0}` does not exist")]
    DirectoryNotExists(String),

    #[error("An IO error raised")]
    Io(#[from] std::io::Error),

    #[error("System errno `{0}`")]
    System(i32),
}
