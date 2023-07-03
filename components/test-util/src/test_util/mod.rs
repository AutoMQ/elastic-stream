//! Util functions for tests.
//!

use std::time::{self, Duration, UNIX_EPOCH};

use bytes::Bytes;
use codec::frame::{Frame, OperationCode};
use log::{debug, error, info, trace, warn};
use model::payload::Payload;
use protocol::rpc::header::{
    AppendResponseT, AppendResultEntryT, CreateRangeRequest, CreateRangeResponseT,
    CreateStreamRequest, CreateStreamResponseT, DescribePlacementDriverClusterRequest,
    DescribePlacementDriverClusterResponseT, DescribeStreamRequest, DescribeStreamResponseT,
    ErrorCode, HeartbeatRequest, HeartbeatResponseT, IdAllocationRequest, IdAllocationResponseT,
    ListRangeRequest, ListRangeResponseT, PlacementDriverClusterT, PlacementDriverNodeT, RangeT,
    ReportMetricsRequest, ReportMetricsResponseT, SealKind, SealRangeRequest, SealRangeResponseT,
    StatusT, StreamT,
};

use tokio::sync::oneshot;
use tokio_uring::net::TcpListener;
use transport::connection::Connection;

fn serve_heartbeat(request: &HeartbeatRequest, frame: &mut Frame) {
    debug!("{:?}", request);
    frame.operation_code = OperationCode::Heartbeat;
    let mut response = HeartbeatResponseT::default();
    response.client_id = request.client_id().map(|client_id| client_id.to_owned());
    response.client_role = request.client_role();
    response.range_server = request
        .range_server()
        .map(|range_server| Box::new(range_server.unpack()));
    let mut status = StatusT::default();
    status.code = ErrorCode::OK;
    status.message = Some("OK".to_owned());
    response.status = Box::new(status);
    let mut builder = flatbuffers::FlatBufferBuilder::with_capacity(1024);
    let resp = response.pack(&mut builder);
    builder.finish(resp, None);

    let hdr = builder.finished_data();

    let buf = Bytes::copy_from_slice(hdr);
    frame.header = Some(buf);
}

fn serve_list_ranges(request: &ListRangeRequest, frame: &mut Frame) {
    trace!("Received a list-ranges request: {:?}", request);
    let mut builder = flatbuffers::FlatBufferBuilder::with_capacity(1024);
    let mut resp = ListRangeResponseT::default();

    let mut status = StatusT::default();
    status.code = ErrorCode::OK;
    status.message = Some(String::from("OK"));
    resp.status = Box::new(status);

    let ranges = (0..10)
        .map(|i| {
            let mut range = RangeT::default();
            range.stream_id = 0;
            range.index = i as i32;
            range.start = i * 100;
            range.end = (i + 1) * 100;
            range
        })
        .collect::<Vec<_>>();
    resp.ranges = ranges;
    let resp = resp.pack(&mut builder);
    builder.finish(resp, None);
    let buf = builder.finished_data();
    frame.header = Some(Bytes::copy_from_slice(buf));
}

fn serve_describe_placement_driver_cluster(
    request: &DescribePlacementDriverClusterRequest,
    frame: &mut Frame,
    port: u16,
) {
    trace!("Received: {:?}", request);
    let mut builder = flatbuffers::FlatBufferBuilder::with_capacity(1024);
    let mut resp = DescribePlacementDriverClusterResponseT::default();
    {
        let mut cluster = PlacementDriverClusterT::default();

        cluster.nodes = (0..1)
            .map(|i| {
                let mut node = PlacementDriverNodeT::default();
                node.is_leader = i == 0;
                node.name = format!("node-{}", i);
                node.advertise_addr = format!("localhost:{}", port);
                node
            })
            .collect();

        let mut status = StatusT::default();
        status.code = ErrorCode::OK;
        status.message = Some(String::from("OK"));
        resp.status = Box::new(status);
        resp.cluster = Box::new(cluster);
    }

    let resp = resp.pack(&mut builder);
    builder.finish(resp, None);
    let buf = builder.finished_data();
    frame.header = Some(Bytes::copy_from_slice(buf));
}

