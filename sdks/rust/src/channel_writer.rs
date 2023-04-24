use codec::frame::Frame;
use log::trace;
use tokio::{io::AsyncWriteExt, net::tcp::OwnedWriteHalf};

use crate::client_error::ClientError;

#[derive(Debug)]
pub(crate) struct ChannelWriter {
    stream: OwnedWriteHalf,
}

impl ChannelWriter {
    pub(crate) fn new(stream: OwnedWriteHalf) -> Self {
        Self { stream }
    }

    pub(crate) async fn write(&mut self, frame: &Frame) -> Result<(), ClientError> {
        let slices = frame.encode()?;
        for slice in slices.into_iter() {
            self.stream.write_all(&slice).await?;
            trace!("Written {}bytes", slice.len());
        }
        Ok(())
    }
}
