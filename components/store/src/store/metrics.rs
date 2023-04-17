use lazy_static::*;
use prometheus::*;

lazy_static! {
    pub static ref STORE_FETCH_COUNT: IntCounter =
        register_int_counter!("store_fetch_count", "the count of completed fetch task",).unwrap();
    pub static ref STORE_APPEND_COUNT: IntCounter =
        register_int_counter!("store_append_count", "the count of completed append task",).unwrap();
    pub static ref STORE_FETCH_BYTES_COUNT: IntCounter =
        register_int_counter!("store_fetch_bytes_count", "total number of bytes fetched",).unwrap();
    pub static ref STORE_APPEND_BYTES_COUNT: IntCounter =
        register_int_counter!("store_append_bytes_count", "total number of bytes appended",)
            .unwrap();
    pub static ref STORE_FAILED_FETCH_COUNT: IntCounter =
        register_int_counter!("store_failed_fetch_count", "the count of failed fetch task",)
            .unwrap();
    pub static ref STORE_FAILED_APPEND_COUNT: IntCounter = register_int_counter!(
        "store_failed_append_count",
        "the count of failed append task",
    )
    .unwrap();
    pub static ref STORE_FETCH_LATENCY_HISTOGRAM: Histogram = register_histogram!(
        "store_fetch_latency_histogram",
        "bucketed histogram of fetch duration, the unit is us",
        exponential_buckets(1.0, 1.5, 32).unwrap()
    )
    .unwrap();
    pub static ref STORE_APPEND_LATENCY_HISTOGRAM: Histogram = register_histogram!(
        "store_append_latency_histogram",
        "bucketed histogram of append duration, the unit is us",
        exponential_buckets(1.0, 1.5, 32).unwrap()
    )
    .unwrap();
}
