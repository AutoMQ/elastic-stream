#[derive(Debug)]
pub(crate) struct RecordHandle {
    /// WAL offset
    pub(crate) wal_offset: u64,

    /// Bytes of the `Record` in WAL
    pub(crate) len: u32,
    
    /// Extended information of the record.
    pub(crate) ext: HandleExt,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum HandleExt {

    /// Hash of the `Record` tag if it contains a single entry.
    Hash(u64),
    
    /// Number of the nested entries included in the pointed `Record`.
    BatchSize(u16)
}
