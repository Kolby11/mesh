use super::WakeHandle;
use super::types::ShellMessage;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::mpsc;

/// The complete set of filesystem inputs for one shell watch generation.
///
/// Paths are kept as logical inputs rather than only as inotify directories.
/// This lets the shell compare a newly prepared activation/catalog against the
/// set already owned by the watcher, while the worker derives the existing
/// directory ancestors needed for atomic file replacement and first creation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct WatchSet {
    pub(super) generation: u64,
    pub(super) paths: Vec<PathBuf>,
}

impl WatchSet {
    pub(super) fn new(generation: u64, mut paths: Vec<PathBuf>) -> Self {
        paths.sort();
        paths.dedup();
        Self { generation, paths }
    }
}

#[derive(Debug)]
enum WatchCommand {
    Replace(WatchSet),
}

pub(super) struct FileWatcherHandle {
    commands: std::sync::mpsc::Sender<WatchCommand>,
    stop: Arc<AtomicBool>,
    join: Option<std::thread::JoinHandle<()>>,
}

impl FileWatcherHandle {
    /// Replace the worker's inotify bindings. A finished worker cannot accept
    /// a command reliably, so the caller can join it and create a fresh worker
    /// for the new generation instead.
    pub(super) fn replace(&self, watch_set: WatchSet) -> bool {
        if self.stop.load(Ordering::Acquire)
            || self
                .join
                .as_ref()
                .is_some_and(std::thread::JoinHandle::is_finished)
        {
            return false;
        }
        self.commands.send(WatchCommand::Replace(watch_set)).is_ok()
    }

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
    watch_set: WatchSet,
    tx: mpsc::UnboundedSender<ShellMessage>,
    wake: WakeHandle,
) -> Option<FileWatcherHandle> {
    let (commands, command_rx) = std::sync::mpsc::channel();
    let stop = Arc::new(AtomicBool::new(false));
    let thread_stop = stop.clone();
    std::thread::Builder::new()
        .name("mesh-file-watch".into())
        .spawn(move || watch_thread(watch_set, command_rx, tx, wake, thread_stop))
        .map(|join| {
            Some(FileWatcherHandle {
                commands,
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
    _watch_set: WatchSet,
    _tx: mpsc::UnboundedSender<ShellMessage>,
    _wake: WakeHandle,
) -> Option<FileWatcherHandle> {
    None
}

fn watch_dirs(paths: &[PathBuf]) -> Vec<PathBuf> {
    let mut seen = HashSet::new();
    let mut dirs = Vec::new();
    for path in paths {
        let Some(dir) = existing_watch_dir(path) else {
            continue;
        };
        if seen.insert(dir.clone()) {
            dirs.push(dir);
        }
    }
    dirs
}

/// Return the nearest existing directory for a logical input. Watching the
/// ancestor is important for atomic replacement: the final file may not exist
/// yet, or its parent may be created by a later module/profile install.
fn existing_watch_dir(path: &Path) -> Option<PathBuf> {
    let mut current = if path.is_dir() {
        path.to_path_buf()
    } else {
        path.parent().map(Path::to_path_buf)?
    };
    loop {
        if current.is_dir() {
            return Some(current);
        }
        if !current.pop() {
            return None;
        }
    }
}

#[cfg(target_os = "linux")]
fn watch_thread(
    initial_watch_set: WatchSet,
    command_rx: std::sync::mpsc::Receiver<WatchCommand>,
    tx: mpsc::UnboundedSender<ShellMessage>,
    wake: WakeHandle,
    stop: Arc<AtomicBool>,
) {
    use rustix::fs::inotify::WatchFlags;
    use std::mem::MaybeUninit;

    let flags = WatchFlags::CLOSE_WRITE
        | WatchFlags::MOVED_TO
        | WatchFlags::MOVED_FROM
        | WatchFlags::CREATE
        | WatchFlags::DELETE
        | WatchFlags::ATTRIB
        | WatchFlags::MOVE_SELF
        | WatchFlags::DELETE_SELF;
    let mut buf = [MaybeUninit::<u8>::uninit(); 4096];
    let mut current_generation = initial_watch_set.generation;
    let mut reader = match open_reader(&initial_watch_set, flags, &mut buf) {
        Ok((reader, watched_paths)) => {
            if !notify_watcher_status(&tx, &wake, current_generation, watched_paths) {
                return;
            }
            reader
        }
        Err(error) => {
            tracing::warn!("failed to initialise file watcher: {error}");
            notify_watcher_stopped(&tx, &wake, current_generation);
            return;
        }
    };

    loop {
        if stop.load(Ordering::Acquire) {
            return;
        }

        loop {
            match command_rx.try_recv() {
                Ok(WatchCommand::Replace(watch_set)) => {
                    current_generation = watch_set.generation;
                    reader = match open_reader(&watch_set, flags, &mut buf) {
                        Ok((reader, watched_paths)) => {
                            if !notify_watcher_status(&tx, &wake, current_generation, watched_paths)
                            {
                                return;
                            }
                            reader
                        }
                        Err(error) => {
                            tracing::warn!("file watcher stopped while rebinding: {error}");
                            notify_watcher_stopped(&tx, &wake, current_generation);
                            return;
                        }
                    };
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => return,
            }
        }

        let Some(reader) = reader.as_mut() else {
            // Keep the manager alive when no current input has an existing
            // directory. A later WatchSet can bind newly-created paths.
            std::thread::sleep(std::time::Duration::from_millis(20));
            continue;
        };

        match reader.next() {
            Ok(_) => {
                if tx
                    .send(ShellMessage::FilesystemChanged {
                        generation: current_generation,
                    })
                    .is_err()
                {
                    return;
                }
                wake.wake();
            }
            Err(error) if error == rustix::io::Errno::WOULDBLOCK => {
                // inotify is non-blocking so configuration commands are
                // observed promptly without a second control pipe.
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            Err(error) => {
                tracing::warn!("file watcher stopped: {error}");
                notify_watcher_stopped(&tx, &wake, current_generation);
                return;
            }
        }
    }
}

#[cfg(target_os = "linux")]
fn open_reader<'a>(
    watch_set: &WatchSet,
    flags: rustix::fs::inotify::WatchFlags,
    buf: &'a mut [std::mem::MaybeUninit<u8>],
) -> Result<
    (
        Option<rustix::fs::inotify::Reader<'a, rustix::fd::OwnedFd>>,
        usize,
    ),
    String,
> {
    use rustix::fs::inotify::{self, CreateFlags};

    let inotify = inotify::init(CreateFlags::CLOEXEC | CreateFlags::NONBLOCK)
        .map_err(|error| format!("inotify init failed: {error}"))?;
    let mut watched = 0usize;
    for dir in watch_dirs(&watch_set.paths) {
        match inotify::add_watch(&inotify, &dir, flags) {
            Ok(_) => watched += 1,
            Err(error) => tracing::warn!("failed to watch {}: {error}", dir.display()),
        }
    }
    if watched == 0 {
        tracing::warn!(
            generation = watch_set.generation,
            "file watcher has no active directories"
        );
        return Ok((None, 0));
    }

    Ok((Some(inotify::Reader::new(inotify, buf)), watched))
}

fn notify_watcher_status(
    tx: &mpsc::UnboundedSender<ShellMessage>,
    wake: &WakeHandle,
    generation: u64,
    watched_paths: usize,
) -> bool {
    if tx
        .send(ShellMessage::FileWatcherStatus {
            generation,
            active: watched_paths > 0,
            watched_paths,
        })
        .is_err()
    {
        return false;
    }
    wake.wake();
    true
}

/// Tell the shell loop the watch thread is gone so it can report unhealthy
/// watcher state and continue with bounded metadata polling. Best effort: if
/// the shell has already gone, there is nothing left to wake.
fn notify_watcher_stopped(
    tx: &mpsc::UnboundedSender<ShellMessage>,
    wake: &WakeHandle,
    generation: u64,
) {
    if tx
        .send(ShellMessage::FileWatcherStopped { generation })
        .is_err()
    {
        return;
    }
    wake.wake();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn watch_set_is_deterministic_and_deduplicated() {
        let set = WatchSet::new(
            7,
            vec![
                PathBuf::from("/tmp/mesh/b"),
                PathBuf::from("/tmp/mesh/a"),
                PathBuf::from("/tmp/mesh/b"),
            ],
        );

        assert_eq!(set.generation, 7);
        assert_eq!(
            set.paths,
            vec![PathBuf::from("/tmp/mesh/a"), PathBuf::from("/tmp/mesh/b")]
        );
    }

    #[test]
    fn watch_dirs_falls_back_to_an_existing_ancestor() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("profiles").join("active.json");
        assert_eq!(watch_dirs(&[path]), vec![root.path().to_path_buf()]);
    }
}
