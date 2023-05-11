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
use log::{error, info, trace, warn};
use model::stream::Stream;
use protocol::rpc::header::{
    CreateStreamRequestT, CreateStreamResponse, ErrorCode, PlacementManagerCluster, StreamT,
    SystemError,
};
use tokio::{net::TcpStream, sync::oneshot};

use crate::{
    channel_reader::ChannelReader, channel_writer::ChannelWriter, client_error::ClientError,
};

lazy_static::lazy_static! {
    static ref STREAM_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
}

#[derive(Debug)]
pub(crate) struct Session {
    inflight: Arc<Mutex<HashMap<u32, oneshot::Sender<Frame>>>>,
    channel_writer: ChannelWriter,
    peer_address: String,
}

impl Session {
    fn spawn_read_loop(
        mut reader: ChannelReader,
        inflight: Arc<Mutex<HashMap<u32, oneshot::Sender<Frame>>>>,
    ) {
        tokio::spawn(async move {
            info!("Session read loop started");
            loop {
                match reader.read_frame().await {
                    Ok(Some(frame)) => {
                        let stream_id = frame.stream_id;
                        {
                            if let Ok(mut guard) = inflight.lock() {
                                if let Some(tx) = guard.remove(&stream_id) {
                                    tx.send(frame).unwrap_or_else(|_e| {
                                        warn!("Failed to notify response frame")
                                    });
                                }
                            }
                        }
                    }
                    Ok(None) => {
                        info!("Connection closed");
                        break;
                    }
                    Err(e) => {
                        error!("Connection reset: {}", e);
                        break;
                    }
                }
            }
            info!("Session read loop completed");
        });
    }

    pub(crate) async fn new(target: &str) -> Result<Self, ClientError> {
        let stream = TcpStream::connect(target).await?;
        stream.set_nodelay(true)?;
        let local_addr = stream.local_addr()?;
        trace!("Port of client: {}", local_addr.port());
        let (read_half, write_half) = stream.into_split();

        let reader = ChannelReader::new(read_half);
        let inflight = Arc::new(Mutex::new(HashMap::new()));

        Self::spawn_read_loop(reader, Arc::clone(&inflight));

        let writer = ChannelWriter::new(write_half);

        Ok(Self {
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
    ) -> Result<Stream, ClientError> {
        let stream_id = STREAM_ID_COUNTER.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        {
            if let Ok(mut guard) = self.inflight.lock() {
                guard.insert(stream_id, tx);
            }
        }

        let mut frame = Frame::new(OperationCode::CreateStream);
        frame.stream_id = stream_id;

        let mut create_stream_request = CreateStreamRequestT::default();
        let mut stream = StreamT::default();
        stream.replica = replica;
        stream.retention_period_ms = retention_period.as_millis() as i64;
        create_stream_request.stream = Box::new(stream);

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
                let response = flatbuffers::root::<SystemError>(buf)?;
                let response = response.unpack();
                return Err(ClientError::Internal(response.status.as_ref().into()));
            } else {
                return Err(ClientError::UnexpectedResponse(String::from(
                    "Mal-formed system error frame",
                )));
            }
        }

        // Parse create streams response
        if let Some(ref buf) = frame.header {
            let response = flatbuffers::root::<CreateStreamResponse>(buf)?;
            trace!("CreateStreamsResponse: {:?}", response);
            let response = response.unpack();
            let status = response.status;
            match status.code {
                ErrorCode::OK => {
                    trace!("Stream created");
                }

                ErrorCode::PM_NOT_LEADER => {
                    warn!(
                        "Failed to create stream: targeting placement manager node is not leader"
                    );
                    if let Some(detail) = status.detail {
                        let placement_manager_cluster =
                            flatbuffers::root::<PlacementManagerCluster>(&detail)?;
                        let placement_manager_cluster = placement_manager_cluster.unpack();
                        let nodes = placement_manager_cluster
                            .nodes
                            .iter()
                            .map(Into::into)
                            .collect::<Vec<_>>();
                        return Err(ClientError::LeadershipChanged { nodes });
                    }
                }

                ErrorCode::PM_NO_AVAILABLE_DN => {
                    error!("Placement manager has no enough data nodes");
                    return Err(ClientError::DataNodeNotAvailable);
                }

                _ => {
                    error!("Unexpected error code from server: {}", self.peer_address);
                    return Err(ClientError::UnexpectedResponse(format!(
                        "Unexpected response status: {:?}",
                        status
                    )));
                }
            }

            if let Some(stream) = response.stream {
                trace!("Created stream: {:?}", stream);
                return Ok(Into::<Stream>::into(*stream));
            }
        }
        Err(ClientError::UnexpectedResponse("Bad response".to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use std::{error::Error, sync::atomic::Ordering, time::Duration};

    use log::{error, info};

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
        let port = 2378;
        let port = run_listener().await;
        let target = format!("127.0.0.1:{}", port);
        info!("Connecting {}", target);
        match Session::new(&target).await {
            Ok(mut session) => {
                info!("Session connected");
                let stream = session
                    .create_stream(
                        1,
                        Duration::from_secs(60 * 60 * 24 * 3),
                        Duration::from_secs(3),
                    )
                    .await
                    .unwrap();
                assert_eq!(1, stream.replica);
                assert_eq!(60 * 60 * 24 * 3, stream.retention_period.as_secs());
            }
            Err(e) => {
                error!("Failed to create session: {:?}", e);
            }
        }
        Ok(())
    }

    #[tokio::test]
    async fn test_create_stream_timeout() -> Result<(), Box<dyn Error>> {
        let port = run_listener().await;
        let target = format!("127.0.0.1:{}", port);
        info!("Connecting {}", target);
        match Session::new(&target).await {
            Ok(mut session) => {
                info!("Session connected");
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
                error!("Failed to create session: {:?}", e);
            }
        }
        Ok(())
    }
}
