use std::{
    ffi::{CStr, CString},
    io::Cursor,
    sync::Arc,
};

use bytes::Buf;
use rocksdb::{
    compaction_filter::CompactionFilter,
    compaction_filter_factory::{CompactionFilterContext, CompactionFilterFactory},
    CompactionDecision,
};

use log::{info, trace};

use super::MinOffset;

pub(crate) struct IndexCompactionFilter {
    name: CString,
    min_offset: u64,
}

impl IndexCompactionFilter {
    pub(crate) fn new(name: CString, min_offset: u64) -> Self {
        Self { name, min_offset }
    }
}

impl CompactionFilter for IndexCompactionFilter {
    fn filter(&mut self, _level: u32, key: &[u8], value: &[u8]) -> CompactionDecision {
        if key.len() != 8 {
            return CompactionDecision::Keep;
        }

        if value.len() < 8 {
            return CompactionDecision::Remove;
        }
        let mut rdr = Cursor::new(value);
        let offset = rdr.get_u64();
        if offset < self.min_offset {
            trace!(
                "Removed {} -> {}, min-offset: {}",
                Cursor::new(key).get_u64(),
                offset,
                self.min_offset
            );
            CompactionDecision::Remove
        } else {
            CompactionDecision::Keep
        }
    }

    fn name(&self) -> &CStr {
        &self.name
    }
}

pub(crate) struct IndexCompactionFilterFactory {
    name: CString,
    min_offset: Arc<dyn MinOffset>,
}

impl IndexCompactionFilterFactory {
    pub(crate) fn new(name: CString, min_offset: Arc<dyn MinOffset>) -> Self {
        Self { name, min_offset }
    }
}

impl CompactionFilterFactory for IndexCompactionFilterFactory {
    type Filter = IndexCompactionFilter;

    fn create(&mut self, context: CompactionFilterContext) -> Self::Filter {
        info!(
            "Created a `IndexCompactionFilter`: full_compaction: {}, manual_compaction: {}, min_offset: {}",
            context.is_full_compaction,
            context.is_manual_compaction,
            self.min_offset.min_offset(),
        );

        IndexCompactionFilter::new(self.name.clone(), self.min_offset.min_offset())
    }

    fn name(&self) -> &CStr {
        &self.name
    }
}

pub(crate) struct RangeCompactionFilter {
    name: CString,
    min_offset: u64,
}

impl RangeCompactionFilter {
    pub(crate) fn new(name: CString, min_offset: u64) -> Self {
        Self { name, min_offset }
    }
}

impl CompactionFilter for RangeCompactionFilter {
    fn filter(&mut self, _level: u32, _key: &[u8], value: &[u8]) -> CompactionDecision {
        if value.len() == 8 {
            return CompactionDecision::Keep;
        }

        if value.len() == 16 {
            let mut rdr = Cursor::new(value);
            let start = rdr.get_u64();
            let end = rdr.get_u64();
            debug_assert!(
                end >= start,
                "Range end offset should be greater or equal to start"
            );
            if end <= self.min_offset {
                return CompactionDecision::Remove;
            }
        }

        CompactionDecision::Keep
    }

    fn name(&self) -> &CStr {
        &self.name
    }
}

pub(crate) struct RangeCompactionFilterFactory {
    name: CString,
    min_offset: Arc<dyn MinOffset>,
}

impl CompactionFilterFactory for RangeCompactionFilterFactory {
    type Filter = RangeCompactionFilter;

    fn create(&mut self, _context: CompactionFilterContext) -> Self::Filter {
        Self::Filter::new(self.name.clone(), self.min_offset.min_offset())
    }

    fn name(&self) -> &CStr {
        &self.name
    }
}
