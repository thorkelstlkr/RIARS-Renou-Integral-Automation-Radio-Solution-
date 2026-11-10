//! One process per library database. Two processes on the same database would both
//! write the session, the schedule and the RDS settings next to it, and both would
//! play and log the same station.

use std::fs::{File, OpenOptions, TryLockError};
use std::path::{Path, PathBuf};

/// Held for as long as the process uses the database; the lock goes when it is
/// dropped or the process ends, however it ends.
#[derive(Debug)]
pub struct InstanceLock {
    _file: File,
    path: PathBuf,
}

impl InstanceLock {
    /// Take the lock for the database at `db_path` (the file `riars.lock` next to it).
    /// Fails when another process holds it.
    pub fn acquire(db_path: &Path) -> Result<Self, String> {
        let path = db_path.with_file_name("riars.lock");
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&path)
            .map_err(|e| format!("Cannot open {}: {e}", path.display()))?;
        match file.try_lock() {
            Ok(()) => Ok(Self { _file: file, path }),
            Err(TryLockError::WouldBlock) => Err(format!(
                "Another RIARS process is using {} (lock {})",
                db_path.display(),
                path.display()
            )),
            Err(TryLockError::Error(e)) => Err(format!("Cannot lock {}: {e}", path.display())),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_second_lock_on_the_same_database_fails_until_the_first_is_dropped() {
        let dir = std::env::temp_dir().join(format!("riars-instance-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let db = dir.join("riars_library.db");
        let first = InstanceLock::acquire(&db).unwrap();
        assert_eq!(first.path(), dir.join("riars.lock"));
        let err = InstanceLock::acquire(&db).unwrap_err();
        assert!(err.starts_with("Another RIARS process is using"), "{err}");
        drop(first);
        InstanceLock::acquire(&db).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }
}
