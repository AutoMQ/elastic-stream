use bytes::{Bytes, BytesMut};
use clap::{arg, Parser};
use frontend::{Frontend, StreamOptions};
use futures::future;
use log::info;
use model::{record::flat_record::FlatRecordBatch, RecordBatch};
use std::{error::Error, rc::Rc};
use tokio::time::Duration;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Number of streams to create and append records into
    #[arg(short, long, default_value_t = 16)]
    stream: usize,

    /// Number of iterations to append to each stream
    #[arg(short, long, default_value_t = 128)]
    iteration: usize,

    #[arg(short, long, default_value_t = 16)]
    batch_size: usize,

    #[arg(short, long, default_value_t = 1024)]
    payload_size: usize,
}

fn main() -> Result<(), Box<dyn Error + 'static>> {
    let args = Args::parse();
    frontend::init_log();
    tokio_uring::start(async {
        let frontend = Rc::new(Frontend::new("127.0.0.1:12378")?);
        let handles = (0..args.stream)
            .into_iter()
            .map(|_| {
                let frontend = Rc::clone(&frontend);
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
                    let stream = frontend.open(stream_id, 0).await.unwrap();
                    let mut payload = BytesMut::with_capacity(args.payload_size);
                    payload.resize(args.payload_size, b'x');
                    let payload = payload.freeze();
                    debug_assert_eq!(args.payload_size, payload.len());
                    for _ in 0..args.iteration {
                        let futures = (0..args.batch_size)
                            .map(|_| {
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
                                stream.append(vec_bytes_to_bytes(flat_record_batch_bytes))
                            })
                            .collect::<Vec<_>>();
                        future::join_all(futures).await;
                        info!("Appended {} record batches", args.batch_size);
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
