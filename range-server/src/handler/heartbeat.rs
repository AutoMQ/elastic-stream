use std::{cell::UnsafeCell, fmt, rc::Rc};

use bytes::Bytes;
use codec::frame::Frame;
use log::trace;
use protocol::rpc::header::{ErrorCode, HeartbeatRequest, HeartbeatResponseT, StatusT};
use store::Store;

use crate::stream_manager::StreamManager;

use super::util::root_as_rpc_request;

#[derive(Debug)]
pub(crate) struct Heartbeat<'a> {
    request: HeartbeatRequest<'a>,
}

impl<'a> Heartbeat<'a> {
    pub(crate) fn parse_frame(frame: &'a Frame) -> Result<Self, ErrorCode> {
        let request = frame
            .header
            .as_ref()
            .map(|buf| root_as_rpc_request::<HeartbeatRequest>(buf))
            .ok_or(ErrorCode::BAD_REQUEST)?
            .map_err(|_e| ErrorCode::BAD_REQUEST)?;

        Ok(Self { request })
    }

    pub(crate) async fn apply<S, M>(
        &self,
        _store: Rc<S>,
        _stream_manager: Rc<UnsafeCell<M>>,
        response: &mut Frame,
    ) where
        S: Store,
        M: StreamManager,
    {
        trace!("Prepare heartbeat response header for {:?}", self.request);

        let mut builder = flatbuffers::FlatBufferBuilder::new();
        let mut response_header = HeartbeatResponseT::default();

        let mut status = StatusT::default();
        status.code = ErrorCode::OK;
        status.message = Some(String::from("OK"));
        response_header.status = Box::new(status);

        response_header.client_id = self.request.client_id().map(|id| id.to_owned());
        response_header.client_role = self.request.client_role();

        let header = response_header.pack(&mut builder);
        builder.finish(header, None);
        let data = builder.finished_data();
        response.header = Some(Bytes::copy_from_slice(data));

        trace!("Heartbeat response header built");
    }
}

impl<'a> fmt::Display for Heartbeat<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self.request)
    }
}
