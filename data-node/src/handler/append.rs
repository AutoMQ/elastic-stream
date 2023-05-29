use bytes::Bytes;
use codec::frame::Frame;

use chrono::prelude::*;
use flatbuffers::FlatBufferBuilder;
use futures::future::join_all;
use log::{error, trace, warn};
use model::{payload::Payload, record::flat_record::FlatRecordBatch, Batch};
use protocol::rpc::header::{
    AppendRequest, AppendResponseArgs, AppendResultEntryArgs, ErrorCode, StatusArgs,
};
use std::{cell::RefCell, rc::Rc};
use store::{
    error::AppendError, option::WriteOptions, AppendRecordRequest, AppendResult, ElasticStore,
    Store,
};

use crate::stream_manager::StreamManager;

use super::util::{
    finish_response_builder, root_as_rpc_request, system_error_frame_bytes, MIN_BUFFER_SIZE,
};

#[derive(Debug)]
pub(crate) struct Append<'a> {
    /// The append request already parsed by flatbuffers
    append_request: AppendRequest<'a>,

    // Layout of the request payload
    // +-------------------+-------------------+-------------------+-------------------+
    // |  AppendEntry 1    |  AppendEntry 2    |  AppendEntry 3    |        ...        |
    // +-------------------+-------------------+-------------------+-------------------+
    //
    // Layout of AppendEntry
    // +-------------------+-------------------+-------------------+------------------------------------------+
    // |  Magic Code(1B)   |  Meta Len(4B)     |       Meta        |  Payload Len(4B) | Record Batch Payload  |
    // +-------------------+-------------------+-------------------+------------------------------------------+
    payload: Bytes,
}

impl<'a> Append<'a> {
    pub(crate) fn parse_frame(request: &Frame) -> Result<Append, ErrorCode> {
        let request_buf = match request.header {
            Some(ref buf) => buf,
            None => {
                warn!(
                    "AppendRequest[stream-id={}] received without payload",
                    request.stream_id
                );
                return Err(ErrorCode::BAD_REQUEST);
            }
        };

        let append_request = match root_as_rpc_request::<AppendRequest>(request_buf) {
            Ok(request) => request,
            Err(e) => {
                warn!(
                    "AppendRequest[stream-id={}] received with invalid payload. Cause: {:?}",
                    request.stream_id, e
                );
                return Err(ErrorCode::BAD_REQUEST);
            }
        };

        let payload = match request.payload {
            // For append frame, the payload must be a single buffer
            Some(ref buf) if buf.len() == 1 => buf.first().ok_or(ErrorCode::BAD_REQUEST)?,
            _ => {
                warn!(
                    "AppendRequest[stream-id={}] received without payload",
                    request.stream_id
                );
                return Err(ErrorCode::BAD_REQUEST);
            }
        };

        Ok(Append {
            append_request,
            payload: payload.clone(),
        })
    }

