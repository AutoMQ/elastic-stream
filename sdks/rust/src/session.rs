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
use protocol::rpc::header::{CreateStreamsRequestT, CreateStreamsResponse, ErrorCode, StreamT};
use slog::{error, info, trace, warn, Logger};
use tokio::{net::TcpStream, sync::oneshot};

use crate::{
    channel_reader::ChannelReader, channel_writer::ChannelWriter, client_error::ClientError,
};

const STREAM_ID_COUNTER: AtomicU32 = AtomicU32::new(0);

#[derive(Debug)]
pub(crate) struct Session {
    log: Logger,
    inflight: Arc<Mutex<HashMap<u32, oneshot::Sender<Frame>>>>,
    channel_writer: ChannelWriter,
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
        })
    }

    pub(crate) async fn create_stream(
        &mut self,
        replica: i8,
        retention_period: Duration,
    ) -> Result<(), ClientError> {
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

        let frame = rx.await?;

        if let Some(buf) = frame.header {
            let response = flatbuffers::root::<CreateStreamsResponse>(&buf)?;
            let response = response.unpack();
            if let Some(status) = response.status {
                match status.code {
                    ErrorCode::NONE => {}
                    ErrorCode::PM_NOT_LEADER => {
                        if let Some(detail) = status.detail {
                            dbg!(detail);
                        }
                        // Wrap redirect info...
                    }
                    _ => {
                        // Return error
                    }
                }
            }

            if let Some(results) = response.create_responses {
                dbg!(results);
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::{error::Error, time::Duration};

    use slog::{error, info};

    use crate::{session::Session, test_server::run_listener};

    #[tokio::test]
    async fn test_session() -> Result<(), Box<dyn Error>> {
        let log = test_util::terminal_logger();
        let port = run_listener(log.clone()).await;
        let target = format!("127.0.0.1:{}", port);
        info!(log, "Connecting {}", target);
        match Session::new(&target, log.clone()).await {
            Ok(mut session) => {
                info!(log, "Session connected");
                session
                    .create_stream(2, Duration::from_secs(60 * 60 * 24 * 3))
                    .await
                    .unwrap();
            }
            Err(e) => {
                error!(log, "Failed to create session: {:?}", e);
            }
        }
        Ok(())
    }
}
