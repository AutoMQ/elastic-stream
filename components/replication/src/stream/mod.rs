use bytes::Bytes;
use model::RecordBatch;

use crate::ReplicationError;

pub(crate) mod cache;
pub(crate) mod object_reader;
pub(crate) mod replication_range;
pub(crate) mod replication_stream;
pub(crate) mod replicator;
pub(crate) mod stream_manager;

trait Stream {
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
    ) -> Result<Vec<Bytes>, ReplicationError>;

    async fn trim(&self, _new_start_offset: u64) -> Result<(), ReplicationError>;
}
