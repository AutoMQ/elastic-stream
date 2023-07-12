use std::{cmp::min, rc::Rc};

use log::{error, warn};
use model::object::ObjectMetadata;

use crate::{stream::FetchDataset, ReplicationError::Internal};

use super::{
    object_reader::{ObjectMetadataManager, ObjectReader},
    records_block::RecordsBlock,
    Stream,
};

pub(crate) struct ObjectStream<S, R> {
    stream: Rc<S>,
    object_metadata_manager: ObjectMetadataManager,
    object_reader: R,
}

impl<S, R> ObjectStream<S, R>
where
    S: Stream + 'static,
    R: ObjectReader + 'static,
{
    pub(crate) fn new(stream: Rc<S>, object_reader: R) -> Rc<Self> {
        Rc::new(Self {
            stream,
            object_metadata_manager: ObjectMetadataManager::new(),
            object_reader,
        })
    }

    async fn fetch0(
        &self,
        start_offset: u64,
        end_offset: u64,
        batch_max_bytes: u32,
    ) -> Result<super::FetchDataset, crate::ReplicationError> {
        let mut start_offset = start_offset;
        let mut remaining_size = batch_max_bytes;
        let mut final_blocks = vec![];
        loop {
            let dataset = self
                .stream
                .fetch(start_offset, end_offset, remaining_size)
                .await?;
            let (mut records_blocks, objects) = match dataset {
                FetchDataset::Full(blocks) => (blocks, vec![]),
                FetchDataset::Partial(blocks) => (blocks, vec![]),
                FetchDataset::Mixin(blocks, objects) => (blocks, objects),
            };
            objects.iter().for_each(|object| {
                self.object_metadata_manager.add_object_metadata(object);
            });
            let blocks_start_offset = records_blocks
                .first()
                .ok_or_else(|| {
                    error!("records blocks is empty");
                    Internal
                })?
                .start_offset();
            if start_offset < blocks_start_offset {
                while !(start_offset >= blocks_start_offset || remaining_size == 0) {
                    let mut object_blocks = self
                        .object_reader
                        .read_first_object_blocks(
                            start_offset,
                            Some(blocks_start_offset),
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
                    start_offset = object_blocks_end_offset;
                    remaining_size -= min(object_blocks_len, remaining_size);
                    final_blocks.append(&mut object_blocks);
                }
            }
            if remaining_size > 0 {
                let records_blocks_len = records_blocks.iter().map(|b| b.len()).sum();
                remaining_size -= min(records_blocks_len, remaining_size);
                start_offset = records_blocks.last().unwrap().end_offset();
                final_blocks.append(&mut records_blocks);
            } else {
                break;
            }
            if start_offset >= end_offset {
                break;
            }
        }
        // TODO: sequential read check
        Ok(FetchDataset::Full(final_blocks))
    }
}

/// only used for test
#[allow(dead_code)]
fn object_read_first(
    records_blocks: Vec<RecordsBlock>,
    objects: Vec<ObjectMetadata>,
) -> (Vec<RecordsBlock>, Vec<ObjectMetadata>) {
    let objects_end_offset = objects
        .iter()
        .map(|object| (object.start_offset + object.end_offset_delta as u64))
        .max()
        .unwrap_or_default();
    let mut blocks = vec![];
    let mut end_offset = 0;
    for mut block in records_blocks.into_iter() {
        end_offset = block.end_offset();
        block.trim(objects_end_offset, None);
        if block.len() > 0 {
            blocks.push(block);
        }
    }
    if blocks.is_empty() {
        (vec![RecordsBlock::empty_block(end_offset)], objects)
    } else {
        (blocks, objects)
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
