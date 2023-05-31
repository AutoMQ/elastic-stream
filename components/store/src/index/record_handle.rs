#[derive(Debug)]
pub(crate) struct RecordHandle {
    /// WAL offset
    pub(crate) wal_offset: u64,

    /// Length of the record in WAL
    pub(crate) len: u32,
    
    pub(crate) ext: HandleExt,
}

#[derive(Debug)]
pub(crate) enum HandleExt {

    /// Hash of the record tag
    Hash(u64),
    
    /// Size of the pointed record batch
    BatchSize(u16)
}
