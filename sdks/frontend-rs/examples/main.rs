use std::time::Duration;

use bytes::{Bytes, BytesMut};
use frontend::{Frontend, StreamOptions};
use model::{record::flat_record::FlatRecordBatch, RecordBatch};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();
    tokio_uring::start(async {
        let frontend = Frontend::new("127.0.0.1:12378")?;
        let stream_id = frontend
            .create(StreamOptions {
                replica: 1,
                ack: 1,
                retention: Duration::from_secs(3600),
            })
            .await?;
        println!("Created stream with id: {}", stream_id);
        let stream = frontend.open(stream_id, 0).await?;

        let payload = Bytes::from("Hello World!");
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
        let append_result = stream
            .append(vec_bytes_to_bytes(flat_record_batch_bytes))
            .await?;
        println!("Append result: {:#?}", append_result);

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
