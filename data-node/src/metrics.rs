use lazy_static::*;
use prometheus::*;

lazy_static! {
    pub static ref DATA_NODE_APPEND_LATENCY_HISTOGRAM: Histogram = register_histogram!(
        "data_node_append_latency_histogram",
        "bucketed histogram of append duration, the unit is us",
        linear_buckets(0.0, 500.0, 100).unwrap()
    )
    .unwrap();
    pub static ref DATA_NODE_FETCH_LATENCY_HISTOGRAM: Histogram = register_histogram!(
        "data_node_fetch_latency_histogram",
        "bucketed histogram of fetch duration, the unit is us",
        linear_buckets(0.0, 500.0, 100).unwrap()
    )
    .unwrap();
}
