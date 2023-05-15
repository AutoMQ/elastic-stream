use std::cell::OnceCell;

use crate::{request, response};
use log::error;
use tokio::sync::oneshot;

#[derive(Debug)]
pub struct InvocationContext {
    request: request::Request,
    pub(crate) response_observer: OnceCell<oneshot::Sender<response::Response>>,
}

impl InvocationContext {
    pub(crate) fn new(
        request: request::Request,
        response_observer: oneshot::Sender<response::Response>,
    ) -> Self {
        let cell = OnceCell::new();
        let _ = cell.set(response_observer);
        Self {
            request,
            response_observer: cell,
        }
    }

    pub(crate) fn request(&self) -> &request::Request {
        &self.request
    }

    pub(crate) fn is_closed(&self) -> bool {
        self.response_observer
            .get()
            .map_or(true, |tx| tx.is_closed())
    }

    pub(crate) fn write_response(&mut self, response: response::Response) {
        if let Some(tx) = self.response_observer.take() {
            if let Err(response) = tx.send(response) {
                error!("Failed to forward response: {:?}", response);
            }
        }
    }

    pub(crate) fn response_observer(&mut self) -> Option<oneshot::Sender<response::Response>> {
        self.response_observer.take()
    }
}
