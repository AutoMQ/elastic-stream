use std::time::Instant;

use lazy_static::*;
use prometheus::{
    core::{Atomic, AtomicF64, AtomicU64},
    *,
};

#[derive(Debug)]
pub struct RangeServerStatistics {
    last_instant: Instant,
    network_append_old: u64,
    network_fetch_old: u64,
    network_failed_append_old: u64,
    network_failed_fetch_old: u64,
    network_append_rate: i16,
    network_fetch_rate: i16,
    network_failed_append_rate: i16,
    network_failed_fetch_rate: i16,
}

impl Default for RangeServerStatistics {
    fn default() -> Self {
        Self {
            last_instant: Instant::now(),
            network_append_old: 0,
            network_fetch_old: 0,
            network_failed_append_old: 0,
            network_failed_fetch_old: 0,
            network_append_rate: 0,
            network_fetch_rate: 0,
            network_failed_append_rate: 0,
            network_failed_fetch_rate: 0,
        }
    }
}

impl RangeServerStatistics {
    pub fn new() -> Self {
        Self::default()
    }

    /// The record() is responsible for capturing the current state of metrics,
    /// based on this data, it calculates the corresponding rates.
    pub fn record(&mut self) {
        let current_instant = Instant::now();
        let time_delta = current_instant
            .saturating_duration_since(self.last_instant)
            .as_millis() as u64
            / 1000;
        self.last_instant = current_instant;
        update_rate(
            &mut self.network_append_old,
            &mut self.network_append_rate,
            STORE_APPEND_COUNT.get(),
            time_delta,
        );
        update_rate(
            &mut self.network_fetch_old,
            &mut self.network_fetch_rate,
            STORE_FETCH_COUNT.get(),
            time_delta,
        );
        update_rate(
            &mut self.network_failed_append_old,
            &mut self.network_failed_append_rate,
            STORE_FAILED_APPEND_COUNT.get(),
            time_delta,
        );
        update_rate(
            &mut self.network_failed_fetch_old,
            &mut self.network_failed_fetch_rate,
            STORE_FAILED_FETCH_COUNT.get(),
            time_delta,
        );
    }
    /// The observe_append_latency() is responsible for recording a new append latency
    /// and calculating the average append latency over the past minute.
    /// It uses EWMA (Exponentially Weighted Moving Average) to determine the value.
    pub fn observe_append_latency(latency: i16) {
        let cur_observe_append_time = START_TIME.elapsed().as_millis() as u64;
        let last_observe_append_time = RANGE_SERVER_APPEND_OBSERVE_TIME.get();
        if last_observe_append_time == u64::MAX {
            RANGE_SERVER_APPEND_AVG_LATENCY.set(latency as f64);
            RANGE_SERVER_APPEND_OBSERVE_TIME.set(cur_observe_append_time);
        } else {
            let delta_observe_append_time = cur_observe_append_time - last_observe_append_time;
            const ONE_MINUTE: f64 = 1000.0 * 60.0;
            let w = f64::exp(-(delta_observe_append_time as f64) / ONE_MINUTE);
            let last_avg_append_latency = RANGE_SERVER_APPEND_AVG_LATENCY.get();
            let cur_avg_append_latency = last_avg_append_latency * w + (1.0 - w) * latency as f64;
            RANGE_SERVER_APPEND_AVG_LATENCY.set(cur_avg_append_latency);
            RANGE_SERVER_APPEND_OBSERVE_TIME.set(cur_observe_append_time);
        }
    }
    /// The observe_fetch_latency() is responsible for recording a new fetch latency
    /// and calculating the average fetch latency over the past minute.
    /// It uses EWMA (Exponentially Weighted Moving Average) to determine the value.
    pub fn observe_fetch_latency(latency: i16) {
        let cur_observe_fetch_time = START_TIME.elapsed().as_millis() as u64;
        let last_observe_fetch_time = RANGE_SERVER_FETCH_OBSERVE_TIME.get();
        if last_observe_fetch_time == u64::MAX {
            RANGE_SERVER_FETCH_AVG_LATENCY.set(latency as f64);
            RANGE_SERVER_FETCH_OBSERVE_TIME.set(cur_observe_fetch_time);
        } else {
            let delta_observe_fetch_time = cur_observe_fetch_time - last_observe_fetch_time;
            const ONE_MINUTE: f64 = 1000.0 * 60.0;
            let w = f64::exp(-(delta_observe_fetch_time as f64) / ONE_MINUTE);
            let last_avg_fetch_latency = RANGE_SERVER_FETCH_AVG_LATENCY.get();
            let cur_avg_fetch_latency = last_avg_fetch_latency * w + (1.0 - w) * latency as f64;
            RANGE_SERVER_FETCH_AVG_LATENCY.set(cur_avg_fetch_latency);
            RANGE_SERVER_FETCH_OBSERVE_TIME.set(cur_observe_fetch_time);
        }
    }
    pub fn get_network_append_rate(&self) -> i16 {
        self.network_append_rate
    }
    pub fn get_network_fetch_rate(&self) -> i16 {
        self.network_fetch_rate
    }
    pub fn get_network_failed_append_rate(&self) -> i16 {
        self.network_failed_append_rate
    }
    pub fn get_network_failed_fetch_rate(&self) -> i16 {
        self.network_failed_fetch_rate
    }
    pub fn get_network_append_avg_latency(&self) -> i16 {
        RANGE_SERVER_APPEND_AVG_LATENCY.get() as i16
    }
    pub fn get_network_fetch_avg_latency(&self) -> i16 {
        RANGE_SERVER_FETCH_AVG_LATENCY.get() as i16
    }
}

