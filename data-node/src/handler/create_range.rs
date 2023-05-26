use std::{cell::RefCell, rc::Rc};

use bytes::Bytes;
use codec::frame::Frame;
use log::{error, trace, warn};
use model::range::RangeMetadata;
use protocol::rpc::header::{CreateRangeRequest, ErrorCode, RangeT, SealRangeResponseT, StatusT};
use store::ElasticStore;

use crate::stream_manager::StreamManager;

use super::util::root_as_rpc_request;

#[derive(Debug)]
pub(crate) struct CreateRange<'a> {
    request: CreateRangeRequest<'a>,
}

impl<'a> CreateRange<'a> {
    pub(crate) fn parse_frame(frame: &'a Frame) -> Result<Self, ErrorCode> {
        let request = frame
            .header
            .as_ref()
            .map(|buf| root_as_rpc_request::<CreateRangeRequest>(buf))
            .ok_or(ErrorCode::BAD_REQUEST)?
            .map_err(|_e| {
                warn!(
                    "Received an invalid create range request[stream-id={}]",
                    frame.stream_id
                );
                ErrorCode::BAD_REQUEST
            })?;

        Ok(Self { request })
    }

    pub(crate) async fn apply(
        &self,
        _store: Rc<ElasticStore>,
        stream_manager: Rc<RefCell<StreamManager>>,
        response: &mut Frame,
    ) {
        let request = self.request.unpack();
        let mut builder = flatbuffers::FlatBufferBuilder::new();
        let mut create_response = SealRangeResponseT::default();

        let mut manager = stream_manager.borrow_mut();

        let range = request.range;
        let range: RangeMetadata = Into::<RangeMetadata>::into(&*range);
        if let Err(e) = manager.create_range(range.clone()).await {
            error!("Failed to create range: {:?}", e);
            let mut status = StatusT::default();
            // TODO: Map service error to the corresponding error code.
            status.code = ErrorCode::DN_INTERNAL_SERVER_ERROR;
            status.message = Some(format!("Failed to create range: {}", e.to_string()));
            create_response.status = Box::new(status);
        } else {
            let mut status = StatusT::default();
            status.code = ErrorCode::OK;
            status.message = Some(String::from("OK"));
            create_response.status = Box::new(status);
            // Write back the range metadata to client so that client may maintain consistent code.
            create_response.range = Some(Box::new(Into::<RangeT>::into(&range)));
            trace!("Created range={:?}", range);
        }

        let resp = create_response.pack(&mut builder);
        builder.finish(resp, None);
        let data = builder.finished_data();
        response.header = Some(Bytes::copy_from_slice(data));
    }
}
