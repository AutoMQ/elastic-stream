#[derive(Debug, Default, Clone)]
pub struct ListRangeCriteria {
    pub node_id: Option<u32>,
    pub stream_id: Option<u64>,
}

impl ListRangeCriteria {
    pub fn new(node_id: Option<u32>, stream_id: Option<u64>) -> Self {
        Self { node_id, stream_id }
    }
}
