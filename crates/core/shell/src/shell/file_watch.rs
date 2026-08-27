use super::types::ShellMessage;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::mpsc;

pub(super) struct FileWatcherHandle {
    stop: Arc<AtomicBool>,
    join: Option<std::thread::JoinHandle<()>>,
}

impl FileWatcherHandle {
    pub(super) fn stop_and_join(mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(join) = self.join.take() {
            if join.join().is_err() {
                tracing::warn!("file watcher thread panicked during shutdown");
            }
        }
    }
}

impl Drop for FileWatcherHandle {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

#[cfg(target_os = "linux")]
pub(super) fn spawn_file_watcher(
    paths: Vec<PathBuf>,
    tx: mpsc::UnboundedSender<ShellMessage>,
    eventfd_fd: std::os::unix::io::RawFd,
) -> Option<FileWatcherHandle> {
    let watch_dirs = watch_dirs(paths);
    if watch_dirs.is_empty() {
        return None;
    }

    let stop = Arc::new(AtomicBool::new(false));
    let thread_stop = stop.clone();
    std::thread::Builder::new()
        .name("mesh-file-watch".into())
        .spawn(move || watch_thread(watch_dirs, tx, eventfd_fd, thread_stop))
        .map(|join| {
            Some(FileWatcherHandle {
                stop,
                join: Some(join),
            })
        })
        .unwrap_or_else(|err| {
            tracing::warn!("failed to spawn file watcher: {err}");
            None
        })
}

#[cfg(not(target_os = "linux"))]
pub(super) fn spawn_file_watcher(
    _paths: Vec<PathBuf>,
    _tx: mpsc::UnboundedSender<ShellMessage>,
    _eventfd_fd: std::os::unix::io::RawFd,
) -> Option<FileWatcherHandle> {
    None
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
    stop: Arc<AtomicBool>,
) {
    use rustix::fd::BorrowedFd;
    use rustix::fs::inotify::{self, CreateFlags, WatchFlags};
    use std::mem::MaybeUninit;

    let inotify = match inotify::init(CreateFlags::CLOEXEC | CreateFlags::NONBLOCK) {
        Ok(fd) => fd,
        Err(err) => {
            tracing::warn!("failed to initialise file watcher: {err}");
            if !stop.load(Ordering::Acquire) {
                notify_watcher_stopped(&tx, eventfd_fd);
            }
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
        if !stop.load(Ordering::Acquire) {
            notify_watcher_stopped(&tx, eventfd_fd);
        }
        return;
    }

    let mut buf = [MaybeUninit::<u8>::uninit(); 4096];
    let mut reader = inotify::Reader::new(inotify, &mut buf);
    loop {
        if stop.load(Ordering::Acquire) {
            return;
        }
        match reader.next() {
            Ok(_) => {
                if tx.send(ShellMessage::FilesystemChanged).is_err() {
                    return;
                }
                let evfd = unsafe { BorrowedFd::borrow_raw(eventfd_fd) };
                let _ = rustix::io::write(&evfd, &1u64.to_ne_bytes());
            }
            Err(err) => {
                if err == rustix::io::Errno::WOULDBLOCK {
                    std::thread::sleep(std::time::Duration::from_millis(20));
                    continue;
                }
                tracing::warn!("file watcher stopped: {err}");
                if !stop.load(Ordering::Acquire) {
                    notify_watcher_stopped(&tx, eventfd_fd);
                }
                return;
            }
        }
    }
}

/// Tell the shell loop the watch thread is gone so it stops trusting
/// `file_watcher_active` and falls back to short-interval polling on the
/// very next reload check instead of staying parked for up to 24h. Best
/// effort: if the shell has already gone, there is nothing left to wake.
#[cfg(target_os = "linux")]
fn notify_watcher_stopped(
    tx: &mpsc::UnboundedSender<ShellMessage>,
    eventfd_fd: std::os::unix::io::RawFd,
) {
    use rustix::fd::BorrowedFd;
    if tx.send(ShellMessage::FileWatcherStopped).is_err() {
        return;
    }
    let evfd = unsafe { BorrowedFd::borrow_raw(eventfd_fd) };
    let _ = rustix::io::write(&evfd, &1u64.to_ne_bytes());
}
