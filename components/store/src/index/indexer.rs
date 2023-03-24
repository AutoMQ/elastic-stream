use std::{ffi::CString, fs, io::Cursor, path::Path, sync::Arc};

use bytes::{Buf, BufMut, Bytes, BytesMut};
use model::range::{Range, StreamRange};
use rocksdb::{
    BlockBasedOptions, ColumnFamilyDescriptor, DBCompressionType, IteratorMode, Options,
    ReadOptions, WriteOptions, DB,
};
use slog::{error, info, warn, Logger};
use tokio::sync::mpsc;

use crate::error::StoreError;

use super::{record_handle::RecordHandle, MinOffset};

const INDEX_COLUMN_FAMILY: &str = "index";
const METADATA_COLUMN_FAMILY: &str = "metadata";

/// Key-value in metadata column family, flagging WAL offset, prior to which primary index are already built.
const WAL_CHECKPOINT: &str = "wal_checkpoint";

/// Representation of a range in metadata column family: {range-prefix: 1B}{stream_id:8B}{begin:8B} --> {status: 1B}[{end: 8B}].
///
/// If a range is sealed, its status is changed from `0` to `1` and logical `end` will be appended.
const RANGE_PREFIX: u8 = b'r';

pub(crate) struct Indexer {
    log: Logger,
    db: DB,
    write_opts: WriteOptions,
}

impl Indexer {
    fn build_index_column_family_options(
        log: Logger,
        min_offset: Arc<dyn MinOffset>,
    ) -> Result<Options, StoreError> {
        let mut index_cf_opts = Options::default();
        index_cf_opts.enable_statistics();
        index_cf_opts.set_write_buffer_size(128 * 1024 * 1024);
        index_cf_opts.set_max_write_buffer_number(4);
        index_cf_opts.set_compression_type(DBCompressionType::None);
        {
            // 128MiB block cache
            let cache = rocksdb::Cache::new_lru_cache(128 << 20)
                .map_err(|e| StoreError::RocksDB(e.into_string()))?;
            let mut table_opts = BlockBasedOptions::default();
            table_opts.set_block_cache(&cache);
            table_opts.set_block_size(128 << 10);
            index_cf_opts.set_block_based_table_factory(&table_opts);
        }

        let compaction_filter_name = CString::new("index-compaction-filter")
            .map_err(|e| StoreError::Internal("Failed to create CString".to_owned()))?;

        let index_compaction_filter_factory = super::compaction::IndexCompactionFilterFactory::new(
            log.clone(),
            compaction_filter_name,
            min_offset,
        );

        index_cf_opts.set_compaction_filter_factory(index_compaction_filter_factory);
        Ok(index_cf_opts)
    }

    pub(crate) fn new(
        log: Logger,
        path: &str,
        min_offset: Arc<dyn MinOffset>,
    ) -> Result<Self, StoreError> {
        let path = Path::new(path);
        if !path.exists() {
            info!(log, "Create directory: {:?}", path);
            fs::create_dir_all(path)?;
        }

        let index_cf_opts = Self::build_index_column_family_options(log.clone(), min_offset)?;
        let index_cf = ColumnFamilyDescriptor::new(INDEX_COLUMN_FAMILY, index_cf_opts);

        let mut metadata_cf_opts = Options::default();
        metadata_cf_opts.enable_statistics();
        metadata_cf_opts.create_if_missing(true);
        metadata_cf_opts.optimize_for_point_lookup(16 << 20);
        let metadata_cf = ColumnFamilyDescriptor::new(METADATA_COLUMN_FAMILY, metadata_cf_opts);

        let mut db_opts = Options::default();
        db_opts.set_atomic_flush(true);
        db_opts.create_if_missing(true);
        db_opts.create_missing_column_families(true);
        db_opts.enable_statistics();
        db_opts.set_stats_dump_period_sec(10);
        db_opts.set_stats_persist_period_sec(10);
        // Threshold of all memtable across column families added in size
        db_opts.set_db_write_buffer_size(1024 * 1024 * 1024);
        db_opts.set_max_background_jobs(2);

        let mut write_opts = WriteOptions::default();
        write_opts.disable_wal(true);
        write_opts.set_sync(false);

        let db = DB::open_cf_descriptors(&db_opts, path, vec![index_cf, metadata_cf])
            .map_err(|e| StoreError::RocksDB(e.into_string()))?;

        Ok(Self {
            log,
            db,
            write_opts,
        })
    }

