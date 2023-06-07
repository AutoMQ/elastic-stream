use bytes::{BufMut, BytesMut};
use codec::frame::Frame;

use flatbuffers::FlatBufferBuilder;
use futures::{future, Future};
use log::{trace, warn};
use minstant::Instant;
use protocol::rpc::header::{
    ErrorCode, FetchRequest, FetchResponse, FetchResponseArgs, FetchResultEntry,
    FetchResultEntryArgs, Status, StatusArgs,
};
use std::{cell::UnsafeCell, fmt, pin::Pin, rc::Rc};
use store::{error::FetchError, option::ReadOptions, ElasticStore, FetchResult, Store};

use crate::stream_manager::StreamManager;

use super::util::{finish_response_builder, root_as_rpc_request, MIN_BUFFER_SIZE};

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
    pub(crate) async fn apply(
        &self,
        store: Rc<ElasticStore>,
        stream_manager: Rc<UnsafeCell<StreamManager>>,
        response: &mut Frame,
    ) {
        let store_requests = self.build_store_requests(unsafe { &mut *stream_manager.get() });
        let futures = store_requests
            .into_iter()
            .map(|fetch_option| match fetch_option {
                Ok(fetch_option) => Box::pin(store.fetch(fetch_option))
                    as Pin<Box<dyn Future<Output = Result<store::FetchResult, FetchError>>>>,
                Err(fetch_err) => {
                    let res: Result<store::FetchResult, FetchError> = Err(fetch_err);
                    Box::pin(future::ready(res))
                        as Pin<Box<dyn Future<Output = Result<store::FetchResult, FetchError>>>>
                }
            })
            .collect::<Vec<_>>();

        // TODO: handle store timeout
        let start = Instant::now();
        let res_from_store: Vec<Result<FetchResult, FetchError>> = future::join_all(futures).await;
        trace!(
            "Fetch records from store took {:?}us",
            start.elapsed().as_micros()
        );

        let mut builder = FlatBufferBuilder::with_capacity(MIN_BUFFER_SIZE);
        let mut payloads = Vec::new();
        let ok_status = Status::create(
            &mut builder,
            &StatusArgs {
                code: ErrorCode::OK,
                message: None,
                detail: None,
            },
        );
        let fetch_results: Vec<_> = res_from_store
            .into_iter()
            .map(|res| match res {
                Ok(fetch_result) => {
                    let fetch_result_args = FetchResultEntryArgs {
                        stream_id: fetch_result.stream_id,
                        batch_count: fetch_result.results.len() as i32,
                        status: Some(ok_status),
                    };
                    trace!(
                        "Fetch Stream[id={}] returns {} buffer slices",
                        fetch_result.stream_id,
                        fetch_result.results.len()
                    );
                    payloads.push(fetch_result.results);
                    FetchResultEntry::create(&mut builder, &fetch_result_args)
                }
                Err(e) => {
                    warn!("Failed to fetch from store. Cause: {:?}", e);

                    let (err_code, err_message) = self.convert_store_error(&e);
                    let mut err_message_fb = None;
                    if let Some(err_message) = err_message {
                        err_message_fb = Some(builder.create_string(err_message.as_str()));
                    }
                    let status = Status::create(
                        &mut builder,
                        &StatusArgs {
                            code: err_code,
                            message: err_message_fb,
                            detail: None,
                        },
                    );
                    let fetch_result_args = FetchResultEntryArgs {
                        stream_id: 0,
                        batch_count: 0,
                        status: Some(status),
                    };
                    FetchResultEntry::create(&mut builder, &fetch_result_args)
                }
            })
            .collect();

        let fetch_results_fb = builder.create_vector(&fetch_results);
        let fetch_response_args = FetchResponseArgs {
            throttle_time_ms: 0,
            entries: Some(fetch_results_fb),
            status: Some(ok_status),
        };
        let fetch_response = FetchResponse::create(&mut builder, &fetch_response_args);
        response.header = Some(finish_response_builder(&mut builder, fetch_response));

        // Flatten the payloads, since we already identified the sequence of the payloads
        let payloads: Vec<_> = payloads
            .into_iter()
            .flatten()
            .map(|payload| {
                // Copy all the buffer from the SingleFetchResult.
                // TODO: Find a efficient way to avoid copying the payload
                let mut bytes = BytesMut::with_capacity(
                    payload.total_len() - store::RECORD_PREFIX_LENGTH as usize,
                );

                // Strip the first `RECORD_PREFIX_LENGTH` bytes of the storage prefix
                let mut prefix = store::RECORD_PREFIX_LENGTH as usize;

                payload.iter().for_each(|buf| {
                    if prefix < buf.len() {
                        bytes.put_slice(&buf[prefix..]);
                        prefix = 0;
                    } else {
                        prefix -= buf.len();
                    }
                });

                bytes.freeze()
            })
            .collect();
        response.payload = Some(payloads);
    }

    /// TODO: this method is out of sync with new replication protocol.
    fn build_store_requests(
        &self,
        stream_manager: &mut StreamManager,
    ) -> Vec<Result<ReadOptions, FetchError>> {
        self.fetch_request
            .entries()
            .iter()
            .flatten()
            .map(|req| {
                // Retrieve stream id from req.range
                let stream_id = req.range().stream_id();
                let range_index = req.range().index();

                // If the stream-range exists and contains the requested offset, build the read options
                // FIXME: Use range_manager instead of stream_manager
                if stream_manager.get_range(stream_id, range_index).is_some() {
                    return Ok(ReadOptions {
                        stream_id,
                        range: range_index as u32,
                        offset: req.fetch_offset(),
                        max_offset: req.end_offset() as u64,
                        max_wait_ms: self.fetch_request.max_wait_ms(),
                        max_bytes: req.batch_max_bytes(),
                    });
                }
                // Cannot find the range in the current data node
                Err(FetchError::RangeNotFound)
            })
            .collect()
    }

    fn convert_store_error(&self, err: &FetchError) -> (ErrorCode, Option<String>) {
        match err {
            FetchError::SubmissionQueue => {
                (ErrorCode::DN_INTERNAL_SERVER_ERROR, Some(err.to_string()))
            }
            FetchError::ChannelRecv => (ErrorCode::DN_INTERNAL_SERVER_ERROR, Some(err.to_string())),
            FetchError::TranslateIndex => {
                (ErrorCode::DN_INTERNAL_SERVER_ERROR, Some(err.to_string()))
            }
            FetchError::NoRecord => (ErrorCode::NO_NEW_RECORD, Some(err.to_string())),
            FetchError::RangeNotFound => (ErrorCode::RANGE_NOT_FOUND, Some(err.to_string())),
            FetchError::BadRequest => (ErrorCode::BAD_REQUEST, Some(err.to_string())),
        }
    }
}

impl<'a> fmt::Display for Fetch<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Fetch[{:?}]", self.fetch_request)
    }
}
