use std::{cmp::min, rc::Rc};

use log::error;
use model::RecordBatch;

use crate::ReplicationError;

use super::{cache::HotCache, FetchDataset, Stream};

pub(crate) struct CacheStream<S> {
    stream_id: u64,
    stream: Rc<S>,
    hot_cache: Rc<HotCache>,
}

impl<S> CacheStream<S>
where
    S: Stream + 'static,
{
    pub(crate) fn new(stream_id: u64, stream: Rc<S>, hot_cache: Rc<HotCache>) -> Rc<Self> {
        Rc::new(Self {
            stream_id,
            stream,
            hot_cache,
        })
    }

    async fn fetch0(
        &self,
        mut start_offset: u64,
        end_offset: u64,
        mut batch_max_bytes: u32,
    ) -> Result<FetchDataset, ReplicationError> {
        let mut blocks = vec![];

        let hot_block =
            self.hot_cache
                .get_block(self.stream_id, start_offset, end_offset, batch_max_bytes);
        start_offset = hot_block.end_offset();
        batch_max_bytes -= min(hot_block.len(), batch_max_bytes);

        blocks.push(hot_block);
        if start_offset >= end_offset || batch_max_bytes == 0 {
            // fullfil by hot cache.
            return Ok(FetchDataset::Full(blocks));
        }

        // TODO: read ahead in background.
        let dataset = self
            .stream
            .fetch(start_offset, end_offset, batch_max_bytes)
            .await?;
        if let FetchDataset::Full(mut remote_blocks) = dataset {
            blocks.append(&mut remote_blocks);
            Ok(FetchDataset::Full(blocks))
        } else {
            error!("fetch dataset is not full");
            Err(ReplicationError::Internal)
        }
    }
}

impl<S> Stream for CacheStream<S>
where
    S: Stream + 'static,
{
    async fn open(&self) -> Result<(), ReplicationError> {
        self.stream.open().await
    }

    async fn close(&self) {
        self.stream.close().await
    }

    fn start_offset(&self) -> u64 {
        self.stream.start_offset()
    }

    fn next_offset(&self) -> u64 {
        self.stream.next_offset()
    }

    async fn append(&self, record_batch: RecordBatch) -> Result<u64, ReplicationError> {
        self.stream.append(record_batch).await
    }

    async fn fetch(
        &self,
        start_offset: u64,
        end_offset: u64,
        batch_max_bytes: u32,
    ) -> Result<FetchDataset, ReplicationError> {
        self.fetch0(start_offset, end_offset, batch_max_bytes).await
    }

    async fn trim(&self, new_start_offset: u64) -> Result<(), ReplicationError> {
        self.stream.trim(new_start_offset).await
    }
}
