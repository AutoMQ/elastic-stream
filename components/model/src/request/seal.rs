use protocol::rpc::header::SealKind;

use crate::range::RangeMetadata;

#[derive(Debug, Clone)]
pub struct SealRange {
    pub kind: SealKind,
    pub range: RangeMetadata,
}
