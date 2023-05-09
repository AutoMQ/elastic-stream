use protocol::rpc::header::{RangeIdT, SealRangeEntryT, SealType};

#[derive(Debug, Clone)]
pub enum Kind {
    Unspecified,
    DataNode,
    PlacementManager,
}

#[derive(Debug)]
pub struct SealRangeRequest {
    pub kind: Kind,
    pub stream_id: u64,
    pub range_index: u32,
    pub end: Option<u64>,
    pub renew: bool,
}

/// Convert custom type to table defined in FlatBuffers IDL
impl From<&SealRangeRequest> for SealRangeEntryT {
    fn from(value: &SealRangeRequest) -> Self {
        let mut req = SealRangeEntryT::default();
        req.type_ = match value.kind {
            Kind::Unspecified => SealType::UNSPECIFIED,
            Kind::DataNode => SealType::DATA_NODE,
            Kind::PlacementManager => SealType::PLACEMENT_MANAGER,
        };
        req.renew = value.renew;
        req.end = match value.end {
            Some(end) => end as i64,
            None => -1,
        };
        let mut range = RangeIdT::default();
        range.stream_id = value.stream_id as i64;
        range.range_index = value.range_index as i32;
        req.range = Box::new(range);
        req
    }
}
