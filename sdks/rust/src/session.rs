use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

use bytes::Bytes;
use codec::frame::{Frame, OperationCode};
use model::stream::Stream;
use protocol::rpc::header::{
    CreateStreamsRequestT, CreateStreamsResponse, ErrorCode, PlacementManagerCluster, StreamT,
    SystemErrorResponse,
};
use slog::{error, info, trace, warn, Logger};
use tokio::{net::TcpStream, sync::oneshot};

use crate::{
    channel_reader::ChannelReader,
    channel_writer::ChannelWriter,
    client_error::ClientError,
    node::{Node, Role},
};

lazy_static::lazy_static! {
    static ref STREAM_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
}

#[derive(Debug)]
pub(crate) struct Session {
    log: Logger,
    inflight: Arc<Mutex<HashMap<u32, oneshot::Sender<Frame>>>>,
    channel_writer: ChannelWriter,
    peer_address: String,
}

impl Session {
    fn spawn_read_loop(
        mut reader: ChannelReader,
        inflight: Arc<Mutex<HashMap<u32, oneshot::Sender<Frame>>>>,
        log: Logger,
    ) {
        tokio::spawn(async move {
            info!(log, "Session read loop started");
            loop {
                match reader.read_frame().await {
                    Ok(Some(frame)) => {
                        let stream_id = frame.stream_id;
                        {
                            if let Ok(mut guard) = inflight.lock() {
                                if let Some(tx) = guard.remove(&stream_id) {
                                    tx.send(frame).unwrap_or_else(|_e| {
                                        warn!(log, "Failed to notify response frame")
                                    });
                                }
                            }
                        }
                    }
                    Ok(None) => {
                        info!(log, "Connection closed");
                        break;
                    }
                    Err(e) => {
                        error!(log, "Connection reset: {}", e);
                        break;
                    }
                }
            }
            info!(log, "Session read loop completed");
        });
    }

    pub(crate) async fn new(target: &str, log: Logger) -> Result<Self, ClientError> {
        let stream = TcpStream::connect(target).await?;
        stream.set_nodelay(true)?;
        let local_addr = stream.local_addr()?;
        trace!(log, "Port of client: {}", local_addr.port());
        let (read_half, write_half) = stream.into_split();

        let reader = ChannelReader::new(read_half, log.clone());
        let inflight = Arc::new(Mutex::new(HashMap::new()));

        Self::spawn_read_loop(reader, Arc::clone(&inflight), log.clone());

        let writer = ChannelWriter::new(write_half, log.clone());

        Ok(Self {
            log,
            inflight: Arc::clone(&inflight),
            channel_writer: writer,
            peer_address: target.to_owned(),
        })
    }

