use crate::range_manager::RangeManager;
use codec::frame::Frame;
use log::trace;
use std::{cell::UnsafeCell, fmt, rc::Rc};
use store::Store;

/// Process Ping request
///
/// Ping-pong mechanism is designed to be a light weight API to probe liveness of range-server.
/// The Pong response return the same header and payload as the Ping request.
#[derive(Debug)]
pub(crate) struct Ping<'a> {
    request: &'a Frame,
}

impl<'a> Ping<'a> {
    pub(crate) fn new(frame: &'a Frame) -> Self {
        Self { request: frame }
    }

    pub(crate) async fn apply<S, M>(
        &self,
        _store: Rc<S>,
        _range_manager: Rc<UnsafeCell<M>>,
        response: &mut Frame,
    ) where
        S: Store,
        M: RangeManager,
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

#[cfg(test)]
mod tests {
    use std::{cell::UnsafeCell, error::Error, rc::Rc};

    use crate::range_manager::MockRangeManager;
    use codec::frame::Frame;
    use protocol::rpc::header::OperationCode;
    use store::MockStore;

    #[test]
    fn test_ping() -> Result<(), Box<dyn Error>> {
        let request = Frame::new(OperationCode::PING);
        let mut response = Frame::new(OperationCode::UNKNOWN);
        let mock_store = MockStore::new();
        tokio_uring::start(async move {
            let ping = super::Ping::new(&request);
            let store = Rc::new(mock_store);
            let range_manager = Rc::new(UnsafeCell::new(MockRangeManager::new()));
            ping.apply(Rc::clone(&store), range_manager, &mut response)
                .await;
            Ok(())
        })
    }
}
