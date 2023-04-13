use model::range::StreamRange;
use model::PlacementManagerNode;
use model::Status;

#[derive(Debug, Clone)]
pub enum Response {
    Heartbeat {
        status: Status,
    },
    ListRange {
        status: Status,
        ranges: Option<Vec<StreamRange>>,
    },

    AllocateId {
        status: Status,
        id: i32,
    },
    DescribePlacementManager {
        status: Status,
        nodes: Option<Vec<PlacementManagerNode>>,
    },
}
