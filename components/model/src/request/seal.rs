use protocol::rpc::header::{RangeT, SealRangeEntryT, SealType};

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

/// Convert custom type to table defined in FlatBuffers IDL
impl From<&SealRangeEntry> for SealRangeEntryT {
    fn from(value: &SealRangeEntry) -> Self {
        let mut req = SealRangeEntryT::default();
        req.type_ = match value.kind {
            Kind::Unspecified => SealType::UNSPECIFIED,
            Kind::DataNode => SealType::DATA_NODE,
            Kind::PlacementManager => SealType::PLACEMENT_MANAGER,
        };
        req.renew = value.renew;
        let mut range = RangeT::default();
        range.stream_id = value.range.stream_id() as i64;
        range.range_index = value.range.index() as i32;
        range.end_offset = match value.range.end() {
            Some(end) => end as i64,
            None => -1,
        };
        
        req.range = Box::new(range);
        req
    }
}
