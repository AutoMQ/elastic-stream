use std::time::Duration;

use jni::objects::GlobalRef;
use tokio::sync::oneshot;

use crate::{ClientError, Frontend, Stream};

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
    OpenStream {
        front_end: &'a mut Frontend,
        stream_id: u64,
        epoch: u64,
        future: GlobalRef,
    },
    MinOffset {
        stream: &'a mut Stream,
        future: GlobalRef,
    },
    MaxOffset {
        stream: &'a mut Stream,
        future: GlobalRef,
    },
    Append {
        stream: &'a mut Stream,
        slice: &'a [u8],
        count: u32,
        future: GlobalRef,
    },
    Read {
        stream: &'a mut Stream,
        start_offset: i64,
        end_offset: i64,
        batch_max_bytes: i32,
        future: GlobalRef,
    },
}
