use std::{
    cell::UnsafeCell, cmp, collections::BTreeMap, ops::Bound, rc::Rc, sync::Arc, time::Instant,
};

use slog::{error, info, trace, Logger};
use thiserror::Error;

use super::buf::AlignedBuf;

#[derive(Error, Debug)]
enum CacheError {
    #[error("Cache miss")]
    Miss,
}

#[derive(Debug)]
pub(crate) struct Entry {
    hit: usize,
    last_hit_instant: Instant,

    /// Some if the entry is not loaded yet.
    /// None if the entry is loaded that has a valid cached buf.
    entry_range: Option<EntryRange>,

    /// None if the entry is not loaded yet.
    buf: Option<Arc<AlignedBuf>>,
}

impl Entry {
    fn new(buf: Arc<AlignedBuf>) -> Self {
        Self {
            buf: Some(buf),
            hit: 0,
            last_hit_instant: Instant::now(),
            entry_range: None,
        }
    }

    fn new_loading_entry(entry_range: EntryRange) -> Self {
        Self {
            buf: None,
            hit: 0,
            last_hit_instant: Instant::now(),
            entry_range: Some(entry_range),
        }
    }

    /// Judge if the cached entry covers specified EntryRange partially.
    ///
    /// Test the cached buf or loading entry whether it covers the specified region partially.
    ///
    /// # Arguments
    /// * `entry_range` - The specified region.
    ///
    /// # Returns
    /// `true` if the cache hit partially;
    /// `false` if the cache has no overlap with the specified region.
    pub(crate) fn covers_partial(&self, entry_range: &EntryRange) -> bool {
        if let Some(buf) = &self.buf {
            buf.covers_partial(entry_range.wal_offset, entry_range.len)
        } else if let Some(loading_entry_range) = &self.entry_range {
            loading_entry_range.wal_offset <= entry_range.wal_offset + entry_range.len as u64
                && entry_range.wal_offset
                    <= loading_entry_range.wal_offset + loading_entry_range.len as u64
        } else {
            false
        }
    }

    /// Return the length of the cached entry.
    pub(crate) fn len(&self) -> u32 {
        if let Some(buf) = &self.buf {
            buf.limit() as u32
        } else if let Some(loading_entry_range) = &self.entry_range {
            loading_entry_range.len
        } else {
            0
        }
    }

    pub(crate) fn wal_offset(&self) -> u64 {
        if let Some(buf) = &self.buf {
            buf.wal_offset
        } else if let Some(loading_entry_range) = &self.entry_range {
            loading_entry_range.wal_offset
        } else {
            0
        }
    }
}

#[derive(Debug)]
pub(crate) struct BlockCache {
    log: Logger,

    // The start wal_offset of the block cache, usually it is the start wal_offset of some segment.
    wal_offset: u64,

    // The key of the map is a relative wal_offset from the start wal_offset of the block cache.
    entries: BTreeMap<u32, Rc<UnsafeCell<Entry>>>,
}

#[derive(Debug, Copy, Clone)]
pub(crate) struct EntryRange {
    // The start wal_offset of the entry which is a absolute wal_offset.
    // It's expected to be aligned to the IO alignment.
    pub(crate) wal_offset: u64,
    pub(crate) len: u32,
}

impl EntryRange {
    /// The new EntryRange is aligned to the specified alignment.
    pub(crate) fn new(wal_offset: u64, len: u32, alignment: u64) -> Self {
        // Alignment must be positive
        debug_assert_ne!(0, alignment);
        // Alignment must be power of 2.
        debug_assert_eq!(0, alignment & (alignment - 1));
        let from = wal_offset / alignment * alignment;

        // We don't align the end offset in the cache query, to avoid loading the last page in read path.
        // Note: the last page always added in write path.
        let to = wal_offset + len as u64;

        Self {
            wal_offset: from,
            len: (to - from) as u32,
        }
    }
}

pub trait MergeRange<T> {
    /// Merge the entries to bigger continuous ranges as possible.
    /// This may reduce the number of loading io tasks.
    ///
    /// # Arguments
    /// * `self` - The entries to be merged.
    ///
    /// # Returns
    /// * The continuous ranges of the missed entries.
    fn merge(self) -> Vec<T>;
}

