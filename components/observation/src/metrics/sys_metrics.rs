use std::time::Instant;

use sysinfo::{DiskExt, System, SystemExt};

#[derive(Debug)]
pub struct DiskStatistics {
    sys: System,
    last_instant: Instant,
    disk_in_old: i64,
    disk_out_old: i64,
    disk_in_rate: i64,
    disk_out_rate: i64,
}

impl Default for DiskStatistics {
    fn default() -> Self {
        Self {
            sys: System::new(),
            last_instant: Instant::now(),
            disk_in_old: 0,
            disk_out_old: 0,
            disk_in_rate: 0,
            disk_out_rate: 0,
        }
    }
}

impl DiskStatistics {
    pub fn new() -> Self {
        Self::default()
    }

    /// The record() is responsible for capturing the current state of metrics,
    /// based on this data, it calculates the corresponding rates,
    /// which indicate the speed at which these metrics are changing over time.
    pub fn record(&mut self) {
        self.sys.refresh_disks_list();
        self.sys.refresh_disks();
        let current_instant = Instant::now();
        let time_delta = current_instant
            .saturating_duration_since(self.last_instant)
            .as_millis() as i64
            / 1000;
        self.last_instant = current_instant;
        if let Ok(proc) = procfs::process::Process::myself() {
            if let Ok(io) = proc.io() {
                // proc.io() accesses the file '/proc/self/io' and retrieves information about IO.
                let read_bytes = io.read_bytes as i64;
                let write_bytes = io.write_bytes as i64;
                update_rate(
                    &mut self.disk_in_old,
                    &mut self.disk_in_rate,
                    read_bytes,
                    time_delta,
                );
                update_rate(
                    &mut self.disk_out_old,
                    &mut self.disk_out_rate,
                    write_bytes,
                    time_delta,
                );
            }
        }
    }
    pub fn get_disk_in_rate(&self) -> i64 {
        self.disk_in_rate
    }
    pub fn get_disk_out_rate(&self) -> i64 {
        self.disk_out_rate
    }
    /// The get_disk_free_space() will search through all disk info
    /// and calculate the total free space, in bytes
    pub fn get_disk_free_space(&self) -> i64 {
        self.sys
            .disks()
            .iter()
            .map(|d| -> i64 { d.available_space() as i64 })
            .sum::<i64>()
    }
}

#[derive(Debug)]
pub struct MemoryStatistics {
    sys: System,
}

impl Default for MemoryStatistics {
    fn default() -> Self {
        Self { sys: System::new() }
    }
}

impl MemoryStatistics {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record(&mut self) {
        self.sys.refresh_memory();
    }
    /// The get_memory_used() returns the amount of used RAM, in bytes.
    pub fn get_memory_used(&self) -> i64 {
        self.sys.used_memory() as i64
    }
}
/// The update_rate() is used to calculate a new rate
/// based on the current metric, old metric, and time_delta.
fn update_rate(old_metric: &mut i64, rate: &mut i64, cur_metric: i64, time_delta: i64) {
    let metric_delta = cur_metric - *old_metric;
    if time_delta > 0 {
        *old_metric = cur_metric;
        *rate = metric_delta / time_delta;
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs::{self, File},
        io::{BufWriter, Write},
        path::Path,
        time::Instant,
    };

    use log::trace;

    use crate::metrics::sys_metrics::{DiskStatistics, MemoryStatistics};

    fn write_one_gb() {
        let path = Path::new("/tmp/test_data");
        // create a new file for writing
        let file = File::create(path).unwrap();
        let mut writer = BufWriter::new(file);

        // write 1GB of random data to the file
        let one_gb: usize = 1024 * 1024 * 1024;
        let mut buf = vec![0; one_gb];
        writer.write_all(&mut buf).unwrap();
        writer.flush().unwrap();
        match fs::remove_file(path) {
            Ok(_) => trace!("File successfully removed."),
            Err(e) => trace!("Failed to remove file: {}", e),
        }
    }
    #[test]
    #[ignore = "This test is just for observing the effect."]
    fn test_statistics() {
        let mut disk_statistics = DiskStatistics::new();
        disk_statistics.record();
        trace!(
            "disk_free_space: {} GB",
            disk_statistics.get_disk_free_space() / 1024 / 1024 / 1024
        );
        trace!(
            "Before write 1GB, in_rate: {}MB, out_rate: {}MB",
            disk_statistics.get_disk_in_rate(),
            disk_statistics.get_disk_out_rate()
        );
        // To simulate write 1GB file
        let x = Instant::now();
        write_one_gb();
        let x = x.elapsed().as_secs();
        trace!("write_time: {} secs", x);
        disk_statistics.record();
        trace!(
            "After write 1GB, in_rate: {}MB, out_rate: {}MB",
            disk_statistics.get_disk_in_rate() / 1024 / 1024,
            disk_statistics.get_disk_out_rate() / 1024 / 1024
        );

        let mut mem_statistics = MemoryStatistics::new();
        mem_statistics.record();
        trace!(
            "Before alloc 1 GB memory, memory_used: {} MB",
            mem_statistics.get_memory_used() / 1024 / 1024
        );
        // To simulate using 1GB of RAM.
        let one_gb = 1024 * 1024 * 1024;
        let mut v: Vec<i32> = Vec::with_capacity(one_gb / std::mem::size_of::<i32>());
        for i in 0..v.capacity() {
            v.push(i as i32);
        }
        mem_statistics.record();
        trace!(
            "After alloc 1 GB memory: {} MB",
            mem_statistics.get_memory_used() / 1024 / 1024
        );
    }
}
