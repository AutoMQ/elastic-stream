use thiserror::Error;

#[derive(Debug, Error)]
pub enum ReplicationError {
    #[error("RPC timeout")]
    RpcTimeout,

    #[error("Internal client error")]
    Internal,
}