    /// Process message publish request
    ///
    /// On receiving a message publish request, it wraps the incoming request to a `Record`.
    /// The record is then moved to `Store::append`, where persistence, replication and other auxillary
    /// operations are properly performed.
    ///
    /// Once the underlying operations are completed, the `Store#append` API shall asynchronously return
    /// `Result<AppendResult, AppendError>`. The result will be rendered into the `response`.
    ///
    /// `response` - Mutable response frame reference, into which required business data are filled.
    ///
    pub(crate) async fn apply(
        &self,
        store: Rc<ElasticStore>,
        stream_manager: Rc<RefCell<StreamManager>>,
        response: &mut Frame,
    ) {
        let to_store_requests = match self.build_store_requests(&stream_manager).await {
            Ok(requests) => requests,
            Err(err_code) => {
                error!("Failed to build store requests. ErrorCode: {:?}", err_code);
                // The request frame is invalid, return a system error frame directly
                response.flag_system_err();
                response.header = Some(system_error_frame_bytes(err_code, "Invalid request"));
                return;
            }
        };

        let futures: Vec<_> = to_store_requests
            .iter()
            .map(|req| {
                let options = WriteOptions::default();
                store.append(options, req.clone())
            })
            .collect();

        let res_from_store: Vec<Result<AppendResult, AppendError>> = join_all(futures).await;

        let mut builder = FlatBufferBuilder::with_capacity(MIN_BUFFER_SIZE);
        let ok_status = protocol::rpc::header::Status::create(
            &mut builder,
            &StatusArgs {
                code: ErrorCode::OK,
                message: None,
                detail: None,
            },
        );
        let append_results: Vec<_> = res_from_store
            .iter()
            .map(|res| {
                match res {
                    Ok(result) => {
                        if let Err(e) = stream_manager
                            .borrow_mut()
                            .commit(result.stream_id, result.offset as u64)
                        {
                            warn!(
                                "Failed to ack offset on store completion to stream manager: {:?}",
                                e
                            );
                        }
                        let args = AppendResultEntryArgs {
                            timestamp_ms: Utc::now().timestamp(),
                            status: Some(ok_status),
                        };
                        protocol::rpc::header::AppendResultEntry::create(&mut builder, &args)
                    }
                    Err(e) => {
                        // TODO: what to do with the offset on failure?
                        warn!("Append failed: {:?}", e);
                        let (err_code, error_message) = self.convert_store_error(e);

                        let mut error_message_fb = None;
                        if let Some(error_message) = error_message {
                            error_message_fb = Some(builder.create_string(error_message.as_str()));
                        }
                        let status = protocol::rpc::header::Status::create(
                            &mut builder,
                            &StatusArgs {
                                code: err_code,
                                message: error_message_fb,
                                detail: None,
                            },
                        );

                        let args = AppendResultEntryArgs {
                            timestamp_ms: 0,
                            status: Some(status),
                        };
                        protocol::rpc::header::AppendResultEntry::create(&mut builder, &args)
                    }
                }
            })
            .collect();

        let append_results_fb = builder.create_vector(&append_results);

        let res_args = AppendResponseArgs {
            throttle_time_ms: 0,
            entries: Some(append_results_fb),
            status: Some(ok_status),
        };

        let response_header =
            protocol::rpc::header::AppendResponse::create(&mut builder, &res_args);
        trace!("AppendResponseHeader: {:?}", response_header);
        let res_header = finish_response_builder(&mut builder, response_header);
        response.header = Some(res_header);
    }

    async fn build_store_requests(
        &self,
        stream_manager: &Rc<RefCell<StreamManager>>,
    ) -> Result<Vec<AppendRecordRequest>, ErrorCode> {
        let mut append_requests: Vec<AppendRecordRequest> = Vec::new();
        let mut pos = 0;
        while let (Some(entry), len) =
            Payload::parse_append_entry(&self.payload[pos..]).map_err(|e| {
                error!(
                    "Failed to decode append entries from payload. Cause: {:?}",
                    e
                );
                ErrorCode::BAD_REQUEST
            })?
        {
            let request = AppendRecordRequest {
                stream_id: entry.stream_id as i64,
                range_index: entry.index as i32,
                offset: entry.offset as i64,
                len: entry.len as usize,
                buffer: self.payload.slice(pos..pos + len),
            };

            pos += len;

            if let Some(range) = stream_manager
                .borrow_mut()
                .stream_range_of(request.stream_id, request.offset())
            {
                if let Some(window) = range.window_mut() {
                    if window.fast_forward(&request) {
                        append_requests.push(request);
                        // Try to dispatch previously buffered requests that become continuous
                        if let Some(req) = window.pop() {
                            append_requests.push(req);
                        }
                    } else {
                        // Buffer request whose previous has not arrived yet.
                        window.push(request);
                    }
                }
            } else {
                warn!(
                    "Target stream/range is not found. stream-id={}, range-index={}",
                    request.stream_id, request.range_index
                );
                // TODO: propagate error.
            }
        }

        Ok(append_requests)
    }

    fn convert_store_error(&self, err: &AppendError) -> (ErrorCode, Option<String>) {
        match err {
            AppendError::SubmissionQueue => (
                ErrorCode::PM_NO_AVAILABLE_DN,
                Some(AppendError::SubmissionQueue.to_string()),
            ),
            AppendError::ChannelRecv => (
                ErrorCode::PM_NO_AVAILABLE_DN,
                Some(AppendError::SubmissionQueue.to_string()),
            ),
            AppendError::System(_) => (
                ErrorCode::PM_NO_AVAILABLE_DN,
                Some(AppendError::SubmissionQueue.to_string()),
            ),
            AppendError::BadRequest => (
                ErrorCode::BAD_REQUEST,
                Some(AppendError::SubmissionQueue.to_string()),
            ),
            AppendError::Internal => (
                ErrorCode::PM_NO_AVAILABLE_DN,
                Some(AppendError::SubmissionQueue.to_string()),
            ),
        }
    }
}
