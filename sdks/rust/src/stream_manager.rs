use std::thread;

use crate::{command::Command, worker::Worker, ClientError, Stream, StreamOptions};

use log::error;
use tokio::sync::{mpsc, oneshot};

pub struct StreamManager {
    tx: mpsc::UnboundedSender<Command>,
    join_handle: thread::JoinHandle<()>,
}

impl StreamManager {
    pub fn new(access_point: &str) -> Result<Self, ClientError> {
        let (tx, rx) = mpsc::unbounded_channel();
        let join_handle = Worker::spawn(rx, access_point)?;
        Ok(Self { tx, join_handle })
    }

    pub async fn create(&self, options: StreamOptions) -> Result<Stream, ClientError> {
        let (tx, rx) = oneshot::channel();
        let command = Command::CreateStream {
            options,
            observer: tx,
        };

        self.tx.send(command).map_err(|e| {
            error!("Broken MPSC channel: failed to send command: {:?}", e.0);
            ClientError::BrokenChannel(format!("Command: {:?}", e.0))
        })?;

        let metadata = rx.await.map_err(|_e| {
            error!("Failed to recieve from oneshot channel");
            ClientError::BrokenChannel(format!("Failed to receive oneshot"))
        })??;
        Ok(Stream::new(metadata, self.tx.clone()))
    }

    pub async fn open(&self, id: i64) -> Result<Stream, ClientError> {
        let (tx, rx) = oneshot::channel();
        let command = Command::OpenStream {
            stream_id: id,
            observer: tx,
        };

        self.tx.send(command).map_err(|e| {
            error!("Broken MPSC channel: failed to send command: {:?}", e.0);
            ClientError::BrokenChannel(format!("Command: {:?}", e.0))
        })?;

        let metadata = rx.await.map_err(|_e| {
            error!("Failed to receive from oneshot channel");
            ClientError::BrokenChannel(format!("Failed to receive oneshot"))
        })??;

        Ok(Stream::new(metadata, self.tx.clone()))
    }
}
