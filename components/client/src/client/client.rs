use super::session_manager::SessionManager;
use crate::error::ClientError;
use bytes::Bytes;
use log::{error, trace, warn};
use model::{
    range::Range, range_criteria::RangeCriteria, request::seal::SealRange,
    response::append::AppendResultEntry,
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
        let session = session_manager
            .get_composite_session(&self.config.placement_manager)
            .await?;
        let future = session.allocate_id(host, self.config.client_io_timeout());
        time::timeout(self.config.client_io_timeout(), future)
            .await
            .map_err(|e| {
                warn!("Timeout when allocate ID. {}", e);
                ClientError::ClientInternal
            })?
            .map_err(|e| {
                error!(
                    "Failed to receive response from broken channel. Cause: {:?}",
                    e
                );
                ClientError::ClientInternal
            })
    }

    pub async fn list_range(&self, stream_id: Option<i64>) -> Result<Vec<Range>, ClientError> {
        let criteria = if let Some(stream_id) = stream_id {
            trace!(
                "list_range from {}, stream-id={}",
                self.config.placement_manager,
                stream_id
            );
            RangeCriteria::StreamId(stream_id)
        } else {
            trace!(
                "List stream ranges from placement manager for node-id={}",
                self.config.server.node_id
            );
            RangeCriteria::DataNode(self.config.server.node_id)
        };

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

    pub async fn seal(
        &self,
        target: Option<&str>,
        request: SealRange,
    ) -> Result<Range, ClientError> {
        // Validate request
        match request.kind {
            SealKind::DATA_NODE => {
                if target.is_none() {
                    error!("Target is required while seal range against data nodes");
                    return Err(ClientError::BadRequest);
                }
            }
            SealKind::PLACEMENT_MANAGER => {
                if request.range.end().is_none() {
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
        composite_session.seal(request).await
    }

    /// Append data to a range.
    pub async fn append(
        &self,
        target: &str,
        buf: Bytes,
    ) -> Result<Vec<AppendResultEntry>, ClientError> {
        let session_manager = unsafe { &mut *self.session_manager.get() };
        let session = session_manager.get_composite_session(target).await?;
        session.append(buf).await
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
    use log::trace;
    use model::{range::Range, request::seal::SealRange};
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
    fn test_list_range_by_stream() -> Result<(), ListRangeError> {
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
                let ranges = client.list_range(Some(i as i64)).await.unwrap();
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
                let ranges = client.list_range(None).await.unwrap();
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
            let request = SealRange {
                kind: SealKind::DATA_NODE,
                range: Range::new(0, 0, 0, 0, None),
            };
            let range = client
                .seal(Some(&config.placement_manager), request)
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
            let request = SealRange {
                kind: SealKind::DATA_NODE,
                range: Range::new(0, 0, 0, 0, Some(1)),
            };
            let range = client
                .seal(Some(&config.placement_manager), request)
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
            let request = SealRange {
                kind: SealKind::PLACEMENT_MANAGER,
                range: Range::new(0, 0, 0, 0, Some(1)),
            };
            let range = client
                .seal(Some(&config.placement_manager), request)
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
    fn test_report_metrics() -> Result<(), ClientError> {
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
