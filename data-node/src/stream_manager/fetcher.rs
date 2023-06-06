use client::Client;
use log::{error, trace};
use model::{range::RangeMetadata, stream::StreamMetadata};
use std::rc::Rc;
use tokio::sync::{mpsc, oneshot};

use crate::error::ServiceError;

/// Non-primary `Node` uses this task to delegate query range task to the primary one.
pub struct FetchRangeTask {
    pub node_id: Option<u32>,
    /// Stream-id to query
    pub stream_id: Option<u64>,

    /// Once the query completes, transfer results back to the caller through this oneshot channel.
    pub tx: oneshot::Sender<Result<Vec<RangeMetadata>, ServiceError>>,
}

pub(crate) enum Fetcher {
    /// If a `Node` of `DataNode` is playing primary role, it is carrying the responsibility of communicating with
    /// `PlacementManager`.
    ///
    /// Primary `Node` fetches ranges of a stream for itself or on behalf of other `Node`s.
    PlacementClient { client: Rc<Client> },

    /// Non-primary `Node`s acquires ranges of a stream through delegating to the primary node.
    Channel {
        sender: mpsc::UnboundedSender<FetchRangeTask>,
    },
}

impl Fetcher {
    pub(crate) async fn bootstrap(
        &mut self,
        node_id: u32,
    ) -> Result<Vec<RangeMetadata>, ServiceError> {
        if let Fetcher::PlacementClient { client } = self {
            return client
                .list_ranges(model::ListRangeCriteria::new(Some(node_id), None))
                .await
                .map_err(|_e| {
                    error!("Failed to list ranges by data node from placement manager");
                    ServiceError::AcquireRange
                })
                .inspect(|ranges| {
                    trace!(
                        "Received list ranges response for current data node: {:?}",
                        ranges
                    );
                });
        }
        Err(ServiceError::AcquireRange)
    }

    /// TODO: filter out ranges that is not hosted in current data node.
    pub(crate) async fn list_ranges(
        &mut self,
        node_id: Option<u32>,
        stream_id: Option<u64>,
    ) -> Result<Vec<RangeMetadata>, ServiceError> {
        match self {
            Fetcher::Channel { sender } => {
                Self::list_ranges_from_peer_node(sender, node_id, stream_id).await
            }
            Fetcher::PlacementClient { client } => {
                Self::list_ranges_by_client(
                    client,
                    model::ListRangeCriteria::new(node_id, stream_id),
                )
                .await
            }
        }
    }

    pub(crate) async fn describe_stream(
        &self,
        stream_id: u64,
    ) -> Result<StreamMetadata, ServiceError> {
        if let Fetcher::PlacementClient { client } = self {
            return client
                .describe_stream(stream_id)
                .await
                .map_err(|_e| {
                    error!(
                        "Failed to get stream={} metadata from placement manager",
                        stream_id
                    );
                    ServiceError::DescribeStream
                })
                .inspect(|metadata| {
                    trace!(
                        "Received stream-metadata={:?} from placement manager for stream-id={}",
                        metadata,
                        stream_id,
                    );
                });
        }

        unimplemented!("Describe stream from peer worker is not implemented yet")
    }

    async fn list_ranges_by_client(
        client: &Client,
        criteria: model::ListRangeCriteria,
    ) -> Result<Vec<RangeMetadata>, ServiceError> {
        client
            .list_ranges(criteria.clone())
            .await
            .map_err(|_e| {
                error!(
                    "Failed to list ranges for stream={:?} from placement manager",
                    criteria.stream_id
                );
                ServiceError::AcquireRange
            })
            .inspect(|ranges| {
                trace!(
                    "Ranges for stream={:?} is: {:?}",
                    criteria.stream_id,
                    ranges
                )
            })
    }

    async fn list_ranges_from_peer_node(
        sender: &mpsc::UnboundedSender<FetchRangeTask>,
        node_id: Option<u32>,
        stream_id: Option<u64>,
    ) -> Result<Vec<RangeMetadata>, ServiceError> {
        let (tx, rx) = oneshot::channel();
        let task = FetchRangeTask {
            node_id,
            stream_id,
            tx,
        };
        if let Err(e) = sender.send(task) {
            let task = e.0;
            let _ = task.tx.send(Err(ServiceError::AcquireRange));
        }
        rx.await.map_err(|_e| {
            error!(
                "Failed to get ranges from primary node for stream={:?}",
                stream_id
            );
            ServiceError::Internal("Broken oneshot channel".to_owned())
        })?
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::error::Error;

    use model::range::RangeMetadata;

    use super::Fetcher;

    #[test]
    fn test_fetch_from_peer_node() -> Result<(), Box<dyn Error>> {
        tokio_uring::start(async {
            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

            let mut fetcher = Fetcher::Channel { sender: tx };
            const TOTAL: i32 = 16;

            tokio_uring::spawn(async move {
                loop {
                    match rx.recv().await {
                        Some(task) => {
                            let stream_id = task.stream_id;
                            let ranges = (0..TOTAL)
                                .map(|i| {
                                    if i < TOTAL - 1 {
                                        RangeMetadata::new(
                                            stream_id.map(|v| v as i64).unwrap(),
                                            i,
                                            0,
                                            (i * 100) as u64,
                                            Some(((i + 1) * 100) as u64),
                                        )
                                    } else {
                                        RangeMetadata::new(
                                            stream_id.map(|v| v as i64).unwrap(),
                                            i,
                                            0,
                                            (i * 100) as u64,
                                            None,
                                        )
                                    }
                                })
                                .collect::<Vec<_>>();
                            if let Err(_e) = task.tx.send(Ok(ranges)) {
                                panic!("Failed to transfer mocked ranges");
                            }
                        }
                        None => {
                            break;
                        }
                    }
                }
            });

            let res = fetcher.list_ranges(Some(0), Some(1)).await?;
            assert_eq!(res.len(), TOTAL as usize);
            drop(fetcher);
            Ok(())
        })
    }
}
