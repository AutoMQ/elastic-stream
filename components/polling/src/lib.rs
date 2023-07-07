#![feature(extract_if)]
#![feature(iter_collect_into)]

pub mod client_call;
pub mod polling_service;
pub mod stream_observer;

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, error::Error};

    use codec::frame::Frame;
    use log::trace;
    use protocol::rpc::header::OperationCode;

    use super::{
        client_call::ClientCall,
        polling_service::PollingService,
        stream_observer::{StreamError, StreamObserver},
    };

    struct TestResponseObserver;

    impl StreamObserver for TestResponseObserver {
        fn on_next(&self, response: &Frame) -> Result<(), StreamError> {
            trace!("Write response[stream-id={}]", response.stream_id);
            Ok(())
        }

        fn on_complete(&self) {
            trace!("Complete stream observer");
        }
    }

    struct TestPollingService<Observer> {
        entries: HashMap<u64, Vec<ClientCall<Observer>>>,
    }

    impl<Observer> PollingService<Observer> for TestPollingService<Observer>
    where
        Observer: StreamObserver,
    {
        fn put(&mut self, stream_id: u64, call: ClientCall<Observer>) {
            self.entries.entry(stream_id).or_insert(vec![]).push(call);
        }

        fn drain(&mut self, stream_id: u64, offset: u64) -> Option<Vec<ClientCall<Observer>>> {
            if let Some(v) = self.entries.get_mut(&stream_id) {
                Some(v.extract_if(|call| call.ready(offset)).collect())
            } else {
                None
            }
        }

        fn drain_if<F>(&mut self, pred: F) -> Vec<ClientCall<Observer>>
        where
            F: Fn(&ClientCall<Observer>) -> bool,
        {
            let mut res = vec![];
            self.entries.iter_mut().for_each(|(_, v)| {
                v.extract_if(|call| pred(call)).collect_into(&mut res);
            });
            res
        }
    }

    #[test]
    fn test_polling_service() -> Result<(), Box<dyn Error>> {
        let mut service = TestPollingService::<TestResponseObserver> {
            entries: HashMap::new(),
        };
        service.put(
            0,
            ClientCall::new(
                Frame::new(OperationCode::FETCH),
                TestResponseObserver,
                0,
                minstant::Instant::now() + std::time::Duration::from_secs(1),
            ),
        );
        if let Some(client_calls) = service.drain(0, 0) {
            assert_eq!(1, client_calls.len());
            for call in client_calls {
                call.stream_observer()
                    .on_next(&Frame::new(OperationCode::FETCH))?;
                call.stream_observer().on_complete();
            }
        } else {
            panic!("No client call found");
        }
        Ok(())
    }
}
