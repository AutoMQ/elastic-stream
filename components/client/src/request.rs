use bytes::{Bytes, BytesMut};
use model::fetch::FetchRequestEntry;
use model::stream::StreamMetadata;
use model::{
    client_role::ClientRole, data_node::DataNode, range::RangeMetadata, ListRangeCriteria,
};
use protocol::rpc::header::{
    AppendRequestT, CreateRangeRequestT, CreateStreamRequestT, DataNodeMetricsT,
    DescribePlacementManagerClusterRequestT, DescribeStreamRequestT, FetchEntryT, FetchRequestT,
    HeartbeatRequestT, IdAllocationRequestT, ListRangeCriteriaT, ListRangeRequestT, RangeT,
    ReportMetricsRequestT, SealKind, SealRangeRequestT,
};
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct Request {
    pub timeout: Duration,
    pub headers: Headers,
}

#[derive(Debug, Clone)]
pub enum Headers {
    Heartbeat {
        client_id: String,
        role: ClientRole,
        data_node: Option<DataNode>,
    },

    CreateStream {
        stream_metadata: StreamMetadata,
    },

    DescribeStream {
        stream_id: u64,
    },

    ListRange {
        criteria: ListRangeCriteria,
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
        buf: Vec<Bytes>,
    },

    Fetch {
        entries: Vec<FetchRequestEntry>,
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
        match &req.headers {
            Headers::Heartbeat {
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

            Headers::CreateStream { stream_metadata } => {
                let mut request = CreateStreamRequestT::default();
                let stream = stream_metadata.into();
                request.stream = Box::new(stream);
                let request = request.pack(&mut builder);
                builder.finish(request, None);
            }

            Headers::DescribeStream { stream_id } => {
                let mut request = DescribeStreamRequestT::default();
                request.timeout_ms = req.timeout.as_millis() as i32;
                request.stream_id = *stream_id as i64;
                let request = request.pack(&mut builder);
                builder.finish(request, None);
            }

            Headers::ListRange { criteria } => {
                let mut criteria_t = ListRangeCriteriaT::default();
                if let Some(node_id) = criteria.node_id {
                    criteria_t.node_id = node_id as i32;
                }

                if let Some(stream_id) = criteria.stream_id {
                    criteria_t.stream_id = stream_id as i64;
                }

                let mut request = ListRangeRequestT::default();
                request.timeout_ms = req.timeout.as_millis() as i32;
                request.criteria = Box::new(criteria_t);

                // TODO: Fill more fields for ListRange request.

                let req = request.pack(&mut builder);
                builder.finish(req, None);
            }

            Headers::AllocateId { host } => {
                let mut request = IdAllocationRequestT::default();
                request.host = host.clone();
                let request = request.pack(&mut builder);
                builder.finish(request, None);
            }

            Headers::DescribePlacementManager { data_node } => {
                let mut request = DescribePlacementManagerClusterRequestT::default();
                request.data_node = Box::new(data_node.into());
                let request = request.pack(&mut builder);
                builder.finish(request, None);
            }

            Headers::CreateRange { range } => {
                let mut request = CreateRangeRequestT::default();
                request.timeout_ms = req.timeout.as_millis() as i32;
                request.range = Box::new(range.into());
                let request = request.pack(&mut builder);
                builder.finish(request, None);
            }

            Headers::SealRange { kind, range } => {
                let mut request = SealRangeRequestT::default();
                request.timeout_ms = req.timeout.as_millis() as i32;
                request.kind = *kind;
                let mut range_t = RangeT::default();
                range_t.stream_id = range.stream_id();
                range_t.index = range.index();
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

            Headers::Append { buf: _ } => {
                let mut request = AppendRequestT::default();
                request.timeout_ms = req.timeout.as_millis() as i32;
                let request = request.pack(&mut builder);
                builder.finish(request, None);
            }

            Headers::Fetch { entries } => {
                let mut request = FetchRequestT::default();
                request.max_wait_ms = req.timeout.as_millis() as i32;
                request.entries = Some(
                    entries
                        .iter()
                        .map(|entry| {
                            let mut entry_t = FetchEntryT::default();
                            let mut range_t = RangeT::default();
                            range_t.stream_id = entry.stream_id;
                            range_t.index = entry.index;
                            entry_t.range = Box::new(range_t);
                            entry_t.fetch_offset = entry.start_offset as i64;
                            entry_t.end_offset = entry.end_offset as i64;
                            entry_t.batch_max_bytes = entry.batch_max_bytes as i32;
                            entry_t
                        })
                        .collect(),
                );
                let request = request.pack(&mut builder);
                builder.finish(request, None);
            }

            Headers::ReportMetrics {
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
