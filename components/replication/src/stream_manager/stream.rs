use super::{range::Range, window::Window};

pub(crate) struct Stream {
    id: i64,
    window: Option<Window>,
    ranges: Vec<Range>,
}

impl Stream {
    pub(crate) fn new(id: i64, ranges: Vec<Range>) -> Self {
        Self {
            id,
            window: None,
            ranges,
        }
    }
}
