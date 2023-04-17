use lazy_static::*;
use prometheus::*;

lazy_static! {
    pub static ref URING_READ_TASK_COUNT : IntCounter = register_int_counter!(
        "uring_read_task_count",
        "the count of completed read task",
    )
    .unwrap();

    pub static ref URING_WRITE_TASK_COUNT : IntCounter = register_int_counter!(
        "uring_write_task_count",
        "the count of completed write task",
    )
    .unwrap();

    pub static ref URING_INFLIGHT_TASK_GAUGE: IntGauge = register_int_gauge!(
        "uring_inflight_task_gauge",
        "count of inflight io-task, same as the value of io.inflight"
    ).unwrap();

    pub static ref URING_PENDING_TASK_GAUGE: IntGauge = register_int_gauge!(
        "uring_pending_task_gauge",
        "count of pending io-task, same as the lenght of io.pending_data_tasks"
    ).unwrap();

    pub static ref URING_READ_TASK_LATENCY_HISTOGRAM: Histogram = register_histogram!(
        "uring_read_task_latency_histogram",
        "bucketed histogram of read duration, the unit is us",
        linear_buckets(0.0, 500.0, 100).unwrap()
    )
    .unwrap();

    pub static ref URING_WRITE_TASK_LATENCY_HISTOGRAM: Histogram = register_histogram!(
        "uring_write_task_latency_histogram",
        "bucketed histogram of write duration, the unit is us",
        linear_buckets(0.0, 500.0, 100).unwrap()
    )
    .unwrap();

    pub static ref URING_READ_BYTES_COUNT : IntCounter = register_int_counter!(
        "uring_read_bytes_count",
        "total number of bytes read by uring",
    )
    .unwrap();

    pub static ref URING_WRITE_BYTES_COUNT : IntCounter = register_int_counter!(
        "uring_write_bytes_count",
        "total number of bytes written by uring",
    )
    .unwrap();

    // dimensions for io_uring
    pub static ref URING_IO_DEPTH: IntGauge = register_int_gauge!(
        "uring_io_depth",
        "the io-depth of uring"
    ).unwrap();

}
