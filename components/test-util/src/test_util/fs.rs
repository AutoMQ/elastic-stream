use std::{
    env, fs, io,
    path::{Path, PathBuf},
};

use log::{error, trace};
use uuid::Uuid;

pub struct DirectoryRemovalGuard<'a> {
    path: &'a Path,
}

impl<'a> DirectoryRemovalGuard<'a> {
    pub fn new(path: &'a Path) -> Self {
        Self { path }
    }
}

impl<'a> Drop for DirectoryRemovalGuard<'a> {
    fn drop(&mut self) {
        let path = Path::new(&self.path);
        let _ = path.read_dir().map(|read_dir| {
            read_dir
                .flatten()
                .map(|entry| {
                    trace!("Deleting {:?}", entry.path());
                })
                .count();
        });
        if let Err(e) = fs::remove_dir_all(path) {
            error!("Failed to remove directory: {:?}. Error: {:?}", path, e);
        }
    }
}

pub fn create_random_path() -> io::Result<PathBuf> {
    let uuid = Uuid::new_v4();
    let mut wal_dir = env::temp_dir();
    wal_dir.push(uuid.simple().to_string());
    let db_path = wal_dir.as_path();
    fs::create_dir_all(db_path)?;
    Ok(wal_dir)
}

#[cfg(test)]
mod tests {
    use std::{error::Error, fs};

    #[test]
    fn test_directory_removal() -> Result<(), Box<dyn Error>> {
        let path = super::create_random_path()?;
        let path = path.as_path();
        {
            let _guard = super::DirectoryRemovalGuard::new(path);
            if !path.exists() {
                fs::create_dir(path)?;
            }

            let _files: Vec<_> = (0..3)
                .into_iter()
                .map(|i| {
                    let file1 = path.join(&format!("file{}.txt", i));
                    fs::File::create(file1.as_path())
                })
                .flatten()
                .collect();
        }
        assert_eq!(false, path.exists());
        Ok(())
    }
}
