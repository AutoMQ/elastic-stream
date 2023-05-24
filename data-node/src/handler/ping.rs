use crate::stream_manager::StreamManager;
use codec::frame::Frame;
use log::trace;
use std::{cell::RefCell, rc::Rc};
use store::ElasticStore;

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

    pub(crate) async fn apply(
        &self,
        _store: Rc<ElasticStore>,
        _stream_manager: Rc<RefCell<StreamManager>>,
        response: &mut Frame,
    ) {
        trace!("Ping[stream-id={}] received", self.request.stream_id);
        response.header = self.request.header.clone();
        response.payload = self.request.payload.clone();
    }
}