    ///
    /// # Arguments
    ///
    /// * `stream_id` - Stream Identifier
    /// * `offset` Logical offset within the stream, similar to row-id within a database table.
    /// * `handle` - Pointer to record in WAL.
    ///
    pub(crate) fn index(
        &self,
        stream_id: i64,
        offset: u64,
        handle: &RecordHandle,
    ) -> Result<(), StoreError> {
        match self.db.cf_handle(INDEX_COLUMN_FAMILY) {
            Some(cf) => {
                let mut key_buf = BytesMut::with_capacity(16);
                key_buf.put_i64(stream_id);
                key_buf.put_u64(offset);

                let mut value_buf = BytesMut::with_capacity(20);
                value_buf.put_u64(handle.wal_offset);
                let length_type = handle.len << 8;
                value_buf.put_u32(length_type);
                value_buf.put_u64(handle.hash);
                self.db
                    .put_cf_opt(cf, &key_buf[..], &value_buf[..], &self.write_opts)
                    .map_err(|e| StoreError::RocksDB(e.into_string()))
            }
            None => Err(StoreError::RocksDB("No column family".to_owned())),
        }
    }

    /// The specific offset is not guaranteed to exist,
    /// because it may have been compacted or point to a intermediate position of a record batch.
    /// So the `scan_record_handles_left_shift` will left shift the offset as a lower bound to scan the records
    pub(crate) fn scan_record_handles_left_shift(
        &self,
        stream_id: i64,
        offset: u64,
        batch_size: u32,
    ) -> Result<Option<Vec<RecordHandle>>, StoreError> {
        let left_key = self.retrieve_left_key(stream_id, offset)?;
        let left_key = left_key.unwrap_or(self.build_index_key(stream_id, offset));
        let mut read_opts = ReadOptions::default();
        read_opts.set_iterate_lower_bound(&left_key[..]);
        read_opts.set_iterate_upper_bound((stream_id + 1).to_be_bytes());

        self.scan_record_handles_from(read_opts, batch_size)
    }

    pub(crate) fn scan_record_handles(
        &self,
        stream_id: i64,
        offset: u64,
        batch_size: u32,
    ) -> Result<Option<Vec<RecordHandle>>, StoreError> {
        let mut read_opts = ReadOptions::default();
        let lower = self.build_index_key(stream_id, offset);
        read_opts.set_iterate_lower_bound(&lower[..]);
        read_opts.set_iterate_upper_bound((stream_id + 1).to_be_bytes());
        self.scan_record_handles_from(read_opts, batch_size)
    }

    fn scan_record_handles_from(
        &self,
        read_opts: ReadOptions,
        batch_size: u32,
    ) -> Result<Option<Vec<RecordHandle>>, StoreError> {
        match self.db.cf_handle(INDEX_COLUMN_FAMILY) {
            Some(cf) => {
                let record_handles: Vec<_> = self
                    .db
                    .iterator_cf_opt(cf, read_opts, IteratorMode::Start)
                    .flatten()
                    .filter(|(_k, v)| {
                        if v.len() < 8 + 4 {
                            warn!(
                                self.log,
                                "Got an invalid index entry: len(value) = {}",
                                v.len()
                            );
                        }
                        v.len() > 8 /* WAL offset */ + 4 /* length-type */
                    })
                    .map(|(_k, v)| {
                        let mut rdr = Cursor::new(&v[..]);
                        let offset = rdr.get_u64();
                        let length_type = rdr.get_u32();
                        let mut hash = 0;
                        if length_type & 0xFF == 0 {
                            hash = rdr.get_u64();
                        }
                        let len = length_type >> 8;
                        RecordHandle {
                            wal_offset: offset,
                            len,
                            hash,
                        }
                    })
                    .take(batch_size as usize)
                    .collect();

                if record_handles.is_empty() {
                    Ok(None)
                } else {
                    Ok(Some(record_handles))
                }
            }
            None => Err(StoreError::RocksDB(format!(
                "No column family: `{}`",
                INDEX_COLUMN_FAMILY
            ))),
        }
    }

