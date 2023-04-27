use crate::ReplicationError;

use super::{range::Range, window::Window};
use client::Client;
use std::rc::Weak;

pub(crate) struct Stream {
    id: i64,
    window: Option<Window>,
    ranges: Vec<Range>,
    client: Weak<Client>,
}

impl Stream {
    pub(crate) fn new(id: i64, client: Weak<Client>) -> Self {
        Self {
            id,
            window: None,
            ranges: vec![],
            client,
        }
    }

    pub(crate) async fn open(&mut self) -> Result<(), ReplicationError> {
        let client = self.client.upgrade().ok_or(ReplicationError::Internal)?;
        // client.list_range( Some(stream_id), self.)
        Ok(())
    }
}
