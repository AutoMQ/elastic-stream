#[derive(Debug)]
struct Boundary {
    start: i64,
    end: Option<i64>,
}

#[derive(Debug)]
pub(crate) struct Range {
    stream_id: i64,
    index: i32,
    boundary: Boundary,
}
