//! Util functions for tests.
//!

use std::time::Duration;

use bytes::Bytes;
use codec::frame::{Frame, OperationCode};
use log::{debug, error, info, trace, warn};
use protocol::rpc::header::{
    CreateStreamResultT, CreateStreamsRequest, CreateStreamsResponseT, ErrorCode, HeartbeatRequest,
    HeartbeatResponseT, ListRangesRequest, ListRangesResponseT, ListRangesResultT, RangeT, StatusT,
    StreamT,
};

use tokio::{net::TcpListener, sync::oneshot};

use crate::{channel_reader::ChannelReader, channel_writer::ChannelWriter};

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

/// Run a dummy listening server.
/// Once it accepts a connection, it quits immediately.
pub async fn run_listener() -> u16 {
    let (tx, rx) = oneshot::channel();
    tokio::spawn(async move {
        // We are using dual-stack mode.
        // Binding to "[::]:0", the any address for IPv6, will also listen for IPv4.
        let listener = TcpListener::bind("[::]:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        debug!("TestServer is up, listening {}", port);
        tx.send(port).unwrap();
        while let Ok((conn, sock_addr)) = listener.accept().await {
            info!("TestServer accepted a connection from {:?}", sock_addr);

            tokio::spawn(async move {
                let addr = sock_addr.to_string();
                let (read_half, write_half) = conn.into_split();
                let mut channel_reader = ChannelReader::new(read_half);
                let mut channel_writer = ChannelWriter::new(write_half);

                loop {
                    if let Ok(frame) = channel_reader.read_frame().await {
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
                                            serve_heartbeat(&heartbeat, &mut response_frame);
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
                                            serve_list_ranges(&req, &mut response_frame);
                                        } else {
                                            error!("Failed to decode list-range-request header");
                                        }
                                    }
                                }

                                OperationCode::CreateStreams => {
                                    response_frame.operation_code = OperationCode::CreateStreams;
                                    if let Some(buf) = frame.header.as_ref() {
                                        if let Ok(req) =
                                            flatbuffers::root::<CreateStreamsRequest>(buf)
                                        {
                                            // Wait 100ms such that we can unit test timeout
                                            tokio::time::sleep(Duration::from_millis(100)).await;
                                            serve_create_streams(&req, &mut response_frame);
                                        } else {
                                            error!(
                                                "Failed to decode create-streams request header"
                                            );
                                        }
                                    }
                                }

                                _ => {
                                    warn!("Unsupported operation code: {}", frame.operation_code);
                                    unimplemented!("Unimplemented operation code");
                                }
                            }

                            match channel_writer.write(&response_frame).await {
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

fn serve_create_streams(req: &CreateStreamsRequest, response_frame: &mut Frame) {
    trace!("CreateStreams {:?}", req);

    let request = req.unpack();

    let mut builder = flatbuffers::FlatBufferBuilder::new();
    let mut response = CreateStreamsResponseT::default();

    let results = request
        .streams
        .into_iter()
        .flat_map(|stream| stream.into_iter())
        .enumerate()
        .map(|(id, mut stream)| {
            stream.stream_id = id as i64;

            let mut result = CreateStreamResultT::default();
            result.stream = Some(Box::new(stream));
            let mut status = StatusT::default();
            status.code = ErrorCode::OK;
            status.message = Some(String::from("OK"));
            result.status = Box::new(status);

            result
        })
        .collect();

    response.create_responses = Some(results);
    let mut status = StatusT::default();
    status.code = ErrorCode::OK;
    status.message = Some("OK".to_owned());
    response.status = Box::new(status);

    let mut stream = StreamT::default();
    stream.stream_id = 1;
    let response = response.pack(&mut builder);
    builder.finish(response, None);
    let data = builder.finished_data();
    response_frame.header = Some(Bytes::copy_from_slice(data));
}
