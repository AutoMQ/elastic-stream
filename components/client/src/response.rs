use crate::request::Request;
use crate::request::RequestExtension;
use codec::frame::Frame;
use codec::frame::OperationCode;
use log::error;
use log::trace;
use log::warn;
use model::payload::Payload;
use model::AppendResultEntry;
use protocol::rpc::header::AppendResponse;
use protocol::rpc::header::DescribePlacementManagerClusterResponse;
use protocol::rpc::header::ErrorCode;
use protocol::rpc::header::HeartbeatResponse;
use protocol::rpc::header::IdAllocationResponse;
use protocol::rpc::header::ListRangeResponse;
use protocol::rpc::header::ReportMetricsResponse;
use protocol::rpc::header::SealRangeResponse;
use protocol::rpc::header::SystemError;

use model::range::Range;
use model::PlacementManagerNode;
use model::Status;

use crate::invocation_context::InvocationContext;

#[derive(Debug, Clone)]
pub struct Response {
    /// The operation code of the response.
    pub operation_code: OperationCode,

    /// Status line
    pub status: Status,

    /// Optional response extension, containing additional operation-code-specific data.
    pub extension: Option<ResponseExtension>,
}

impl Response {
    pub fn new(operation_code: OperationCode) -> Self {
        Self {
            operation_code,
            status: Status::decode(),
            extension: None,
        }
    }

    pub fn ok(&self) -> bool {
        self.status.code == ErrorCode::OK
    }

    pub fn on_system_error(&mut self, frame: &Frame) {
        if let Some(ref buf) = frame.header {
            match flatbuffers::root::<SystemError>(buf) {
                Ok(system_error) => {
                    let system_error = system_error.unpack();
                    self.status = system_error.status.as_ref().into();
                }
                Err(e) => {
                    // Deserialize error
                    warn!(
                        "Failed to decode `SystemError` using FlatBuffers. Cause: {}",
                        e
                    );
                }
            }
        }
    }

    pub fn on_heartbeat(&mut self, frame: &Frame) {
        if let Some(ref buf) = frame.header {
            match flatbuffers::root::<HeartbeatResponse>(buf) {
                Ok(heartbeat) => {
                    trace!("Received Heartbeat response: {:?}", heartbeat);
                    let hb = heartbeat.unpack();
                    let _client_id = hb.client_id;
                    let _client_role = hb.client_role;
                    let _status = hb.status;
                    self.status = _status.as_ref().into();
                }

                Err(e) => {
                    error!("Failed to parse Heartbeat response header: {:?}", e);
                }
            }
        }
    }

    pub fn on_list_ranges(&mut self, frame: &Frame) {
        if let Some(ref buf) = frame.header {
            if let Ok(response) = flatbuffers::root::<ListRangeResponse>(buf) {
                self.status = Into::<Status>::into(&response.status().unpack());
                if self.status.code != ErrorCode::OK {
                    return;
                }
                let range = response
                    .ranges()
                    .iter()
                    .map(|item| Into::<Range>::into(&item.unpack()))
                    .collect::<Vec<_>>();
                self.extension = Some(ResponseExtension::ListRange {
                    ranges: Some(range),
                });
            }
        }
    }

    pub fn on_allocate_id(&mut self, frame: &Frame) {
        if let Some(ref buf) = frame.header {
            match flatbuffers::root::<IdAllocationResponse>(buf) {
                Ok(response) => {
                    if response.status().code() != ErrorCode::OK {
                        self.status = Into::<Status>::into(&response.status().unpack());
                        return;
                    }
                    self.status = Status::ok();
                    self.extension = Some(ResponseExtension::AllocateId { id: response.id() });
                }
                Err(e) => {
                    // Deserialize error
                    warn!( "Failed to decode `IdAllocation` response header using FlatBuffers. Cause: {}", e);
                }
            }
        }
    }

