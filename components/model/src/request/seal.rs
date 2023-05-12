use protocol::rpc::header::SealKind;

use crate::range::Range;

#[derive(Debug, Clone)]
pub struct SealRange {
    pub kind: SealKind,
    pub range: Range,
}
