use error::ConfigurationError;
use nix::sys::stat;
use serde::{Deserialize, Serialize};
pub mod error;

#[derive(Debug, Serialize, Deserialize)]
pub struct Server {
    pub host: String,
    pub port: u16,
    pub concurrency: usize,

    pub uring: Uring,

    #[serde(rename = "placement-manager")]
    pub placement_manager: String,
}

impl Default for Server {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_owned(),
            port: 10911,
            concurrency: 1,
            uring: Uring::default(),
            placement_manager: "localhost:2378".to_owned(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
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
        Self {
            base: "/tmp/data".to_owned(),
            wal: "wal".to_owned(),
            metadata: "metadata".to_owned(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Store {
    #[serde(rename = "mkdirs-if-missing")]
    pub mkdirs_if_missing: bool,

    pub path: Path,

    #[serde(rename = "segment-size")]
    pub segment_size: u64,

    #[serde(rename = "max-cache-size")]
    pub max_cache_size: u64,

    pub alignment: usize,

    #[serde(rename = "read-block-size")]
    pub read_block_size: u32,

    #[serde(rename = "pre-allocate-segment-file-number")]
    pub pre_allocate_segment_file_number: usize,

    pub uring: Uring,

    pub rocksdb: RocksDB,
}

impl Default for Store {
    fn default() -> Self {
        Self {
            mkdirs_if_missing: true,
            path: Path::default(),
            segment_size: 1048576,
            max_cache_size: 1048576,
            alignment: 4096,
            read_block_size: 131072,
            pre_allocate_segment_file_number: 2,
            uring: Uring::default(),
            rocksdb: RocksDB::default(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
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

#[derive(Debug, Serialize, Deserialize)]
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

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Configuration {
    pub server: Server,
    pub store: Store,
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
        Ok(())
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
            .join("etc/config.yaml");
        let mut file = File::open(path.as_path())?;
        let mut content = String::new();
        file.read_to_string(&mut content)?;
        let config: Configuration = serde_yaml::from_str(&content)?;
        assert_eq!(10911, config.server.port);
        assert_eq!(1, config.server.concurrency);
        assert_eq!(128, config.server.uring.queue_depth);
        assert_eq!(32768, config.store.rocksdb.flush_threshold);
        Ok(())
    }
}
