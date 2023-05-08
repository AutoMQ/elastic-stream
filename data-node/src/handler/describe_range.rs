use bytes::Bytes;
use codec::frame::Frame;
use log::{trace, warn};
use protocol::rpc::header::{
    DescribeRangeResultT, DescribeRangesRequest, DescribeRangesResponseT, ErrorCode, RangeT,
    StatusT,
};
use std::{cell::RefCell, rc::Rc};
use store::ElasticStore;

use crate::stream_manager::StreamManager;

use super::util::root_as_rpc_request;

#[derive(Debug)]
pub(crate) struct DescribeRange<'a> {
    /// The describe request already parsed by flatbuffers
    describe_request: DescribeRangesRequest<'a>,
}

impl<'a> DescribeRange<'a> {
    pub(crate) fn parse_frame(request: &Frame) -> Result<DescribeRange, ErrorCode> {
        let request_buf = match request.header {
            Some(ref buf) => buf,
            None => {
                warn!(
                    "DescribeRangesRequest[stream-id={}] received without payload",
                    request.stream_id
                );
                return Err(ErrorCode::BAD_REQUEST);
            }
        };

        let describe_request = match root_as_rpc_request::<DescribeRangesRequest>(request_buf) {
            Ok(request) => request,
            Err(e) => {
                warn!(
                    "DescribeRangesRequest[stream-id={}] received with invalid payload. Cause: {:?}",
                    request.stream_id,
                    e
                );
                return Err(ErrorCode::BAD_REQUEST);
            }
        };

        Ok(DescribeRange { describe_request })
    }

    pub(crate) async fn apply(
        &self,
        store: Rc<ElasticStore>,
        stream_manager: Rc<RefCell<StreamManager>>,
        response: &mut Frame,
    ) {
        let request = self.describe_request.unpack();
        let mut builder = flatbuffers::FlatBufferBuilder::new();
        let mut describe_range_response = DescribeRangesResponseT::default();
        let mut status = StatusT::default();
        status.code = ErrorCode::OK;
        status.message = Some(String::from("OK"));
        describe_range_response.status = Box::new(status);
        let mut manager = stream_manager.borrow_mut();

        if let Some(ranges) = request.ranges {
            let mut results = vec![];
            for range in ranges {
                let mut result = DescribeRangeResultT::default();
                result.stream_id = range.stream_id;
                let mut status = StatusT::default();
                match manager
                    .describe_range(range.stream_id, range.range_index)
                    .await
                {
                    Ok(range) => {
                        status.code = ErrorCode::OK;
                        status.message = Some(String::from("OK"));
                        let mut range_t = RangeT::default();
                        range_t.stream_id = range.stream_id();
                        range_t.range_index = range.index();
                        range_t.start_offset = range.start() as i64;
                        if let Some(end) = range.end() {
                            range_t.end_offset = end as i64;
                        } else {
                            range_t.end_offset = -1;
                        }
                        result.range = Some(Box::new(range_t));
                    }
                    Err(e) => {
                        status.code = ErrorCode::PM_INTERNAL_SERVER_ERROR;
                        status.message = Some(format!("{}", e));
                    }
                };
                result.status = Box::new(status);
                results.push(result);
            }
            describe_range_response.describe_responses = Some(results);
        }

        trace!("{:?}", describe_range_response);

        let describe_response = describe_range_response.pack(&mut builder);
        builder.finish(describe_response, None);
        let data = builder.finished_data();
        response.header = Some(Bytes::copy_from_slice(data));
    }
}
