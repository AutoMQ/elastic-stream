use super::session_manager::SessionManager;

use crate::{composite_session::CompositeSession, error::ClientError};

use bytes::Bytes;
use log::{error, trace, warn};
use model::{
    fetch::FetchRequestEntry, fetch::FetchResultEntry, range::RangeMetadata,
    stream::StreamMetadata, AppendResultEntry, ListRangeCriteria,
};
use observation::metrics::{
    store_metrics::DataNodeStatistics,
    sys_metrics::{DiskStatistics, MemoryStatistics},
    uring_metrics::UringStatistics,
};
use protocol::rpc::header::SealKind;
use std::{cell::UnsafeCell, rc::Rc, sync::Arc};
use tokio::{sync::broadcast, time};

/// `Client` is used to send
pub struct Client {
    pub(crate) session_manager: Rc<UnsafeCell<SessionManager>>,
    pub(crate) config: Arc<config::Configuration>,
}

impl Client {
    pub fn new(config: Arc<config::Configuration>, shutdown: broadcast::Sender<()>) -> Self {
        let session_manager = Rc::new(UnsafeCell::new(SessionManager::new(&config, shutdown)));

        Self {
            session_manager,
            config,
        }
    }

    pub async fn allocate_id(&self, host: &str) -> Result<i32, ClientError> {
        let session_manager = unsafe { &mut *self.session_manager.get() };
        let composite_session = session_manager
            .get_composite_session(&self.config.placement_manager)
            .await?;
        let future = composite_session.allocate_id(host, None);
        time::timeout(self.config.client_io_timeout(), future)
            .await
            .map_err(|e| {
                warn!("Timeout when allocate ID. {}", e);
                ClientError::RpcTimeout {
                    timeout: self.config.client_io_timeout(),
                }
            })?
            .map_err(|e| {
                error!(
                    "Failed to receive response from composite session. Cause: {:?}",
                    e
                );
                ClientError::ClientInternal
            })
    }

    pub async fn list_ranges(
        &self,
        criteria: ListRangeCriteria,
    ) -> Result<Vec<RangeMetadata>, ClientError> {
        let session_manager = unsafe { &mut *self.session_manager.get() };
        let session = session_manager
            .get_composite_session(&self.config.placement_manager)
            .await?;
        let future = session.list_range(criteria);
        time::timeout(self.config.client_io_timeout(), future)
            .await
            .map_err(|elapsed| {
                warn!("Timeout when list range. {}", elapsed);
                ClientError::RpcTimeout {
                    timeout: self.config.client_io_timeout(),
                }
            })?
            .map_err(|e| {
                error!(
                    "Failed to receive response from broken channel. Cause: {:?}",
                    e
                );
                ClientError::ClientInternal
            })
    }

    /// Broadcast heartbeats to all sessions in the `CompositeSession`.
    ///
    /// # Arguments
    /// `target` - Placement manager access URL.
    ///
    /// # Returns
    ///
    pub async fn broadcast_heartbeat(&self, target: &str) -> Result<(), ClientError> {
        let session_manager = unsafe { &mut *self.session_manager.get() };
        let composite_session = session_manager.get_composite_session(target).await?;
        trace!("Broadcast heartbeat to composite-channel={}", target);

        composite_session.heartbeat().await
    }

    pub async fn create_stream(
        &self,
        stream_metadata: StreamMetadata,
    ) -> Result<StreamMetadata, ClientError> {
        let session_manager = unsafe { &mut *self.session_manager.get() };
        let composite_session = session_manager
            .get_composite_session(&self.config.placement_manager)
            .await?;
        let future = composite_session.create_stream(stream_metadata);
        time::timeout(self.config.client_io_timeout(), future)
            .await
            .map_err(|e| {
                error!("Timeout when create stream. {}", e);
                ClientError::RpcTimeout {
                    timeout: self.config.client_io_timeout(),
                }
            })?
    }

    pub async fn describe_stream(&self, stream_id: u64) -> Result<StreamMetadata, ClientError> {
        let session_manager = unsafe { &mut *self.session_manager.get() };
        let composite_session = session_manager
            .get_composite_session(&self.config.placement_manager)
            .await?;
        let future = composite_session.describe_stream(stream_id);
        time::timeout(self.config.client_io_timeout(), future)
            .await
            .map_err(|e| {
                error!("Timeout when describe stream[stream-id={stream_id}]. {}", e);
                ClientError::RpcTimeout {
                    timeout: self.config.client_io_timeout(),
                }
            })?
    }

