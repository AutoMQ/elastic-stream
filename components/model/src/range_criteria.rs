#[derive(Debug)]
pub enum RangeCriteria {
    DataNode(i32),
    StreamId(i64),
}
