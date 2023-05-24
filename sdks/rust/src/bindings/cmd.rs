use std::time::Duration;

use jni::objects::GlobalRef;

pub enum Command {
    CreateStream {
        replica: u8,
        ack_count: u8,
        retention: Duration,
        future: GlobalRef,
    },
}
