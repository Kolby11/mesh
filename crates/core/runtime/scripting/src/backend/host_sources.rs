//! Native event sources for backend modules.
//!
//! `mesh.exec_stream` adopts a subprocess, which costs a helper process per
//! source (`socat` for a socket, a monitor tool for a file) and, where no
//! monitor exists, forces providers into fork-per-poll loops. These sources
//! read the host object directly and deliver records through the same bounded
//! [`StreamState`] queue, handle identity, and lifecycle as subprocess
//! streams, so scripts consume them with the same hooks:
//!
//! - `mesh.socket_stream(path)` reads newline-delimited records from a Unix
//!   stream socket (`net.socket`);
//! - `mesh.watch_path(path)` emits one `modified` record per wakeup in which
//!   the file changed, including kernfs attributes that call `sysfs_notify`
//!   (`fs.watch`);
//! - `mesh.socket_request(path, request)` performs one bounded synchronous
//!   request/response exchange on a Unix stream socket (`net.socket`).

use std::io::{Read, Write};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::net::UnixStream as StdUnixStream;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use tokio::io::unix::AsyncFd;
use tokio::net::UnixStream;
use tokio::sync::oneshot;

use super::exec_stream::{
    ChildBudgetGuard, StreamEvent, StreamEventKind, StreamExitStatus, StreamHandle, StreamProcess,
    StreamState, read_lines,
};

/// Capability required to connect to Unix stream sockets.
pub const SOCKET_CAPABILITY: &str = "net.socket";
/// Capability required to observe file modification.
pub const WATCH_CAPABILITY: &str = "fs.watch";

/// Program label used by socket stream handles and legacy stream hooks.
pub fn socket_stream_program(path: &str) -> String {
    format!("socket:{path}")
}

/// Program label used by path watch handles and legacy stream hooks.
pub fn watch_program(path: &str) -> String {
    format!("watch:{path}")
}

/// Record emitted once per wakeup in which a watched path changed.
pub const WATCH_MODIFIED_LINE: &str = "modified";

fn clean_exit() -> StreamExitStatus {
    StreamExitStatus {
        code: Some(0),
        success: true,
        signal: None,
    }
}

pub(super) fn spawn_source<F>(
    state: &Arc<StreamState>,
    program: String,
    path: &str,
    run: impl FnOnce(Arc<StreamState>, StreamHandle, oneshot::Receiver<()>) -> F,
) -> std::io::Result<StreamHandle>
where
    F: std::future::Future<Output = ()> + Send + 'static,
{
    let runtime = tokio::runtime::Handle::try_current()
        .map_err(|_| std::io::Error::other("native stream sources require a Tokio runtime"))?;
    let stream = state.register(program, vec![path.to_string()])?;
    let (stop_tx, stop_rx) = oneshot::channel();
    let accounted = Arc::new(AtomicBool::new(false));
    let guard = ChildBudgetGuard {
        resources: state.resources.clone(),
        accounted: Arc::clone(&accounted),
    };
    let future = run(Arc::clone(state), stream.clone(), stop_rx);
    let task = runtime.spawn(async move {
        let _budget = guard;
        future.await;
    });
    state.insert_process(StreamProcess {
        stream: stream.clone(),
        stop: Some(stop_tx),
        task,
        child_accounted: accounted,
    });
    Ok(stream)
}

/// Connect to a Unix stream socket and deliver its newline-delimited records.
pub fn spawn_socket_stream(state: &Arc<StreamState>, path: &str) -> std::io::Result<StreamHandle> {
    // Connect synchronously so a missing socket is reported to the caller
    // instead of as an asynchronous failure record.
    let socket = connect_unix(path, Instant::now() + SOCKET_STREAM_CONNECT_TIMEOUT)?;
    let program = socket_stream_program(path);
    spawn_source(
        state,
        program,
        path,
        move |state, stream, stop_rx| async move {
            let socket = match UnixStream::from_std(socket) {
                Ok(socket) => socket,
                Err(error) => {
                    finish(
                        &state,
                        stream,
                        Some(format!("socket registration failed: {error}")),
                    );
                    return;
                }
            };
            state.push_event(StreamEvent {
                stream: stream.clone(),
                kind: StreamEventKind::Started,
            });
            let reader = read_lines(Arc::clone(&state), stream.clone(), socket);
            tokio::select! {
                _ = reader => {}
                _ = stop_rx => {}
            }
            finish(&state, stream, None);
        },
    )
}

