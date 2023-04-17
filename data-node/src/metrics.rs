use lazy_static::*;
use prometheus::*;

lazy_static! {
    pub static ref APPEND_LATENCY: Histogram = register_histogram!(
        "append_latency",
        "Histogram of append record latency in microseconds",
        linear_buckets(0.0, 100.0, 100).unwrap()
    )
    .unwrap();
    pub static ref FETCH_LATENCY: Histogram = register_histogram!(
        "fetch_latency",
        "Histogram of fetch records latency in microseconds",
        linear_buckets(0.0, 100.0, 100).unwrap()
    )
    .unwrap();
}