    /// Create a new range by send request to placement manager.
    pub async fn create_range(
        &self,
        range_metadata: RangeMetadata,
    ) -> Result<RangeMetadata, ClientError> {
        let session_manager = unsafe { &mut *self.session_manager.get() };
        let composite_session = session_manager
            .get_composite_session(&self.config.placement_manager)
            .await?;
        self.create_range0(composite_session, range_metadata).await
    }

    /// Create a new range replica by send request to data node.
    pub async fn create_range_replica(
        &self,
        target: &str,
        range_metadata: RangeMetadata,
    ) -> Result<(), ClientError> {
        let session_manager = unsafe { &mut *self.session_manager.get() };
        let composite_session = session_manager.get_composite_session(target).await?;
        trace!("Create range replica to composite-channel={}", target);
        self.create_range0(composite_session, range_metadata)
            .await
            .map(|_| ())
    }

    async fn create_range0(
        &self,
        composite_session: Rc<CompositeSession>,
        range: RangeMetadata,
    ) -> Result<RangeMetadata, ClientError> {
        let future = composite_session.create_range(range);
        time::timeout(self.config.client_io_timeout(), future)
            .await
            .map_err(|e| {
                error!("Timeout when create range. {}", e);
                ClientError::RpcTimeout {
                    timeout: self.config.client_io_timeout(),
                }
            })?
    }

    pub async fn seal(
        &self,
        target: Option<&str>,
        kind: SealKind,
        range: RangeMetadata,
    ) -> Result<RangeMetadata, ClientError> {
        // Validate request
        match kind {
            SealKind::DATA_NODE => {
                if target.is_none() {
                    error!("Target is required while seal range against data nodes");
                    return Err(ClientError::BadRequest);
                }
            }
            SealKind::PLACEMENT_MANAGER => {
                if range.end().is_none() {
                    error!(
                        "SealRange.range.end MUST be present while seal against placement manager"
                    );
                    return Err(ClientError::BadRequest);
                }
            }
            _ => {
                error!("Seal request kind must specify");
                return Err(ClientError::BadRequest);
            }
        }

        let session_manager = unsafe { &mut *self.session_manager.get() };
        let composite_session = match target {
            None => {
                session_manager
                    .get_composite_session(&self.config.placement_manager)
                    .await?
            }

            Some(addr) => session_manager.get_composite_session(addr).await?,
        };
        composite_session.seal(kind, range).await
    }

    /// Append data to a range.
    pub async fn append(
        &self,
        target: &str,
        buf: Vec<Bytes>,
    ) -> Result<Vec<AppendResultEntry>, ClientError> {
        let session_manager = unsafe { &mut *self.session_manager.get() };
        let session = session_manager.get_composite_session(target).await?;
        let future = session.append(buf);
        time::timeout(self.config.client_io_timeout(), future)
            .await
            .map_err(|_e| ClientError::RpcTimeout {
                timeout: self.config.client_io_timeout(),
            })?
    }

    /// Fetch data from a range replica.
    pub async fn fetch(
        &self,
        target: &str,
        request: FetchRequestEntry,
    ) -> Result<FetchResultEntry, ClientError> {
        let session_manager = unsafe { &mut *self.session_manager.get() };
        let session = session_manager.get_composite_session(target).await?;
        let future = session.fetch(request);
        time::timeout(self.config.client_io_timeout(), future)
            .await
            .map_err(|_e| ClientError::RpcTimeout {
                timeout: self.config.client_io_timeout(),
            })?
    }

    /// Report metrics to placement manager
    ///
    /// # Arguments
    /// `target` - Placement manager access URL.
    ///
    /// # Returns
    ///
    pub async fn report_metrics(
        &self,
        target: &str,
        uring_statistics: &UringStatistics,
        data_node_statistics: &DataNodeStatistics,
        disk_statistics: &DiskStatistics,
        memory_statistics: &MemoryStatistics,
    ) -> Result<(), ClientError> {
        let session_manager = unsafe { &mut *self.session_manager.get() };
        let composite_session = session_manager.get_composite_session(target).await?;
        trace!("Report metrics to composite-channel={}", target);

        composite_session
            .report_metrics(
                uring_statistics,
                data_node_statistics,
                disk_statistics,
                memory_statistics,
            )
            .await
    }
}

