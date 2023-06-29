use crate::range::RangeMetadata;
use crate::PlacementDriverNode;
use crate::Status;

use self::append::AppendResultEntry;

pub mod append;

#[derive(Debug, Clone)]
pub enum Response {
    Heartbeat {
        status: Status,
    },

    ListRange {
        status: Status,
        ranges: Option<Vec<RangeMetadata>>,
    },

    AllocateId {
        status: Status,
        id: i32,
    },

    DescribePlacementDriver {
        status: Status,
        nodes: Option<Vec<PlacementDriverNode>>,
    },

    SealRange {
        status: Status,
        range: Option<RangeMetadata>,
    },

    Append {
        status: Status,
        entries: Vec<AppendResultEntry>,
    },

    ReportMetrics {
        status: Status,
    },
}
