use bytes::{Bytes, BytesMut};
use frontend::{Frontend, StreamOptions};
use log::info;
use model::{record::flat_record::FlatRecordBatch, RecordBatch};
use std::error::Error;
use tokio::time::Duration;

fn main() -> Result<(), Box<dyn Error + 'static>> {
    frontend::init_log();
    tokio_uring::start(async {
        let frontend = Frontend::new("127.0.0.1:12378")?;
        let stream_id = frontend
            .create(StreamOptions {
                replica: 1,
                ack: 1,
                retention: Duration::from_secs(3600),
            })
            .await?;
        info!("Created stream with id: {}", stream_id);
        let stream = frontend.open(stream_id, 0).await?;

        info!("Step1: append 10-record batch");
        let futures = (0..256)
            .map(|i| {
                let payload = Bytes::from(format!("Hello World {i:0>8}!"));
                let record_batch = RecordBatch::new_builder()
                    .with_stream_id(stream_id as i64)
                    .with_range_index(0)
                    .with_base_offset(0)
                    .with_last_offset_delta(10)
                    .with_payload(payload)
                    .build()
                    .unwrap();
                let flat_record_batch: FlatRecordBatch = Into::into(record_batch);
                let (flat_record_batch_bytes, _) = flat_record_batch.encode();
                stream.append(vec_bytes_to_bytes(flat_record_batch_bytes))
            })
            .collect::<Vec<_>>();
        futures::future::join_all(futures).await;
        println!("It works");
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