#[cfg(test)]
mod tests {
    use bytes::BytesMut;
    use log::trace;
    use model::stream::StreamMetadata;
    use model::ListRangeCriteria;
    use model::{
        range::RangeMetadata,
        record::{flat_record::FlatRecordBatch, RecordBatchBuilder},
    };
    use observation::metrics::{
        store_metrics::DataNodeStatistics,
        sys_metrics::{DiskStatistics, MemoryStatistics},
        uring_metrics::UringStatistics,
    };
    use protocol::rpc::header::SealKind;
    use std::{error::Error, sync::Arc};
    use test_util::run_listener;
    use tokio::sync::broadcast;

    use crate::{
        error::{ClientError, ListRangeError},
        Client,
    };

    #[test]
    fn test_broadcast_heartbeat() -> Result<(), ClientError> {
        tokio_uring::start(async {
            #[allow(unused_variables)]
            let port = 2378;
            let port = run_listener().await;
            let mut config = config::Configuration::default();
            config.placement_manager = format!("localhost:{}", port);
            config.server.host = "localhost".to_owned();
            config.server.port = 10911;
            config.check_and_apply().unwrap();
            let config = Arc::new(config);
            let (tx, _rx) = broadcast::channel(1);
            let client = Client::new(Arc::clone(&config), tx);
            client.broadcast_heartbeat(&config.placement_manager).await
        })
    }

    #[test]
    fn test_allocate_id() -> Result<(), Box<dyn Error>> {
        test_util::try_init_log();
        tokio_uring::start(async {
            #[allow(unused_variables)]
            let port = 2378;
            let port = run_listener().await;
            let mut config = config::Configuration::default();
            config.server.host = "localhost".to_owned();
            config.server.port = 10911;
            config.placement_manager = format!("localhost:{}", port);
            let config = Arc::new(config);
            let (tx, _rx) = broadcast::channel(1);
            let client = Client::new(config, tx);
            let id = client.allocate_id("localhost").await?;
            assert_eq!(1, id);
            Ok(())
        })
    }

    #[test]
    fn test_create_stream() -> Result<(), Box<dyn Error>> {
        test_util::try_init_log();
        tokio_uring::start(async move {
            #[allow(unused_variables)]
            let port = 12378;
            let port = run_listener().await;
            let mut config = config::Configuration::default();
            config.server.host = "localhost".to_owned();
            config.server.port = 10911;
            config.placement_manager = format!("localhost:{}", port);
            let config = Arc::new(config);
            let (tx, _rx) = broadcast::channel(1);
            let client = Client::new(config, tx);
            let stream_metadata = client
                .create_stream(StreamMetadata {
                    stream_id: None,
                    replica: 1,
                    ack_count: 1,
                    retention_period: std::time::Duration::from_secs(1),
                })
                .await?;
            dbg!(&stream_metadata);
            assert!(stream_metadata.stream_id.is_some());
            assert_eq!(1, stream_metadata.replica);
            assert_eq!(
                std::time::Duration::from_secs(1),
                stream_metadata.retention_period
            );
            Ok(())
        })
    }

    #[test]
    fn test_describe_stream() -> Result<(), Box<dyn Error>> {
        test_util::try_init_log();
        tokio_uring::start(async move {
            #[allow(unused_variables)]
            let port = 12378;
            let port = run_listener().await;
            let mut config = config::Configuration::default();
            config.server.host = "localhost".to_owned();
            config.server.port = 10911;
            config.placement_manager = format!("localhost:{}", port);
            let config = Arc::new(config);
            let (tx, _rx) = broadcast::channel(1);
            let client = Client::new(config, tx);

            let stream_metadata = client
                .describe_stream(1)
                .await
                .expect("Describe stream should not fail");
            assert_eq!(Some(1), stream_metadata.stream_id);
            assert_eq!(1, stream_metadata.replica);
            assert_eq!(
                std::time::Duration::from_secs(3600 * 24),
                stream_metadata.retention_period
            );
            Ok(())
        })
    }

    #[test]
    fn test_create_range() -> Result<(), ClientError> {
        test_util::try_init_log();
        tokio_uring::start(async {
            #[allow(unused_variables)]
            let port = 12378;
            let port = run_listener().await;
            let mut config = config::Configuration::default();
            config.server.host = "localhost".to_owned();
            config.server.port = 10911;
            config.placement_manager = format!("localhost:{}", port);
            let target = config.placement_manager.clone();
            let config = Arc::new(config);
            let (tx, _rx) = broadcast::channel(1);
            let client = Client::new(config, tx);
            let range = RangeMetadata::new(100, 0, 0, 0, None);
            client.create_range_replica(&target, range).await
        })
    }

