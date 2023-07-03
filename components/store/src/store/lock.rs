use std::{
    fs::{File, OpenOptions},
    io::Write,
    os::fd::{FromRawFd, IntoRawFd, RawFd},
};

use crate::error::StoreError;
use byteorder::{BigEndian, ReadBytesExt};
use client::IdGenerator;
use log::{error, info, warn};
use nix::fcntl::{flock, FlockArg};

pub(crate) struct Lock {
    fd: RawFd,
    id: i32,
}

impl Lock {
    pub(crate) fn new(
        config: &config::Configuration,
        id_generator: Box<dyn IdGenerator>,
    ) -> Result<Self, StoreError> {
        let store_base_path = config.store.path.base_path();
        let lock_file_path = store_base_path.join("LOCK");
        let (fd, id) = if lock_file_path.as_path().exists() {
            let mut file = OpenOptions::new()
                .read(true)
                .write(true)
                .open(lock_file_path.as_path())?;
            match file.metadata() {
                Ok(metadata) => {
                    if metadata.len() >= 4 {
                        let id = file.read_i32::<BigEndian>()?;
                        (file.into_raw_fd(), id)
                    } else {
                        warn!(
                            "LOCK file has only {} bytes. Generate a new range-server ID from PD now",
                            metadata.len()
                        );
                        let id: i32 = match id_generator.generate() {
                            Ok(id) => id,
                            Err(_e) => {
                                error!("Failed to acquire range-server ID from placement-driver");
                                return Err(StoreError::Configuration(String::from(
                                    "Failed to acquire range-server ID",
                                )));
                            }
                        };
                        file.write_all(&id.to_be_bytes())?;
                        file.sync_all()?;
                        info!("range-server ID is: {id}");
                        (file.into_raw_fd(), id)
                    }
                }
                Err(e) => return Err(StoreError::IO(e)),
            }
        } else {
            let mut file = OpenOptions::new()
                .create(true)
                .read(true)
                .write(true)
                .open(lock_file_path.as_path())?;

            let id: i32 = match id_generator.generate() {
                Ok(id) => id,
                Err(_e) => {
                    error!("Failed to acquire range-server ID from placement-driver");
                    return Err(StoreError::Configuration(String::from(
                        "Failed to acquire range-server ID",
                    )));
                }
            };
            file.write_all(&id.to_be_bytes())?;
            file.sync_all()?;
            info!("range-server ID is: {id}");
            (file.into_raw_fd(), id)
        };

        info!(
            "Acquiring store lock: {:?}, range-server ID={}",
            lock_file_path.as_path(),
            id
        );

        flock(fd, FlockArg::LockExclusive).map_err(|e| {
            error!("Failed to acquire store lock. errno={}", e);
            StoreError::AcquireLock
        })?;

        info!("Store lock acquired. range-server ID={}", id);

        Ok(Self { fd, id })
    }

    pub(crate) fn id(&self) -> i32 {
        self.id
    }
}

impl Drop for Lock {
    fn drop(&mut self) {
        if let Err(e) = flock(self.fd, FlockArg::Unlock) {
            error!("Failed to release store lock. errno={}", e);
        }
        let _file = unsafe { File::from_raw_fd(self.fd) };
    }
}

#[cfg(test)]
mod tests {
    use super::Lock;
    use crate::mocks::run_listener;
    use client::PlacementDriverIdGenerator;
    use std::{error::Error, sync::Arc};
    use tokio::sync::oneshot;

    #[test]
    fn test_lock_normal() -> Result<(), Box<dyn Error>> {
        let (stop_tx, stop_rx) = oneshot::channel();
        let (port_tx, port_rx) = oneshot::channel();

        let handle = std::thread::spawn(move || {
            tokio_uring::start(async {
                let port = run_listener().await;
                let _ = port_tx.send(port);
                let _ = stop_rx.await;
            });
        });

        let port = port_rx.blocking_recv().unwrap();
        let pd_address = format!("localhost:{}", port);
        let mut config = config::Configuration::default();
        config.placement_driver = pd_address;
        config
            .check_and_apply()
            .expect("Failed to check-and-apply configuration");
        let cfg = Arc::new(config);
        let generator = Box::new(PlacementDriverIdGenerator::new(&cfg));
        let _lock = Lock::new(&cfg, generator)?;
        let _ = stop_tx.send(());
        let _ = handle.join();
        Ok(())
    }
}
