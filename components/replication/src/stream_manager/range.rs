use model::range::StreamRange;

#[derive(Debug)]
pub(crate) struct Boundary {
    start: u64,
    pub(crate) end: Option<u64>,
}

#[derive(Debug)]
pub(crate) struct Range {
    stream_id: i64,
    index: i32,
    pub(crate) boundary: Boundary,
}

impl From<&StreamRange> for Range {
    fn from(value: &StreamRange) -> Self {
        Self {
            stream_id: value.stream_id(),
            index: value.index(),
            boundary: Boundary {
                start: value.start(),
                end: value.end(),
            },
        }
    }
}
