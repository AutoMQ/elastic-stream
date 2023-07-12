use std::fmt::Debug;
use std::fmt::Formatter;

use bytes::Buf;
use bytes::{Bytes, BytesMut};
use model::{error::DecodeError, record::flat_record::RecordMagic};
use protocol::flat_model::records::RecordBatchMeta;

pub(crate) struct RecordsBlock {
    pub records: Vec<BlockRecord>,
    empty_block_offset: Option<u64>,
}

impl RecordsBlock {
    pub(crate) fn trim(&mut self, start_offset: u64, end_offset: Option<u64>) {
        self.records.retain(|r| {
            (r.start_offset + r.end_offset_delta as u64) > start_offset
                && end_offset.map_or(true, |end_offset| r.start_offset < end_offset)
        });
    }

    #[allow(dead_code)]
    pub(crate) fn get_records(&self, start_offset: u64, end_offset: u64) -> Vec<BlockRecord> {
        let mut records = vec![];
        for record in &self.records {
            let record_end_offset = record.start_offset + record.end_offset_delta as u64;
            if record_end_offset <= start_offset {
                continue;
            }
            if record.start_offset >= end_offset {
                break;
            }
            records.push(record.clone());
        }
        records
    }

    pub(crate) fn len(&self) -> u32 {
        self.records.iter().map(|r| r.data.len()).sum::<usize>() as u32
    }

    pub(crate) fn start_offset(&self) -> u64 {
        if self.empty_block_offset.is_some() {
            return self.empty_block_offset.unwrap();
        }
        self.records.first().map(|r| r.start_offset).unwrap_or(0)
    }

    pub(crate) fn end_offset(&self) -> u64 {
        if self.empty_block_offset.is_some() {
            return self.empty_block_offset.unwrap();
        }
        self.records
            .last()
            .map(|r| r.start_offset + r.end_offset_delta as u64)
            .unwrap_or(0)
    }

    pub(crate) fn empty_block(offset: u64) -> Self {
        Self {
            records: vec![],
            empty_block_offset: Some(offset),
        }
    }

    pub(crate) fn new(
        bytes: Bytes,
        block_size: u32,
        deep_copy: bool,
    ) -> Result<Vec<RecordsBlock>, DecodeError> {
        let mut cursor = bytes.clone();
        let mut blocks = vec![];
        let mut block = RecordsBlock {
            records: vec![],
            empty_block_offset: None,
        };
        let mut block_length = 0;
        let mut relative_position = 0;
        let mut reach_end = false;
        loop {
            if block_length >= block_size {
                blocks.push(block);
                block = RecordsBlock {
                    records: vec![],
                    empty_block_offset: None,
                };
                block_length = 0;
            }
            if cursor.remaining() < 9 || reach_end {
                if !block.records.is_empty() {
                    blocks.push(block);
                }
                break;
            }
            let magic_code = cursor.get_i8();
            if magic_code != RecordMagic::Magic0 as i8 {
                return Err(DecodeError::InvalidMagic);
            }
            let metadata_len = cursor.get_i32() as usize;
            if cursor.remaining() < metadata_len {
                reach_end = true;
                continue;
            }
            let metadata_slice = cursor.slice(0..metadata_len);
            let metadata = flatbuffers::root::<RecordBatchMeta>(&metadata_slice)?;
            let start_offset = metadata.base_offset() as u64;
            let end_offset_delta = metadata.last_offset_delta() as u32;
            cursor.advance(metadata_len);
            let payload_len = cursor.get_i32();
            if cursor.remaining() < payload_len as usize {
                reach_end = true;
                continue;
            }
            cursor.advance(payload_len as usize);

            let record_len = 1 + 4 + metadata_len + 4 + payload_len as usize;
            let record_bytes = if deep_copy {
                // deep copy the bytes to quick free memory when record don't need.
                let mut record_bytes = BytesMut::zeroed(record_len);
                record_bytes.copy_from_slice(
                    &bytes.slice(relative_position..(relative_position + record_len)),
                );
                record_bytes.freeze()
            } else {
                bytes.slice(relative_position..(relative_position + record_len))
            };
            block.records.push(BlockRecord {
                start_offset,
                end_offset_delta,
                data: vec![record_bytes],
            });

            relative_position += record_len;
            block_length += record_len as u32;
        }
        Ok(blocks)
    }
}

impl Debug for RecordsBlock {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RecordsBlock")
            .field("start_offset", &self.start_offset())
            .field("end_offset", &self.end_offset())
            .finish()
    }
}

#[derive(Clone)]
pub(crate) struct BlockRecord {
    pub start_offset: u64,
    pub end_offset_delta: u32,
    pub data: Vec<Bytes>,
}