    /// Create a new stream in placement manager.
    ///
    /// # Arguments
    /// * `replica` - Number of replica required before acknowledgement
    /// * `retention_period` - Data retention period of the created stream.
    ///
    pub(crate) async fn create_stream(
        &mut self,
        replica: i8,
        retention_period: Duration,
        timeout: Duration,
    ) -> Result<Vec<Stream>, ClientError> {
        let stream_id = STREAM_ID_COUNTER.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        {
            if let Ok(mut guard) = self.inflight.lock() {
                guard.insert(stream_id, tx);
            }
        }

        let mut frame = Frame::new(OperationCode::CreateStreams);
        frame.stream_id = stream_id;

        let mut create_stream_request = CreateStreamsRequestT::default();
        let mut stream = StreamT::default();
        stream.replica_nums = replica;
        stream.retention_period_ms = retention_period.as_millis() as i64;
        create_stream_request.streams = Some(vec![stream]);

        let mut builder = flatbuffers::FlatBufferBuilder::new();
        let req = create_stream_request.pack(&mut builder);
        builder.finish(req, None);
        let data = builder.finished_data();
        let buf = Bytes::copy_from_slice(data);
        frame.header = Some(buf);

        self.channel_writer.write(&frame).await?;

        let frame = match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(frame)) => frame,
            Ok(Err(e)) => return Err(e.into()),
            Err(elapsed) => return Err(elapsed.into()),
        };

        // Check if RPC is OK
        if frame.system_error() {
            if let Some(ref buf) = frame.header {
                let system_error = flatbuffers::root::<SystemErrorResponse>(buf)?;
                let system_error = system_error.unpack();
                if let Some(status) = system_error.status.map(|st| model::Status {
                    code: st.code,
                    message: st.message.unwrap_or_default(),
                    details: st.detail.map(|details| Bytes::copy_from_slice(&details)),
                }) {
                    return Err(ClientError::Internal(status));
                }
            } else {
                return Err(ClientError::UnexpectedResponse(String::from(
                    "Mal-formed system error frame",
                )));
            }
        }

        // Parse create streams response
        if let Some(ref buf) = frame.header {
            let response = flatbuffers::root::<CreateStreamsResponse>(buf)?;
            trace!(self.log, "CreateStreamsResponse: {:?}", response);
            let response = response.unpack();
            if let Some(status) = response.status {
                match status.code {
                    ErrorCode::OK => {
                        trace!(self.log, "Stream created");
                    }

                    ErrorCode::PM_NOT_LEADER => {
                        warn!(self.log, "Failed to create stream: targeting placement manager node is not leader");
                        if let Some(detail) = status.detail {
                            let placement_manager_cluster =
                                flatbuffers::root::<PlacementManagerCluster>(&detail)?;
                            let placement_manager_cluster = placement_manager_cluster.unpack();
                            let nodes = placement_manager_cluster
                                .nodes
                                .iter()
                                .flat_map(|nodes| nodes.iter())
                                .map(|node| Node {
                                    name: node.name.clone().unwrap_or_default(),
                                    advertise_address: node
                                        .advertise_addr
                                        .clone()
                                        .unwrap_or_default(),
                                    role: if node.is_leader {
                                        Role::Leader
                                    } else {
                                        Role::Follower
                                    },
                                })
                                .collect::<Vec<_>>();
                            return Err(ClientError::LeadershipChanged { nodes });
                        }
                    }

                    ErrorCode::PM_NO_AVAILABLE_DN => {
                        error!(self.log, "Placement manager has no enough data nodes");
                        return Err(ClientError::DataNodeNotAvailable);
                    }

                    _ => {
                        error!(
                            self.log,
                            "Unexpected error code from server: {}", self.peer_address
                        );
                        return Err(ClientError::UnexpectedResponse(format!(
                            "Unexpected response status: {:?}",
                            status
                        )));
                    }
                }
            }

            if let Some(results) = response.create_responses {
                trace!(self.log, "CreateStreamsResults: {:?}", results);
                let streams = results
                    .into_iter()
                    .filter(|result| {
                        if let Some(ref status) = result.status {
                            return status.code == ErrorCode::OK && result.stream.is_some();
                        }
                        false
                    })
                    .map(|result| {
                        let created = result.stream.expect("Stream should be present");
                        Stream::with_id(created.stream_id)
                    })
                    .collect::<Vec<_>>();
                return Ok(streams);
            }
        }
        Err(ClientError::UnexpectedResponse("Bad response".to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use std::{error::Error, sync::atomic::Ordering, time::Duration};

    use slog::{error, info};

    use crate::{client_error::ClientError, session::Session, test_server::run_listener};

    use super::STREAM_ID_COUNTER;

    #[test]
    fn test_stream_id_increasing() {
        let mut prev = STREAM_ID_COUNTER.fetch_add(1, Ordering::Relaxed);
        for _ in 0..10 {
            let stream_id = STREAM_ID_COUNTER.fetch_add(1, Ordering::Relaxed);
            assert!(stream_id > prev, "Stream ID should be increasing");
            prev = stream_id;
        }
    }

    #[tokio::test]
    async fn test_create_stream() -> Result<(), Box<dyn Error>> {
        let log = test_util::terminal_logger();
        let port = 2378;
        let port = run_listener(log.clone()).await;
        let target = format!("127.0.0.1:{}", port);
        info!(log, "Connecting {}", target);
        match Session::new(&target, log.clone()).await {
            Ok(mut session) => {
                info!(log, "Session connected");
                let streams = session
                    .create_stream(
                        1,
                        Duration::from_secs(60 * 60 * 24 * 3),
                        Duration::from_secs(3),
                    )
                    .await
                    .unwrap();
                assert_eq!(1, streams.len());
            }
            Err(e) => {
                error!(log, "Failed to create session: {:?}", e);
            }
        }
        Ok(())
    }

    #[tokio::test]
    async fn test_create_stream_timeout() -> Result<(), Box<dyn Error>> {
        let log = test_util::terminal_logger();
        let port = run_listener(log.clone()).await;
        let target = format!("127.0.0.1:{}", port);
        info!(log, "Connecting {}", target);
        match Session::new(&target, log.clone()).await {
            Ok(mut session) => {
                info!(log, "Session connected");
                let res = session
                    .create_stream(
                        1,
                        Duration::from_secs(60 * 60 * 24 * 3),
                        Duration::from_millis(10),
                    )
                    .await;
                if let Err(ClientError::Timeout(ref _elapsed)) = res {
                } else {
                    panic!("Should get a timeout error");
                }
            }
            Err(e) => {
                error!(log, "Failed to create session: {:?}", e);
            }
        }
        Ok(())
    }
}
