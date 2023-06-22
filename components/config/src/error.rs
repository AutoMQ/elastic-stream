use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigurationError {
    #[error("core-id: `{0}` is invalid")]
    InvalidCoreId(usize),

    #[error("Directory `{0}` does not exist")]
    DirectoryNotExists(String),

    #[error("An IO error raised")]
    Io(#[from] std::io::Error),

    #[error("System errno `{0}`")]
    System(i32),
}
