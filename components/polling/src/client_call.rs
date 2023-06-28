use std::time::Duration;

use codec::frame::{Frame, OperationCode};

use super::stream_observer::StreamObserver;

pub struct ClientCall<Observer> {
    /// Original client request.
    #[allow(unused)]
    request: Frame,

    /// Stream observer
    observer: Observer,

    /// Fetch offset
    offset: u64,

    /// Request timeout instant.
    ///
    /// Client would generate timeout response if it does not receive response
    /// from server prior to this time point.
    timeout: minstant::Instant,
}

impl<Observer> ClientCall<Observer>
where
    Observer: StreamObserver,
{
    pub fn new(
        request: Frame,
        observer: Observer,
        offset: u64,
        timeout: minstant::Instant,
    ) -> Self {
        debug_assert_eq!(OperationCode::Fetch, request.operation_code);
        Self {
            request,
            observer,
            offset,
            timeout,
        }
    }

    pub fn ready(&self, offset: u64) -> bool {
        offset >= self.offset
    }

    pub fn expire_after(&self, duration: Duration) -> bool {
        minstant::Instant::now() + duration >= self.timeout
    }

    pub fn stream_observer(&self) -> &Observer {
        &self.observer
    }
}
