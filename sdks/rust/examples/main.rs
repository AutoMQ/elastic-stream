use bytes::Bytes;
use frontend::{Frontend, Stream, StreamOptions};
use model::{record::flat_record::FlatRecordBatch, RecordBatch};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let frontend = Frontend::new("127.0.0.1:12378")?;
    let stream_id = frontend
        .create(StreamOptions {
            replica: 1,
            ack: 1,
            retention: 100000000,
        })
        .await?;
    println!("Created stream with id: {}", stream_id);
    let stream = frontend.open(stream_id, 0).await?;

    let payload = Bytes::from("Hello World!");
    let record_batch = RecordBatch::new_builder()
        .with_stream_id(stream_id)
        .with_last_offset_delta(10)
        .with_payload(payload)
        .build()
        .unwrap();
    let flat_record_batch: FlatRecordBatch = Into::into(record_batch);
    let (flat_record_batch_bytes, _) = flat_record_batch.encode();
    let append_result = stream.append(flat_record_batch_bytes).await?;
    println!("Append result: {:#?}", append_result);

    Ok(())
}
