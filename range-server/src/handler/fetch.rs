use super::util::{root_as_rpc_request, MIN_BUFFER_SIZE};
use crate::stream_manager::{fetcher::PlacementFetcher, StreamManager};
use codec::frame::Frame;
use flatbuffers::FlatBufferBuilder;
use log::{trace, warn};
use minstant::Instant;
use protocol::rpc::header::{
    ErrorCode, FetchRequest, FetchResponse, FetchResponseArgs, FetchResponseT, Status, StatusArgs,
    StatusT,
};
use std::{cell::UnsafeCell, fmt, rc::Rc};
use store::{error::FetchError, option::ReadOptions, Store};

#[derive(Debug)]
pub(crate) struct Fetch<'a> {
    /// The append request already parsed by flatbuffers
    fetch_request: FetchRequest<'a>,
}

impl<'a> Fetch<'a> {
    pub(crate) fn parse_frame(request: &Frame) -> Result<Fetch, ErrorCode> {
        let request_buf = match request.header {
            Some(ref buf) => buf,
            None => {
                warn!(
                    "FetchRequest[stream-id={}] received without payload",
                    request.stream_id
                );
                return Err(ErrorCode::BAD_REQUEST);
            }
        };

        let fetch_request = match root_as_rpc_request::<FetchRequest>(request_buf) {
            Ok(request) => request,
            Err(e) => {
                warn!(
                    "FetchRequest[stream-id={}] received with invalid payload. Cause: {:?}",
                    request.stream_id, e
                );
                return Err(ErrorCode::BAD_REQUEST);
            }
        };
        trace!("Received {fetch_request:?}");
        Ok(Fetch { fetch_request })
    }

    /// Apply the fetch requests to the store
    pub(crate) async fn apply<S, F>(
        &self,
        store: Rc<S>,
        stream_manager: Rc<UnsafeCell<StreamManager<S, F>>>,
        response: &mut Frame,
    ) where
        S: Store,
        F: PlacementFetcher,
    {
        let mut builder = FlatBufferBuilder::with_capacity(MIN_BUFFER_SIZE);

        let option = match self.build_read_opt(unsafe { &mut *stream_manager.get() }) {
            Ok(opt) => opt,
            Err(_e) => {
                Self::handle_fetch_error(_e, &mut builder, response);
                return;
            }
        };

        // TODO: handle store timeout
        let start = Instant::now();
        match store.fetch(option).await {
            Ok(fetch_result) => {
                trace!(
                    "Fetch records from store took {:?}us",
                    start.elapsed().as_micros()
                );
                let status_args = StatusArgs {
                    code: ErrorCode::OK,
                    ..Default::default()
                };
                let status = Status::create(&mut builder, &status_args);

                let fetch_response_args = FetchResponseArgs {
                    status: Some(status),
                    throttle_time_ms: -1,
                };
                let fetch_response = FetchResponse::create(&mut builder, &fetch_response_args);
                builder.finish(fetch_response, None);
                let data = builder.finished_data();
                response.header = Some(bytes::Bytes::copy_from_slice(data));
                let buffers = fetch_result
                    .results
                    .into_iter()
                    .flat_map(|i| i.into_iter())
                    .collect::<Vec<_>>();
                response.payload = Some(buffers);
            }
            Err(e) => {
                Self::handle_fetch_error(e, &mut builder, response);
            }
        }
    }

    fn handle_fetch_error(
        e: FetchError,
        builder: &mut FlatBufferBuilder<'_>,
        response: &mut Frame,
    ) {
        let mut fetch_response = FetchResponseT::default();
        let mut status = StatusT::default();
        Self::convert_store_error(&e, &mut status);
        fetch_response.status = Box::new(status);
        let fetch_response = fetch_response.pack(builder);
        builder.finish(fetch_response, None);
        let data = builder.finished_data();
        response.header = Some(bytes::Bytes::copy_from_slice(data));
    }

    fn build_read_opt<S, F>(
        &self,
        stream_manager: &mut StreamManager<S, F>,
    ) -> Result<ReadOptions, FetchError>
    where
        S: Store,
        F: PlacementFetcher,
    {
        // Retrieve stream id from req.range
        let stream_id = self.fetch_request.range().stream_id();
        let range_index = self.fetch_request.range().index();
        let offset = self.fetch_request.offset();
        let limit = self.fetch_request.limit();

        // If the stream-range exists and contains the requested offset, build the read options
        // FIXME: Use range_manager instead of stream_manager
        if stream_manager.get_range(stream_id, range_index).is_some() {
            Ok(ReadOptions {
                stream_id,
                range: range_index as u32,
                offset,
                max_offset: limit as u64,
                max_wait_ms: self.fetch_request.max_wait_ms(),
                max_bytes: self.fetch_request.max_wait_ms(),
            })
        } else {
            Err(FetchError::RangeNotFound)
        }
    }

    fn convert_store_error(err: &FetchError, status: &mut StatusT) {
        status.code = match err {
            FetchError::SubmissionQueue => ErrorCode::RS_INTERNAL_SERVER_ERROR,
            FetchError::ChannelRecv => ErrorCode::RS_INTERNAL_SERVER_ERROR,
            FetchError::TranslateIndex => ErrorCode::RS_INTERNAL_SERVER_ERROR,
            FetchError::NoRecord => ErrorCode::NO_NEW_RECORD,
            FetchError::RangeNotFound => ErrorCode::RANGE_NOT_FOUND,
            FetchError::BadRequest => ErrorCode::BAD_REQUEST,
        };
        status.message = Some(err.to_string());
    }
}

impl<'a> fmt::Display for Fetch<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Fetch[{:?}]", self.fetch_request)
    }
}