impl MergeRange<EntryRange> for Vec<EntryRange> {
    /// The merge algorithm supports merging disorderly, repeated entries as well as overlapping entries.
    fn merge(self) -> Vec<EntryRange> {
        let mut ranges = self;
        ranges.sort_by_key(|r| r.wal_offset);

        let mut merged_ranges: Vec<EntryRange> = Vec::with_capacity(ranges.len());

        // Iterate the sorted ranges from the end.
        for range in ranges.into_iter() {
            if let Some(last_m) = merged_ranges.last_mut() {
                if last_m.wal_offset + last_m.len as u64 >= range.wal_offset {
                    // Adjust the length of the last merged range.
                    last_m.len = cmp::max(
                        range.wal_offset + range.len as u64 - last_m.wal_offset,
                        last_m.len as u64,
                    ) as u32;
                    continue;
                }
            }
            merged_ranges.push(range);
        }
        merged_ranges
    }
}

impl BlockCache {
    pub(crate) fn new(log: Logger, offset: u64) -> Self {
        Self {
            log,
            wal_offset: offset,
            entries: BTreeMap::new(),
        }
    }

    /// Add a new entry to the cache.
    /// The newly added entry will replace the existing entry if the start wal_offset conflicts.
    ///
    /// # Note
    /// * The upper layer should ensure the cached entries are not overlapping after a new entry is added.
    ///
    /// # Arguments
    /// * `buf` - The buffer to be added to the cache.
    pub(crate) fn add_entry(&mut self, buf: Arc<AlignedBuf>) {
        trace!(
            self.log,
            "Add block cache entry: [{}, {})",
            buf.wal_offset,
            buf.wal_offset + buf.limit() as u64
        );
        debug_assert!(buf.wal_offset >= self.wal_offset);
        let from = (buf.wal_offset - self.wal_offset) as u32;
        let entry = Rc::new(UnsafeCell::new(Entry::new(buf)));

        // The replace occurs when the new entry overlaps with the existing entry.
        self.entries.insert(from, entry);
    }

    /// Add a loading entry to the cache.
    /// The upper layer should ensure that a loading io task is submitted for the entry.
    /// # Arguments
    /// * `entry_range` - The entry range to be added to the cache.
    pub(crate) fn add_loading_entry(&mut self, entry_range: EntryRange) {
        trace!(
            self.log,
            "Add loading block cache entry: [{}, {})",
            entry_range.wal_offset,
            entry_range.wal_offset + entry_range.len as u64
        );
        debug_assert!(entry_range.wal_offset >= self.wal_offset);
        let from = (entry_range.wal_offset - self.wal_offset) as u32;
        let entry = Rc::new(UnsafeCell::new(Entry::new_loading_entry(entry_range)));

        // The replace occurs when the new entry overlaps with the existing entry.
        self.entries.insert(from, entry);
    }

    /// Get cached entries from the cache.
    /// If the cache couldn't meet the query needs, it will return the missed entries.
    ///
    /// # Arguments
    /// * `wal_offset` - The start wal_offset of the query.
    /// * `len` - The length of the query.
    ///
    /// # Returns
    /// * `Ok` - The cached entries. If the ok result is None, it means the caller should wait for the ongoing io tasks.
    /// * `Err` - The missed entries, may split the request into multiple missed ranges.
    pub(crate) fn try_get_entries(
        &self,
        entry_range: EntryRange,
    ) -> Result<Option<Vec<Arc<AlignedBuf>>>, Vec<EntryRange>> {
        let wal_offset = entry_range.wal_offset;
        let len = entry_range.len;

        let from = wal_offset.checked_sub(self.wal_offset);

        if let Some(from) = from {
            let from = from as u32;
            let to = from + len;

            // The start cursor is the first entry whose start wal_offset is less than or equal the query wal_offset.
            // This behavior is aim to resolve the case that the query wal_offset is in the middle of an entry.
            let start_cursor = self.entries.upper_bound(Bound::Included(&from));
            let start_key = start_cursor.key().unwrap_or(&from);

            let search: Vec<_> = self
                .entries
                .range(start_key..&to)
                .filter(|(_k, entry)| {
                    let item = unsafe { &mut *entry.get() };
                    if item.covers_partial(&entry_range) {
                        return true;
                    }
                    return false;
                })
                .collect();

            // Return a complete missed entry if the search result is empty.
            if search.is_empty() {
                return Err(vec![entry_range]);
            }

            // Return partially missed entries if the search result does not cover the specified range.
            let mut missed_entries: Vec<_> = Vec::new();
            let mut last_end = from;

            search.iter().for_each(|(k, entry)| {
                let item = unsafe { &mut *entry.get() };
                if **k > last_end {
                    missed_entries.push(EntryRange {
                        wal_offset: self.wal_offset + last_end as u64,
                        len: **k - last_end,
                    });
                }
                last_end = **k + item.len() as u32;
            });

            if last_end < to {
                missed_entries.push(EntryRange {
                    wal_offset: self.wal_offset + last_end as u64,
                    len: to - last_end,
                });
            }

            if !missed_entries.is_empty() {
                return Err(missed_entries);
            }

            let search_len = search.len();
            let buf_res: Vec<_> = search
                .into_iter()
                .map(|(_k, entry)| {
                    let item = unsafe { &mut *entry.get() };

                    // Although here we may not return the buffer to the caller,
                    // we still increase the hit count to reduce the chance of being dropped.
                    item.hit += 1;
                    item.last_hit_instant = Instant::now();
                    &item.buf
                })
                .flatten()
                .map(|buf| Arc::clone(buf))
                .collect();

            if buf_res.len() != search_len {
                // There is some loading entry in the search result, the caller should wait for the loading.
                // So we return a empty ok result to indicate the caller to wait.
                return Ok(None);
            }

            Ok(Some(buf_res))
        } else {
            error!(
                self.log,
                "Invalid wal_offset: {}, cache wal_offset: {}", wal_offset, self.wal_offset
            );

            Err(vec![entry_range])
        }
    }