/// Watch one path for content modification.
pub fn spawn_path_watch(state: &Arc<StreamState>, path: &str) -> std::io::Result<StreamHandle> {
    let inotify = Inotify::watch(Path::new(path))?;
    let program = watch_program(path);
    spawn_source(
        state,
        program,
        path,
        move |state, stream, mut stop_rx| async move {
            let fd = match AsyncFd::new(inotify) {
                Ok(fd) => fd,
                Err(error) => {
                    finish(
                        &state,
                        stream,
                        Some(format!("watch registration failed: {error}")),
                    );
                    return;
                }
            };
            state.push_event(StreamEvent {
                stream: stream.clone(),
                kind: StreamEventKind::Started,
            });
            let mut failure = None;
            loop {
                tokio::select! {
                    _ = &mut stop_rx => break,
                    ready = fd.readable() => {
                        let mut guard = match ready {
                            Ok(guard) => guard,
                            Err(error) => {
                                failure = Some(format!("watch wait failed: {error}"));
                                break;
                            }
                        };
                        match guard.get_inner().drain() {
                            Ok(summary) => {
                                guard.clear_ready();
                                if summary.modified {
                                    state.push_event(StreamEvent {
                                        stream: stream.clone(),
                                        kind: StreamEventKind::Line(WATCH_MODIFIED_LINE.to_string()),
                                    });
                                }
                                if summary.gone {
                                    state.push_event(StreamEvent {
                                        stream: stream.clone(),
                                        kind: StreamEventKind::Eof,
                                    });
                                    break;
                                }
                            }
                            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                                guard.clear_ready();
                            }
                            Err(error) => {
                                failure = Some(format!("watch read failed: {error}"));
                                break;
                            }
                        }
                    }
                }
            }
            finish(&state, stream, failure);
        },
    )
}

pub(super) fn finish(state: &Arc<StreamState>, stream: StreamHandle, failure: Option<String>) {
    let exit = match failure {
        Some(message) => {
            state.push_event(StreamEvent {
                stream: stream.clone(),
                kind: StreamEventKind::Failed(message),
            });
            StreamExitStatus {
                code: None,
                success: false,
                signal: None,
            }
        }
        None => clean_exit(),
    };
    state.push_event(StreamEvent {
        stream,
        kind: StreamEventKind::Exited(exit),
    });
}

/// Outcome of one bounded socket exchange.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SocketReply {
    pub success: bool,
    pub response: String,
    pub error: String,
}

/// Write `request` to a Unix stream socket and read until the peer closes,
/// the reply exceeds `max_bytes`, or `timeout` elapses.
pub fn socket_request(
    path: &str,
    request: &[u8],
    timeout: Duration,
    max_bytes: usize,
) -> SocketReply {
    match socket_request_inner(path, request, timeout, max_bytes) {
        Ok(response) => SocketReply {
            success: true,
            response,
            error: String::new(),
        },
        Err(error) => SocketReply {
            success: false,
            response: String::new(),
            error,
        },
    }
}