fn serve_report_metrics(request: &ReportMetricsRequest, frame: &mut Frame) {
    debug!("{:?}", request);
    frame.operation_code = OperationCode::ReportMetrics;
    let mut response = ReportMetricsResponseT::default();
    response.range_server = request
        .range_server()
        .map(|range_server| Box::new(range_server.unpack()));
    let mut status = StatusT::default();
    status.code = ErrorCode::OK;
    status.message = Some("OK".to_owned());
    response.status = Box::new(status);
    let mut builder = flatbuffers::FlatBufferBuilder::with_capacity(1024);
    let resp = response.pack(&mut builder);
    builder.finish(resp, None);
    let hdr = builder.finished_data();
    let buf = Bytes::copy_from_slice(hdr);
    frame.header = Some(buf);
}
/// Run a dummy listening server.
/// Once it accepts a connection, it quits immediately.
pub async fn run_listener() -> u16 {
    let (tx, rx) = oneshot::channel();
    tokio_uring::spawn(async move {
        // We are using dual-stack mode.
        // Binding to "[::]:0", the any address for IPv6, will also listen for IPv4.
        let listener = TcpListener::bind("[::]:0".parse().unwrap()).unwrap();
        let port = listener.local_addr().unwrap().port();
        debug!("TestServer is up, listening {}", port);
        tx.send(port).unwrap();
        while let Ok((conn, sock_addr)) = listener.accept().await {
            info!("TestServer accepted a connection from {:?}", sock_addr);
            tokio_uring::spawn(async move {
                let addr = sock_addr.to_string();
                let channel = Connection::new(conn, &addr);

                loop {
                    if let Ok(frame) = channel.read_frame().await {
                        if let Some(frame) = frame {
                            info!(
                                "TestServer is processing a `{}` request",
                                frame.operation_code
                            );

                            let mut response_frame = Frame::new(OperationCode::Unknown);
                            response_frame.flag_response();
                            response_frame.stream_id = frame.stream_id;

                            match frame.operation_code {
                                OperationCode::Heartbeat => {
                                    if let Some(buf) = &frame.header {
                                        if let Ok(heartbeat) =
                                            flatbuffers::root::<HeartbeatRequest>(buf)
                                        {
                                            trace!("Start to sleep...");
                                            tokio::time::sleep(Duration::from_millis(500)).await;
                                            trace!("Heartbeat sleep completed");
                                            serve_heartbeat(&heartbeat, &mut response_frame);
                                        } else {
                                            error!("Failed to decode heartbeat request header");
                                        }
                                    }
                                }

                                OperationCode::ListRange => {
                                    response_frame.operation_code = OperationCode::ListRange;
                                    if let Some(buf) = frame.header.as_ref() {
                                        if let Ok(req) = flatbuffers::root::<ListRangeRequest>(buf)
                                        {
                                            serve_list_ranges(&req, &mut response_frame);
                                        } else {
                                            error!("Failed to decode list-range-request header");
                                        }
                                    }
                                }

                                OperationCode::AllocateId => {
                                    response_frame.operation_code = OperationCode::AllocateId;
                                    if let Some(buf) = frame.header.as_ref() {
                                        match flatbuffers::root::<IdAllocationRequest>(buf) {
                                            Ok(request) => {
                                                allocate_id(&request, &mut response_frame);
                                            }
                                            Err(_e) => {
                                                error!(
                                                    "Failed to decode id-allocation-request header"
                                                );
                                            }
                                        }
                                    }
                                }

                                OperationCode::DescribePlacementDriver => {
                                    response_frame.operation_code =
                                        OperationCode::DescribePlacementDriver;
                                    if let Some(ref buf) = frame.header {
                                        match flatbuffers::root::<
                                            DescribePlacementDriverClusterRequest,
                                        >(buf)
                                        {
                                            Ok(request) => serve_describe_placement_driver_cluster(
                                                &request,
                                                &mut response_frame,
                                                port,
                                            ),
                                            Err(e) => {
                                                error!(
                                                    "Failed to decode describe-placement-driver-request header: {:?}", e
                                                );
                                            }
                                        }
                                    }
                                }

                                OperationCode::CreateStream => {
                                    response_frame.operation_code = OperationCode::CreateStream;
                                    if let Some(buf) = frame.header.as_ref() {
                                        if let Ok(req) =
                                            flatbuffers::root::<CreateStreamRequest>(buf)
                                        {
                                            serve_create_stream(&req, &mut response_frame);
                                        } else {
                                            error!("Failed to decode create-stream-request header");
                                        }
                                    }
                                }

                                OperationCode::DescribeStream => {
                                    response_frame.operation_code = OperationCode::DescribeStream;
                                    if let Some(buf) = frame.header.as_ref() {
                                        if let Ok(req) =
                                            flatbuffers::root::<DescribeStreamRequest>(buf)
                                        {
                                            serve_describe_stream(&req, &mut response_frame);
                                        } else {
                                            error!(
                                                "Failed to decode describe-stream-request header"
                                            );
                                        }
                                    }
                                }

                                OperationCode::CreateRange => {
                                    response_frame.operation_code = OperationCode::CreateRange;
                                    if let Some(buf) = frame.header.as_ref() {
                                        if let Ok(req) =
                                            flatbuffers::root::<CreateRangeRequest>(buf)
                                        {
                                            serve_create_range(&req, &mut response_frame);
                                        } else {
                                            error!("Failed to decode create-range-request header");
                                        }
                                    }
                                }

                                OperationCode::SealRange => {
                                    response_frame.operation_code = OperationCode::SealRange;
                                    if let Some(buf) = frame.header.as_ref() {
                                        if let Ok(req) = flatbuffers::root::<SealRangeRequest>(buf)
                                        {
                                            serve_seal_range(&req, &mut response_frame);
                                        } else {
                                            error!("Failed to decode seal-ranges-request header");
                                        }
                                    }
                                }

                                OperationCode::ReportMetrics => {
                                    response_frame.operation_code = OperationCode::ReportMetrics;
                                    if let Some(buf) = &frame.header {
                                        if let Ok(reportmetrics) =
                                            flatbuffers::root::<ReportMetricsRequest>(buf)
                                        {
                                            trace!("Start to sleep...");
                                            tokio::time::sleep(Duration::from_millis(500)).await;
                                            trace!("ReportMetrics sleep completed");
                                            serve_report_metrics(
                                                &reportmetrics,
                                                &mut response_frame,
                                            );
                                        } else {
                                            error!("Failed to decode heartbeat request header");
                                        }
                                    }
                                }

                                OperationCode::Append => {
                                    response_frame.operation_code = OperationCode::Append;
                                    serve_append(frame.payload, &mut response_frame);
                                }

                                _ => {
                                    warn!("Unsupported operation code: {}", frame.operation_code);
                                    unimplemented!("Unimplemented operation code");
                                }
                            }

                            let opcode = response_frame.operation_code;
                            match channel.write_frame(response_frame).await {
                                Ok(_) => {
                                    trace!(
                                        "TestServer writes the `{}` response back directly",
                                        opcode
                                    );
                                }
                                Err(e) => {
                                    error!(
                                        "TestServer failed to process `{}`. Cause: {:?}",
                                        opcode, e
                                    );
                                }
                            };
                        } else {
                            debug!("Connection from {} is closed", addr);
                            break;
                        }
                    } else {
                        warn!(
                            "Connection from {} is reset, dropping some buffered data",
                            addr
                        );
                        break;
                    }
                }
            });
        }
        info!("TestServer shut down OK");
    });
    rx.await.unwrap()
}

