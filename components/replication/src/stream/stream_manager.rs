use std::{
    cell::RefCell,
    collections::HashMap,
    rc::{Rc, Weak},
    sync::Arc,
};

use client::Client;
use config::Configuration;
use log::{error, warn};
use model::{client_role::ClientRole, stream::StreamMetadata};
use tokio::sync::{broadcast, oneshot};

use crate::{
    request::{
        AppendRequest, AppendResponse, CloseStreamRequest, CreateStreamRequest,
        CreateStreamResponse, OpenStreamRequest, OpenStreamResponse, ReadRequest, ReadResponse,
        TrimRequest,
    },
    stream::replication_stream::ReplicationStream,
    ReplicationError,
};

use super::{
    cache::HotCache,
    cache_stream::CacheStream,
    object_reader::{AsyncObjectReader, DefaultObjectReader},
    object_stream::ObjectStream,
    FetchDataset, Stream,
};

// final stream type
type FStream = CacheStream<ObjectStream<ReplicationStream, DefaultObjectReader>>;

/// `StreamManager` is intended to be used in thread-per-core usage case. It is NOT `Send`.
pub(crate) struct StreamManager {
    clients: Vec<Rc<Client>>,
    round_robin: usize,
    streams: Rc<RefCell<HashMap<u64, Rc<FStream>>>>,
    cache: Rc<HotCache>,
    object_reader: Rc<AsyncObjectReader>,
}

impl StreamManager {
    pub(crate) fn new(config: Arc<Configuration>) -> Self {
        let (shutdown, _rx) = broadcast::channel(1);
        let streams = Rc::new(RefCell::new(HashMap::new()));
        let cache = Rc::new(HotCache::new(Self::get_max_cache_size()));

        let mut clients = vec![];
        for _ in 0..config.replication.connection_pool_size {
            let client = Rc::new(Client::new(Arc::clone(&config), shutdown.clone()));
            Self::schedule_heartbeat(&client, config.client_heartbeat_interval());
            clients.push(client);
        }

        let object_reader = Rc::new(AsyncObjectReader::new());

        Self {
            clients,
            round_robin: 0,
            streams,
            cache,
            object_reader,
        }
    }

    fn route_client(&mut self) -> Result<Rc<Client>, ReplicationError> {
        debug_assert!(!self.clients.is_empty(), "Clients should NOT be empty");
        let index = self.round_robin % self.clients.len();
        self.round_robin += 1;
        self.clients
            .get(index)
            .cloned()
            .ok_or(ReplicationError::Internal)
    }

    fn schedule_heartbeat(client: &Rc<Client>, interval: std::time::Duration) {
        // Spawn a task to broadcast heartbeat to servers.
        //
        // TODO: watch ctrl-c signal to shutdown timely.
        let client = Rc::clone(client);
        tokio_uring::spawn(async move {
            let mut interval = tokio::time::interval(interval);
            loop {
                interval.tick().await;
                client.broadcast_heartbeat(ClientRole::Frontend).await;
            }
        });
    }

    pub fn append(
        &mut self,
        request: AppendRequest,
        tx: oneshot::Sender<Result<AppendResponse, ReplicationError>>,
    ) {
        let stream = self.streams.borrow().get(&request.stream_id).map(Rc::clone);
        if let Some(stream) = stream {
            tokio_uring::spawn(async move {
                let result = stream
                    .append(request.record_batch)
                    .await
                    .map(|offset| AppendResponse { offset });
                let _ = tx.send(result);
            });
        } else {
            let _ = tx.send(Err(ReplicationError::StreamNotExist));
        }
    }

    pub fn fetch(
        &mut self,
        request: ReadRequest,
        tx: oneshot::Sender<Result<ReadResponse, ReplicationError>>,
    ) {
        let stream = self.streams.borrow().get(&request.stream_id).map(Rc::clone);
        if let Some(stream) = stream {
            tokio_uring::spawn(async move {
                let result = stream
                    .fetch(
                        request.start_offset,
                        request.end_offset,
                        request.batch_max_bytes,
                    )
                    .await;
                let dataset = match result {
                    Ok(dataset) => dataset,
                    Err(e) => {
                        let _ = tx.send(Err(e));
                        return;
                    }
                };
                if let FetchDataset::Full(blocks) = dataset {
                    let mut data = vec![];
                    for block in blocks {
                        for record in block.records {
                            data.extend_from_slice(&record.data);
                        }
                    }
                    let _ = tx.send(Ok(ReadResponse { data }));
                } else {
                    error!("Fetch dataset should be full");
                    let _ = tx.send(Err(ReplicationError::Internal));
                }
            });
        } else {
            let _ = tx.send(Err(ReplicationError::StreamNotExist));
        }
    }

