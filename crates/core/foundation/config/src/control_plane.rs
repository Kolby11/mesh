use std::fs::{self, File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};

pub(crate) const CONTROL_PLANE_LOCK_FILE: &str = ".mesh-control-plane.lock";

/// Exclusive advisory lock shared by settings, profile and package writers.
/// Package writers acquire their package lock first and retain this guard
/// through commit or rollback. Readers do not acquire a package lock.
pub struct ControlPlaneLock {
    file: File,
    directory: PathBuf,
}

impl ControlPlaneLock {
    pub fn directory(&self) -> &Path {
        &self.directory
    }

    pub fn ensure_directory(&self, directory: &Path) -> io::Result<()> {
        if fs::canonicalize(directory)? != self.directory {
            return Err(io::Error::other(
                "control-plane lock belongs to another configuration directory",
            ));
        }
        Ok(())
    }

    pub fn acquire(directory: &Path) -> io::Result<Self> {
        fs::create_dir_all(directory)?;
        let path = directory.join(CONTROL_PLANE_LOCK_FILE);
        let mut options = OpenOptions::new();
        options.create(true).read(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
        }
        let file = options.open(path)?;
        if !file.metadata()?.is_file() {
            return Err(io::Error::other(
                "control-plane lock must be a regular file",
            ));
        }
        #[cfg(unix)]
        if unsafe { libc::flock(std::os::fd::AsRawFd::as_raw_fd(&file), libc::LOCK_EX) } != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(Self {
            file,
            directory: fs::canonicalize(directory)?,
        })
    }
}

impl Drop for ControlPlaneLock {
    fn drop(&mut self) {
        #[cfg(unix)]
        unsafe {
            libc::flock(std::os::fd::AsRawFd::as_raw_fd(&self.file), libc::LOCK_UN);
        }
    }
}

/// Resolve the root graph consistently for CLI and shell, honoring an explicit
/// graph override before the development checkout and installed home.
pub fn default_module_graph_path() -> PathBuf {
    if let Ok(path) = std::env::var("MESH_MODULE_GRAPH_PATH")
        && !path.trim().is_empty()
    {
        return path.into();
    }
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../..")
        .join("config/module.json");
    if repo.exists() {
        repo
    } else {
        crate::mesh_home_path().join("module.json")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_guard_for_another_directory_cannot_commit_or_check_settings() {
        let root = std::env::temp_dir().join(format!(
            "mesh-control-plane-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(root.join("settings")).unwrap();
        let lock = ControlPlaneLock::acquire(&root.join("other")).unwrap();
        let path = root.join("settings/settings.json");
        let mut store =
            crate::SettingsStore::from_value(path.clone(), serde_json::json!({})).unwrap();
        assert!(store.check_revision_locked(0, &lock).is_err());
        assert!(store.save_if_revision_locked(0, &lock).is_err());
        assert!(!path.exists());
        drop(lock);
        fs::remove_dir_all(root).unwrap();
    }
}
