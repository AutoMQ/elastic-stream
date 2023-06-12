use std::{
    process,
    sync::atomic::{AtomicUsize, Ordering},
    time::Duration,
};

use error::ConfigurationError;
use model::DataNode;
use nix::sys::stat;
use serde::{Deserialize, Serialize};
pub mod error;

lazy_static::lazy_static! {
    static ref CLIENT_ID_COUNTER: AtomicUsize = AtomicUsize::new(0);
}

fn client_id() -> String {
    let hostname = gethostname::gethostname()
        .into_string()
        .unwrap_or(String::from("unknown"));
    format!(
        "{}-{}-{}",
        hostname,
        process::id(),
        CLIENT_ID_COUNTER.fetch_add(1, Ordering::Relaxed)
    )
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Client {
    /// Establish connection timeout in ticks
    #[serde(rename = "connect-timeout")]
    pub connect_timeout: u64,

    /// IO timeout in ticks
    #[serde(rename = "io-timeout")]
    pub io_timeout: u64,

    /// Client ID
    #[serde(rename = "client-id")]
    pub client_id: String,

    /// Max transparent client retries
    #[serde(rename = "max-attempt")]
    pub max_attempt: usize,

    #[serde(rename = "heartbeat-interval")]
    pub heartbeat_interval: u64,

    #[serde(rename = "refresh-pm-cluster-interval")]
    pub refresh_pm_cluster_interval: u64,
}

impl Default for Client {
    fn default() -> Self {
        Self {
            connect_timeout: 20,
            io_timeout: 10,
            client_id: "".to_owned(),
            max_attempt: 3,
            heartbeat_interval: 30,
            refresh_pm_cluster_interval: 300,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Profiling {
    pub enable: bool,

    #[serde(rename = "sampling-frequency")]
    pub sampling_frequency: i32,

    #[serde(rename = "report-interval")]
    pub report_interval: u64,

    ///  Path to save flamegraph files: if a relative path is configured, it will be relative to current working directory;
    ///  If an absolute path is configured, the absolute path is used.
    #[serde(rename = "report-path")]
    pub report_path: String,

    #[serde(rename = "max-report-backup")]
    pub max_report_backup: usize,
}

impl Default for Profiling {
    fn default() -> Self {
        Self {
            enable: true,
            sampling_frequency: 1000,
            report_interval: 300,
            report_path: "flamegraph".to_owned(),
            max_report_backup: 3,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Server {
    pub host: String,
    pub port: u16,

    /// Data Node ID
    #[serde(default)]
    pub node_id: i32,

    pub concurrency: usize,

    pub uring: Uring,

    #[serde(rename = "connection-idle-duration")]
    pub connection_idle_duration: u64,

    #[serde(rename = "grace-period")]
    pub grace_period: u64,

    pub profiling: Profiling,
}

impl Server {
    pub fn data_node(&self) -> DataNode {
        DataNode {
            node_id: self.node_id,
            advertise_address: format!("{}:{}", self.host, self.port),
        }
    }
}

impl Default for Server {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_owned(),
            port: 10911,
            node_id: 0,
            concurrency: 1,
            uring: Uring::default(),
            connection_idle_duration: 60,
            grace_period: 120,
            profiling: Profiling::default(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Path {
    /// Full qualified path to base store directory, which contains lock, immutable properties and other configuration files
    base: String,

    /// Path to WAL files directory. It may be absolute or relative to `base`.
    wal: String,

    /// Path to RocksDB directory. It may be absolute or relative to `base`.
    metadata: String,
}

impl Path {
    pub fn set_base(&mut self, base: &str) {
        self.base = base.to_owned();
    }

    pub fn base_path(&self) -> &std::path::Path {
        std::path::Path::new(&self.base)
    }

    pub fn set_wal(&mut self, wal: &str) {
        self.wal = wal.to_owned();
    }

    pub fn wal_path(&self) -> std::path::PathBuf {
        self.base_path().join(&self.wal)
    }

    pub fn set_metadata(&mut self, metadata: &str) {
        self.metadata = metadata.to_owned();
    }

    pub fn metadata_path(&self) -> std::path::PathBuf {
        self.base_path().join(&self.metadata)
    }
}

impl Default for Path {
    fn default() -> Self {
        let tmp_store_path = std::env::temp_dir().join("store");
        Self {
            base: tmp_store_path
                .as_path()
                .to_str()
                .unwrap_or("/tmp/store")
                .to_owned(),
            wal: "wal".to_owned(),
            metadata: "metadata".to_owned(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Store {
    #[serde(rename = "mkdirs-if-missing")]
    pub mkdirs_if_missing: bool,

    pub path: Path,

    #[serde(rename = "segment-size")]
    pub segment_size: u64,

    #[serde(rename = "max-cache-size")]
    pub max_cache_size: u64,

    // Device block size
    #[serde(default)]
    pub alignment: usize,

    // Total number of blocks of the device that backs store-base.
    #[serde(default)]
    pub blocks: u64,

    #[serde(rename = "read-block-size")]
    pub read_block_size: u32,

    #[serde(rename = "pre-allocate-segment-file-number")]
    pub pre_allocate_segment_file_number: usize,

    pub uring: Uring,

    pub rocksdb: RocksDB,

    #[serde(rename = "total-segment-file-size")]
    pub total_segment_file_size: u64,
}

impl Default for Store {
    fn default() -> Self {
        Self {
            mkdirs_if_missing: true,
            path: Path::default(),
            segment_size: 1048576,
            max_cache_size: 1048576,
            alignment: 4096,
            blocks: 0,
            read_block_size: 131072,
            pre_allocate_segment_file_number: 2,
            uring: Uring::default(),
            rocksdb: RocksDB::default(),
            total_segment_file_size: u64::MAX,
        }
    }
}

impl Store {
    pub fn max_segment_number(&self) -> u64 {
        (self.blocks * (self.alignment as u64) + self.segment_size - 1) / self.segment_size
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Uring {
    #[serde(rename = "queue-depth")]
    pub queue_depth: u32,

    #[serde(rename = "sqpoll-idle-ms", default)]
    pub sqpoll_idle_ms: u32,

    #[serde(rename = "sqpoll-cpu", default)]
    pub sqpoll_cpu: u32,

    #[serde(rename = "max-bounded-worker", default)]
    pub max_bounded_worker: u32,

    #[serde(rename = "max-unbounded-worker", default)]
    pub max_unbounded_worker: u32,
}

impl Default for Uring {
    fn default() -> Self {
        Self {
            queue_depth: 128,
            sqpoll_idle_ms: 2000,
            sqpoll_cpu: 1,
            max_bounded_worker: 2,
            max_unbounded_worker: 2,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RocksDB {
    #[serde(rename = "create-if-missing")]
    pub create_if_missing: bool,

    #[serde(rename = "flush-threshold")]
    pub flush_threshold: usize,
}

impl Default for RocksDB {
    fn default() -> Self {
        Self {
            create_if_missing: true,
            flush_threshold: 32768,
        }
    }
}

/// Configurable items of the replication layer.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Replication {
    #[serde(rename = "connection-pool-size")]
    pub connection_pool_size: usize,
}

impl Default for Replication {
    fn default() -> Self {
        Self {
            connection_pool_size: 3,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Configuration {
    /// Unit of time in milliseconds.
    pub tick: u64,

    #[serde(rename = "placement-manager")]
    pub placement_manager: String,

    pub client: Client,

    pub server: Server,

    pub store: Store,

    pub replication: Replication,
}

impl Default for Configuration {
    fn default() -> Self {
        Self {
            tick: 100,
            placement_manager: "127.0.0.1:12378".to_owned(),
            client: Default::default(),
            server: Default::default(),
            store: Default::default(),
            replication: Default::default(),
        }
    }
}

impl Configuration {
    /// Check and apply the configuration.
    ///
    /// While applying configuration, store, its WAL and metadata directories
    /// are potentially created.
    pub fn check_and_apply(&mut self) -> Result<(), ConfigurationError> {
        let total_processor_num = num_cpus::get();
        if self.server.concurrency + 1 > total_processor_num {
            return Err(ConfigurationError::ConcurrencyTooLarge);
        }

        if self.client.client_id.is_empty() {
            let client_id = client_id();
            self.client.client_id.push_str(&client_id);
        }

        let base = std::path::Path::new(&self.store.path.base);
        if !base.exists() {
            if !self.store.mkdirs_if_missing {
                return Err(ConfigurationError::DirectoryNotExists(
                    self.store.path.base.clone(),
                ));
            } else {
                std::fs::create_dir_all(base)?;
            }
        }

        let wal = base.join(&self.store.path.wal);
        if !wal.exists() {
            if !self.store.mkdirs_if_missing {
                return Err(ConfigurationError::DirectoryNotExists(
                    wal.as_path().to_str().unwrap().to_owned(),
                ));
            } else {
                std::fs::create_dir_all(wal.as_path())?;
            }
        }
        let file_stat =
            stat::stat(wal.as_path()).map_err(|e| ConfigurationError::System(e as i32))?;
        self.store.alignment = file_stat.st_blksize as usize;
        self.store.blocks = file_stat.st_blocks as u64;

        let metadata = base.join(&self.store.path.metadata);
        if !metadata.exists() {
            if !self.store.mkdirs_if_missing {
                return Err(ConfigurationError::DirectoryNotExists(
                    metadata.as_path().to_str().unwrap().to_owned(),
                ));
            } else {
                std::fs::create_dir_all(metadata)?;
            }
        }

        if self.replication.connection_pool_size == 0 {
            // If connection-pool-size is 0, use processor number as default
            self.replication.connection_pool_size = num_cpus::get();
        }

        Ok(())
    }

    pub fn connection_idle_duration(&self) -> Duration {
        Duration::from_millis(self.tick * self.server.connection_idle_duration)
    }

    pub fn client_io_timeout(&self) -> Duration {
        Duration::from_millis(self.tick * self.client.io_timeout)
    }

    pub fn client_connect_timeout(&self) -> Duration {
        Duration::from_millis(self.tick * self.client.connect_timeout)
    }

    pub fn client_heartbeat_interval(&self) -> Duration {
        Duration::from_millis(self.tick * self.client.heartbeat_interval)
    }

    pub fn client_refresh_placement_manager_cluster_interval(&self) -> Duration {
        Duration::from_millis(self.tick * self.client.refresh_pm_cluster_interval)
    }

    pub fn server_grace_period(&self) -> Duration {
        Duration::from_millis(self.tick * self.server.grace_period)
    }
}

#[cfg(test)]
mod tests {
    use super::Configuration;
    use std::{error::Error, fs::File, io::Read, path::Path};

    #[test]
    fn test_yaml() -> Result<(), Box<dyn Error>> {
        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR")?;
        let path = Path::new(&manifest_dir);
        let path = path
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("etc/data-node.yaml");
        let mut file = File::open(path.as_path())?;
        let mut content = String::new();
        file.read_to_string(&mut content)?;
        let config: Configuration = serde_yaml::from_str(&content)?;
        assert_eq!(10911, config.server.port);
        assert_eq!(1, config.server.concurrency);
        assert_eq!(128, config.server.uring.queue_depth);
        assert_eq!(32768, config.store.rocksdb.flush_threshold);

        assert_eq!(3, config.replication.connection_pool_size);
        Ok(())
    }

    // Ensure generated client-id are unique.
    #[test]
    fn test_client_id() {
        let mut set = std::collections::HashSet::new();
        for _ in 0..100 {
            let client_id = super::client_id();
            assert!(!set.contains(&client_id));
            set.insert(client_id);
        }
        assert_eq!(100, set.len());
    }
}
