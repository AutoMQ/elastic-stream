use bytes::{Bytes, BytesMut};
use frontend::{Frontend, StreamOptions};
use log::info;
use model::{record::flat_record::FlatRecordBatch, RecordBatch};
use rand::RngCore;
use tokio::time::Duration;

fn main() -> Result<(), Box<dyn std::error::Error>> {
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

        // append 10-record batch
        let mut payload = [0u8; 1 * 1024 * 1024];
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

        for _ in 0..10 {
            append(&stream, flat_record_batch_bytes.clone()).await?;
        }

        Ok(())
    })
}

async fn append(stream: &frontend::Stream, bytes: Bytes) -> Result<(), Box<dyn std::error::Error>> {
    let start = std::time::Instant::now();
    let append_result = stream.append(bytes).await?;
    info!(
        "Append result: {:?}, used {:?}",
        append_result,
        start.elapsed()
    );
    Ok(())
}

fn vec_bytes_to_bytes(vec_bytes: Vec<Bytes>) -> Bytes {
    let mut bytes_mut = BytesMut::new();
    for bytes in vec_bytes {
        bytes_mut.extend_from_slice(&bytes[..]);
    }
    bytes_mut.freeze()
}
