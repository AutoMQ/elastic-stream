use model::stream::StreamMetadata;
use tokio::sync::oneshot;

use crate::{ClientError, StreamOptions};

#[derive(Debug)]
pub(crate) enum Command {
    CreateStream {
        options: StreamOptions,
        observer: oneshot::Sender<Result<StreamMetadata, ClientError>>,
    },

    OpenStream {
        stream_id: i64,
        observer: oneshot::Sender<Result<StreamMetadata, ClientError>>,
    },
}
