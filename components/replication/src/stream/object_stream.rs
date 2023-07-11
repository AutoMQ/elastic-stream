use std::cmp::min;

use log::{error, warn};

use crate::{stream::FetchDataset, ReplicationError::Internal};

use super::{
    object_reader::{ObjectMetadataManager, ObjectReader},
    Stream,
};

pub(crate) struct ObjectStream<S, R> {
    stream: S,
    object_metadata_manager: ObjectMetadataManager,
    object_reader: R,
}

impl<S, R> ObjectStream<S, R>
where
    S: Stream + 'static,
    R: ObjectReader + 'static,
{
    pub(crate) fn new(stream: S, object_reader: R) -> Self {
        Self {
            stream,
            object_metadata_manager: ObjectMetadataManager::new(),
            object_reader,
        }
    }

    async fn fetch0(
        &self,
        start_offset: u64,
        end_offset: u64,
        batch_max_bytes: u32,
    ) -> Result<super::FetchDataset, crate::ReplicationError> {
        let dataset = self
            .stream
            .fetch(start_offset, end_offset, batch_max_bytes)
            .await?;
        let (blocks, objects) = match dataset {
            FetchDataset::Records(blocks) => (blocks, vec![]),
            FetchDataset::Mixin(blocks, objects) => (blocks, objects),
        };
        if objects.is_empty() {
            return Ok(FetchDataset::Records(blocks));
        }
        objects.iter().for_each(|object| {
            self.object_metadata_manager.add_object_metadata(object);
        });
        let mut start_offset = start_offset;
        let mut remaining_size = batch_max_bytes;
        let mut final_blocks = vec![];
        for record_block in blocks.into_iter() {
            if start_offset >= end_offset || remaining_size == 0 {
                break;
            }
            let record_block_start_offset = record_block.start_offset();
            if record_block_start_offset <= start_offset {
                start_offset = record_block.end_offset();
                remaining_size -= min(record_block.len(), remaining_size);
                final_blocks.push(record_block);
                continue;
            }
            loop {
                // Fill missing part with object storage
                if remaining_size == 0 {
                    break;
                }
                let mut object_blocks = self
                    .object_reader
                    .read_first_object_blocks(
                        start_offset,
                        Some(record_block_start_offset),
                        remaining_size,
                        &self.object_metadata_manager,
                    )
                    .await
                    .map_err(|e| {
                        warn!("Failed to read object block: {}", e);
                        Internal
                    })?;
                let object_blocks_end_offset = object_blocks
                    .last()
                    .ok_or_else(|| {
                        error!("Object blocks is empty");
                        Internal
                    })?
                    .end_offset();
                let object_blocks_len = object_blocks.iter().map(|b| b.len()).sum();
                final_blocks.append(&mut object_blocks);
                if object_blocks_end_offset < record_block_start_offset {
                    start_offset = object_blocks_end_offset;
                    remaining_size -= min(object_blocks_len, remaining_size);
                    continue;
                }
            }
            if remaining_size != 0 {
                remaining_size -= min(record_block.len(), remaining_size);
                final_blocks.push(record_block);
            }
        }
        Ok(FetchDataset::Records(final_blocks))
    }
}

/// delegate Stream trait to inner stream beside #fetch
impl<S, R> Stream for ObjectStream<S, R>
where
    S: Stream + 'static,
    R: ObjectReader + 'static,
{
    async fn fetch(
        &self,
        start_offset: u64,
        end_offset: u64,
        batch_max_bytes: u32,
    ) -> Result<super::FetchDataset, crate::ReplicationError> {
        self.fetch0(start_offset, end_offset, batch_max_bytes).await
    }

    async fn open(&self) -> Result<(), crate::ReplicationError> {
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

    async fn append(
        &self,
        record_batch: model::RecordBatch,
    ) -> Result<u64, crate::ReplicationError> {
        self.stream.append(record_batch).await
    }

    async fn trim(&self, new_start_offset: u64) -> Result<(), crate::ReplicationError> {
        self.stream.trim(new_start_offset).await
    }
}