    pub fn on_append(&mut self, frame: &Frame, ctx: &InvocationContext) {
        if let Some(ref buf) = frame.header {
            match flatbuffers::root::<AppendResponse>(buf) {
                Ok(response) => {
                    let response = response.unpack();
                    if response.status.code != ErrorCode::OK {
                        self.status = response.status.as_ref().into();
                        return;
                    }
                    self.status = Status::ok();

                    let append_entries = if let RequestExtension::Append { ref buf, .. } =
                        ctx.request().extension
                    {
                        match Payload::parse_append_entries(buf) {
                            Ok(entries) => entries,
                            Err(_e) => {
                                error!(
                                    "Failed to parse append entries from request payload: {:?}",
                                    _e
                                );
                                self.status =
                                    Status::bad_request("Request payload corrupted.".to_owned());
                                return;
                            }
                        }
                    } else {
                        unreachable!()
                    };

                    if let Some(items) = response.entries {
                        debug_assert_eq!(
                            append_entries.len(),
                            items.len(),
                            "Each append-entry should have a corresponding result."
                        );
                        let entries = items
                            .into_iter()
                            .map(Into::<AppendResultEntry>::into)
                            .zip(append_entries.into_iter())
                            .map(|(mut result, entry)| {
                                result.entry = entry;
                                result
                            })
                            .collect();

                        self.extension = Some(ResponseExtension::Append { entries });
                    }
                }
                Err(e) => {
                    error!(
                        "Failed to decode AppendResponse using FlatBuffers. Cause: {}",
                        e
                    );
                }
            }
        }
    }

    pub fn on_create_range(&mut self, frame: &Frame, ctx: &InvocationContext) {}

    pub fn on_seal_range(&mut self, frame: &Frame, ctx: &InvocationContext) {
        if let Some(ref buf) = frame.header {
            match flatbuffers::root::<SealRangeResponse>(buf) {
                Ok(response) => {
                    self.status = Into::<Status>::into(&response.status().unpack());
                    if self.status.code != ErrorCode::OK {
                        if let RequestExtension::SealRange { kind, range } =
                            &ctx.request().extension
                        {
                            warn!(
                                "Seal range failed: seal-kind={:?}, range={:?}, status={:?}",
                                kind, range, self.status
                            );
                        }
                        return;
                    }
                    self.extension = Some(ResponseExtension::SealRange {
                        range: response
                            .range()
                            .map(|range| Into::<Range>::into(&range.unpack())),
                    });
                }
                Err(e) => {
                    error!(
                        "Failed to decode SealRangesResponse using FlatBuffers. Cause: {}",
                        e
                    );
                }
            }
        }
    }

    pub fn on_report_metrics(&mut self, frame: &Frame) {
        if let Some(ref buf) = frame.header {
            match flatbuffers::root::<ReportMetricsResponse>(buf) {
                Ok(response) => {
                    trace!("Received Report Metrics response: {:?}", response);
                    self.status = Into::<Status>::into(&response.status().unpack());
                }

                Err(e) => {
                    println!("buf = {:?}", buf);
                    error!("Failed to parse Report Metrics response header: {:?}", e);
                }
            }
        }
    }

    pub fn on_describe_placement_manager(&mut self, frame: &Frame) {
        if let Some(ref buf) = frame.header {
            match flatbuffers::root::<DescribePlacementManagerClusterResponse>(buf) {
                Ok(response) => {
                    self.status = Into::<Status>::into(&response.status().unpack());
                    if ErrorCode::OK != self.status.code {
                        return;
                    }

                    let nodes = response
                        .cluster()
                        .unpack()
                        .nodes
                        .iter()
                        .map(Into::into)
                        .collect::<Vec<PlacementManagerNode>>();

                    self.extension =
                        Some(ResponseExtension::DescribePlacementManager { nodes: Some(nodes) });
                }
                Err(_e) => {
                    // Deserialize error
                    warn!( "Failed to decode `DescribePlacementManagerClusterResponse` response header using FlatBuffers. Cause: {}", _e);
                }
            }
        }
    }
}

#[derive(Debug, Clone)]
pub enum ResponseExtension {
    ListRange {
        ranges: Option<Vec<Range>>,
    },

    AllocateId {
        id: i32,
    },

    DescribePlacementManager {
        nodes: Option<Vec<PlacementManagerNode>>,
    },

    SealRange {
        range: Option<Range>,
    },

    Append {
        entries: Vec<AppendResultEntry>,
    },
}
