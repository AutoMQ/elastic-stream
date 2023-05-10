use std::{cell::RefCell, rc::Rc};

use bytes::Bytes;
use codec::frame::Frame;
use log::{error, trace, warn};
use protocol::rpc::header::{
    ErrorCode, RangeT, SealRangeResultT, SealRangesRequest, SealRangesResponseT, SealType, StatusT,
};
use store::ElasticStore;

use crate::stream_manager::StreamManager;

use super::util::root_as_rpc_request;

#[derive(Debug)]
pub(crate) struct SealRange<'a> {
    request: SealRangesRequest<'a>,
}

impl<'a> SealRange<'a> {
    pub(crate) fn parse_frame(frame: &'a Frame) -> Result<Self, ErrorCode> {
        let request = frame
            .header
            .as_ref()
            .map(|buf| root_as_rpc_request::<SealRangesRequest>(buf))
            .ok_or(ErrorCode::BAD_REQUEST)?
            .map_err(|_e| {
                warn!(
                    "Received an invalid seal range request[stream-id={}]",
                    frame.stream_id
                );
                ErrorCode::BAD_REQUEST
            })?;

        Ok(Self { request })
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
        for entry in request.entries.iter() {
            debug_assert_eq!(entry.type_, SealType::DATA_NODE);
            let mut result = SealRangeResultT::default();
            let mut status = StatusT::default();
            match manager
                .seal(entry.range.stream_id, entry.range.range_index)
                .await
            {
                Ok(offset) => {
                    status.code = ErrorCode::OK;
                    status.message = Some(String::from("OK"));
                    let mut item = RangeT::default();
                    item.stream_id = entry.range.stream_id;
                    item.range_index = entry.range.range_index;
                    item.end_offset = offset as i64;
                    result.range = Some(Box::new(item));
                }
                Err(e) => {
                    error!(
                        "Failed to seal stream-id={}, range_index={}",
                        entry.range.stream_id, entry.range.range_index
                    );
                    status.code = ErrorCode::DN_INTERNAL_SERVER_ERROR;
                    status.message = Some(format!("{:?}", e));
                }
            }
            result.status = Box::new(status);
            results.push(result);
        }
        seal_response.results = results;
        trace!("{:?}", seal_response);
        let resp = seal_response.pack(&mut builder);
        builder.finish(resp, None);
        let data = builder.finished_data();
        response.header = Some(Bytes::copy_from_slice(data));
    }
}
