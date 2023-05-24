use std::time::Duration;

use jni::objects::GlobalRef;
use tokio::sync::oneshot;

use crate::{ClientError, Frontend};

pub enum Command<'a> {
    GetFrontend {
        access_point: String,
        tx: oneshot::Sender<Result<Frontend, ClientError>>,
    },
    CreateStream {
        front_end: &'a mut Frontend,
        replica: u8,
        ack_count: u8,
        retention: Duration,
        future: GlobalRef,
    },
}