    #[test]
    fn test_create_range_data_node() -> Result<(), ClientError> {
        test_util::try_init_log();
        tokio_uring::start(async {
            #[allow(unused_variables)]
            let placement_manager_port = 12378;
            let placement_manager_port = run_listener().await;

            #[allow(unused_variables)]
            let data_node_port = 10911;
            let data_node_port = run_listener().await;

            let mut config = config::Configuration::default();
            config.server.host = "127.0.0.1".to_owned();
            config.server.port = data_node_port;
            config.placement_manager = format!("127.0.0.1:{}", placement_manager_port);

            let target = format!("127.0.0.1:{}", data_node_port);

            let config = Arc::new(config);
            let (tx, _rx) = broadcast::channel(1);
            let client = Client::new(config, tx);
            let range = RangeMetadata::new_range(203, 0, 0, 0, None, 1, 1);
            client.create_range_replica(&target, range).await
        })
    }

    #[test]
    fn test_list_range_by_stream() -> Result<(), ListRangeError> {
        test_util::try_init_log();
        tokio_uring::start(async {
            #[allow(unused_variables)]
            let port = 2378;
            let port = run_listener().await;
            let mut config = config::Configuration::default();
            config.server.host = "localhost".to_owned();
            config.server.port = 10911;
            config.placement_manager = format!("localhost:{}", port);
            let config = Arc::new(config);
            let (tx, _rx) = broadcast::channel(1);
            let client = Client::new(config, tx);

            for i in 1..2 {
                let criteria = ListRangeCriteria::new(None, Some(i as u64));
                let ranges = client.list_ranges(criteria).await.unwrap();
                assert_eq!(
                    false,
                    ranges.is_empty(),
                    "Test server should have fed some mocking ranges"
                );
                for range in ranges.iter() {
                    trace!("{}", range)
                }
            }

            Ok(())
        })
    }

    #[test]
    fn test_list_range_by_data_node() -> Result<(), ListRangeError> {
        test_util::try_init_log();
        tokio_uring::start(async {
            #[allow(unused_variables)]
            let port = 2378;
            let port = run_listener().await;
            let mut config = config::Configuration::default();
            config.server.host = "localhost".to_owned();
            config.server.port = 10911;
            config.placement_manager = format!("localhost:{}", port);
            let config = Arc::new(config);
            let (tx, _rx) = broadcast::channel(1);
            let client = Client::new(config, tx);

            for _i in 1..2 {
                let criteria = ListRangeCriteria::new(None, None);
                let ranges = client.list_ranges(criteria).await.unwrap();
                assert_eq!(
                    false,
                    ranges.is_empty(),
                    "Test server should have fed some mocking ranges"
                );
                for range in ranges.iter() {
                    trace!("{}", range)
                }
            }

            Ok(())
        })
    }

    /// Test seal data node without end. This RPC is used when the single writer takes over a stream from a failed writer.
    #[test]
    fn test_seal_data_node() -> Result<(), ClientError> {
        test_util::try_init_log();
        tokio_uring::start(async move {
            #[allow(unused_variables)]
            let port = 2378;
            let port = run_listener().await;
            let mut config = config::Configuration::default();
            config.placement_manager = format!("localhost:{}", port);
            config.server.host = "localhost".to_owned();
            config.server.port = 10911;
            config.check_and_apply().unwrap();
            let config = Arc::new(config);
            let (tx, _rx) = broadcast::channel(1);
            let client = Client::new(Arc::clone(&config), tx);
            let range = RangeMetadata::new(0, 0, 0, 0, None);
            let range = client
                .seal(Some(&config.placement_manager), SealKind::DATA_NODE, range)
                .await?;
            assert_eq!(0, range.stream_id());
            assert_eq!(0, range.index());
            assert_eq!(0, range.epoch());
            assert_eq!(0, range.start());
            assert_eq!(Some(100), range.end());
            Ok(())
        })
    }

