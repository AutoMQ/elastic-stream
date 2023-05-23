use std::sync::Arc;

use crate::{ClientError, Stream, StreamOptions};

use config::Configuration;
use log::info;
use replication::StreamClient;

pub struct Frontend {
    config: Arc<Configuration>,
    stream_client: StreamClient,
}

impl Frontend {
    pub fn new(access_point: &str) -> Result<Self, ClientError> {
        info!("New frontend with access-point: {access_point}");
        let mut config = Configuration::default();
        config.placement_manager = access_point.to_owned();
        let config = Arc::new(config);
        let stream_client = StreamClient::new(Arc::clone(&config));
        Ok(Self {
            config,
            stream_client,
        })
    }

    pub async fn create(&self, options: StreamOptions) -> Result<Stream, ClientError> {
        info!("Creating stream {options:#?}");
        let stream_id = self
            .stream_client
            .create_stream(options.replica, options.ack, options.retention)
            .await?;
        info!("Stream[id={stream_id}] created");
        Ok(Stream::new(stream_id, self.stream_client.clone()))
    }

    pub async fn open(&self, stream_id: u64, epoch: u64) -> Result<Stream, ClientError> {
        info!("Opening stream[id={stream_id}]");
        self.stream_client.open_stream(stream_id, epoch).await?;
        info!("Opened Stream[id={stream_id}]");
        Ok(Stream::new(stream_id, self.stream_client.clone()))
    }
}
