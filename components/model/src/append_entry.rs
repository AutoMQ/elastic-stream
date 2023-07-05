use std::fmt::{self, Display};

#[derive(Debug, Clone, Default)]
pub struct AppendEntry {
    /// Stream ID
    pub stream_id: u64,

    /// Range index
    pub index: u32,

    /// Base offset
    pub offset: Option<u64>,

    /// Quantity of nested records
    pub len: u32,
}

impl Display for AppendEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{{ stream_id: {}, index: {}, offset: {}, len: {} }}",
            self.stream_id,
            self.index,
            self.offset.map(|value| value as i64).unwrap_or(-1),
            self.len
        )
    }
}
