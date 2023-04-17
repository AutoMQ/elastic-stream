use lazy_static::*;
use prometheus::*;

lazy_static! {
    pub static ref DATA_NODE_APPEND_LATENCY_HISTOGRAM: Histogram = register_histogram!(
        "uring_read_task_latency_histogram",
        "bucketed histogram of read duration, the unit is us",
        linear_buckets(0.0, 500.0, 100).unwrap()
    )
    .unwrap();
    pub static ref DATA_NODE_FETCH_LATENCY_HISTOGRAM: Histogram = register_histogram!(
        "uring_write_task_latency_histogram",
        "bucketed histogram of write duration, the unit is us",
        linear_buckets(0.0, 500.0, 100).unwrap()
    )
    .unwrap();
}
