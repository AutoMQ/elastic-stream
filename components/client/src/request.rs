use bytes::{Bytes, BytesMut};
use model::{
    client_role::ClientRole, data_node::DataNode, range::RangeMetadata,
    range_criteria::RangeCriteria,
};
use protocol::rpc::header::{
    AppendRequestT, CreateRangeRequestT, DataNodeMetricsT, DescribePlacementManagerClusterRequestT,
    HeartbeatRequestT, IdAllocationRequestT, ListRangeCriteriaT, ListRangeRequestT, RangeT,
    ReportMetricsRequestT, SealKind, SealRangeRequestT,
};
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct Request {
    pub timeout: Duration,
    pub extension: RequestExtension,
}

#[derive(Debug, Clone)]
pub enum RequestExtension {
    Heartbeat {
        client_id: String,
        role: ClientRole,
        data_node: Option<DataNode>,
    },

    ListRange {
        criteria: RangeCriteria,
    },

    AllocateId {
        host: String,
    },

    DescribePlacementManager {
        data_node: DataNode,
    },

    CreateRange {
        range: RangeMetadata,
    },

    SealRange {
        kind: SealKind,
        range: RangeMetadata,
    },

    Append {
        buf: Bytes,
    },

    ReportMetrics {
        data_node: DataNode,
        disk_in_rate: i64,
        disk_out_rate: i64,
        disk_free_space: i64,
        disk_unindexed_data_size: i64,
        memory_used: i64,
        uring_task_rate: i16,
        uring_inflight_task_cnt: i16,
        uring_pending_task_cnt: i32,
        uring_task_avg_latency: i16,
        network_append_rate: i16,
        network_fetch_rate: i16,
        network_failed_append_rate: i16,
        network_failed_fetch_rate: i16,
        network_append_avg_latency: i16,
        network_fetch_avg_latency: i16,
        range_missing_replica_cnt: i16,
        range_active_cnt: i16,
    },
}

impl From<&Request> for Bytes {
    fn from(req: &Request) -> Self {
        let mut builder = flatbuffers::FlatBufferBuilder::new();
        match &req.extension {
            RequestExtension::Heartbeat {
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

            RequestExtension::ListRange { criteria } => {
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
                request.timeout_ms = req.timeout.as_millis() as i32;
                request.criteria = Box::new(criteria_t);

                // TODO: Fill more fields for ListRange request.

                let req = request.pack(&mut builder);
                builder.finish(req, None);
            }

            RequestExtension::AllocateId { host } => {
                let mut request = IdAllocationRequestT::default();
                request.host = host.clone();
                let request = request.pack(&mut builder);
                builder.finish(request, None);
            }

            RequestExtension::DescribePlacementManager { data_node } => {
                let mut request = DescribePlacementManagerClusterRequestT::default();
                request.data_node = Box::new(data_node.into());
                let request = request.pack(&mut builder);
                builder.finish(request, None);
            }

            RequestExtension::CreateRange { range } => {
                let mut request = CreateRangeRequestT::default();
                request.timeout_ms = req.timeout.as_millis() as i32;
                request.range = Box::new(range.into());
                let request = request.pack(&mut builder);
                builder.finish(request, None);
            }

            RequestExtension::SealRange { kind, range } => {
                let mut request = SealRangeRequestT::default();
                request.timeout_ms = req.timeout.as_millis() as i32;
                request.kind = *kind;
                let mut range_t = RangeT::default();
                range_t.stream_id = range.stream_id() as i64;
                range_t.index = range.index() as i32;
                range_t.epoch = range.epoch() as i64;
                range_t.start = range.start() as i64;
                range_t.end = match range.end() {
                    Some(offset) => offset as i64,
                    None => -1,
                };

                request.range = Box::new(range_t);
                let request = request.pack(&mut builder);
                builder.finish(request, None);
            }

            RequestExtension::Append { buf: _ } => {
                let mut request = AppendRequestT::default();
                request.timeout_ms = req.timeout.as_millis() as i32;
                let request = request.pack(&mut builder);
                builder.finish(request, None);
            }

            RequestExtension::ReportMetrics {
                data_node,
                disk_in_rate,
                disk_out_rate,
                disk_free_space,
                disk_unindexed_data_size,
                memory_used,
                uring_task_rate,
                uring_inflight_task_cnt,
                uring_pending_task_cnt,
                uring_task_avg_latency,
                network_append_rate,
                network_fetch_rate,
                network_failed_append_rate,
                network_failed_fetch_rate,
                network_append_avg_latency,
                network_fetch_avg_latency,
                range_missing_replica_cnt,
                range_active_cnt,
            } => {
                let mut metrics = DataNodeMetricsT::default();
                metrics.disk_in_rate = *disk_in_rate;
                metrics.disk_out_rate = *disk_out_rate;
                metrics.disk_free_space = *disk_free_space;
                metrics.disk_unindexed_data_size = *disk_unindexed_data_size;
                metrics.memory_used = *memory_used;
                metrics.uring_task_rate = *uring_task_rate;
                metrics.uring_inflight_task_cnt = *uring_inflight_task_cnt;
                metrics.uring_pending_task_cnt = *uring_pending_task_cnt;
                metrics.uring_task_avg_latency = *uring_task_avg_latency;
                metrics.network_append_rate = *network_append_rate;
                metrics.network_fetch_rate = *network_fetch_rate;
                metrics.network_failed_append_rate = *network_failed_append_rate;
                metrics.network_failed_fetch_rate = *network_failed_fetch_rate;
                metrics.network_append_avg_latency = *network_append_avg_latency;
                metrics.network_fetch_avg_latency = *network_fetch_avg_latency;
                metrics.range_missing_replica_cnt = *range_missing_replica_cnt;
                metrics.range_active_cnt = *range_active_cnt;

                let mut request = ReportMetricsRequestT::default();
                request.data_node = Some(Box::new(data_node.into()));
                request.metrics = Some(Box::new(metrics));

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
