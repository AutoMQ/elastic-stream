use crate::{ClientError, Stream, StreamOptions};

pub struct StreamManager {}

impl StreamManager {
    pub async fn create(&self, options: StreamOptions) -> Result<Stream, ClientError> {
        todo!()
    }

    pub async fn open(&self, id: i64) -> Result<Stream, ClientError> {
        todo!()
    }
}
