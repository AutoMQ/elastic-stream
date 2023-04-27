use model::range::StreamRange;

#[derive(Debug)]
pub(crate) struct ReplicationRange {
    pub(crate) metadata: StreamRange,
}
