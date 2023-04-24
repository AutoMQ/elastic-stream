//! Util functions for tests.
//!

use std::time::Duration;

use bytes::Bytes;
use codec::frame::{Frame, OperationCode};
use log::{debug, error, info, trace, warn};
use protocol::rpc::header::{
    DescribePlacementManagerClusterRequest, DescribePlacementManagerClusterResponseT, ErrorCode,
    HeartbeatRequest, HeartbeatResponseT, IdAllocationRequest, IdAllocationResponseT,
    ListRangesRequest, ListRangesResponseT, ListRangesResultT, PlacementManagerClusterT,
    PlacementManagerNodeT, RangeT, StatusT,
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
    response.data_node = request.data_node().map(|dn| Box::new(dn.unpack()));
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

fn serve_list_ranges(request: &ListRangesRequest, frame: &mut Frame) {
    trace!("Received a list-ranges request: {:?}", request);
    let mut builder = flatbuffers::FlatBufferBuilder::with_capacity(1024);
    let mut resp = ListRangesResponseT::default();

    {
        let mut result = ListRangesResultT::default();

        let mut status = StatusT::default();
        status.code = ErrorCode::OK;
        status.message = Some(String::from("OK"));
        result.status = Box::new(status);

        let ranges = (0..10)
            .map(|i| {
                let mut range = RangeT::default();
                range.stream_id = 0;
                range.range_index = i as i32;
                range.start_offset = i * 100;
                range.next_offset = (i + 1) * 100;
                range.end_offset = (i + 1) * 100;
                range
            })
            .collect::<Vec<_>>();

        result.ranges = Some(ranges);
        resp.list_responses = Some(vec![result]);
    }

    let resp = resp.pack(&mut builder);
    builder.finish(resp, None);
    let buf = builder.finished_data();
    frame.header = Some(Bytes::copy_from_slice(buf));
}

fn serve_describe_placement_manager_cluster(
    request: &DescribePlacementManagerClusterRequest,
    frame: &mut Frame,
    port: u16,
) {
    trace!("Received a list-ranges request: {:?}", request);
    let mut builder = flatbuffers::FlatBufferBuilder::with_capacity(1024);
    let mut resp = DescribePlacementManagerClusterResponseT::default();

    {
        let mut cluster = PlacementManagerClusterT::default();

        cluster.nodes = (0..3)
            .into_iter()
            .map(|i| {
                let mut node = PlacementManagerNodeT::default();
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
                                            serve_heartbeat(&&heartbeat, &mut response_frame);
                                        } else {
                                            error!("Failed to decode heartbeat request header");
                                        }
                                    }
                                }
                                OperationCode::ListRanges => {
                                    response_frame.operation_code = OperationCode::ListRanges;
                                    if let Some(buf) = frame.header.as_ref() {
                                        if let Ok(req) = flatbuffers::root::<ListRangesRequest>(buf)
                                        {
                                            serve_list_ranges(&&req, &mut response_frame);
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
                                                allocate_id(&&request, &mut response_frame);
                                            }
                                            Err(_e) => {
                                                error!(
                                                    "Failed to decode id-allocation-request header"
                                                );
                                            }
                                        }
                                    }
                                }

                                OperationCode::DescribePlacementManager => {
                                    response_frame.operation_code =
                                        OperationCode::DescribePlacementManager;
                                    if let Some(ref buf) = frame.header {
                                        match flatbuffers::root::<
                                            DescribePlacementManagerClusterRequest,
                                        >(buf)
                                        {
                                            Ok(request) => {
                                                serve_describe_placement_manager_cluster(
                                                    &request,
                                                    &mut response_frame,
                                                    port,
                                                )
                                            }
                                            Err(e) => {
                                                error!(
                                                    "Failed to decode describe-placement-manager-request header: {:?}", e
                                                );
                                            }
                                        }
                                    }
                                }

                                _ => {
                                    warn!("Unsupported operation code: {}", frame.operation_code);
                                    unimplemented!("Unimplemented operation code");
                                }
                            }

                            match channel.write_frame(&response_frame).await {
                                Ok(_) => {
                                    trace!(
                                        "TestServer writes the `{}` response back directly",
                                        response_frame.operation_code
                                    );
                                }
                                Err(e) => {
                                    error!(
                                        "TestServer failed to process `{}`. Cause: {:?}",
                                        response_frame.operation_code, e
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

fn allocate_id(request: &IdAllocationRequest, response_frame: &mut Frame) {
    let request = request.unpack();
    if let Some(ref host) = request.host {
        info!("Allocate ID for host={:?}", host);
    } else {
        warn!("Host for which to allocate ID is unknown");
    }

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

pub mod fs;
pub mod log_util;
pub mod store;