    /// Test seal data node with end. This RPC is used when the single writer takes over a stream from a graceful closed writer.
    #[test]
    fn test_seal_data_node_with_end() -> Result<(), ClientError> {
        test_util::try_init_log();
        tokio_uring::start(async move {
            #[allow(unused_variables)]
            let port = 2378;
            let port = run_listener().await;
            let mut config = config::Configuration::default();
            config.placement_manager = format!("localhost:{}", port);
            config.server.host = "localhost".to_owned();
            config.server.port = 10911;
            config.check_and_apply().unwrap();
            let config = Arc::new(config);
            let (tx, _rx) = broadcast::channel(1);
            let client = Client::new(Arc::clone(&config), tx);
            let range = RangeMetadata::new(0, 0, 0, 0, Some(1));
            let range = client
                .seal(Some(&config.placement_manager), SealKind::DATA_NODE, range)
                .await?;
            assert_eq!(0, range.stream_id());
            assert_eq!(0, range.index());
            assert_eq!(0, range.epoch());
            assert_eq!(0, range.start());
            assert_eq!(Some(1), range.end());
            Ok(())
        })
    }

    /// Test seal placement manager.
    #[test]
    fn test_seal_placement_manager() -> Result<(), ClientError> {
        test_util::try_init_log();
        tokio_uring::start(async move {
            #[allow(unused_variables)]
            let port = 2378;
            let port = run_listener().await;
            let mut config = config::Configuration::default();
            config.placement_manager = format!("localhost:{}", port);
            config.server.host = "localhost".to_owned();
            config.server.port = 10911;
            config.check_and_apply().unwrap();
            let config = Arc::new(config);
            let (tx, _rx) = broadcast::channel(1);
            let client = Client::new(Arc::clone(&config), tx);
            let range = RangeMetadata::new(0, 0, 0, 0, Some(1));
            let range = client
                .seal(
                    Some(&config.placement_manager),
                    SealKind::PLACEMENT_MANAGER,
                    range,
                )
                .await?;
            assert_eq!(0, range.stream_id());
            assert_eq!(0, range.index());
            assert_eq!(0, range.epoch());
            assert_eq!(0, range.start());
            assert_eq!(Some(1), range.end());
            Ok(())
        })
    }

    #[test]
    fn test_append() -> Result<(), Box<dyn Error>> {
        test_util::try_init_log();
        tokio_uring::start(async move {
            #[allow(unused_variables)]
            let port = 2378;
            let port = run_listener().await;
            let mut config = config::Configuration::default();
            config.placement_manager = format!("localhost:{}", port);
            config.server.host = "localhost".to_owned();
            config.server.port = 10911;
            config.check_and_apply().unwrap();
            let config = Arc::new(config);
            let (tx, _rx) = broadcast::channel(1);
            let client = Client::new(Arc::clone(&config), tx);
            let stream_id = 0;
            let index = 0;

            const BATCH: i64 = 100;
            const PAYLOAD_LENGTH: usize = 1024;

            let mut payload = BytesMut::with_capacity(PAYLOAD_LENGTH);
            payload.resize(PAYLOAD_LENGTH, 65);
            let payload = payload.freeze();

            let mut buf = BytesMut::new();
            for i in 0..BATCH {
                let batch = RecordBatchBuilder::default()
                    .with_stream_id(stream_id)
                    .with_base_offset(i * 10)
                    .with_last_offset_delta(10)
                    .with_range_index(index)
                    .with_payload(payload.clone())
                    .build()?;
                let flat_batch = Into::<FlatRecordBatch>::into(batch);
                let (slices, total) = flat_batch.encode();
                assert_eq!(
                    total as usize,
                    slices.iter().map(|s| s.len()).sum::<usize>()
                );
                for slice in &slices {
                    buf.extend_from_slice(slice);
                }
            }

            let response = client
                .append(&config.placement_manager, vec![buf.freeze()])
                .await?;

            assert_eq!(response.len(), BATCH as usize);
            Ok(())
        })
    }

    #[test]
    fn test_report_metrics() -> Result<(), ClientError> {
        test_util::try_init_log();
        tokio_uring::start(async {
            #[allow(unused_variables)]
            let port = 2378;
            let port = run_listener().await;
            let mut config = config::Configuration::default();
            config.placement_manager = format!("localhost:{}", port);
            config.server.host = "localhost".to_owned();
            config.server.port = 10911;
            config.check_and_apply().unwrap();
            let config = Arc::new(config);
            let (tx, _rx) = broadcast::channel(1);
            let client = Client::new(Arc::clone(&config), tx);
            let uring_statistics = UringStatistics::new();
            let data_node_statistics = DataNodeStatistics::new();
            let disk_statistics = DiskStatistics::new();
            let memory_statistics = MemoryStatistics::new();
            client
                .report_metrics(
                    &config.placement_manager,
                    &uring_statistics,
                    &data_node_statistics,
                    &disk_statistics,
                    &memory_statistics,
                )
                .await
        })
    }
}