fn socket_request_inner(
    path: &str,
    request: &[u8],
    timeout: Duration,
    max_bytes: usize,
) -> Result<String, String> {
    let deadline = Instant::now() + timeout;
    let mut socket = connect_unix(path, deadline).map_err(|error| {
        if error.kind() == std::io::ErrorKind::TimedOut {
            format!("socket connect timed out after {timeout:?}")
        } else {
            error.to_string()
        }
    })?;
    socket
        .set_nonblocking(false)
        .map_err(|error| error.to_string())?;
    socket
        .set_write_timeout(Some(timeout))
        .map_err(|error| error.to_string())?;
    socket
        .write_all(request)
        .map_err(|error| error.to_string())?;
    // Half-close so peers that read to EOF see the end of the request.
    let _ = socket.shutdown(std::net::Shutdown::Write);
    let mut response = Vec::new();
    let mut buffer = [0_u8; 8192];
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(format!("socket reply timed out after {timeout:?}"));
        }
        socket
            .set_read_timeout(Some(remaining))
            .map_err(|error| error.to_string())?;
        match socket.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => {
                if response.len() + read > max_bytes {
                    return Err(format!("socket reply exceeded {max_bytes} bytes"));
                }
                response.extend_from_slice(&buffer[..read]);
            }
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                return Err(format!("socket reply timed out after {timeout:?}"));
            }
            Err(error) => return Err(error.to_string()),
        }
    }
    Ok(String::from_utf8_lossy(&response).into_owned())
}

/// Bound on establishing a `mesh.socket_stream` connection.
const SOCKET_STREAM_CONNECT_TIMEOUT: Duration = Duration::from_secs(2);

/// Connects a nonblocking Unix stream socket, giving up at `deadline`.
///
/// A blocking `connect` waits indefinitely while the listener's accept backlog
/// is full. Linux reports that state to a nonblocking AF_UNIX `connect` as
/// `EAGAIN` rather than `EINPROGRESS`, with no readiness to wait for, so the
/// attempt is retried with a short backoff. Other errors, such as a missing
/// path, return at once. The returned stream is nonblocking.
fn connect_unix(path: &str, deadline: Instant) -> std::io::Result<StdUnixStream> {
    let bytes = path.as_bytes();
    // SAFETY: `sockaddr_un` is plain old data; all-zero is a valid value.
    let mut address: libc::sockaddr_un = unsafe { std::mem::zeroed() };
    if bytes.is_empty() || bytes.contains(&0) || bytes.len() >= address.sun_path.len() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("invalid Unix socket path '{path}'"),
        ));
    }
    address.sun_family = libc::AF_UNIX as libc::sa_family_t;
    for (target, byte) in address.sun_path.iter_mut().zip(bytes) {
        *target = *byte as libc::c_char;
    }
    let length = (std::mem::offset_of!(libc::sockaddr_un, sun_path) + bytes.len() + 1)
        as libc::socklen_t;

    // SAFETY: plain socket creation; the result is checked before use.
    let raw = unsafe {
        libc::socket(
            libc::AF_UNIX,
            libc::SOCK_STREAM | libc::SOCK_NONBLOCK | libc::SOCK_CLOEXEC,
            0,
        )
    };
    if raw < 0 {
        return Err(std::io::Error::last_os_error());
    }
    // SAFETY: `raw` is a freshly created descriptor owned by nothing else.
    let socket = unsafe { OwnedFd::from_raw_fd(raw) };
    let mut backoff = Duration::from_millis(1);
    loop {
        // SAFETY: `address` is initialized and `length` covers its used prefix.
        let result = unsafe {
            libc::connect(
                socket.as_raw_fd(),
                std::ptr::addr_of!(address).cast(),
                length,
            )
        };
        if result == 0 {
            return Ok(StdUnixStream::from(socket));
        }
        let error = std::io::Error::last_os_error();
        match error.raw_os_error() {
            Some(libc::EINTR) => {}
            Some(libc::EAGAIN) => {
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::TimedOut,
                        format!("connecting to {path} timed out"),
                    ));
                }
                std::thread::sleep(backoff.min(remaining));
                backoff = (backoff * 2).min(Duration::from_millis(20));
            }
            Some(libc::EINPROGRESS) => {
                wait_writable(&socket, deadline, path)?;
                return Ok(StdUnixStream::from(socket));
            }
            _ => return Err(error),
        }
    }
}

