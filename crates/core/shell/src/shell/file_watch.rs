use super::types::ShellMessage;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use tokio::sync::mpsc;

#[cfg(target_os = "linux")]
pub(super) fn spawn_file_watcher(
    paths: Vec<PathBuf>,
    tx: mpsc::UnboundedSender<ShellMessage>,
    eventfd_fd: std::os::unix::io::RawFd,
) -> bool {
    let watch_dirs = watch_dirs(paths);
    if watch_dirs.is_empty() {
        return false;
    }

    std::thread::Builder::new()
        .name("mesh-file-watch".into())
        .spawn(move || watch_thread(watch_dirs, tx, eventfd_fd))
        .map(|_| true)
        .unwrap_or_else(|err| {
            tracing::warn!("failed to spawn file watcher: {err}");
            false
        })
}

#[cfg(not(target_os = "linux"))]
pub(super) fn spawn_file_watcher(
    _paths: Vec<PathBuf>,
    _tx: mpsc::UnboundedSender<ShellMessage>,
    _eventfd_fd: std::os::unix::io::RawFd,
) -> bool {
    false
}

fn watch_dirs(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut seen = HashSet::new();
    let mut dirs = Vec::new();
    for path in paths {
        let dir = if path.is_dir() {
            path
        } else {
            path.parent().map(Path::to_path_buf).unwrap_or(path)
        };
        if !dir.is_dir() {
            continue;
        }
        if seen.insert(dir.clone()) {
            dirs.push(dir);
        }
    }
    dirs
}

#[cfg(target_os = "linux")]
fn watch_thread(
    watch_dirs: Vec<PathBuf>,
    tx: mpsc::UnboundedSender<ShellMessage>,
    eventfd_fd: std::os::unix::io::RawFd,
) {
    use rustix::fd::BorrowedFd;
    use rustix::fs::inotify::{self, CreateFlags, WatchFlags};
    use std::mem::MaybeUninit;

    let inotify = match inotify::init(CreateFlags::CLOEXEC) {
        Ok(fd) => fd,
        Err(err) => {
            tracing::warn!("failed to initialise file watcher: {err}");
            return;
        }
    };

    let flags = WatchFlags::CLOSE_WRITE
        | WatchFlags::MOVED_TO
        | WatchFlags::MOVED_FROM
        | WatchFlags::CREATE
        | WatchFlags::DELETE
        | WatchFlags::ATTRIB
        | WatchFlags::MOVE_SELF
        | WatchFlags::DELETE_SELF;

    let mut watched = 0usize;
    for dir in watch_dirs {
        match inotify::add_watch(&inotify, &dir, flags) {
            Ok(_) => watched += 1,
            Err(err) => tracing::warn!("failed to watch {}: {err}", dir.display()),
        }
    }
    if watched == 0 {
        tracing::warn!("file watcher has no active directories");
        return;
    }

    let mut buf = [MaybeUninit::<u8>::uninit(); 4096];
    let mut reader = inotify::Reader::new(inotify, &mut buf);
    loop {
        match reader.next() {
            Ok(_) => {
                if tx.send(ShellMessage::FilesystemChanged).is_err() {
                    return;
                }
                let evfd = unsafe { BorrowedFd::borrow_raw(eventfd_fd) };
                let _ = rustix::io::write(&evfd, &1u64.to_ne_bytes());
            }
            Err(err) => {
                tracing::warn!("file watcher stopped: {err}");
                return;
            }
        }
    }
}
