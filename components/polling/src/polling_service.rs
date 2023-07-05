use super::client_call::ClientCall;

pub trait PollingService<Observer> {
    /// Put an inflight fetch request into wait-list.
    ///
    /// # Arguments
    /// `stream_id` - Elastic Stream Identifier;
    /// `call` - The client call, composed of fetch request and response writer in form of stream observer;
    fn put(&mut self, stream_id: u64, call: ClientCall<Observer>);

    fn drain(&mut self, stream_id: u64, offset: u64) -> Option<Vec<ClientCall<Observer>>>;

    /// Drain all entries if the predicate returns true.
    ///
    /// A typical use case is to drain all inflight requests that are about to time out.
    fn drain_if<F>(&mut self, pred: F) -> Vec<ClientCall<Observer>>
    where
        F: Fn(&ClientCall<Observer>) -> bool + 'static;
}