fn wait_writable(socket: &OwnedFd, deadline: Instant, path: &str) -> std::io::Result<()> {
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                format!("connecting to {path} timed out"),
            ));
        }
        let mut poll = libc::pollfd {
            fd: socket.as_raw_fd(),
            events: libc::POLLOUT,
            revents: 0,
        };
        let millis = remaining.as_millis().clamp(1, i32::MAX as u128) as i32;
        // SAFETY: `poll` points at one initialized pollfd.
        let ready = unsafe { libc::poll(&mut poll, 1, millis) };
        if ready < 0 {
            let error = std::io::Error::last_os_error();
            if error.kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            return Err(error);
        }
        if ready == 0 {
            continue;
        }
        let mut pending: libc::c_int = 0;
        let mut size = std::mem::size_of::<libc::c_int>() as libc::socklen_t;
        // SAFETY: `pending` and `size` describe a valid c_int out-parameter.
        let result = unsafe {
            libc::getsockopt(
                socket.as_raw_fd(),
                libc::SOL_SOCKET,
                libc::SO_ERROR,
                std::ptr::addr_of_mut!(pending).cast(),
                &mut size,
            )
        };
        if result < 0 {
            return Err(std::io::Error::last_os_error());
        }
        return if pending == 0 {
            Ok(())
        } else {
            Err(std::io::Error::from_raw_os_error(pending))
        };
    }
}

struct DrainSummary {
    modified: bool,
    gone: bool,
}

/// One inotify instance holding a single watch.
struct Inotify {
    fd: OwnedFd,
}

impl AsRawFd for Inotify {
    fn as_raw_fd(&self) -> std::os::fd::RawFd {
        self.fd.as_raw_fd()
    }
}

impl Inotify {
    fn watch(path: &Path) -> std::io::Result<Self> {
        use std::os::unix::ffi::OsStrExt;
        let raw = unsafe { libc::inotify_init1(libc::IN_NONBLOCK | libc::IN_CLOEXEC) };
        if raw < 0 {
            return Err(std::io::Error::last_os_error());
        }
        // SAFETY: `raw` is a freshly created descriptor owned by nothing else.
        let fd = unsafe { OwnedFd::from_raw_fd(raw) };
        let path = std::ffi::CString::new(path.as_os_str().as_bytes())
            .map_err(|_| std::io::Error::other("watch path contains a NUL byte"))?;
        let mask = libc::IN_MODIFY
            | libc::IN_CLOSE_WRITE
            | libc::IN_ATTRIB
            | libc::IN_DELETE_SELF
            | libc::IN_MOVE_SELF;
        let watch = unsafe { libc::inotify_add_watch(fd.as_raw_fd(), path.as_ptr(), mask) };
        if watch < 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(Self { fd })
    }

