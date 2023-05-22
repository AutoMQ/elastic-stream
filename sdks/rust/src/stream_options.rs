use std::time::Duration;

#[derive(Debug, Clone)]
pub struct StreamOptions {
    pub replica: u8,
    pub ack: u8,
    pub retention: Duration,
}