fn serve_describe_stream(req: &DescribeStreamRequest, response_frame: &mut Frame) {
    let mut response = DescribeStreamResponseT::default();
    let mut status = StatusT::default();
    status.code = ErrorCode::OK;
    status.message = Some("OK".to_string());
    response.status = Box::new(status);

    let mut stream = StreamT::default();
    stream.stream_id = req.stream_id();
    stream.replica = 1;
    stream.retention_period_ms = time::Duration::from_secs(3600 * 24).as_millis() as i64;

    response.stream = Some(Box::new(stream));

    let mut builder = flatbuffers::FlatBufferBuilder::new();
    let resp = response.pack(&mut builder);
    builder.finish(resp, None);
    let data = builder.finished_data();
    response_frame.flag_response();
    response_frame.header = Some(Bytes::copy_from_slice(data));
}

fn serve_create_stream(req: &CreateStreamRequest, response_frame: &mut Frame) {
    let mut response = CreateStreamResponseT::default();
    let mut status = StatusT::default();
    status.code = ErrorCode::OK;
    status.message = Some("OK".to_string());
    response.status = Box::new(status);

    let mut stream = req.stream().unpack();
    stream.stream_id = 1;

    response.stream = Some(Box::new(stream));

    let mut builder = flatbuffers::FlatBufferBuilder::new();
    let resp = response.pack(&mut builder);
    builder.finish(resp, None);
    let data = builder.finished_data();
    response_frame.flag_response();
    response_frame.header = Some(Bytes::copy_from_slice(data));
}

