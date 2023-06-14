use bytes::{Bytes, BytesMut};
use clap::Parser;
use frontend::{Frontend, StreamOptions};
use futures::future;
use log::{error, info, warn};
use model::{record::flat_record::FlatRecordBatch, RecordBatch};
use rand::{seq::SliceRandom, RngCore};
use std::{error::Error, rc::Rc};
use tokio::time;

#[cfg(not(target_env = "msvc"))]
use tikv_jemallocator::Jemalloc;

#[cfg(not(target_env = "msvc"))]
#[global_allocator]
static GLOBAL: Jemalloc = Jemalloc;

const APPEND_WARN_THRESHOLD_MS: u64 = 10;

#[derive(Parser, Debug, Clone)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(long, required = true)]
    pm_addr: String,

    #[arg(long, default_value_t = 1)]
    replica_cnt: u8,

    #[arg(long, default_value_t = 1)]
    ack_cnt: u8,

    #[arg(short, long, default_value_t = 1)]
    client_cnt: u8,

    #[arg(short, long, default_value_t = 1024)]
    stream_cnt: u16,

    #[arg(short, long, default_value_t = 1024)]
    payload_size: u32,

    #[arg(short, long, default_value_t = 500 * 1024)]
    rate: u32,

    #[arg(short, long, default_value_t = 60)]
    duration_sec: u32,
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();
    frontend::init_log();
    tokio_uring::start(async {
        let handles = (0..args.client_cnt)
            .into_iter()
            .map(|client_id| {
                let args = args.clone();
                tokio_uring::spawn(async move {
                    let frontend = Frontend::new(&args.pm_addr).unwrap();
                    info!("client[{}] started", client_id);

                    let streams =
                        future::try_join_all((0..args.stream_cnt).into_iter().map(|_| async {
                            let stream_id = frontend
                                .create(StreamOptions {
                                    replica: args.replica_cnt,
                                    ack: args.ack_cnt,
                                    retention: time::Duration::from_secs(3600),
                                })
                                .await
                                .unwrap();
                            frontend.open(stream_id, 0).await
                        }))
                        .await
                        .unwrap();
                    debug_assert_eq!(args.stream_cnt as usize, streams.len());
                    info!("client[{}] created {} streams", client_id, streams.len());

                    let mut payload = BytesMut::with_capacity(args.payload_size as usize);
                    payload.resize(args.payload_size as usize, 0);
                    rand::thread_rng().fill_bytes(&mut payload);
                    let payload = payload.freeze();
                    debug_assert_eq!(args.payload_size as usize, payload.len());

                    let streams = streams.into_iter().map(Rc::new).collect::<Vec<_>>();
                    let mut i: u32 = 0;
                    let start_time = time::Instant::now();
                    let interval_ns = 1_000_000_000 / args.rate as u64;

                    loop {
                        if time::Instant::now().duration_since(start_time).as_secs()
                            >= args.duration_sec as u64
                        {
                            break;
                        }
                        time::sleep_until(start_time + time::Duration::from_nanos(interval_ns) * i)
                            .await;

                        let stream = streams.choose(&mut rand::thread_rng()).unwrap().clone();
                        let payload_bytes = payload_bytes(stream.id(), payload.clone());
                        i = i + 1;

                        tokio_uring::spawn(async move {
                            let start = time::Instant::now();
                            let res = stream.append(payload_bytes).await;
                            let elapsed = start.elapsed();
                            if let Err(e) = res {
                                error!(
                                    "client[{}]-{} failed to append to stream[{}]: {}",
                                    client_id,
                                    i,
                                    stream.id(),
                                    e
                                );
                            }
                            if elapsed.as_millis() as u64 > APPEND_WARN_THRESHOLD_MS {
                                warn!(
                                    "client[{}]-{} took {}ms to append to stream[{}]",
                                    client_id,
                                    i,
                                    elapsed.as_millis(),
                                    stream.id()
                                );
                            }
                        });
                    }
                })
            })
            .collect::<Vec<_>>();
        for handle in handles {
            let _ = handle.await;
        }
    });
    Ok(())
}

fn payload_bytes(stream_id: u64, payload: Bytes) -> Bytes {
    let record_batch = RecordBatch::new_builder()
        .with_stream_id(stream_id as i64)
        .with_range_index(42)
        .with_base_offset(42)
        .with_last_offset_delta(1)
        .with_payload(Bytes::copy_from_slice(&payload[..]))
        .build()
        .unwrap();
    let flat_record_batch: FlatRecordBatch = Into::into(record_batch);
    let (flat_record_batch_vec_bytes, _) = flat_record_batch.encode();
    vec_bytes_to_bytes(flat_record_batch_vec_bytes)
}

fn vec_bytes_to_bytes(vec_bytes: Vec<Bytes>) -> Bytes {
    let mut bytes_mut = BytesMut::new();
    for bytes in vec_bytes {
        bytes_mut.extend_from_slice(&bytes[..]);
    }
    bytes_mut.freeze()
}
