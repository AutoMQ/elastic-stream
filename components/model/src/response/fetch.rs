use bytes::Bytes;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct FetchResultSet {
    pub throttle: Option<Duration>,
    pub payload: Option<Vec<Bytes>>,
}
