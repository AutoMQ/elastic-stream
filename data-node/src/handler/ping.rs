use codec::frame::Frame;
use slog::{trace, Logger};
use std::{cell::RefCell, rc::Rc};
use store::ElasticStore;

use crate::workspace::stream_manager::StreamManager;

/// Process Ping request
///
/// Ping-pong mechanism is designed to be a light weight API to probe liveness of data-node.
/// The Pong response return the same header and payload as the Ping request.
#[derive(Debug)]
pub(crate) struct Ping<'a> {
    log: Logger,
    request: &'a Frame,
}

impl<'a> Ping<'a> {
    pub(crate) fn new(log: Logger, frame: &'a Frame) -> Self {
        Self {
            log,
            request: frame,
        }
    }

    pub(crate) async fn apply(
        &self,
        _store: Rc<ElasticStore>,
        stream_manager: Rc<RefCell<StreamManager>>,
        response: &mut Frame,
    ) {
        trace!(
            self.log,
            "Ping[stream-id={}] received",
            self.request.stream_id
        );
        response.header = self.request.header.clone();
        response.payload = self.request.payload.clone();
    }
}
