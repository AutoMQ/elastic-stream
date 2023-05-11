use model::stream::Stream;
use tokio::sync::oneshot;

use crate::client_error::ClientError;

pub(crate) enum Command {
    CreateStream {
        target: String,
        tx: oneshot::Sender<Result<Stream, ClientError>>,
    },
}
