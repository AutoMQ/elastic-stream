use super::{config::ClientConfig, session_manager::SessionManager};
use crate::error::ClientError;
use model::{range::StreamRange, range_criteria::RangeCriteria};
use slog::{error, trace, warn, Logger};
use std::{cell::UnsafeCell, rc::Rc, time::Duration};
use tokio::time;

/// `Client` is used to send
pub struct Client {
    pub(crate) session_manager: Rc<UnsafeCell<SessionManager>>,
    pub(crate) log: Logger,
    pub(crate) config: Rc<ClientConfig>,
}

impl Client {
    pub fn start(&self) {
        let session_manager = Rc::clone(&self.session_manager);
        let session_manager = unsafe { &*session_manager.get() };
        tokio_uring::spawn(async move { session_manager.start().await });
    }

    pub async fn allocate_id(
        &self,
        target: &str,
        host: &str,
        timeout: Duration,
    ) -> Result<i32, ClientError> {
        let session_manager = unsafe { &mut *self.session_manager.get() };
        let session = session_manager.get_session(target).await?;
        let future = session.allocate_id(host, timeout);
        time::timeout(timeout, future).await.map_err(|e|{
            warn!(self.log, "Timeout when allocate ID. {}", e);
            ClientError::ClientInternal
        })?.map_err(|e|{
            error!(
                self.log,
                "Failed to receive response from broken channel. Cause: {:?}", e; "struct" => "Client"
            );
            ClientError::ClientInternal
        })
    }

    pub async fn list_range(
        &self,
        target: &str,
        stream_id: Option<i64>,
        timeout: Duration,
    ) -> Result<Vec<StreamRange>, ClientError> {
        let criteria = if let Some(stream_id) = stream_id {
            trace!(self.log, "list_range from {}", target; "stream-id" => stream_id);
            RangeCriteria::StreamId(stream_id)
        } else if let Some(ref data_node) = self.config.data_node {
            trace!(
                self.log,
                "List stream ranges from placement manager for {:?}",
                data_node
            );
            RangeCriteria::DataNode(data_node.clone())
        } else {
            return Err(ClientError::ClientInternal);
        };

        let session_manager = unsafe { &mut *self.session_manager.get() };
        let session = session_manager.get_session(target).await?;
        let future = session.list_range(criteria);
        time::timeout(timeout, future).await.map_err(|elapsed| {
            warn!(self.log, "Timeout when list range. {}", elapsed);
            ClientError::RpcTimeout { timeout }
        })?.map_err(|e| {
            error!(
                self.log,
                "Failed to receive response from broken channel. Cause: {:?}", e; "struct" => "Client"
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
        let session = session_manager.get_session(target).await?;
        time::timeout(self.config.io_timeout, session.heartbeat())
            .await
            .map_err(|_elapsed| {
                error!(self.log, "Timeout when heartbeat to {}", target);
                ClientError::RpcTimeout {
                    timeout: self.config.io_timeout,
                }
            })
            .and_then(|_res| {
                trace!(self.log, "Heartbeat to {} OK", target);
                Ok(())
            })
    }
}

#[cfg(test)]
mod tests {
    use std::{error::Error, time::Duration};

    use model::data_node::DataNode;
    use slog::trace;
    use test_util::{run_listener, terminal_logger};

    use crate::{error::ListRangeError, ClientBuilder, ClientConfig};

    #[test]
    fn test_allocate_id() -> Result<(), Box<dyn Error>> {
        tokio_uring::start(async {
            let log = terminal_logger();
            #[allow(unused_variables)]
            let port = 2378;
            let port = run_listener(log.clone()).await;
            let addr = format!("localhost:{}", port);
            let client = ClientBuilder::new()
                .set_log(log.clone())
                .build()
                .map_err(|_e| ListRangeError::Internal)?;
            client.start();
            let timeout = Duration::from_secs(3);
            let id = client.allocate_id(&addr, "localhost", timeout).await?;
            assert_eq!(1, id);
            Ok(())
        })
    }

    #[test]
    fn test_list_range_by_stream() -> Result<(), ListRangeError> {
        tokio_uring::start(async {
            let log = terminal_logger();

            #[allow(unused_variables)]
            let port = 2378;

            let port = run_listener(log.clone()).await;
            let addr = format!("localhost:{}", port);
            let mut client_config = ClientConfig::default();
            client_config.with_data_node(DataNode {
                node_id: 1,
                advertise_address: format!("{}:{}", "localhost", "10911"),
            });
            let client = ClientBuilder::new()
                .set_log(log.clone())
                .set_config(client_config)
                .build()
                .map_err(|_e| ListRangeError::Internal)?;

            client.start();

            let timeout = Duration::from_secs(10);

            for i in 1..2 {
                let ranges = client
                    .list_range(&addr, Some(i as i64), timeout)
                    .await
                    .unwrap();
                assert_eq!(
                    false,
                    ranges.is_empty(),
                    "Test server should have fed some mocking ranges"
                );
                for range in ranges.iter() {
                    trace!(log, "{}", range)
                }
            }

            Ok(())
        })
    }

    #[test]
    fn test_list_range_by_data_node() -> Result<(), ListRangeError> {
        tokio_uring::start(async {
            let log = terminal_logger();

            #[allow(unused_variables)]
            let port = 2378;

            let port = run_listener(log.clone()).await;
            let addr = format!("localhost:{}", port);
            let mut client_config = ClientConfig::default();
            client_config.with_data_node(DataNode {
                node_id: 1,
                advertise_address: format!("{}:{}", "localhost", "10911"),
            });
            let client = ClientBuilder::new()
                .set_log(log.clone())
                .set_config(client_config)
                .build()
                .map_err(|_e| ListRangeError::Internal)?;

            client.start();

            let timeout = Duration::from_secs(10);

            for _i in 1..2 {
                let ranges = client.list_range(&addr, None, timeout).await.unwrap();
                assert_eq!(
                    false,
                    ranges.is_empty(),
                    "Test server should have fed some mocking ranges"
                );
                for range in ranges.iter() {
                    trace!(log, "{}", range)
                }
            }

            Ok(())
        })
    }
}