/// The update_rate() is used to calculate a new rate
/// based on the current metric, old metric, and time_delta.
fn update_rate(old_metric: &mut u64, rate: &mut i16, cur_metric: u64, time_delta: u64) {
    let metric_delta = cur_metric - *old_metric;
    if time_delta > 0 {
        *old_metric = cur_metric;
        *rate = (metric_delta / time_delta) as i16;
    }
}
lazy_static! {
    pub static ref START_TIME: Instant = Instant::now();
    pub static ref RANGE_SERVER_APPEND_AVG_LATENCY: AtomicF64 = AtomicF64::new(f64::MAX);
    pub static ref RANGE_SERVER_APPEND_OBSERVE_TIME: AtomicU64 = AtomicU64::new(u64::MAX);
    pub static ref RANGE_SERVER_FETCH_AVG_LATENCY: AtomicF64 = AtomicF64::new(f64::MAX);
    pub static ref RANGE_SERVER_FETCH_OBSERVE_TIME: AtomicU64 = AtomicU64::new(u64::MAX);
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

#[cfg(test)]
mod tests {
    use log::trace;
    use std::{thread::sleep, time::Duration};

    // use log::trace;

    use super::{
        RangeServerStatistics, STORE_APPEND_COUNT, STORE_FAILED_APPEND_COUNT,
        STORE_FAILED_FETCH_COUNT, STORE_FETCH_COUNT,
    };

    #[test]
    #[ignore = "Due to time jitter, it's hard to determine accuracy, so this test is just for observing the effect."]
    fn test_store_statistics() {
        let mut statistics = RangeServerStatistics::new();
        statistics.record();
        STORE_APPEND_COUNT.inc_by(1);
        STORE_FETCH_COUNT.inc_by(5);
        STORE_FAILED_APPEND_COUNT.inc_by(100);
        STORE_FAILED_FETCH_COUNT.inc_by(50);
        sleep(Duration::from_secs(2));
        statistics.record();
        trace!(
            "network_append_rate: {}, network_fetch_rate: {}, network_failed_append_rate: {}, get_network_failed_fetch_rate: {}",
            statistics.get_network_append_rate(),
            statistics.get_network_fetch_rate(),
            statistics.get_network_failed_append_rate(),
            statistics.get_network_failed_fetch_rate(),
        );
        RangeServerStatistics::observe_fetch_latency(5);
        sleep(Duration::from_secs(1));
        RangeServerStatistics::observe_fetch_latency(5);
        sleep(Duration::from_secs(1));
        RangeServerStatistics::observe_fetch_latency(5);
        sleep(Duration::from_secs(1));
        RangeServerStatistics::observe_fetch_latency(5);
        sleep(Duration::from_secs(1));
        RangeServerStatistics::observe_fetch_latency(5);
        sleep(Duration::from_secs(1));
        trace!("avg[0] = {}", statistics.get_network_fetch_avg_latency());
        RangeServerStatistics::observe_fetch_latency(1);
        trace!("avg[1]: {}", statistics.get_network_fetch_avg_latency());
        sleep(Duration::from_secs(1));
        RangeServerStatistics::observe_fetch_latency(1);
        trace!("avg[2]: {}", statistics.get_network_fetch_avg_latency());
        sleep(Duration::from_secs(1));
        RangeServerStatistics::observe_fetch_latency(1);
        trace!("avg[3]: {}", statistics.get_network_fetch_avg_latency());
        sleep(Duration::from_secs(1));
        RangeServerStatistics::observe_fetch_latency(1);
        trace!("avg[4]: {}", statistics.get_network_fetch_avg_latency());
        sleep(Duration::from_secs(1));
        RangeServerStatistics::observe_fetch_latency(1);
        trace!("avg[5]: {}", statistics.get_network_fetch_avg_latency());
    }
}
