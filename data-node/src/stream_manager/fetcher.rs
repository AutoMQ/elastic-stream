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
        #[allow(dead_code)]
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
}
