pub mod seal;

use crate::{client_role::ClientRole, data_node::DataNode, range_criteria::RangeCriteria};
use bytes::{Bytes, BytesMut};
use protocol::rpc::header::{
    DataNodeT, DescribePlacementManagerClusterRequestT, HeartbeatRequestT, IdAllocationRequestT,
    ListRangeCriteria, ListRangeCriteriaT, ListRangeRequestT, RangeT, SealKind, SealRangeRequestT,
};
use std::time::Duration;

use self::seal::{Kind, SealRangeEntry};

#[derive(Debug, Clone)]
pub enum Request {
    Heartbeat {
        client_id: String,
        role: ClientRole,
        data_node: Option<DataNode>,
    },

    ListRange {
        timeout: Duration,
        criteria: RangeCriteria,
    },

    AllocateId {
        timeout: Duration,
        host: String,
    },

    DescribePlacementManager {
        data_node: DataNode,
    },

    SealRange {
        timeout: Duration,
        entry: SealRangeEntry,
    },
}

impl From<&Request> for Bytes {
    fn from(request: &Request) -> Self {
        let mut builder = flatbuffers::FlatBufferBuilder::new();
        match request {
            Request::Heartbeat {
                client_id,
                role,
                data_node,
            } => {
                let data_node = data_node.as_ref().map(|node| Box::new(node.into()));
                let mut heartbeat_request = HeartbeatRequestT::default();
                heartbeat_request.client_id = Some(client_id.to_owned());
                heartbeat_request.client_role = role.into();
                heartbeat_request.data_node = data_node;
                let heartbeat = heartbeat_request.pack(&mut builder);
                builder.finish(heartbeat, None);
            }

            Request::ListRange { timeout, criteria } => {
                let mut criteria_t = ListRangeCriteriaT::default();
                match criteria {
                    RangeCriteria::StreamId(stream_id) => {
                        criteria_t.stream_id = *stream_id;
                    }
                    RangeCriteria::DataNode(node_id) => {
                        criteria_t.node_id = *node_id;
                    }
                };

                let mut request = ListRangeRequestT::default();
                request.timeout_ms = timeout.as_millis() as i32;
                request.criteria = Box::new(criteria_t);

                // TODO: Fill more fields for ListRange request.

                let req = request.pack(&mut builder);
                builder.finish(req, None);
            }

            Request::AllocateId { timeout: _, host } => {
                let mut request = IdAllocationRequestT::default();
                request.host = host.clone();
                let request = request.pack(&mut builder);
                builder.finish(request, None);
            }

            Request::DescribePlacementManager { data_node } => {
                let mut request = DescribePlacementManagerClusterRequestT::default();
                request.data_node = Box::new(data_node.into());
                let request = request.pack(&mut builder);
                builder.finish(request, None);
            }

            Request::SealRange { timeout, entry } => {
                let mut request = SealRangeRequestT::default();
                request.timeout_ms = timeout.as_millis() as i32;

                request.kind = match entry.kind {
                    Kind::DataNode => SealKind::DATA_NODE,
                    Kind::PlacementManager => SealKind::PLACEMENT_MANAGER,
                    Kind::Unspecified => SealKind::UNSPECIFIED,
                };

                let mut range_t = RangeT::default();
                range_t.stream_id = entry.range.stream_id() as i64;
                range_t.index = entry.range.index() as i32;

                request.range = Box::new(range_t);

                let request = request.pack(&mut builder);
                builder.finish(request, None);
            }
        };
        let buf = builder.finished_data();
        let mut buffer = BytesMut::with_capacity(buf.len());
        buffer.extend_from_slice(buf);
        buffer.freeze()
    }
}
