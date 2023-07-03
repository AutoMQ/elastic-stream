use std::{cell::UnsafeCell, fmt, rc::Rc};

use bytes::Bytes;
use codec::frame::Frame;
use log::{error, trace, warn};
use model::range::RangeMetadata;
use protocol::rpc::header::{ErrorCode, RangeT, SealRangeRequest, SealRangeResponseT, StatusT};
use store::Store;

use crate::{error::ServiceError, stream_manager::StreamManager};

use super::util::root_as_rpc_request;

#[derive(Debug)]
pub(crate) struct SealRange<'a> {
    request: SealRangeRequest<'a>,
}

impl<'a> SealRange<'a> {
    pub(crate) fn parse_frame(frame: &'a Frame) -> Result<Self, ErrorCode> {
        let request = frame
            .header
            .as_ref()
            .map(|buf| root_as_rpc_request::<SealRangeRequest>(buf))
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

    pub(crate) async fn apply<S, M>(
        &self,
        _store: Rc<S>,
        stream_manager: Rc<UnsafeCell<M>>,
        response: &mut Frame,
    ) where
        S: Store,
        M: StreamManager,
    {
        let request = self.request.unpack();
        let mut builder = flatbuffers::FlatBufferBuilder::new();
        let mut seal_response = SealRangeResponseT::default();
        let mut status = StatusT::default();
        status.code = ErrorCode::OK;
        status.message = Some(String::from("OK"));
        let manager = unsafe { &mut *stream_manager.get() };

        let range = request.range;
        let mut range = Into::<RangeMetadata>::into(&*range);

        match manager.seal(&mut range) {
            Ok(_) => {
                status.code = ErrorCode::OK;
                status.message = Some(String::from("OK"));
                seal_response.range = Some(Box::new(Into::<RangeT>::into(&range)));
            }
            Err(e) => {
                error!(
                    "Failed to seal stream-id={}, range_index={}",
                    range.stream_id(),
                    range.index()
                );
                match e {
                    ServiceError::NotFound(_) => status.code = ErrorCode::RANGE_NOT_FOUND,
                    _ => status.code = ErrorCode::RS_INTERNAL_SERVER_ERROR,
                }
                status.message = Some(format!("{:?}", e));
            }
        }
        seal_response.status = Box::new(status);

        trace!("{:?}", seal_response);
        let resp = seal_response.pack(&mut builder);
        builder.finish(resp, None);
        let data = builder.finished_data();
        response.header = Some(Bytes::copy_from_slice(data));
    }
}

impl<'a> fmt::Display for SealRange<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self.request)
    }
}