    pub fn create(
        &mut self,
        request: CreateStreamRequest,
        tx: oneshot::Sender<Result<CreateStreamResponse, ReplicationError>>,
    ) {
        let metadata = StreamMetadata {
            stream_id: None,
            replica: request.replica,
            ack_count: request.ack_count,
            retention_period: request.retention_period,
        };

        let client = match self.route_client() {
            Ok(client) => client,
            Err(e) => {
                let _ = tx.send(Err(e));
                return;
            }
        };
        tokio_uring::spawn(async move {
            let result = client
                .create_stream(metadata)
                .await
                .map(|metadata| CreateStreamResponse {
                    // TODO: unify stream_id type
                    stream_id: metadata.stream_id.expect("stream id cannot be none"),
                })
                .map_err(|e| {
                    warn!("Failed to create stream, {}", e);
                    ReplicationError::Internal
                });
            let _ = tx.send(result);
        });
    }

    pub fn open(
        &mut self,
        request: OpenStreamRequest,
        tx: oneshot::Sender<Result<OpenStreamResponse, ReplicationError>>,
    ) {
        let client = match self.route_client() {
            Ok(client) => client,
            Err(e) => {
                let _ = tx.send(Err(e));
                return;
            }
        };
        let streams = self.streams.clone();
        let cache = self.cache.clone();
        let object_reader = self.object_reader.clone();
        tokio_uring::spawn(async move {
            let client = Rc::downgrade(&client);
            let stream = Self::new_stream(
                request.stream_id,
                request.epoch,
                client,
                cache,
                object_reader,
            );
            if let Err(e) = stream.open().await {
                let _ = tx.send(Err(e));
                return;
            }
            streams.borrow_mut().insert(request.stream_id, stream);
            let _ = tx.send(Ok(OpenStreamResponse {}));
        });
    }

    pub fn close(
        &mut self,
        request: CloseStreamRequest,
        tx: oneshot::Sender<Result<(), ReplicationError>>,
    ) {
        let stream = self
            .streams
            .borrow_mut()
            .remove(&request.stream_id)
            .map(|stream| Rc::clone(&stream));
        if let Some(stream) = stream {
            tokio_uring::spawn(async move {
                stream.close().await;
                let _ = tx.send(Ok(()));
            });
        } else {
            let _ = tx.send(Err(ReplicationError::StreamNotExist));
        }
    }

    pub fn start_offset(
        &mut self,
        stream_id: u64,
        tx: oneshot::Sender<Result<u64, ReplicationError>>,
    ) {
        let result = if let Some(stream) = self.streams.borrow().get(&stream_id) {
            Ok(stream.start_offset())
        } else {
            Err(ReplicationError::StreamNotExist)
        };
        let _ = tx.send(result);
    }

    pub fn next_offset(
        &mut self,
        stream_id: u64,
        tx: oneshot::Sender<Result<u64, ReplicationError>>,
    ) {
        let result = if let Some(stream) = self.streams.borrow().get(&stream_id) {
            Ok(stream.next_offset())
        } else {
            Err(ReplicationError::StreamNotExist)
        };
        let _ = tx.send(result);
    }

    pub fn trim(
        &mut self,
        request: TrimRequest,
        tx: oneshot::Sender<Result<(), ReplicationError>>,
    ) {
        let stream = self
            .streams
            .borrow_mut()
            .remove(&request.stream_id)
            .map(|stream| Rc::clone(&stream));
        if let Some(stream) = stream {
            tokio_uring::spawn(async move {
                let _ = tx.send(stream.trim(request.new_start_offset).await);
            });
        } else {
            let _ = tx.send(Err(ReplicationError::StreamNotExist));
        }
    }

    fn new_stream(
        stream_id: u64,
        epoch: u64,
        client: Weak<Client>,
        cache: Rc<HotCache>,
        object_reader: Rc<AsyncObjectReader>,
    ) -> Rc<FStream> {
        let stream = ReplicationStream::new(stream_id as i64, epoch, client, cache.clone());

        let object_reader = DefaultObjectReader::new(object_reader);
        let stream = ObjectStream::new(stream, object_reader);

        CacheStream::new(stream_id, stream, cache)
    }

    fn get_max_cache_size() -> u64 {
        // get max cache size (MB) from system env
        std::env::var("ES_MAX_CACHE_SIZE")
            .unwrap_or_else(|_| "2048".to_string())
            .parse::<u64>()
            .unwrap_or_else(|_| {
                warn!("Failed to parse MAX_CACHE_SIZE, use default value 2048");
                2048
            })
            * 1024
            * 1024
    }
}