    /// Returns WAL checkpoint offset.
    ///
    /// Note we can call this method as frequently as we need as all operation is memory
    pub(crate) fn get_wal_checkpoint(&self) -> Result<u64, StoreError> {
        if let Some(metadata_cf) = self.db.cf_handle(METADATA_COLUMN_FAMILY) {
            match self
                .db
                .get_cf(metadata_cf, WAL_CHECKPOINT)
                .map_err(|e| StoreError::RocksDB(e.into_string()))?
            {
                Some(value) => {
                    if value.len() < 8 {
                        error!(self.log, "Value of wal_checkpoint is corrupted. Expecting 8 bytes in big endian, actual: {:?}", value);
                        return Err(StoreError::DataCorrupted);
                    }
                    let mut cursor = Cursor::new(&value[..]);
                    Ok(cursor.get_u64())
                }
                None => {
                    info!(
                        self.log,
                        "No KV entry for wal_checkpoint yet. Default wal_checkpoint to 0"
                    );
                    Ok(0)
                }
            }
        } else {
            info!(
                self.log,
                "No column family metadata yet. Default wal_checkpoint to 0"
            );
            Ok(0)
        }
    }

    pub(crate) fn advance_wal_checkpoint(&mut self, offset: u64) -> Result<(), StoreError> {
        let cf = self
            .db
            .cf_handle(METADATA_COLUMN_FAMILY)
            .ok_or(StoreError::RocksDB(
                "Metadata should have been created as we have create_if_missing enabled".to_owned(),
            ))?;
        let mut buf = BytesMut::with_capacity(8);
        buf.put_u64(offset);
        self.db
            .put_cf_opt(cf, WAL_CHECKPOINT, &buf[..], &self.write_opts)
            .map_err(|e| StoreError::RocksDB(e.into_string()))
    }

    /// Flush record index in cache into RocksDB using atomic-flush.
    ///
    /// Normally, we do not have invoke this function as frequently as insertion of entry, mapping stream offset to WAL
    /// offset. The reason behind this is that DB is having `AtomicFlush` enabled. As long as we put mapping entries
    /// first and then update checkpoint `offset` of WAL, integrity of index entries will be guaranteed after recovery
    /// from `checkpoint` WAL offset.
    ///
    /// Once a memtable is full and flushed to SST files, we can guarantee that all mapping entries prior to checkpoint
    /// `offset` of WAL are already persisted.
    ///
    /// To minimize the amount of WAL data to recover, for example, in case of planned reboot, we shall update checkpoint
    /// offset and invoke this method before stopping.
    pub(crate) fn flush(&self) -> Result<(), StoreError> {
        self.db
            .flush()
            .map_err(|e| StoreError::RocksDB(e.into_string()))
    }

    /// Compaction is synchronous and should execute in its own thread.
    pub(crate) fn compact(&self) {
        if let Some(cf) = self.db.cf_handle(INDEX_COLUMN_FAMILY) {
            self.db.compact_range_cf(cf, None::<&[u8]>, None::<&[u8]>);
        }
    }

    pub(crate) fn retrieve_max_key(&self, stream_id: i64) -> Result<Option<Bytes>, StoreError> {
        match self.db.cf_handle(INDEX_COLUMN_FAMILY) {
            Some(cf) => {
                let mut read_opts = ReadOptions::default();
                read_opts.set_iterate_lower_bound(stream_id.to_be_bytes());
                read_opts.set_iterate_upper_bound((stream_id + 1).to_be_bytes());
                let mut iter = self.db.iterator_cf_opt(cf, read_opts, IteratorMode::End);
                if let Some(result) = iter.next() {
                    let (max, _v) = result.map_err(|e| StoreError::RocksDB(e.into_string()))?;
                    Ok(Some(Bytes::from(max)))
                } else {
                    Ok(None)
                }
            }
            None => Err(StoreError::RocksDB(format!(
                "No column family: `{}`",
                INDEX_COLUMN_FAMILY
            ))),
        }
    }

