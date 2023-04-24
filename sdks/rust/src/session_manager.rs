use model::stream::Stream;
use std::sync::Arc;
use tokio::{
    runtime::Runtime,
    sync::{mpsc, oneshot},
};
use util::HandleJoiner;

use crate::{client_error::ClientError, command::Command, io::IO};

#[derive(Debug, Clone)]
pub(crate) struct SessionManager {
    tx: mpsc::UnboundedSender<Command>,
    handle_jointer: Arc<HandleJoiner>,
}

impl SessionManager {
    pub(crate) fn new() -> Result<Self, ClientError> {
        let (tx, rx) = mpsc::unbounded_channel();
        let handle = std::thread::Builder::new()
            .name("IO".to_owned())
            .spawn(move || {
                let rt = Runtime::new().expect("Build tokio runtime");
                let mut io = IO::new(rx);
                rt.block_on(async move {
                    io.run().await;
                })
            })
            .map_err(|_e| ClientError::Thread)?;
        let mut handle_joiner = HandleJoiner::new();
        handle_joiner.push(handle);
        Ok(Self {
            handle_jointer: Arc::new(handle_joiner),
            tx,
        })
    }

    pub(crate) async fn create_stream(&self) -> Result<Option<Stream>, ClientError> {
        let (sender, receiver) = oneshot::channel();
        let command = Command::CreateStream {
            target: String::new(),
            tx: sender,
        };
        match self.tx.send(command) {
            Ok(_) => {}
            Err(_e) => {}
        };
        receiver.await.map_err(|_e| ClientError::ClientInternal)?
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error;

    use super::SessionManager;

    #[test]
    fn test_new() -> Result<(), Box<dyn Error>> {
        let _session_manager = SessionManager::new()?;
        Ok(())
    }
}
