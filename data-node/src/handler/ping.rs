use crate::stream_manager::StreamManager;
use codec::frame::Frame;
use log::trace;
use std::{cell::UnsafeCell, fmt, rc::Rc};
use store::Store;

/// Process Ping request
///
/// Ping-pong mechanism is designed to be a light weight API to probe liveness of data-node.
/// The Pong response return the same header and payload as the Ping request.
#[derive(Debug)]
pub(crate) struct Ping<'a> {
    request: &'a Frame,
}

impl<'a> Ping<'a> {
    pub(crate) fn new(frame: &'a Frame) -> Self {
        Self { request: frame }
    }

    pub(crate) async fn apply<S>(
        &self,
        _store: Rc<S>,
        _stream_manager: Rc<UnsafeCell<StreamManager>>,
        response: &mut Frame,
    ) where
        S: Store,
    {
        trace!("Ping[stream-id={}] received", self.request.stream_id);
        response.header = self.request.header.clone();
        response.payload = self.request.payload.clone();
    }
}

impl<'a> fmt::Display for Ping<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Ping[stream-id={}]", self.request.stream_id)
    }
}