    /// Returns the largest offset less than or equal to the given offset. The function follows the following rules:
    /// 1. `None` is returned if the specific offset < min offset or offset > max offset.
    /// 2. The current offset key is returned if it exists.
    /// 3. Otherwise, the left key is returned.
    fn retrieve_left_key(&self, stream_id: i64, offset: u64) -> Result<Option<Bytes>, StoreError> {
        let max_key = self.retrieve_max_key(stream_id)?;
        if let Some(max_key) = max_key {
            let max_offset = max_key.slice(8..).get_u64();
            if offset > max_offset {
                return Ok(None);
            }

            if offset == max_offset {
                return Ok(Some(max_key));
            }
        } else {
            return Ok(None);
        }

        match self.db.cf_handle(INDEX_COLUMN_FAMILY) {
            Some(cf) => {
                let mut read_opts = ReadOptions::default();
                // If the current offset has a key, return it.
                // So we add 1 to the offset to achieve this behavior.
                let lower = self.build_index_key(stream_id, offset + 1);
                read_opts.set_iterate_lower_bound(stream_id.to_be_bytes());
                read_opts.set_iterate_upper_bound(&lower[..]);

                // Reverse scan from the offset, find a lower key
                let mut reverse_itr = self.db.iterator_cf_opt(cf, read_opts, IteratorMode::End);
                let lower_entry = reverse_itr.next();

                if let Some(result) = lower_entry {
                    let (lower, _v) = result.map_err(|e| StoreError::RocksDB(e.into_string()))?;

                    Ok(Some(Bytes::from(lower)))
                } else {
                    Ok(None)
                }
            }
            None => Err(StoreError::RocksDB(format!(
                "No column family: `{}`",
                INDEX_COLUMN_FAMILY
            ))),
        }
    }

    fn build_index_key(&self, stream_id: i64, offset: u64) -> Bytes {
        let mut index_key = BytesMut::with_capacity(8 + 8);
        index_key.put_i64(stream_id);
        index_key.put_u64(offset);
        index_key.freeze()
    }
}

impl super::LocalRangeManager for Indexer {
    fn list_by_stream(&self, stream_id: i64, tx: mpsc::UnboundedSender<StreamRange>) {
        let mut prefix = BytesMut::with_capacity(9);
        prefix.put_u8(RANGE_PREFIX);
        prefix.put_i64(stream_id);

        if let Some(cf) = self.db.cf_handle(METADATA_COLUMN_FAMILY) {
            let mut read_opts = ReadOptions::default();
            read_opts.set_iterate_lower_bound(&prefix[..]);
            read_opts.set_prefix_same_as_start(true);
            self.db
                .iterator_cf_opt(cf, read_opts, rocksdb::IteratorMode::Start)
                .flatten()
                .map_while(|(k, v)| {
                    if !k.starts_with(&prefix[..]) {
                        None
                    } else {
                        debug_assert_eq!(k.len(), 8 + 4 + 8 + 1);

                        let mut key_reader = Cursor::new(&k[..]);
                        let _prefix = key_reader.get_u8();
                        debug_assert_eq!(_prefix, RANGE_PREFIX);

                        let _stream_id = key_reader.get_i64();
                        debug_assert_eq!(stream_id, _stream_id);

                        let id = key_reader.get_i32();

                        let start = key_reader.get_u64();

                        if v.len() == 1 {
                            Some(StreamRange::new(stream_id, id, start, 0, None))
                        } else {
                            debug_assert_eq!(v.len(), 8 + 1);
                            let mut value_reader = Cursor::new(&v[..]);
                            let _status = value_reader.get_u8();
                            let end = value_reader.get_u64();
                            Some(StreamRange::new(stream_id, id, start, end, Some(end)))
                        }
                    }
                })
                .for_each(|range| {
                    if let Err(e) = tx.send(range) {
                        error!(
                            self.log,
                            "Channel to transfer range for stream={} is closed", stream_id
                        );
                        return;
                    }
                });
        }
    }

