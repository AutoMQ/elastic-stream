use std::path::Path;

use slog::{error, trace, Logger};

pub struct DirectoryRemovalGuard<'a> {
    log: Logger,
    path: &'a Path,
}

impl<'a> DirectoryRemovalGuard<'a> {
    pub fn new(log: Logger, path: &'a Path) -> Self {
        Self { log, path }
    }
}

impl<'a> Drop for DirectoryRemovalGuard<'a> {
    fn drop(&mut self) {
        let path = Path::new(&self.path);
        let _ = path.read_dir().map(|read_dir| {
            read_dir
                .flatten()
                .map(|entry| {
                    trace!(self.log, "Deleting {:?}", entry.path());
                })
                .count();
        });
        if let Err(e) = std::fs::remove_dir_all(path) {
            error!(
                self.log,
                "Failed to remove directory: {:?}. Error: {:?}", path, e
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{env, error::Error};
    use uuid::Uuid;

    #[test]
    fn test_directory_removal() -> Result<(), Box<dyn Error>> {
        let tmp_dir = env::temp_dir();
        let uuid = Uuid::new_v4().simple().to_string();
        let path = tmp_dir.as_path().join(uuid);
        let path = path.as_path();
        {
            let log = crate::terminal_logger();
            let _guard = super::DirectoryRemovalGuard::new(log, path);
            if !path.exists() {
                std::fs::create_dir(path)?;
            }

            let _files: Vec<_> = (0..3)
                .into_iter()
                .map(|i| {
                    let file1 = path.join(&format!("file{}.txt", i));
                    std::fs::File::create(file1.as_path())
                })
                .flatten()
                .collect();
        }
        assert_eq!(false, path.exists());
        Ok(())
    }
}
