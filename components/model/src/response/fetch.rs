use bytes::Bytes;
use std::time::Duration;

use crate::object::ObjectMetadata;

#[derive(Debug, Clone)]
pub struct FetchResultSet {
    pub throttle: Option<Duration>,
    pub payload: Option<Bytes>,
    pub object_metadata_list: Option<Vec<ObjectMetadata>>,
}