    fn list(&self, tx: mpsc::UnboundedSender<StreamRange>) {
        let mut prefix = BytesMut::with_capacity(1);
        prefix.put_u8(RANGE_PREFIX);

        if let Some(cf) = self.db.cf_handle(METADATA_COLUMN_FAMILY) {
            let mut read_opts = ReadOptions::default();
            read_opts.set_iterate_lower_bound(&prefix[..]);
            read_opts.set_prefix_same_as_start(true);
            self.db
                .iterator_cf_opt(cf, read_opts, rocksdb::IteratorMode::Start)
                .flatten()
                .map_while(|(k, v)| {
                    if !k.starts_with(&prefix[..]) {
                        None
                    } else {
                        debug_assert_eq!(k.len(), 8 + 4 + 8 + 1);

                        let mut key_reader = Cursor::new(&k[..]);
                        let _prefix = key_reader.get_u8();
                        debug_assert_eq!(_prefix, RANGE_PREFIX);

                        let _stream_id = key_reader.get_i64();

                        let id = key_reader.get_i32();

                        let start = key_reader.get_u64();

                        if v.len() == 1 {
                            debug_assert_eq!(0, v[0]);
                            Some(StreamRange::new(_stream_id, id, start, 0, None))
                        } else {
                            debug_assert_eq!(v.len(), 8 + 1);
                            let mut value_reader = Cursor::new(&v[..]);
                            let _status = value_reader.get_u8();
                            debug_assert_eq!(1u8, _status);
                            let end = value_reader.get_u64();
                            Some(StreamRange::new(_stream_id, id, start, end, Some(end)))
                        }
                    }
                })
                .for_each(|range| {
                    if let Err(e) = tx.send(range) {
                        warn!(self.log, "Channel to transfer stream ranges is closed");
                        return;
                    }
                });
        }
    }

    fn seal(&self, stream_id: i64, range: &StreamRange) -> Result<(), StoreError> {
        debug_assert!(range.is_sealed(), "Range is not sealed yet");
        let end = range.end().ok_or(StoreError::Internal("".to_owned()))?;
        debug_assert!(end >= range.start(), "End of range cannot less than start");

        let mut key_buf = BytesMut::with_capacity(1 + 8 + 4 + 8);
        key_buf.put_u8(RANGE_PREFIX);
        key_buf.put_i64(stream_id);
        key_buf.put_i32(range.index());
        key_buf.put_u64(range.start());

        let mut value_buf = BytesMut::with_capacity(1 + 8);
        value_buf.put_u8(1);
        value_buf.put_u64(end);

        if let Some(cf) = self.db.cf_handle(METADATA_COLUMN_FAMILY) {
            self.db
                .put_cf_opt(cf, &key_buf[..], &value_buf[..], &self.write_opts)
                .map_err(|e| StoreError::RocksDB(e.into_string()))
        } else {
            Err(StoreError::RocksDB(format!(
                "No column family: {}",
                METADATA_COLUMN_FAMILY
            )))
        }
    }

