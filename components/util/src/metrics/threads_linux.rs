// Copyright 2017 TiKV Project Authors. Licensed under Apache-2.0.

use std::{
    io::{Error, ErrorKind, Result},
    sync::Mutex,
    time::Duration,
    time::Instant,
};

use prometheus::{
    self,
    core::{Collector, Desc},
    proto, GaugeVec, IntGaugeVec, Opts,
};

// use super::thread::{self, Pid, THREAD_NAME_HASHMAP};
use crate::metrics::threads_linux::thread::{Pid, THREAD_NAME_HASHMAP};
/// Monitors threads of the current process.
pub fn monitor_threads<S: Into<String>>(namespace: S) -> Result<()> {
    let pid = thread::process_id();
    let tc = ThreadsCollector::new(pid, namespace);
    prometheus::register(Box::new(tc)).map_err(|e| to_io_err(format!("{:?}", e)))
}

struct Metrics {
    cpu_totals: GaugeVec,
    io_totals: GaugeVec,
    threads_state: IntGaugeVec,
    voluntary_ctxt_switches: IntGaugeVec,
    nonvoluntary_ctxt_switches: IntGaugeVec,
}

impl Metrics {
    fn new<S: Into<String>>(namespace: S) -> Metrics {
        let ns = namespace.into();
        let cpu_totals = GaugeVec::new(
            Opts::new(
                "thread_cpu_seconds_total",
                "Total user and system CPU time spent in \
                 seconds by threads.",
            )
            .namespace(ns.clone()),
            &["name", "tid"],
        )
        .unwrap();
        let threads_state = IntGaugeVec::new(
            Opts::new("threads_state", "Number of threads in each state.").namespace(ns.clone()),
            &["state"],
        )
        .unwrap();
        let io_totals = GaugeVec::new(
            Opts::new(
                "threads_io_bytes_total",
                "Total number of bytes which threads cause to be fetched from or sent to the storage layer.",
            ).namespace(ns.clone()),
            &["name", "tid", "io"],
        )
        .unwrap();
        let voluntary_ctxt_switches = IntGaugeVec::new(
            Opts::new(
                "thread_voluntary_context_switches",
                "Number of thread voluntary context switches.",
            )
            .namespace(ns.clone()),
            &["name", "tid"],
        )
        .unwrap();
        let nonvoluntary_ctxt_switches = IntGaugeVec::new(
            Opts::new(
                "thread_nonvoluntary_context_switches",
                "Number of thread nonvoluntary context switches.",
            )
            .namespace(ns),
            &["name", "tid"],
        )
        .unwrap();

        Metrics {
            cpu_totals,
            io_totals,
            threads_state,
            voluntary_ctxt_switches,
            nonvoluntary_ctxt_switches,
        }
    }

    fn descs(&self) -> Vec<Desc> {
        let mut descs: Vec<Desc> = vec![];
        descs.extend(self.cpu_totals.desc().into_iter().cloned());
        descs.extend(self.threads_state.desc().into_iter().cloned());
        descs.extend(self.io_totals.desc().into_iter().cloned());
        descs.extend(self.voluntary_ctxt_switches.desc().into_iter().cloned());
        descs.extend(self.nonvoluntary_ctxt_switches.desc().into_iter().cloned());
        descs
    }

    fn reset(&mut self) {
        self.cpu_totals.reset();
        self.threads_state.reset();
        self.io_totals.reset();
        self.voluntary_ctxt_switches.reset();
        self.nonvoluntary_ctxt_switches.reset();
    }
}

/// A collector to collect threads metrics, including CPU usage
/// and threads state.
struct ThreadsCollector {
    pid: Pid,
    descs: Vec<Desc>,
    metrics: Mutex<Metrics>,
    tid_retriever: Mutex<TidRetriever>,
}

impl ThreadsCollector {
    fn new<S: Into<String>>(pid: Pid, namespace: S) -> ThreadsCollector {
        let metrics = Metrics::new(namespace);
        ThreadsCollector {
            pid,
            descs: metrics.descs(),
            metrics: Mutex::new(metrics),
            tid_retriever: Mutex::new(TidRetriever::new(pid)),
        }
    }
}

impl Collector for ThreadsCollector {
    fn desc(&self) -> Vec<&Desc> {
        self.descs.iter().collect()
    }

