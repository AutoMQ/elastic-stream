use std::time::Duration;

use bytes::{Bytes, BytesMut};
use frontend::{Frontend, StreamOptions};
use log::{info, LevelFilter};
use model::{record::flat_record::FlatRecordBatch, RecordBatch};
use std::io::Write;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::Builder::new()
        .format(|buf, record| {
            writeln!(
                buf,
                "{}:{} {} [{}] - {}",
                record.file().unwrap_or("unknown"),
                record.line().unwrap_or(0),
                chrono::Local::now().format("%Y-%m-%dT%H:%M:%S"),
                record.level(),
                record.args()
            )
        })
        .filter_level(LevelFilter::Trace)
        .init();
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

        // append 10 record batch
        for i in 0..10 {
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
            let append_result = stream
                .append(vec_bytes_to_bytes(flat_record_batch_bytes))
                .await?;
            assert_eq!(i * 10, append_result.base_offset as i32);
            info!("Append result: {:#?}", append_result);
        }

        // fetch and check
        // single read
        for i in 0..10 {
            let start = i;
            let end = i + 10;
            let mut bytes = vec_bytes_to_bytes(stream.read(start, end, i32::MAX).await?);
            let records = decode_flat_record_batch(&mut bytes)?;
            assert_eq!(1, records.len());
            let record = records.iter().next().unwrap();
            assert_eq!(
                record.payload(),
                Bytes::from(format!("Hello World {i:0>8}!", i = i))
            );
            info!("Fetch [{start}, {end}) result: {:#?}", record);
        }

        // crosss read

        // reopen the same stream
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

fn decode_flat_record_batch(
    bytes: &mut Bytes,
) -> Result<Vec<RecordBatch>, Box<dyn std::error::Error>> {
    let mut records = vec![];
    loop {
        if bytes.is_empty() {
            break;
        }
        let flat_record_batch = FlatRecordBatch::init_from_buf(bytes)?;
        let record_batch = flat_record_batch.decode()?;
        records.push(record_batch);
    }
    Ok(records)
}
