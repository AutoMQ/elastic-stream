#[derive(Debug, Default, Clone)]
pub struct ListRangeCriteria {
    pub server_id: Option<u32>,
    pub stream_id: Option<u64>,
}

impl ListRangeCriteria {
    pub fn new(server_id: Option<u32>, stream_id: Option<u64>) -> Self {
        Self {
            server_id,
            stream_id,
        }
    }
}
