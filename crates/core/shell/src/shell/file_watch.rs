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
    control: WakeHandle,
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
        if self
            .commands
            .send(WatchCommand::Replace(watch_set))
            .is_err()
        {
            return false;
        }
        self.control.wake();
        true
    }

    pub(super) fn stop_and_join(mut self) {
        self.stop.store(true, Ordering::Release);
        self.control.wake();
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
        self.control.wake();
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
    let control_fd = rustix::event::eventfd(
        0,
        rustix::event::EventfdFlags::CLOEXEC | rustix::event::EventfdFlags::NONBLOCK,
    )
    .ok()?;
    let control = WakeHandle::from_fd(&control_fd).ok()?;
    let (commands, command_rx) = std::sync::mpsc::channel();
    let stop = Arc::new(AtomicBool::new(false));
    let thread_stop = stop.clone();
    std::thread::Builder::new()
        .name("mesh-file-watch".into())
        .spawn(move || watch_thread(watch_set, command_rx, tx, wake, thread_stop, control_fd))
        .map(|join| {
            Some(FileWatcherHandle {
                commands,
                stop,
                control,
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
    control: rustix::fd::OwnedFd,
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

        let mut notification = [0u8; 8];
        let _ = rustix::io::read(&control, &mut notification);
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

        if stop.load(Ordering::Acquire) {
            return;
        }
        let Some((reader, readiness)) = reader.as_mut() else {
            if let Err(error) = wait_for_watch_event(&control, None) {
                tracing::warn!("file watcher control wait failed: {error}");
                notify_watcher_stopped(&tx, &wake, current_generation);
                return;
            }
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
                if let Err(error) = wait_for_watch_event(&control, Some(readiness)) {
                    tracing::warn!("file watcher readiness wait failed: {error}");
                    notify_watcher_stopped(&tx, &wake, current_generation);
                    return;
                }
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
fn wait_for_watch_event(
    control: &rustix::fd::OwnedFd,
    inotify: Option<&rustix::fd::OwnedFd>,
) -> Result<(), rustix::io::Errno> {
    use rustix::event::{PollFd, PollFlags, poll};
    let flags = PollFlags::IN | PollFlags::ERR | PollFlags::HUP;
    let mut fds = vec![PollFd::new(control, flags)];
    if let Some(inotify) = inotify {
        fds.push(PollFd::new(inotify, flags));
    }
    match poll(&mut fds, -1) {
        Ok(_)
            if fds.iter().any(|fd| {
                fd.revents()
                    .intersects(PollFlags::ERR | PollFlags::HUP | PollFlags::NVAL)
            }) =>
        {
            Err(rustix::io::Errno::IO)
        }
        Ok(_) | Err(rustix::io::Errno::INTR) => Ok(()),
        Err(error) => Err(error),
    }
}

#[cfg(target_os = "linux")]
fn open_reader<'a>(
    watch_set: &WatchSet,
    flags: rustix::fs::inotify::WatchFlags,
    buf: &'a mut [std::mem::MaybeUninit<u8>],
) -> Result<
    (
        Option<(
            rustix::fs::inotify::Reader<'a, rustix::fd::OwnedFd>,
            rustix::fd::OwnedFd,
        )>,
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

    // Reader owns its fd without exposing AsFd; poll the same queue through
    // a duplicate while retaining buffered events between reads.
    let readiness = inotify
        .try_clone()
        .map_err(|error| format!("inotify readiness fd: {error}"))?;
    Ok((
        Some((inotify::Reader::new(inotify, buf), readiness)),
        watched,
    ))
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
    #[cfg(target_os = "linux")]
    fn receive(rx: &mut mpsc::UnboundedReceiver<ShellMessage>) -> ShellMessage {
        let until = std::time::Instant::now() + std::time::Duration::from_secs(3);
        loop {
            match rx.try_recv() {
                Ok(message) => return message,
                Err(mpsc::error::TryRecvError::Disconnected) => panic!("watcher disconnected"),
                Err(mpsc::error::TryRecvError::Empty) => {
                    assert!(std::time::Instant::now() < until, "watcher timed out");
                    std::thread::sleep(std::time::Duration::from_millis(2));
                }
            }
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn watcher_control_rebinds_an_idle_worker_and_shutdown_wakes_it() {
        use rustix::event::{EventfdFlags, eventfd};
        let first = tempfile::tempdir().unwrap();
        let second = tempfile::tempdir().unwrap();
        let fd = eventfd(0, EventfdFlags::CLOEXEC | EventfdFlags::NONBLOCK).unwrap();
        let wake = WakeHandle::from_fd(&fd).unwrap();
        let (tx, mut rx) = mpsc::unbounded_channel();
        let watcher =
            spawn_file_watcher(WatchSet::new(1, vec![first.path().into()]), tx, wake).unwrap();
        assert!(matches!(
            receive(&mut rx),
            ShellMessage::FileWatcherStatus {
                generation: 1,
                active: true,
                ..
            }
        ));
        assert!(watcher.replace(WatchSet::new(2, vec![second.path().into()])));
        assert!(matches!(
            receive(&mut rx),
            ShellMessage::FileWatcherStatus {
                generation: 2,
                active: true,
                ..
            }
        ));
        std::fs::write(second.path().join("settings.json"), "{}").unwrap();
        assert!(matches!(
            receive(&mut rx),
            ShellMessage::FilesystemChanged { generation: 2 }
        ));
        watcher.stop_and_join();
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn watcher_with_no_bound_directories_accepts_control_and_stops() {
        use rustix::event::{EventfdFlags, eventfd};
        let root = tempfile::tempdir().unwrap();
        let fd = eventfd(0, EventfdFlags::CLOEXEC | EventfdFlags::NONBLOCK).unwrap();
        let wake = WakeHandle::from_fd(&fd).unwrap();
        let (tx, mut rx) = mpsc::unbounded_channel();
        let watcher = spawn_file_watcher(WatchSet::new(1, vec![]), tx, wake).unwrap();
        assert!(matches!(
            receive(&mut rx),
            ShellMessage::FileWatcherStatus {
                generation: 1,
                active: false,
                ..
            }
        ));
        assert!(watcher.replace(WatchSet::new(2, vec![root.path().into()])));
        assert!(matches!(
            receive(&mut rx),
            ShellMessage::FileWatcherStatus {
                generation: 2,
                active: true,
                ..
            }
        ));
        watcher.stop_and_join();
    }

    #[cfg(target_os = "linux")]
    #[test]
    #[ignore = "release-only idle watcher wait gate"]
    fn watcher_idle_wait_profile() {
        use rustix::event::{EventfdFlags, eventfd};
        use std::time::Duration;
        assert!(!cfg!(debug_assertions), "run with --release");
        for run in 0..3 {
            let mut counts = Vec::new();
            for blocking in [false, true] {
                let control = eventfd(0, EventfdFlags::CLOEXEC | EventfdFlags::NONBLOCK).unwrap();
                let notification = WakeHandle::from_fd(&control).unwrap();
                let stop = Arc::new(AtomicBool::new(false));
                let worker_stop = stop.clone();
                let (started_tx, started_rx) = std::sync::mpsc::channel();
                let worker = std::thread::spawn(move || {
                    let mut idle_wakeups = 0;
                    started_tx.send(()).unwrap();
                    loop {
                        if blocking {
                            wait_for_watch_event(&control, None).unwrap();
                        } else {
                            std::thread::sleep(Duration::from_millis(20));
                        }
                        if worker_stop.load(Ordering::Acquire) {
                            return idle_wakeups;
                        }
                        idle_wakeups += 1;
                    }
                });
                started_rx.recv().unwrap();
                std::thread::sleep(Duration::from_millis(160));
                stop.store(true, Ordering::Release);
                notification.wake();
                counts.push(worker.join().unwrap());
            }
            eprintln!(
                "idle watcher run={run} duration=160ms polling_wakes={} blocking_wakes={} (shutdown wake excluded)",
                counts[0], counts[1]
            );
            assert!(counts[0] >= 3);
            assert_eq!(counts[1], 0, "watcher_idle_wait must have no timed wakeups");
        }
    }
}
