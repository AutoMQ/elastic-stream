use tokio::sync::oneshot;

#[derive(Debug)]
pub struct AppendRequest {}

#[derive(Debug)]
pub struct AppendResponse {}

#[derive(Debug)]
pub struct ReadRequest {}

#[derive(Debug)]
pub struct ReadResponse {}

#[derive(Debug)]
pub(crate) enum Request {
    Append {
        request: AppendRequest,
        tx: oneshot::Sender<AppendResponse>,
    },
    Read {
        request: ReadRequest,
        tx: oneshot::Sender<ReadResponse>,
    },
}
