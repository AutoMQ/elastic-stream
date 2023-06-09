use std::{cell::UnsafeCell, rc::Rc, sync::Arc};

use bytes::{Bytes, BytesMut};
use frontend::{Frontend, StreamOptions};
use log::{debug, error, info, warn};
use model::{record::flat_record::FlatRecordBatch, RecordBatch};
use rand::RngCore;
use tokio::time;

#[cfg(not(target_env = "msvc"))]
use tikv_jemallocator::Jemalloc;

#[cfg(not(target_env = "msvc"))]
#[global_allocator]
static GLOBAL: Jemalloc = Jemalloc;

const PM_ADDR: &str = "192.168.123.143:12378";
const REPLICA_CNT: u8 = 2;
const ACK_CNT: u8 = 1;

const BATCH_SIZE: usize = 1 * 1024 * 1024;
const RATE: u64 = 100;
const MAX_APPEND_INFLIGHT: u32 = 2;

const APPEND_WARN_THRESHOLD_MS: u64 = 500;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    frontend::init_log();
    tokio_uring::start(async {
        tokio_uring::spawn(async {
            t().await.unwrap();
        });
        t().await.unwrap();
    });
    Ok(())
}

async fn t() -> Result<(), Box<dyn std::error::Error>> {
    let frontend = Frontend::new(PM_ADDR)?;
    let stream_id = frontend
        .create(StreamOptions {
            replica: REPLICA_CNT,
            ack: ACK_CNT,
            retention: time::Duration::from_secs(3600),
        })
        .await?;
    info!("Created stream with id: {}", stream_id);
    let stream = frontend.open(stream_id, 0).await?;

    // append 10-record batch
    let mut payload = [0u8; BATCH_SIZE];
    rand::thread_rng().fill_bytes(&mut payload);
    let record_batch = RecordBatch::new_builder()
        .with_stream_id(stream_id as i64)
        .with_range_index(0)
        .with_base_offset(0)
        .with_last_offset_delta(10)
        .with_payload(Bytes::copy_from_slice(&payload[..]))
        .build()
        .unwrap();
    let flat_record_batch: FlatRecordBatch = Into::into(record_batch);
    let (flat_record_batch_vec_bytes, _) = flat_record_batch.encode();
    let flat_record_batch_bytes = vec_bytes_to_bytes(flat_record_batch_vec_bytes);

    let stream = Arc::new(stream);
    let mut i: u32 = 0;
    let start = time::Instant::now();
    let interval_ns = 1_000_000_000 / RATE;

    let cnt = Rc::new(UnsafeCell::new(0));

    let cnt2 = Rc::clone(&cnt);
    tokio_uring::spawn(async move {
        loop {
            tokio::time::sleep(time::Duration::from_secs(1)).await;
            let cnt = unsafe { &mut *cnt2.get() };
            info!("In progress append: {}", *cnt);
        }
    });

    loop {
        let b = flat_record_batch_bytes.clone();
        let s = stream.clone();
        let cnt1 = Rc::clone(&cnt);
        i = i + 1;
        tokio_uring::spawn(async move {
            let cnt = unsafe { &mut *cnt1.get() };
            *cnt += 1;
            append(&s, b, i).await.unwrap_or_else(|e| {
                error!("Append error: {:?}", e);
            });
            *cnt -= 1;
        });
        loop {
            time::sleep_until(start + time::Duration::from_nanos(interval_ns) * i).await;
            if unsafe { *cnt.get() } < MAX_APPEND_INFLIGHT {
                break;
            }
        }
    }
}

async fn append(
    stream: &frontend::Stream,
    bytes: Bytes,
    index: u32,
) -> Result<(), Box<dyn std::error::Error>> {
    let start = time::Instant::now();
    if index % 100 == 0 {
        debug!("[{}] Append {} bytes", index, bytes.len());
    }
    let append_result = stream.append(bytes).await?;
    let time_used = start.elapsed();
    if time_used > time::Duration::from_millis(APPEND_WARN_THRESHOLD_MS) {
        warn!(
            "[{}] Append result: {:?}, used {:?}",
            index, append_result, time_used
        );
    } else if index % 100 == 0 {
        debug!(
            "[{}] Append result: {:?}, used {:?}",
            index, append_result, time_used
        );
    }
    Ok(())
}

fn vec_bytes_to_bytes(vec_bytes: Vec<Bytes>) -> Bytes {
    let mut bytes_mut = BytesMut::new();
    for bytes in vec_bytes {
        bytes_mut.extend_from_slice(&bytes[..]);
    }
    bytes_mut.freeze()
}