    fn add(&self, stream_id: i64, range: &StreamRange) -> Result<(), StoreError> {
        let mut key_buf = BytesMut::with_capacity(1 + 8 + 4 + 8);
        key_buf.put_u8(RANGE_PREFIX);
        key_buf.put_i64(stream_id);
        key_buf.put_i32(range.index());
        key_buf.put_u64(range.start());

        let mut value_buf = BytesMut::with_capacity(1 + 8);
        if let Some(end) = range.end() {
            value_buf.put_u8(1);
            value_buf.put_u64(end);
        } else {
            value_buf.put_u8(0);
        }

        if let Some(cf) = self.db.cf_handle(METADATA_COLUMN_FAMILY) {
            self.db
                .put_cf_opt(cf, &key_buf[..], &value_buf[..], &self.write_opts)
                .map_err(|e| StoreError::RocksDB(e.into_string()))
        } else {
            Err(StoreError::RocksDB(format!(
                "No column family: {}",
                METADATA_COLUMN_FAMILY
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        error::Error,
        sync::{
            atomic::{AtomicU64, Ordering},
            Arc,
        },
    };

    use bytes::Buf;

    use crate::index::{record_handle::RecordHandle, MinOffset};

    struct SampleMinOffset {
        min: AtomicU64,
    }

    impl SampleMinOffset {
        fn set_min(&self, offset: u64) {
            self.min.store(offset, Ordering::Relaxed);
        }
    }

    impl MinOffset for SampleMinOffset {
        fn min_offset(&self) -> u64 {
            self.min.load(Ordering::Relaxed)
        }
    }

    fn new_indexer() -> Result<super::Indexer, Box<dyn Error>> {
        let log = test_util::terminal_logger();
        let path = test_util::create_random_path()?;
        let _guard = test_util::DirectoryRemovalGuard::new(log.clone(), path.as_path());
        let path_str = path.as_os_str().to_str().unwrap();
        let min_offset = Arc::new(SampleMinOffset {
            min: AtomicU64::new(0),
        });
        let indexer = super::Indexer::new(log, path_str, min_offset as Arc<dyn MinOffset>)?;
        Ok(indexer)
    }

    #[test]
    fn test_wal_checkpoint() -> Result<(), Box<dyn Error>> {
        let mut indexer = new_indexer()?;
        assert_eq!(0, indexer.get_wal_checkpoint()?);
        let wal_offset = 100;
        indexer.advance_wal_checkpoint(wal_offset)?;
        assert_eq!(wal_offset, indexer.get_wal_checkpoint()?);
        Ok(())
    }

    #[test]
    fn test_retrieve_max_key() -> Result<(), Box<dyn Error>> {
        // Case one: have a left key
        let indexer = new_indexer()?;
        let start_offset = 10;
        let stream_id = 1;

        let ptr = RecordHandle {
            wal_offset: 1024,
            len: 128,
            hash: 10,
        };
        indexer.index(stream_id, start_offset, &ptr)?;
        indexer.index(stream_id, start_offset + 1, &ptr)?;

        // Case one: have a max key
        let mut max_key = indexer.retrieve_max_key(stream_id).unwrap().unwrap();
        assert_eq!(stream_id, max_key.get_i64());
        assert_eq!(start_offset + 1, max_key.get_u64());

        //Case two: no max key
        let max_key = indexer.retrieve_max_key(stream_id + 1).unwrap();
        assert!(max_key.is_none());

        Ok(())
    }

    #[test]
    fn test_retrieve_left_key() -> Result<(), Box<dyn Error>> {
        // Case one: have a left key
        let indexer = new_indexer()?;
        let left_offset = 10;
        let stream_id = 1;

        let ptr = RecordHandle {
            wal_offset: 1024,
            len: 128,
            hash: 10,
        };
        indexer.index(stream_id, left_offset, &ptr)?;
        indexer.index(stream_id, left_offset + 2, &ptr)?;
        indexer.index(stream_id, left_offset + 4, &ptr)?;

        let mut left_key = indexer
            .retrieve_left_key(stream_id, left_offset + 1)
            .unwrap()
            .unwrap();
        assert_eq!(stream_id, left_key.get_i64());
        assert_eq!(left_offset, left_key.get_u64());

        // Case two: the specific key is equal to the left key
        let mut left_key = indexer
            .retrieve_left_key(stream_id, left_offset + 2)
            .unwrap()
            .unwrap();

        assert_eq!(stream_id, left_key.get_i64());
        assert_eq!(left_offset + 2, left_key.get_u64());

        // Case three: no left key
        let left_key = indexer
            .retrieve_left_key(stream_id, left_offset - 1)
            .unwrap();
        assert_eq!(None, left_key);

        // Case four: the smallest key
        let mut left_key = indexer
            .retrieve_left_key(stream_id, left_offset)
            .unwrap()
            .unwrap();

        assert_eq!(stream_id, left_key.get_i64());
        assert_eq!(left_offset, left_key.get_u64());

        // Case five: the biggest key
        let mut left_key = indexer
            .retrieve_left_key(stream_id, left_offset + 4)
            .unwrap()
            .unwrap();

        assert_eq!(stream_id, left_key.get_i64());
        assert_eq!(left_offset + 4, left_key.get_u64());

        // Case six: the biggest key + 1

        let left_key = indexer
            .retrieve_left_key(stream_id, left_offset + 5)
            .unwrap();
        assert_eq!(None, left_key);

        Ok(())
    }

    #[test]
    fn test_index_scan_from_left_key() -> Result<(), Box<dyn Error>> {
        let indexer = new_indexer()?;
        const CNT: u64 = 1024;
        (1..CNT)
            .into_iter()
            .map(|n| {
                let ptr = RecordHandle {
                    wal_offset: n * 128,
                    len: 128,
                    hash: 10,
                };
                indexer.index(0, n * 10, &ptr)
            })
            .flatten()
            .count();

        // The record handle physical offset is 128, 256, 384, ...
        // While the logical offset is 10, 20, 30, ..., which means each record batch contains 10 records.

        // Case one: scan from a exist key
        let handles = indexer.scan_record_handles_left_shift(0, 10, 2)?;
        assert_eq!(true, handles.is_some());
        let handles = handles.unwrap();
        assert_eq!(2, handles.len());

        handles.into_iter().enumerate().for_each(|(i, handle)| {
            assert_eq!(((i + 1) * 128) as u64, handle.wal_offset);
            assert_eq!(128, handle.len);
            assert_eq!(10, handle.hash);
        });

        // Case two: scan from a left key
        let handles = indexer.scan_record_handles_left_shift(0, 12, 2)?;
        assert_eq!(true, handles.is_some());
        let handles = handles.unwrap();
        assert_eq!(2, handles.len());

        handles.into_iter().enumerate().for_each(|(i, handle)| {
            assert_eq!(((i + 1) * 128) as u64, handle.wal_offset);
            assert_eq!(128, handle.len);
            assert_eq!(10, handle.hash);
        });

        // Case three: scan from a key smaller than the smallest key
        let handles = indexer.scan_record_handles_left_shift(0, 1, 2)?;
        assert_eq!(true, handles.is_some());
        let handles = handles.unwrap();
        assert_eq!(2, handles.len());

        handles.into_iter().enumerate().for_each(|(i, handle)| {
            assert_eq!(((i + 1) * 128) as u64, handle.wal_offset);
            assert_eq!(128, handle.len);
            assert_eq!(10, handle.hash);
        });

        // Case four: scan from a key bigger than the biggest key

        let handles = indexer.scan_record_handles_left_shift(0, CNT * 11, 2)?;
        assert_eq!(true, handles.is_none());

        Ok(())
    }

    #[test]
    fn test_index_scan() -> Result<(), Box<dyn Error>> {
        let indexer = new_indexer()?;
        const CNT: u64 = 1024;
        (0..CNT)
            .into_iter()
            .map(|n| {
                let ptr = RecordHandle {
                    wal_offset: n,
                    len: 128,
                    hash: 10,
                };
                indexer.index(0, n, &ptr)
            })
            .flatten()
            .count();

        let handles = indexer.scan_record_handles(0, 0, 10)?;
        assert_eq!(true, handles.is_some());
        let handles = handles.unwrap();
        assert_eq!(10, handles.len());
        handles.into_iter().enumerate().for_each(|(i, handle)| {
            assert_eq!(i as u64, handle.wal_offset);
            assert_eq!(128, handle.len);
            assert_eq!(10, handle.hash);
        });

        Ok(())
    }

    #[test]
    fn test_compaction() -> Result<(), Box<dyn Error>> {
        let log = test_util::terminal_logger();
        let path = test_util::create_random_path()?;
        let _guard = test_util::DirectoryRemovalGuard::new(log.clone(), path.as_path());
        let path_str = path.as_os_str().to_str().unwrap();
        let min_offset = Arc::new(SampleMinOffset {
            min: AtomicU64::new(0),
        });
        let indexer =
            super::Indexer::new(log, path_str, Arc::clone(&min_offset) as Arc<dyn MinOffset>)?;

        const CNT: u64 = 1024;
        (0..CNT)
            .into_iter()
            .map(|n| {
                let ptr = RecordHandle {
                    wal_offset: n,
                    len: 128,
                    hash: 10,
                };
                indexer.index(0, n, &ptr)
            })
            .flatten()
            .count();

        indexer.flush()?;
        indexer.compact();

        let handles = indexer.scan_record_handles(0, 0, 10)?.unwrap();
        assert_eq!(0, handles[0].wal_offset);
        min_offset.set_min(10);

        indexer.compact();
        let handles = indexer.scan_record_handles(0, 0, 10)?.unwrap();
        Ok(())
    }
}
