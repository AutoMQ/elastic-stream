use protocol::rpc::header::RangeT;

use crate::range::StreamRange;

#[derive(Debug, Clone)]
pub enum Kind {
    Unspecified,
    DataNode,
    PlacementManager,
}

#[derive(Debug, Clone)]
pub struct SealRangeEntry {
    pub kind: Kind,
    pub range: StreamRange,
    pub renew: bool,
}
