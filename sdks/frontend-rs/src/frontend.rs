use std::{cell::RefCell, sync::Arc};

use crate::{ClientError, Stream, StreamOptions};

use config::Configuration;
use log::info;
use replication::{ReplicationError, StreamClient};

#[derive(Debug, Clone)]
pub struct Frontend {
    #[allow(dead_code)]
    config: Arc<Configuration>,
    stream_clients: Vec<StreamClient>,
    round_robin: RefCell<usize>,
}

impl Frontend {
    pub fn new(access_point: &str) -> Result<Self, ClientError> {
        info!("New frontend with access-point: {access_point}");
        let mut config = Configuration {
            placement_driver: access_point.to_owned(),
            ..Default::default()
        };
        config.replication.connection_pool_size = 2;
        config.replication.thread_count = 4;
        let config = Arc::new(config);

        let mut stream_clients = vec![];
        for i in 0..config.replication.thread_count {
            let stream_client = StreamClient::new(Arc::clone(&config), i);
            stream_clients.push(stream_client);
        }
        Ok(Self {
            config,
            stream_clients,
            round_robin: RefCell::new(0),
        })
    }

    pub async fn create(&self, options: StreamOptions) -> Result<u64, ClientError> {
        info!("Creating stream {options:?}");
        let stream_id = self.stream_clients[0]
            .create_stream(options.replica, options.ack, options.retention)
            .await?;
        info!("Created Stream[id={stream_id}]");
        Ok(stream_id)
    }

    pub async fn open(&self, stream_id: u64, epoch: u64) -> Result<Stream, ClientError> {
        info!("Opening stream[id={stream_id}]");
        let stream_client = self.route_client()?;
        stream_client.open_stream(stream_id, epoch).await?;
        info!("Opened Stream[id={stream_id}]");
        Ok(Stream::new(stream_id, stream_client))
    }

    fn route_client(&self) -> Result<StreamClient, ReplicationError> {
        debug_assert!(
            !self.stream_clients.is_empty(),
            "Clients should NOT be empty"
        );
        let index = *self.round_robin.borrow() % self.stream_clients.len();
        *self.round_robin.borrow_mut() += 1;
        self.stream_clients
            .get(index)
            .cloned()
            .ok_or(ReplicationError::Internal)
    }
}
