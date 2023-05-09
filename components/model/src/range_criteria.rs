#[derive(Debug, Clone)]
pub enum RangeCriteria {
    DataNode(i32),
    StreamId(i64),
}
