use std::time::Instant;

use lazy_static::*;
use prometheus::{
    core::{Atomic, AtomicF64, AtomicU64},
    *,
};
pub struct UringStatistics {
    last_instant: Instant,
    uring_task_old: i16,
    uring_task_rate: i16,
}

impl Default for UringStatistics {
    fn default() -> Self {
        Self {
            last_instant: Instant::now(),
            uring_task_old: 0,
            uring_task_rate: 0,
        }
    }
}

impl UringStatistics {
    pub fn new() -> Self {
        Self::default()
    }
    /// The record() is responsible for capturing the current state of metrics,
    /// based on this data, it calculates the corresponding rates.
    pub fn record(&mut self) {
        let current_instant = Instant::now();
        // time_delta represents the time interval
        // between the last record and the current one, in seconds.
        let time_delta = current_instant
            .saturating_duration_since(self.last_instant)
            .as_millis() as i64
            / 1000;
        self.last_instant = current_instant;
        update_rate(
            &mut self.uring_task_old,
            &mut self.uring_task_rate,
            COMPLETED_READ_IO.get() as i16 + COMPLETED_WRITE_IO.get() as i16,
            time_delta,
        );
    }
    /// The observe_latency() is responsible for recording a new latency
    /// and calculating the average latency over the past minute.
    /// It uses EWMA (Exponentially Weighted Moving Average) to determine the value.
    pub fn observe_latency(latency: i16) {
        let cur_observe_time = START_TIME.elapsed().as_millis() as u64;
        let last_observe_time = URING_OBSERVE_TIME.get();
        if last_observe_time == u64::MAX {
            URING_AVG_LATENCY.set(latency as f64);
            URING_OBSERVE_TIME.set(cur_observe_time);
        } else {
            let delta_observe_time = cur_observe_time - last_observe_time;
            const ONE_MINUTE: f64 = 1000.0 * 60.0;
            let w = f64::exp(-(delta_observe_time as f64) / ONE_MINUTE);
            let last_avg_latency = URING_AVG_LATENCY.get();

            // Use the latest recorded latency and the previous average latency to determine the "cur_avg_latency"
            let cur_avg_latency = last_avg_latency * w + (1.0 - w) * latency as f64;
            URING_AVG_LATENCY.set(cur_avg_latency);
            URING_OBSERVE_TIME.set(cur_observe_time);
        }
    }
    pub fn get_uring_task_rate(&self) -> i16 {
        self.uring_task_rate
    }
    pub fn get_uring_inflight_task_cnt(&self) -> i16 {
        INFLIGHT_IO.get() as i16
    }
    pub fn get_uring_pending_task_cnt(&self) -> i32 {
        PENDING_TASK.get() as i32
    }
    pub fn get_uring_task_avg_latency(&self) -> i16 {
        URING_AVG_LATENCY.get() as i16
    }
}

/// The update_rate() is used to calculate a new rate
/// based on the current metric, old metric, and time_delta.
fn update_rate(old_metric: &mut i16, rate: &mut i16, cur_metric: i16, time_delta: i64) {
    let metric_delta = cur_metric - *old_metric;
    if time_delta > 0 {
        *old_metric = cur_metric;
        *rate = metric_delta / time_delta as i16;
    }
}

lazy_static! {
    pub static ref START_TIME: Instant = Instant::now();
    // URING_AVG_LATENCY represents the average latency of the uring task, in milliseconds.
    pub static ref URING_AVG_LATENCY: AtomicF64 = AtomicF64::new(f64::MAX);
    // URING_OBSERVE_TIME represents the timestamp of the last time we observed the latency, in milliseconds
    // It is necessary to implement EWMA, which helps determine the latest average latency
    pub static ref URING_OBSERVE_TIME: AtomicU64 = AtomicU64::new(u64::MAX);

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

#[cfg(test)]
mod tests {
    use std::{thread::sleep, time::Duration};

    use log::trace;

    use crate::metrics::uring_metrics::{UringStatistics, COMPLETED_READ_IO, COMPLETED_WRITE_IO};

    #[test]
    #[ignore = "This test is just for observing the effect."]
    fn test_uring_statistics() {
        let mut statistics = UringStatistics::new();
        statistics.record();
        COMPLETED_WRITE_IO.inc_by(100);
        COMPLETED_READ_IO.inc_by(50);
        sleep(Duration::from_secs(2));
        statistics.record();
        assert_eq!((100 + 50) / 2, statistics.get_uring_task_rate());
        UringStatistics::observe_latency(5);
        assert_eq!(5, statistics.get_uring_task_avg_latency());
        sleep(Duration::from_secs(1));
        UringStatistics::observe_latency(5);
        sleep(Duration::from_secs(1));
        UringStatistics::observe_latency(5);
        sleep(Duration::from_secs(1));
        UringStatistics::observe_latency(5);
        sleep(Duration::from_secs(1));
        UringStatistics::observe_latency(5);
        sleep(Duration::from_secs(1));
        trace!("avg[0] = {}", statistics.get_uring_task_avg_latency());
        UringStatistics::observe_latency(1);
        trace!("avg[1] = {}", statistics.get_uring_task_avg_latency());
        sleep(Duration::from_secs(1));
        UringStatistics::observe_latency(1);
        trace!("avg[2] = {}", statistics.get_uring_task_avg_latency());
        sleep(Duration::from_secs(1));
        UringStatistics::observe_latency(1);
        trace!("avg[3] = {}", statistics.get_uring_task_avg_latency());
        sleep(Duration::from_secs(1));
        UringStatistics::observe_latency(1);
        trace!("avg[4] = {}", statistics.get_uring_task_avg_latency());
        sleep(Duration::from_secs(1));
        UringStatistics::observe_latency(1);
        trace!("avg[5] = {}", statistics.get_uring_task_avg_latency());
    }
}
