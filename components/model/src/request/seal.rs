use protocol::rpc::header::RangeT;

use crate::range::Range;

#[derive(Debug, Clone)]
pub enum Kind {
    Unspecified,
    DataNode,
    PlacementManager,
}

#[derive(Debug, Clone)]
pub struct SealRangeEntry {
    pub kind: Kind,
    pub range: Range,
    pub renew: bool,
}