    fn collect(&self) -> Vec<proto::MetricFamily> {
        // Synchronous collecting metrics.
        let mut metrics = self.metrics.lock().unwrap();
        // Clean previous threads state.
        metrics.threads_state.reset();

        let mut tid_retriever = self.tid_retriever.lock().unwrap();
        let (tids, updated) = tid_retriever.get_tids();
        if updated {
            metrics.reset();
        }
        for tid in tids {
            let tid = *tid;
            if let Ok(stat) = thread::full_thread_stat(self.pid, tid) {
                // Threads CPU time.
                let total = thread::linux::cpu_total(&stat);
                // sanitize thread name before push metrics.
                let name = if let Some(thread_name) = THREAD_NAME_HASHMAP.lock().unwrap().get(&tid)
                {
                    sanitize_thread_name(tid, thread_name)
                } else {
                    sanitize_thread_name(tid, &stat.comm)
                };
                let cpu_total = metrics
                    .cpu_totals
                    .get_metric_with_label_values(&[&name, &format!("{}", tid)])
                    .unwrap();
                cpu_total.set(total);

                // Threads states.
                let state = metrics
                    .threads_state
                    .get_metric_with_label_values(&[stat.state.to_string().as_str()])
                    .unwrap();
                state.inc();
                if let Ok(proc) = procfs::process::Process::new(tid) {
                    if let Ok(io) = proc.io() {
                        let read_bytes = io.read_bytes;
                        let write_bytes = io.write_bytes;
                        // Threads IO.
                        let read_total = metrics
                            .io_totals
                            .get_metric_with_label_values(&[&name, &format!("{}", tid), "read"])
                            .unwrap();
                        read_total.set(read_bytes as f64);

                        let write_total = metrics
                            .io_totals
                            .get_metric_with_label_values(&[&name, &format!("{}", tid), "write"])
                            .unwrap();
                        write_total.set(write_bytes as f64);
                    }
                }

                if let Ok(status) = procfs::process::Process::new(tid).unwrap().status() {
                    // Thread voluntary context switches.
                    if let Some(voluntary_ctxt_switches) = status.voluntary_ctxt_switches {
                        let voluntary_total = metrics
                            .voluntary_ctxt_switches
                            .get_metric_with_label_values(&[&name, &format!("{}", tid)])
                            .unwrap();
                        voluntary_total.set(voluntary_ctxt_switches as i64);
                    }
                    // Thread nonvoluntary context switches.
                    if let Some(nonvoluntary_ctxt_switches) = status.nonvoluntary_ctxt_switches {
                        let nonvoluntary_total = metrics
                            .nonvoluntary_ctxt_switches
                            .get_metric_with_label_values(&[&name, &format!("{}", tid)])
                            .unwrap();
                        nonvoluntary_total.set(nonvoluntary_ctxt_switches as i64);
                    }
                }
            }
        }
        let mut mfs = metrics.cpu_totals.collect();
        mfs.extend(metrics.threads_state.collect());
        mfs.extend(metrics.io_totals.collect());
        mfs.extend(metrics.voluntary_ctxt_switches.collect());
        mfs.extend(metrics.nonvoluntary_ctxt_switches.collect());
        mfs
    }
}

/// Sanitizes the thread name. Keeps `a-zA-Z0-9_:`, replaces `-` and ` ` with
/// `_`, and drops the others.
///
/// Examples:
///
/// ```ignore
/// assert_eq!(sanitize_thread_name(0, "ok123"), "ok123");
/// assert_eq!(sanitize_thread_name(0, "Az_1"), "Az_1");
/// assert_eq!(sanitize_thread_name(0, "a-b"), "a_b");
/// assert_eq!(sanitize_thread_name(0, "a b"), "a_b");
/// assert_eq!(sanitize_thread_name(1, "@123"), "123");
/// assert_eq!(sanitize_thread_name(1, "@@@@"), "1");
/// ```
fn sanitize_thread_name(tid: Pid, raw: &str) -> String {
    let mut name = String::with_capacity(raw.len());
    // sanitize thread name.
    for c in raw.chars() {
        match c {
            // Prometheus label characters `[a-zA-Z0-9_:]`
            'a'..='z' | 'A'..='Z' | '0'..='9' | '_' | ':' => {
                name.push(c);
            }
            '-' | ' ' => {
                name.push('_');
            }
            _ => (),
        }
    }
    if name.is_empty() {
        name = format!("{}", tid)
    }
    name
}

fn to_io_err(s: String) -> Error {
    Error::new(ErrorKind::Other, s)
}

const TID_MIN_UPDATE_INTERVAL: Duration = Duration::from_micros(1);
const TID_MAX_UPDATE_INTERVAL: Duration = Duration::from_micros(10);
// const TID_MIN_UPDATE_INTERVAL: Duration = Duration::from_secs(15);
// const TID_MAX_UPDATE_INTERVAL: Duration = Duration::from_secs(10 * 60);

