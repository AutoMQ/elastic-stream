use bytes::{Bytes, BytesMut};
use clap::{arg, Parser};
use frontend::{Frontend, StreamOptions};
use local_sync::semaphore::Semaphore;
use log::{error, info};
use model::{record::flat_record::FlatRecordBatch, RecordBatch};
use std::{cell::RefCell, error::Error, rc::Rc};
use tokio::time::Duration;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Number of streams to create and append records into
    #[arg(short, long, default_value_t = 16)]
    stream: usize,

    /// Number of iterations to append to each stream
    #[arg(short, long, default_value_t = 128)]
    time: usize,

    #[arg(short, long, default_value_t = 16)]
    batch_size: usize,

    #[arg(short, long, default_value_t = 1024)]
    payload_size: usize,
}

pub struct Stats {
    success: usize,
    failure: usize,
}

impl Stats {
    fn new() -> Self {
        Self {
            success: 0,
            failure: 0,
        }
    }

    pub fn reset(&mut self) -> (usize, usize) {
        let res = (self.success, self.failure);
        self.success = 0;
        self.failure = 0;
        res
    }

    pub fn inc_success(&mut self) {
        self.success += 1;
    }

    pub fn inc_failure(&mut self) {
        self.failure += 1;
    }
}

/// Example program to kickoff append workload.
fn main() -> Result<(), Box<dyn Error + 'static>> {
    let args = Args::parse();
    frontend::init_log();
    tokio_uring::start(async {
        let frontend = Rc::new(Frontend::new("127.0.0.1:12378")?);
        let stats = Rc::new(RefCell::new(Stats::new()));

        let log_stat = Rc::clone(&stats);
        tokio_uring::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(1));
            loop {
                interval.tick().await;
                let (success, failure) = log_stat.borrow_mut().reset();
                info!("Success: {}, Failure: {}", success, failure);
            }
        });

        let handles = (0..args.stream)
            .into_iter()
            .map(|_| {
                let frontend = Rc::clone(&frontend);
                let stats = Rc::clone(&stats);
                tokio_uring::spawn(async move {
                    let stream_id = frontend
                        .create(StreamOptions {
                            replica: 1,
                            ack: 1,
                            retention: Duration::from_secs(3600),
                        })
                        .await
                        .unwrap();
                    info!("Created stream with id: {}", stream_id);
                    let stream = Rc::new(frontend.open(stream_id, 0).await.unwrap());
                    let mut payload = BytesMut::with_capacity(args.payload_size);
                    payload.resize(args.payload_size, b'x');
                    let payload = payload.freeze();
                    debug_assert_eq!(args.payload_size, payload.len());
                    let semaphore = Rc::new(Semaphore::new(args.batch_size));
                    let instant = minstant::Instant::now();
                    let record_batch = RecordBatch::new_builder()
                        .with_stream_id(stream_id as i64)
                        .with_range_index(0)
                        .with_base_offset(0)
                        .with_last_offset_delta(10)
                        .with_payload(payload.clone())
                        .build()
                        .unwrap();
                    let flat_record_batch: FlatRecordBatch = Into::into(record_batch);
                    let (flat_record_batch_bytes, _) = flat_record_batch.encode();
                    let buf = vec_bytes_to_bytes(flat_record_batch_bytes);
                    loop {
                        if instant.elapsed().as_secs() as usize > args.time {
                            info!("Completed stress test for stream: {}", stream_id);
                            break;
                        }
                        match Rc::clone(&semaphore).acquire_owned().await {
                            Ok(permit) => {
                                let stream_ = Rc::clone(&stream);
                                let buf_ = buf.clone();
                                let stats = Rc::clone(&stats);
                                tokio_uring::spawn(async move {
                                    if let Err(_) = stream_.append(buf_).await {
                                        stats.borrow_mut().inc_failure();
                                        error!("Failed to append");
                                    } else {
                                        stats.borrow_mut().inc_success();
                                    }
                                    drop(permit);
                                });
                            }
                            Err(_e) => break,
                        }
                    }
                })
            })
            .collect::<Vec<_>>();

        for handle in handles {
            let _ = handle.await;
        }

        Ok(())
    })
}

fn vec_bytes_to_bytes(vec_bytes: Vec<Bytes>) -> Bytes {
    let mut bytes_mut = BytesMut::new();
    for bytes in vec_bytes {
        bytes_mut.extend_from_slice(&bytes[..]);
    }
    bytes_mut.freeze()
}
