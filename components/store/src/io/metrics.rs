use lazy_static::*;
use prometheus::*;

lazy_static! {
    pub static ref COMPLETED_READ_IO : IntCounter = register_int_counter!(
        "completed_read_io",
        "Number of completed read IO",
    )
    .unwrap();

    pub static ref COMPLETED_WRITE_IO : IntCounter = register_int_counter!(
        "completed_write_io",
        "Number of completed write IO",
    )
    .unwrap();

    pub static ref INFLIGHT_IO: IntGauge = register_int_gauge!(
        "inflight_io",
        "Number of inflight io tasks"
    ).unwrap();

    pub static ref PENDING_TASK: IntGauge = register_int_gauge!(
        "pending_task",
        "Number of pending io-task"
    ).unwrap();

    pub static ref READ_IO_LATENCY: Histogram = register_histogram!(
        "read_io_latency",
        "Histogram of read IO latency in microseconds",
        linear_buckets(0.0, 100.0, 100).unwrap()
    )
    .unwrap();

    pub static ref WRITE_IO_LATENCY: Histogram = register_histogram!(
        "write_io_latency",
        "Histogram of write IO latency in microseconds",
        linear_buckets(0.0, 100.0, 100).unwrap()
    )
    .unwrap();

    pub static ref READ_BYTES_TOTAL : IntCounter = register_int_counter!(
        "read_bytes_total",
        "Total number of bytes read",
    )
    .unwrap();

    pub static ref WRITE_BYTES_TOTAL : IntCounter = register_int_counter!(
        "write_bytes_total",
        "Total number of bytes written by uring",
    )
    .unwrap();

    // dimensions for io_uring
    pub static ref IO_DEPTH: IntGauge = register_int_gauge!(
        "io_depth",
        "the io-depth of io_uring"
    ).unwrap();

}
