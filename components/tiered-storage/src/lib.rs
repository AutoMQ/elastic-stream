#![feature(async_fn_in_trait)]
#![feature(map_try_insert)]

pub mod object_manager;
pub mod object_storage;
mod range_accumulator;
pub mod range_fetcher;
mod range_offload;

use model::object::ObjectMetadata;

#[cfg(test)]
use mockall::{automock, predicate::*};

#[cfg_attr(test, automock)]
pub trait TieredStorage {
    /// new record arrived notify
    fn new_record_arrived(
        &self,
        stream_id: u64,
        range_index: u32,
        end_offset: u64,
        record_size: u32,
    );
}

#[cfg_attr(test, automock)]
pub trait ObjectManager {
    fn is_owner(&self, stream_id: u64, range_index: u32) -> Option<Owner>;

    fn commit_object(&self, object_metadata: ObjectMetadata);

    fn get_objects(
        &self,
        stream_id: u64,
        range_index: u32,
        start_offset: u64,
        end_offset: u64,
    ) -> Vec<ObjectMetadata>;
}

pub struct Owner {
    pub epoch: u32,
    pub start_offset: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RangeKey {
    stream_id: u64,
    range_index: u32,
}

impl RangeKey {
    pub fn new(stream_id: u64, range_index: u32) -> Self {
        Self {
            stream_id,
            range_index,
        }
    }
}