fn serve_create_range(request: &CreateRangeRequest, response_frame: &mut Frame) {
    let mut response = CreateRangeResponseT::default();
    response.status = Box::<StatusT>::default();
    response.status.as_mut().code = ErrorCode::OK;
    response.range = Some(Box::new(request.range().unpack()));

    let mut builder = flatbuffers::FlatBufferBuilder::new();
    let resp = response.pack(&mut builder);
    builder.finish(resp, None);
    let data = builder.finished_data();
    response_frame.flag_response();
    response_frame.header = Some(Bytes::copy_from_slice(data));
}

fn serve_append(payload: Option<Vec<Bytes>>, response_frame: &mut Frame) {
    let mut response = AppendResponseT::default();
    let mut status_ok = StatusT::default();
    status_ok.code = ErrorCode::OK;
    status_ok.message = Some(String::from("OK"));
    response.status = Box::new(status_ok.clone());

    let mut entries = vec![];

    if let Some(payloads) = payload {
        for payload in payloads {
            if let Ok(append_entries) = Payload::parse_append_entries(&payload) {
                for entry in &append_entries {
                    info!("Append entry: {:?}", entry);
                    let mut append_result_entry = AppendResultEntryT::default();
                    append_result_entry.status = Box::new(status_ok.clone());
                    append_result_entry.timestamp_ms = std::time::SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_millis() as i64;
                    entries.push(append_result_entry);
                }
            }
        }
    }
    response.entries = Some(entries);
    response.throttle_time_ms = 0;

    let mut builder = flatbuffers::FlatBufferBuilder::new();
    let response = response.pack(&mut builder);
    builder.finish(response, None);
    let data = builder.finished_data();
    response_frame.header = Some(Bytes::copy_from_slice(data));
}

fn serve_seal_range(req: &SealRangeRequest, response_frame: &mut Frame) {
    let request = req.unpack();
    info!("SealRangeRequest: {request:?}");

    let mut response = SealRangeResponseT::default();
    let mut status_ok = StatusT::default();
    status_ok.code = ErrorCode::OK;
    status_ok.message = Some(String::from("OK"));
    response.status = Box::new(status_ok);

    match request.kind {
        SealKind::RANGE_SERVER => {}

        SealKind::PLACEMENT_DRIVER => {
            if request.range.end < 0 {
                let mut status_bad_request = StatusT::default();
                status_bad_request.code = ErrorCode::BAD_REQUEST;
                status_bad_request.message = Some(String::from(
                    "end-offset should be non-negative in case of placement driver seal",
                ));
                response.status = Box::new(status_bad_request);
            }
        }

        _ => {
            let mut status_bad_request = StatusT::default();
            status_bad_request.code = ErrorCode::BAD_REQUEST;
            status_bad_request.message = Some(String::from("Unsupported seal type"));
            response.status = Box::new(status_bad_request);
        }
    };

    let mut range = request.range.clone();
    if request.range.end < 0 {
        range.end = request.range.start + 100;
    }

    response.range = Some(range);

    let mut builder = flatbuffers::FlatBufferBuilder::new();
    let resp = response.pack(&mut builder);
    builder.finish(resp, None);
    let data = builder.finished_data();
    response_frame.flag_response();
    response_frame.header = Some(Bytes::copy_from_slice(data));
}

fn allocate_id(request: &IdAllocationRequest, response_frame: &mut Frame) {
    let request = request.unpack();
    info!("Allocate ID for host={:?}", request.host);

    let mut builder = flatbuffers::FlatBufferBuilder::new();
    let mut response = IdAllocationResponseT::default();
    response.id = 1;

    let mut status = StatusT::default();
    status.code = ErrorCode::OK;
    status.message = Some(String::from("OK"));
    status.detail = None;
    response.status = Box::new(status);

    let resp = response.pack(&mut builder);
    builder.finish(resp, None);
    let data = builder.finished_data();

    response_frame.flag_response();
    response_frame.header = Some(Bytes::copy_from_slice(data));
}

pub mod log_util;