    /// Read every queued event, reporting whether content changed and
    /// whether the watched object went away.
    fn drain(&self) -> std::io::Result<DrainSummary> {
        let mut summary = DrainSummary {
            modified: false,
            gone: false,
        };
        let mut buffer = [0_u8; 4096];
        let mut read_any = false;
        loop {
            let read = unsafe {
                libc::read(
                    self.fd.as_raw_fd(),
                    buffer.as_mut_ptr().cast(),
                    buffer.len(),
                )
            };
            if read < 0 {
                let error = std::io::Error::last_os_error();
                if error.kind() == std::io::ErrorKind::WouldBlock && read_any {
                    return Ok(summary);
                }
                if error.kind() == std::io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(error);
            }
            if read == 0 {
                return Ok(summary);
            }
            read_any = true;
            let mut offset = 0;
            let header = std::mem::size_of::<libc::inotify_event>();
            while offset + header <= read as usize {
                // SAFETY: the kernel writes whole `inotify_event` records;
                // read_unaligned tolerates the byte buffer's alignment.
                let event: libc::inotify_event =
                    unsafe { std::ptr::read_unaligned(buffer.as_ptr().add(offset).cast()) };
                if event.mask & libc::IN_Q_OVERFLOW != 0 {
                    return Err(std::io::Error::other(
                        "inotify queue overflow; resync required",
                    ));
                }
                if event.mask & (libc::IN_MODIFY | libc::IN_CLOSE_WRITE | libc::IN_ATTRIB) != 0 {
                    summary.modified = true;
                }
                if event.mask & (libc::IN_DELETE_SELF | libc::IN_MOVE_SELF | libc::IN_IGNORED) != 0
                {
                    summary.gone = true;
                }
                offset += header + event.len as usize;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::net::UnixListener;

    async fn collect_until(
        state: &Arc<StreamState>,
        done: impl Fn(&[StreamEvent]) -> bool,
    ) -> Vec<StreamEvent> {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
        let mut events = Vec::new();
        while tokio::time::Instant::now() < deadline && !done(&events) {
            let _ = tokio::time::timeout(Duration::from_millis(200), state.wait_for_event()).await;
            events.extend(state.drain_events());
        }
        events
    }

    fn lines(events: &[StreamEvent]) -> Vec<&str> {
        events
            .iter()
            .filter_map(|event| match &event.kind {
                StreamEventKind::Line(line) => Some(line.as_str()),
                _ => None,
            })
            .collect()
    }

    #[tokio::test]
    async fn socket_stream_delivers_lines_then_exits_on_peer_close() {
        let dir = tempdir();
        let path = dir.join("events.sock");
        let listener = UnixListener::bind(&path).unwrap();
        let server = std::thread::spawn(move || {
            let (mut peer, _) = listener.accept().unwrap();
            peer.write_all(b"workspace>>2\nactivewindow>>kitty,shell\n")
                .unwrap();
        });
        let state = StreamState::new();
        let handle = spawn_socket_stream(&state, path.to_str().unwrap()).unwrap();
        assert_eq!(
            handle.program(),
            socket_stream_program(path.to_str().unwrap())
        );
        let events = collect_until(&state, |events| {
            events
                .iter()
                .any(|event| matches!(event.kind, StreamEventKind::Exited(_)))
        })
        .await;
        server.join().unwrap();
        assert_eq!(
            lines(&events),
            vec!["workspace>>2", "activewindow>>kitty,shell"]
        );
        assert!(matches!(
            events.first().unwrap().kind,
            StreamEventKind::Started
        ));
        assert!(matches!(
            events.last().unwrap().kind,
            StreamEventKind::Exited(StreamExitStatus { success: true, .. })
        ));
        state.shutdown().await;
    }

    #[tokio::test]
    async fn socket_stream_reports_missing_socket_synchronously() {
        let state = StreamState::new();
        assert!(spawn_socket_stream(&state, "/nonexistent/mesh-test.sock").is_err());
        assert_eq!(state.active_stream_count(), 0);
    }

    #[tokio::test]
    async fn shutdown_stops_a_socket_stream_whose_peer_stays_open() {
        let dir = tempdir();
        let path = dir.join("idle.sock");
        let listener = UnixListener::bind(&path).unwrap();
        let state = StreamState::new();
        let _handle = spawn_socket_stream(&state, path.to_str().unwrap()).unwrap();
        let (_peer, _) = listener.accept().unwrap();
        state.shutdown().await;
        assert_eq!(state.active_stream_count(), 0);
    }

    #[tokio::test]
    async fn path_watch_reports_modification_and_removal() {
        let dir = tempdir();
        let path = dir.join("value");
        std::fs::write(&path, "1").unwrap();
        let state = StreamState::new();
        let _handle = spawn_path_watch(&state, path.to_str().unwrap()).unwrap();
        std::fs::write(&path, "2").unwrap();
        let events = collect_until(&state, |events| !lines(events).is_empty()).await;
        assert!(lines(&events).contains(&WATCH_MODIFIED_LINE));

        std::fs::remove_file(&path).unwrap();
        let events = collect_until(&state, |events| {
            events
                .iter()
                .any(|event| matches!(event.kind, StreamEventKind::Exited(_)))
        })
        .await;
        assert!(
            events
                .iter()
                .any(|event| matches!(event.kind, StreamEventKind::Eof))
        );
        state.shutdown().await;
    }

    #[test]
    fn socket_request_reads_reply_until_close_and_bounds_size() {
        let dir = tempdir();
        let path = dir.join("request.sock");
        let listener = UnixListener::bind(&path).unwrap();
        let server = std::thread::spawn(move || {
            for reply in [b"{\"id\":3}".as_slice(), &[b'x'; 64]] {
                let (mut peer, _) = listener.accept().unwrap();
                let mut request = Vec::new();
                peer.read_to_end(&mut request).unwrap();
                assert_eq!(request, b"j/activeworkspace");
                peer.write_all(reply).unwrap();
            }
        });
        let path = path.to_str().unwrap();
        let reply = socket_request(path, b"j/activeworkspace", Duration::from_secs(2), 1024);
        assert!(reply.success, "{reply:?}");
        assert_eq!(reply.response, "{\"id\":3}");
        let reply = socket_request(path, b"j/activeworkspace", Duration::from_secs(2), 16);
        assert!(!reply.success);
        assert!(reply.error.contains("exceeded"));
        server.join().unwrap();
    }

    #[test]
    fn socket_request_times_out_on_a_silent_peer() {
        let dir = tempdir();
        let path = dir.join("silent.sock");
        let listener = UnixListener::bind(&path).unwrap();
        let path_text = path.to_str().unwrap().to_string();
        let started = Instant::now();
        let client = std::thread::spawn(move || {
            socket_request(&path_text, b"x", Duration::from_millis(100), 1024)
        });
        let (_peer, _) = listener.accept().unwrap();
        let reply = client.join().unwrap();
        assert!(!reply.success);
        assert!(reply.error.contains("timed out"), "{reply:?}");
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    /// A listener whose accept backlog is full, plus the clients holding it.
    fn saturated_listener(path: &std::path::Path) -> (UnixListener, Vec<StdUnixStream>) {
        let listener = UnixListener::bind(path).unwrap();
        // SAFETY: re-listening on an owned, bound socket only shrinks its backlog.
        assert_eq!(unsafe { libc::listen(listener.as_raw_fd(), 0) }, 0);
        let mut held = Vec::new();
        let text = path.to_str().unwrap();
        loop {
            match connect_unix(text, Instant::now()) {
                Ok(client) => held.push(client),
                Err(error) if error.kind() == std::io::ErrorKind::TimedOut => break,
                Err(error) => panic!("{error}"),
            }
            assert!(held.len() < 64, "backlog never filled");
        }
        (listener, held)
    }

    #[test]
    fn socket_request_bounds_connection_to_a_full_backlog() {
        let dir = tempdir();
        let path = dir.join("full.sock");
        let (_listener, _held) = saturated_listener(&path);

        let started = Instant::now();
        let reply = socket_request(path.to_str().unwrap(), b"x", Duration::from_millis(150), 64);
        assert!(!reply.success);
        assert!(reply.error.contains("connect timed out"), "{reply:?}");
        assert!(started.elapsed() < Duration::from_secs(1));
    }

    #[tokio::test]
    async fn socket_stream_bounds_connection_to_a_full_backlog() {
        let dir = tempdir();
        let path = dir.join("full-stream.sock");
        let (_listener, _held) = saturated_listener(&path);
        let state = StreamState::new();

        let started = Instant::now();
        let error = spawn_socket_stream(&state, path.to_str().unwrap()).unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::TimedOut);
        assert!(started.elapsed() < SOCKET_STREAM_CONNECT_TIMEOUT + Duration::from_secs(1));
        assert_eq!(state.active_stream_count(), 0);
    }

    fn tempdir() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "mesh-host-sources-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
}