/// A helper that buffers the thread id list internally.
struct TidRetriever {
    pid: Pid,
    tid_buffer: Vec<i32>,
    tid_buffer_last_update: Instant,
    tid_buffer_update_interval: Duration,
}

impl TidRetriever {
    pub fn new(pid: Pid) -> Self {
        let mut tid_buffer: Vec<_> = thread::thread_ids(pid).unwrap();
        tid_buffer.sort_unstable();
        Self {
            pid,
            tid_buffer,
            tid_buffer_last_update: Instant::now(),
            tid_buffer_update_interval: TID_MIN_UPDATE_INTERVAL,
        }
    }

    pub fn get_tids(&mut self) -> (&[Pid], bool) {
        // Update the tid list according to tid_buffer_update_interval.
        // If tid is not changed, update the tid list less frequently.
        let mut updated = false;
        // self.tid_buffer_last_update.saturating_elapsed()
        if self.tid_buffer_last_update.elapsed() >= self.tid_buffer_update_interval {
            let mut new_tid_buffer: Vec<_> = thread::thread_ids(self.pid).unwrap();
            new_tid_buffer.sort_unstable();
            if new_tid_buffer == self.tid_buffer {
                self.tid_buffer_update_interval *= 2;
                if self.tid_buffer_update_interval > TID_MAX_UPDATE_INTERVAL {
                    self.tid_buffer_update_interval = TID_MAX_UPDATE_INTERVAL;
                }
            } else {
                self.tid_buffer = new_tid_buffer;
                self.tid_buffer_update_interval = TID_MIN_UPDATE_INTERVAL;
                updated = true;
            }
            self.tid_buffer_last_update = Instant::now();
        }

        (&self.tid_buffer, updated)
    }
}

pub mod thread {
    use std::sync::Mutex;

    use std::collections::HashMap;

    #[inline]
    fn cpu_total(sys_time: u64, user_time: u64) -> f64 {
        (sys_time + user_time) as f64 / ticks_per_second() as f64
    }

    #[cfg(target_os = "linux")]
    pub mod linux {
        use procfs::process::Stat;

        #[inline]
        pub fn cpu_total(stat: &Stat) -> f64 {
            super::cpu_total(stat.stime, stat.utime)
        }
    }

    #[cfg(target_os = "linux")]
    mod imp {
        use std::{
            fs,
            io::{self},
            iter::FromIterator,
        };

        pub use libc::pid_t as Pid;
        use procfs::{process::Stat, ProcResult};

        lazy_static::lazy_static! {
            // getconf CLK_TCK
            static ref CLOCK_TICK: i64 = {
                unsafe {
                    libc::sysconf(libc::_SC_CLK_TCK)
                }
            };

            static ref PROCESS_ID: Pid = unsafe { libc::getpid() };
        }

        #[inline]
        pub fn ticks_per_second() -> i64 {
            *CLOCK_TICK
        }

        /// Gets the ID of the current process.
        #[inline]
        pub fn process_id() -> Pid {
            *PROCESS_ID
        }

        /// Gets the ID of the current thread.
        #[inline]
        pub fn thread_id() -> Pid {
            thread_local! {
                static TID: Pid = unsafe { libc::syscall(libc::SYS_gettid) as Pid };
            }
            TID.with(|t| *t)
        }

        /// Gets thread ids of the given process id.
        /// WARN: Don't call this function frequently. Otherwise there will be a lot
        /// of memory fragments.
        pub fn thread_ids<C: FromIterator<Pid>>(pid: Pid) -> io::Result<C> {
            let dir = fs::read_dir(format!("/proc/{}/task", pid))?;
            Ok(dir
                .filter_map(|task| {
                    let file_name = match task {
                        Ok(t) => t.file_name(),
                        Err(_e) => {
                            return None;
                        }
                    };

                    match file_name.to_str() {
                        Some(tid) => match tid.parse() {
                            Ok(tid) => Some(tid),
                            Err(_) => None,
                        },
                        None => None,
                    }
                })
                .collect())
        }

        pub fn full_thread_stat(pid: Pid, tid: Pid) -> ProcResult<Stat> {
            procfs::process::Process::new(tid).unwrap().stat()
        }
    }

    pub use self::imp::*;

    lazy_static::lazy_static! {
        pub static ref THREAD_NAME_HASHMAP: Mutex<HashMap<Pid, String>> = Mutex::new(HashMap::default());
        pub static ref THREAD_START_HOOKS: Mutex<Vec<Box<dyn Fn() + Sync + Send>>> = Mutex::new(Vec::new());
    }
}
