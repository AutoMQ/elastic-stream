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

pub trait PlacementFetcher {
    async fn bootstrap(&mut self, node_id: u32) -> Result<Vec<RangeMetadata>, ServiceError>;

    async fn describe_stream(&self, stream_id: u64) -> Result<StreamMetadata, ServiceError>;
}

pub(crate) struct PlacementClient {
    client: Rc<Client>,
}

impl PlacementClient {
    pub(crate) fn new(client: Rc<Client>) -> Self {
        Self { client }
    }
}

impl PlacementFetcher for PlacementClient {
    async fn bootstrap(&mut self, node_id: u32) -> Result<Vec<RangeMetadata>, ServiceError> {
        self.client
            .list_ranges(model::ListRangeCriteria::new(Some(node_id), None))
            .await
            .map_err(|_e| {
                error!("Failed to list ranges by data node from placement driver");
                ServiceError::AcquireRange
            })
            .inspect(|ranges| {
                trace!(
                    "Received list ranges response for current data node: {:?}",
                    ranges
                );
            })
    }

    async fn describe_stream(&self, stream_id: u64) -> Result<StreamMetadata, ServiceError> {
        self.client
            .describe_stream(stream_id)
            .await
            .map_err(|_e| {
                error!(
                    "Failed to get stream={} metadata from placement driver",
                    stream_id
                );
                ServiceError::DescribeStream
            })
            .inspect(|metadata| {
                trace!(
                    "Received stream-metadata={:?} from placement driver for stream-id={}",
                    metadata,
                    stream_id,
                );
            })
    }
}

pub(crate) struct DelegatePlacementClient {
    #[allow(dead_code)]
    sender: mpsc::UnboundedSender<FetchRangeTask>,
}

impl DelegatePlacementClient {
    pub(crate) fn new(sender: mpsc::UnboundedSender<FetchRangeTask>) -> Self {
        Self { sender }
    }
}

impl PlacementFetcher for DelegatePlacementClient {
    async fn bootstrap(&mut self, _node_id: u32) -> Result<Vec<RangeMetadata>, ServiceError> {
        todo!()
    }

    async fn describe_stream(&self, _stream_id: u64) -> Result<StreamMetadata, ServiceError> {
        todo!()
    }
}
