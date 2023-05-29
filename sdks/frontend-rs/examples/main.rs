use bytes::{Bytes, BytesMut};
use frontend::{Frontend, StreamOptions};
use log::{info, LevelFilter};
use model::{record::flat_record::FlatRecordBatch, RecordBatch};
use std::io::Write;
use tokio::time::{sleep, Duration};

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

        info!("Step1: append 10-record batch");
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

        info!("Step2: read 10 record batch one by one");
        for i in 0..10 {
            let start = i * 10;
            let end = i * 10 + 10;
            let mut bytes = vec_bytes_to_bytes(stream.read(start, end, i32::MAX).await?);
            let records = decode_flat_record_batch(&mut bytes)?;
            assert_eq!(1, records.len());
            let record = records.iter().next().unwrap();
            assert_eq!(i * 10, record.base_offset());
            assert_eq!(
                record.payload(),
                Bytes::from(format!("Hello World {i:0>8}!", i = i))
            );
            info!("Fetch [{start}, {end}) result: {:#?}", record);
        }

        info!("Step3: cross read 10 record batch");
        for i in 0..9 {
            let start = i * 10 + 1; // in middle of first record batch
            let end = i * 10 + 11; // in middle of next record batch
            let mut bytes = vec_bytes_to_bytes(stream.read(start, end, i32::MAX).await?);
            let records = decode_flat_record_batch(&mut bytes)?;
            // expect to read 2 record batches
            assert_eq!(2, records.len());
            for j in 0..2 as i64 {
                let record = &records[j as usize];
                assert_eq!((i + j) * 10, record.base_offset());
                assert_eq!(
                    record.payload(),
                    Bytes::from(format!("Hello World {i:0>8}!", i = i + j))
                );
            }
            info!("Fetch [{start}, {end}) result: {:#?}", records);
        }

        info!("Step4: reopen stream");
        drop(stream);
        // await stream close
        sleep(Duration::from_secs(1)).await;
        let stream = frontend.open(stream_id, 0).await?;

        info!("Step5: append more 10-record batches");
        for i in 0..10 {
            let offset = i + 10;
            let payload = Bytes::from(format!("Hello World {offset:0>8}!"));
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
            assert_eq!(offset * 10, append_result.base_offset as i32);
            info!("Append result: {:#?}", append_result);
        }

        info!("Step6: read 20 record batch one by one");
        for i in 0..20 {
            let start = i;
            let end = i + 10;
            let mut bytes = vec_bytes_to_bytes(stream.read(start, end, i32::MAX).await?);
            let records = decode_flat_record_batch(&mut bytes)?;
            assert_eq!(1, records.len());
            let record = records.iter().next().unwrap();
            assert_eq!(i * 10, record.base_offset());
            assert_eq!(
                record.payload(),
                Bytes::from(format!("Hello World {i:0>8}!", i = i))
            );
            info!("Fetch [{start}, {end}) result: {:#?}", record);
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
