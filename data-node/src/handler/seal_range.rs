use std::{cell::RefCell, rc::Rc};

use bytes::Bytes;
use codec::frame::Frame;
use protocol::rpc::header::{
    ErrorCode, RangeT, SealRangesRequest, SealRangesResponseT, SealRangesResultT, StatusT,
};
use slog::{error, trace, warn, Logger};
use store::ElasticStore;

use crate::stream_manager::StreamManager;

use super::util::root_as_rpc_request;

#[derive(Debug)]
pub(crate) struct SealRange<'a> {
    log: Logger,
    request: SealRangesRequest<'a>,
}

impl<'a> SealRange<'a> {
    pub(crate) fn parse_frame(log: Logger, frame: &'a Frame) -> Result<Self, ErrorCode> {
        let request = frame
            .header
            .as_ref()
            .map(|buf| root_as_rpc_request::<SealRangesRequest>(buf))
            .ok_or(ErrorCode::BAD_REQUEST)?
            .map_err(|_e| {
                warn!(
                    log,
                    "Received an invalid seal range request[stream-id={}]", frame.stream_id
                );
                ErrorCode::BAD_REQUEST
            })?;

        Ok(Self { log, request })
    }

    pub(crate) async fn apply(
        &self,
        store: Rc<ElasticStore>,
        stream_manager: Rc<RefCell<StreamManager>>,
        response: &mut Frame,
    ) {
        let request = self.request.unpack();
        let mut builder = flatbuffers::FlatBufferBuilder::new();
        let mut seal_response = SealRangesResponseT::default();
        let mut status = StatusT::default();
        status.code = ErrorCode::OK;
        status.message = Some(String::from("OK"));
        seal_response.status = Box::new(status);
        let mut manager = stream_manager.borrow_mut();

        let mut results = vec![];
        for range in request.ranges.iter().flatten() {
            let mut result = SealRangesResultT::default();
            let mut status = StatusT::default();
            match manager.seal(range.stream_id, range.range_index).await {
                Ok(offset) => {
                    status.code = ErrorCode::OK;
                    status.message = Some(String::from("OK"));
                    let mut item = RangeT::default();
                    item.stream_id = range.stream_id;
                    item.range_index = range.range_index;
                    item.end_offset = offset as i64;
                    item.next_offset = offset as i64;
                    result.range = Some(Box::new(item));
                }
                Err(e) => {
                    error!(
                        self.log,
                        "Failed to seal stream-id={}, range_index={}",
                        range.stream_id,
                        range.range_index
                    );
                    status.code = ErrorCode::DN_INTERNAL_SERVER_ERROR;
                    status.message = Some(format!("{:?}", e));
                }
            }
            result.status = Box::new(status);
            results.push(result);
        }
        seal_response.seal_responses = Some(results);
        trace!(self.log, "{:?}", seal_response);
        let resp = seal_response.pack(&mut builder);
        builder.finish(resp, None);
        let data = builder.finished_data();
        response.header = Some(Bytes::copy_from_slice(data));
    }
}
