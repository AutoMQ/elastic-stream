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

    pub(crate) async fn apply<S, F>(
        &self,
        _store: Rc<S>,
        _stream_manager: Rc<UnsafeCell<StreamManager<S, F>>>,
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

#[cfg(test)]
mod tests {
    use std::{cell::UnsafeCell, error::Error, rc::Rc};

    use codec::frame::{Frame, OperationCode};
    use store::Store;

    use crate::stream_manager::{fetcher::PlacementFetcher, StreamManager};

    struct MockStore {}

    #[allow(unused_variables)]
    impl Store for MockStore {
        async fn append(
            &self,
            options: store::option::WriteOptions,
            request: store::AppendRecordRequest,
        ) -> Result<store::AppendResult, store::error::AppendError> {
            todo!()
        }

        async fn fetch(
            &self,
            options: store::option::ReadOptions,
        ) -> Result<store::FetchResult, store::error::FetchError> {
            todo!()
        }

        async fn list<F>(
            &self,
            filter: F,
        ) -> Result<Vec<model::range::RangeMetadata>, store::error::StoreError>
        where
            F: Fn(&model::range::RangeMetadata) -> bool,
        {
            todo!()
        }

        async fn list_by_stream<F>(
            &self,
            stream_id: i64,
            filter: F,
        ) -> Result<Vec<model::range::RangeMetadata>, store::error::StoreError>
        where
            F: Fn(&model::range::RangeMetadata) -> bool,
        {
            todo!()
        }

        async fn seal(
            &self,
            range: model::range::RangeMetadata,
        ) -> Result<(), store::error::StoreError> {
            todo!()
        }

        async fn create(
            &self,
            range: model::range::RangeMetadata,
        ) -> Result<(), store::error::StoreError> {
            todo!()
        }

        fn max_record_offset(
            &self,
            stream_id: i64,
            range: u32,
        ) -> Result<Option<u64>, store::error::StoreError> {
            todo!()
        }

        fn id(&self) -> i32 {
            todo!()
        }

        fn config(&self) -> std::sync::Arc<config::Configuration> {
            todo!()
        }
    }

    struct MockPlacementFetcher {}

    #[allow(unused_variables)]
    impl PlacementFetcher for MockPlacementFetcher {
        async fn bootstrap(
            &mut self,
            node_id: u32,
        ) -> Result<Vec<model::range::RangeMetadata>, crate::error::ServiceError> {
            todo!()
        }

        async fn describe_stream(
            &self,
            stream_id: u64,
        ) -> Result<model::stream::StreamMetadata, crate::error::ServiceError> {
            todo!()
        }
    }

    #[test]
    fn test_ping() -> Result<(), Box<dyn Error>> {
        let request = Frame::new(OperationCode::Ping);
        let mut response = Frame::new(OperationCode::Unknown);

        tokio_uring::start(async move {
            let ping = super::Ping::new(&request);
            let store = Rc::new(MockStore {});
            let stream_manager = Rc::new(UnsafeCell::new(StreamManager::new(
                MockPlacementFetcher {},
                Rc::clone(&store),
            )));
            ping.apply(Rc::clone(&store), stream_manager, &mut response)
                .await;
            Ok(())
        })
    }
}
