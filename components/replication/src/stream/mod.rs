use model::{object::ObjectMetadata, RecordBatch};

use crate::ReplicationError;

use self::records_block::RecordsBlock;

pub(crate) mod cache;
pub(crate) mod object_reader;
pub(crate) mod object_stream;
pub(crate) mod records_block;
pub(crate) mod replication_range;
pub(crate) mod replication_stream;
pub(crate) mod replicator;
pub(crate) mod stream_manager;

pub(crate) trait Stream {
    async fn open(&self) -> Result<(), ReplicationError>;

    async fn close(&self);

    fn start_offset(&self) -> u64;

    fn next_offset(&self) -> u64;

    async fn append(&self, record_batch: RecordBatch) -> Result<u64, ReplicationError>;

    async fn fetch(
        &self,
        start_offset: u64,
        end_offset: u64,
        batch_max_bytes: u32,
    ) -> Result<FetchDataset, ReplicationError>;

    async fn trim(&self, _new_start_offset: u64) -> Result<(), ReplicationError>;
}

pub(crate) enum FetchDataset {
    Full(Vec<RecordsBlock>),
    Partial(Vec<RecordsBlock>), // only partial data in dataset, should retry fetch again with new start_offset
    Mixin(Vec<RecordsBlock>, Vec<ObjectMetadata>),
}
