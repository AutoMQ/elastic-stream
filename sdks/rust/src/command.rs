use model::stream::StreamMetadata;
use tokio::sync::oneshot;

use crate::client_error::ClientError;

pub(crate) enum Command {
    CreateStream {
        target: String,
        tx: oneshot::Sender<Result<StreamMetadata, ClientError>>,
    },
}
