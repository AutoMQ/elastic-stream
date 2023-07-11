#![feature(async_fn_in_trait)]

use bytes::BytesMut;
use config::ObjectStorageConfig;
use object_storage::range_fetcher::RangeFetchResult;
use object_storage::range_fetcher::RangeFetcher;
use std::env;
use std::time::Duration;
use store::error::FetchError;

use clap::Parser;
use object_storage::object_manager::MemoryObjectManager;
use object_storage::object_storage::DefaultObjectStorage;
use object_storage::ObjectStorage;
use tokio::time::sleep;

fn main() {
    env::set_var("RUST_LOG", "object_storage=debug");
    env_logger::init();
    let args = Args::parse();
    tokio_uring::start(async move {
        let config = ObjectStorageConfig {
            endpoint: args.endpoint,
            bucket: args.bucket,
            region: args.region,
            object_size: args.object_size,
            part_size: args.part_size,
            max_cache_size: args.max_cache_size,
            cache_low_watermark: args.cache_low_watermark,
            force_flush_secs: 60 * 60,
        };
        let range_fetcher = RangeFetcherMock {};
        let memory_object_manager: MemoryObjectManager = Default::default();
        let object_manager = memory_object_manager;
        let object_store = DefaultObjectStorage::new(&config, range_fetcher, object_manager);
        let mut end_offset = 1;
        loop {
            object_store.new_commit(1, 2, 128 * 1024);
            end_offset = end_offset + 1;
            sleep(Duration::from_millis(args.send_interval_millis)).await;
        }
    });
    return;
}

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct Args {
    #[arg(short, long, default_value_t = String::from("https://s3.amazonaws.com"))]
    pub endpoint: String,

    #[arg(short, long)]
    pub bucket: String,

    #[arg(short, long)]
    pub region: String,

    #[arg(short, long, default_value_t = 67108864)]
    pub object_size: u32,

    #[arg(short, long, default_value_t = 8388608)]
    pub part_size: u32,

    #[arg(short, long, default_value_t = 1073741824)]
    pub max_cache_size: u64,

    #[arg(short, long, default_value_t = 536870912)]
    pub cache_low_watermark: u64,

    #[arg(short, long, default_value_t = 10)]
    pub send_interval_millis: u64,
}

struct RangeFetcherMock;

impl RangeFetcher for RangeFetcherMock {
    async fn fetch(
        &self,
        _stream_id: u64,
        _range_index: u32,
        start_offset: u64,
        end_offset: u64,
        _max_size: u32,
    ) -> Result<RangeFetchResult, FetchError> {
        let count = end_offset - start_offset;
        let mut v = Vec::with_capacity(count as usize);
        for _ in 0..count {
            let bytes = BytesMut::zeroed(128 * 1024);
            let bytes = bytes.freeze();
            v.push(bytes);
        }
        Ok(RangeFetchResult { payload: v })
    }
}
