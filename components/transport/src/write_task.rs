use codec::frame::Frame;
use local_sync::oneshot;

#[derive(Debug)]
pub(crate) struct WriteTask {
    pub(crate) frame: Frame,
    pub(crate) observer: oneshot::Sender<Result<(), std::io::Error>>,
}