    /// Remove cache entries if `Predicate` returns `true`.
    ///
    /// # Arguments
    /// * `pred` - Predicate that return true if the entry is supposed to be dropped and false to reserve.
    pub(crate) fn remove<F>(&mut self, pred: F)
    where
        F: Fn(&Entry) -> bool,
    {
        self.entries.drain_filter(|_k, v| {
            let entry = unsafe { &*v.get() };
            if pred(entry) {
                info!(
                    self.log,
                    "Remove block cache entry [{}, {})",
                    entry.wal_offset(),
                    entry.wal_offset() + entry.len() as u64
                );
                true
            } else {
                false
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{atomic::Ordering, Arc};

    use rand::{seq::SliceRandom, thread_rng};

    use crate::io::{block_cache::MergeRange, buf::AlignedBuf};

    /// Test merge missed entry ranges.
    #[test]
    fn test_merge_entries() {
        let block_size = 4096;

        // Case one: add 16 entries, and merge to one range.
        let mut missed_entries: Vec<_> = (0..16)
            .into_iter()
            .map(|n| super::EntryRange {
                wal_offset: n * block_size as u64,
                len: block_size,
            })
            .collect();
        let mut rng = thread_rng();
        missed_entries.shuffle(&mut rng);

        let merged = missed_entries.merge();
        assert_eq!(1, merged.len());
        assert_eq!(0, merged[0].wal_offset);
        assert_eq!(16 * block_size, merged[0].len);

        // Case two: add 16 entries, and merge to two ranges.
        let mut missed_entries: Vec<_> = (0..8)
            .into_iter()
            .map(|n| super::EntryRange {
                wal_offset: n * block_size as u64,
                len: block_size,
            })
            .collect();

        let start_wal_offset = 1024 * block_size as u64;
        (0..8).into_iter().for_each(|n| {
            missed_entries.push(super::EntryRange {
                wal_offset: start_wal_offset + n * block_size as u64,
                len: block_size,
            });
        });

        missed_entries.shuffle(&mut rng);

        let merged = missed_entries.merge();
        assert_eq!(2, merged.len());
        assert_eq!(0, merged[0].wal_offset);
        assert_eq!(8 * block_size, merged[0].len);
        assert_eq!(start_wal_offset, merged[1].wal_offset);
        assert_eq!(8 * block_size, merged[1].len);

        // Case three: no merge
        let mut missed_entries: Vec<_> = (0..8)
            .into_iter()
            .map(|n| super::EntryRange {
                wal_offset: n * 3 * block_size as u64,
                len: block_size,
            })
            .collect();

        missed_entries.shuffle(&mut rng);
        let merged = missed_entries.merge();
        assert_eq!(8, merged.len());
        (0..8).into_iter().for_each(|n| {
            assert_eq!(n * 3 * block_size as u64, merged[n as usize].wal_offset);
        });

        // Case four: discard the redundant range.
        let mut missed_entries: Vec<_> = (0..8)
            .into_iter()
            .map(|n| super::EntryRange {
                wal_offset: n * block_size as u64,
                len: block_size,
            })
            .collect();

        (0..8).into_iter().for_each(|n| {
            missed_entries.push(super::EntryRange {
                wal_offset: n * block_size as u64,
                len: block_size,
            });
        });

        missed_entries.shuffle(&mut rng);
        let merged = missed_entries.merge();
        assert_eq!(1, merged.len());
        assert_eq!(0, merged[0].wal_offset);
        assert_eq!(8 * block_size, merged[0].len);

        // Case five: handle the case that the ranges are overlapped.
        let mut missed_entries: Vec<_> = vec![];

        // Add a range [0, 2 * block_size)
        missed_entries.push(super::EntryRange {
            wal_offset: 0,
            len: 2 * block_size,
        });
        // Add a range [block_size, 3 * block_size)
        missed_entries.push(super::EntryRange {
            wal_offset: block_size as u64,
            len: 3 * block_size,
        });
        // Add a range [1 * block_size, 2* block_size)
        missed_entries.push(super::EntryRange {
            wal_offset: block_size as u64,
            len: 2 * block_size,
        });
        // Add a range [3 * block_size, 3 * block_size)
        missed_entries.push(super::EntryRange {
            wal_offset: 3 * block_size as u64,
            len: 3 * block_size,
        });

        missed_entries.shuffle(&mut rng);
        let merged = missed_entries.merge();
        assert_eq!(1, merged.len());
        assert_eq!(0, merged[0].wal_offset);
        assert_eq!(6 * block_size, merged[0].len);
    }

    /// Test add entry.
    #[test]
    fn test_add_entry() {
        let log = test_util::terminal_logger();
        let mut block_cache = super::BlockCache::new(log.clone(), 0);
        let block_size = 4096;
        for n in (0..16).into_iter() {
            let buf = Arc::new(
                AlignedBuf::new(log.clone(), n * block_size as u64, block_size, block_size)
                    .unwrap(),
            );
            buf.limit.store(block_size, Ordering::Relaxed);
            block_cache.add_entry(buf);
        }

        assert_eq!(16, block_cache.entries.len());
    }

    /// Test get entry.
    #[test]
    fn test_get_entry() {
        // Case one: total hit in a big cached entry
        let log = test_util::terminal_logger();
        let mut block_cache = super::BlockCache::new(log.clone(), 0);
        let block_size = 4096;

        let buf =
            Arc::new(AlignedBuf::new(log.clone(), 4096, block_size * 1024, block_size).unwrap());
        buf.increase_written(block_size * 1024);

        block_cache.add_entry(buf);

        let hit = block_cache
            .try_get_entries(super::EntryRange {
                wal_offset: 4096 * 2,
                len: 4096 * 10,
            })
            .unwrap()
            .unwrap();
        assert_eq!(1, hit.len());
        assert_eq!(block_size * 1024, hit[0].limit());

        // Case two: hit partially in two cached entries
        let mut block_cache = super::BlockCache::new(log.clone(), 0);
        let target_entry = super::EntryRange {
            wal_offset: 0,
            len: 4096 * 10,
        };

        // Add entry 1: [4096, 4096 * 2)
        let wal_offset = 4096;
        let len = 4096 * 2;
        let buf = Arc::new(AlignedBuf::new(log.clone(), wal_offset, len, block_size).unwrap());
        buf.increase_written(len);
        block_cache.add_entry(buf);

        // Add entry 2: [4096 * 4, 4096)
        let wal_offset = 4096 * 4;
        let len = 4096;
        let buf = Arc::new(AlignedBuf::new(log.clone(), wal_offset, len, block_size).unwrap());
        buf.increase_written(len);
        block_cache.add_entry(buf);

        let hit = block_cache.try_get_entries(target_entry).unwrap_err();

        // Miss [0, 4096)
        assert_eq!(3, hit.len());
        assert_eq!(0, hit[0].wal_offset);
        assert_eq!(4096, hit[0].len);

        // Miss [4096 * 3, 4096)
        assert_eq!(4096 * 3, hit[1].wal_offset);
        assert_eq!(4096, hit[1].len);

        // Miss [4096 * 5, 4096 * 5)
        assert_eq!(4096 * 5, hit[2].wal_offset);
        assert_eq!(4096 * 5, hit[2].len);

        // Try add loading entry
        hit.iter().for_each(|r| {
            block_cache.add_loading_entry(r.clone());
        });

        let pending_hit = block_cache.try_get_entries(target_entry);
        assert_eq!(true, pending_hit.is_ok());
        assert_eq!(true, pending_hit.unwrap().is_none());

        // Complete the loading entry
        hit.iter().for_each(|r| {
            let buf = Arc::new(
                AlignedBuf::new(log.clone(), r.wal_offset, r.len as usize, block_size).unwrap(),
            );
            buf.increase_written(r.len as usize);
            block_cache.add_entry(buf);
        });

        // Hit again, with 5 entries returned
        let hit = block_cache.try_get_entries(target_entry).unwrap().unwrap();
        assert_eq!(5, hit.len());
    }
}
