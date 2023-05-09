use bytes::{BufMut, BytesMut};
use codec::frame::Frame;

use flatbuffers::FlatBufferBuilder;
use futures::Future;
use log::warn;
use protocol::rpc::header::{
    ErrorCode, FetchRequest, FetchResponseArgs, FetchResultArgs, StatusArgs,
};
use std::{cell::RefCell, pin::Pin, rc::Rc};
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

        Ok(Fetch { fetch_request })
    }

    /// Apply the fetch requests to the store
    pub(crate) async fn apply(
        &self,
        store: Rc<ElasticStore>,
        stream_manager: Rc<RefCell<StreamManager>>,
        response: &mut Frame,
    ) {
        let store_requests = self.build_store_requests(stream_manager.clone());
        let futures = store_requests
            .into_iter()
            .map(|fetch_option| match fetch_option {
                Ok(fetch_option) => Box::pin(store.fetch(fetch_option))
                    as Pin<Box<dyn Future<Output = Result<store::FetchResult, FetchError>>>>,
                Err(fetch_err) => {
                    let res: Result<store::FetchResult, FetchError> = Err(fetch_err);
                    Box::pin(futures::future::ready(res))
                        as Pin<Box<dyn Future<Output = Result<store::FetchResult, FetchError>>>>
                }
            })
            .collect::<Vec<_>>();

        let res_from_store: Vec<Result<FetchResult, FetchError>> =
            futures::future::join_all(futures).await;

        let mut builder = FlatBufferBuilder::with_capacity(MIN_BUFFER_SIZE);
        let mut payloads = Vec::new();
        let ok_status = protocol::rpc::header::Status::create(
            &mut builder,
            &StatusArgs {
                code: ErrorCode::OK,
                message: None,
                detail: None,
            },
        );
        let fetch_results: Vec<_> = res_from_store
            .into_iter()
            .map(|res| {
                match res {
                    Ok(fetch_result) => {
                        let fetch_result_args = FetchResultArgs {
                            stream_id: fetch_result.stream_id,
                            batch_count: fetch_result.results.len() as i32,
                            status: Some(ok_status),
                        };
                        payloads.push(fetch_result.results);
                        protocol::rpc::header::FetchResult::create(&mut builder, &fetch_result_args)
                    }
                    Err(e) => {
                        warn!("Failed to fetch from store. Cause: {:?}", e);

                        let (err_code, err_message) = self.convert_store_error(&e);
                        let mut err_message_fb = None;
                        if let Some(err_message) = err_message {
                            err_message_fb = Some(builder.create_string(err_message.as_str()));
                        }
                        let status = protocol::rpc::header::Status::create(
                            &mut builder,
                            &StatusArgs {
                                code: err_code,
                                message: err_message_fb,
                                detail: None,
                            },
                        );
                        let fetch_result_args = FetchResultArgs {
                            stream_id: 0,
                            batch_count: 0,
                            status: Some(status),
                        };
                        protocol::rpc::header::FetchResult::create(&mut builder, &fetch_result_args)
                    }
                }
            })
            .collect();

        let fetch_results_fb = builder.create_vector(&fetch_results);
        let res_args = FetchResponseArgs {
            throttle_time_ms: 0,
            results: Some(fetch_results_fb),
            status: Some(ok_status),
        };
        let res_offset = protocol::rpc::header::FetchResponse::create(&mut builder, &res_args);
        let res_header = finish_response_builder(&mut builder, res_offset);
        response.header = Some(res_header);

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

    fn build_store_requests(
        &self,
        stream_manager: Rc<RefCell<StreamManager>>,
    ) -> Vec<Result<ReadOptions, FetchError>> {
        self.fetch_request
            .entries()
            .iter()
            .flatten()
            .map(|req| {
                // Retrieve stream id from req.range, return error if it's a bad request
                let stream_id = match req.range().and_then(|r| Some(r.stream_id())) {
                    Some(stream_id) => stream_id,
                    None => return Err(FetchError::BadRequest),
                };

                // If the stream-range exists and contains the requested offset, build the read options
                if let Some(range) = stream_manager
                    .borrow()
                    .stream_range_of(stream_id, req.fetch_offset() as u64)
                {
                    let max_offset = match range.end() {
                        // For a sealed range, the upper bound is the end offset(exclusive)
                        Some(end) => Some(end as i64),
                        // For a non-sealed range, the upper bound is the limit offset(exclusive)
                        // Please note that the stream manager only updates the limit offset after a successful replication
                        None => Some(range.limit() as i64),
                    };
                    return Ok(ReadOptions {
                        stream_id: stream_id,
                        offset: req.fetch_offset(),
                        max_offset: max_offset,
                        max_wait_ms: self.fetch_request.max_wait_ms(),
                        max_bytes: req.batch_max_bytes(),
                    });
                }
                // Cannot find the range in the current data node
                return Err(FetchError::RangeNotFound);
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
