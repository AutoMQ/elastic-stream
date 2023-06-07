use std::sync::Arc;

use crate::{ClientError, Stream, StreamOptions};

use config::Configuration;
use log::info;
use replication::StreamClient;

#[derive(Debug, Clone)]
pub struct Frontend {
    #[allow(dead_code)]
    config: Arc<Configuration>,

    stream_client: StreamClient,
}

impl Frontend {
    pub fn new(access_point: &str) -> Result<Self, ClientError> {
        info!("New frontend with access-point: {access_point}");
        let config = Configuration {
            placement_manager: access_point.to_owned(),
            ..Default::default()
        };
        let config = Arc::new(config);
        let stream_client = StreamClient::new(Arc::clone(&config));
        Ok(Self {
            config,
            stream_client,
        })
    }

    pub async fn create(&self, options: StreamOptions) -> Result<u64, ClientError> {
        info!("Creating stream {options:?}");
        let stream_id = self
            .stream_client
            .create_stream(options.replica, options.ack, options.retention)
            .await?;
        info!("Created Stream[id={stream_id}]");
        Ok(stream_id)
        // Ok(Stream::new(stream_id, self.stream_client.clone()))
    }

    pub async fn open(&self, stream_id: u64, epoch: u64) -> Result<Stream, ClientError> {
        info!("Opening stream[id={stream_id}]");
        self.stream_client.open_stream(stream_id, epoch).await?;
        info!("Opened Stream[id={stream_id}]");
        Ok(Stream::new(stream_id, self.stream_client.clone()))
    }
}
