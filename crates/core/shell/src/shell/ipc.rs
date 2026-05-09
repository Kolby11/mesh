use super::types::{CoreRequest, ShellMessage};
use std::os::unix::fs::{DirBuilderExt, FileTypeExt, MetadataExt, PermissionsExt};
use std::path::PathBuf;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;
use tokio::runtime::Runtime;
use tokio::sync::mpsc;

const MAX_IPC_COMMAND_BYTES: usize = 4096;

pub(super) fn spawn_ipc_server(
    runtime: &Runtime,
    socket_path: PathBuf,
    tx: mpsc::UnboundedSender<ShellMessage>,
) -> Result<(), std::io::Error> {
    if let Some(parent) = socket_path.parent() {
        prepare_ipc_parent(parent)?;
    }

    if socket_path.exists() {
        let metadata = std::fs::symlink_metadata(&socket_path)?;
        if !metadata.file_type().is_socket() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                format!(
                    "refusing to replace non-socket ipc path {}",
                    socket_path.display()
                ),
            ));
        }
        std::fs::remove_file(&socket_path)?;
    }

    let _guard = runtime.enter();
    let listener = UnixListener::bind(&socket_path)?;
    std::fs::set_permissions(&socket_path, std::fs::Permissions::from_mode(0o600))?;
    tracing::info!("listening for ipc commands on {}", socket_path.display());

    runtime.spawn(async move {
        loop {
            let (stream, _) = match listener.accept().await {
                Ok(stream) => stream,
                Err(err) => {
                    tracing::warn!("ipc accept failed: {err}");
                    continue;
                }
            };

            let tx = tx.clone();
            tokio::spawn(async move {
                if let Err(err) = handle_ipc_client(stream, tx).await {
                    tracing::warn!("ipc client failed: {err}");
                }
            });
        }
    });

    Ok(())
}

fn prepare_ipc_parent(parent: &std::path::Path) -> Result<(), std::io::Error> {
    if !is_private_tmp_ipc_dir(parent) {
        return std::fs::create_dir_all(parent);
    }

    match std::fs::symlink_metadata(parent) {
        Ok(metadata) => {
            let file_type = metadata.file_type();
            if file_type.is_symlink() || !file_type.is_dir() {
                return Err(unsafe_ipc_parent(parent));
            }
            let mode = metadata.permissions().mode() & 0o777;
            if metadata.uid() != current_uid() || mode != 0o700 {
                return Err(unsafe_ipc_parent(parent));
            }
            Ok(())
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            std::fs::DirBuilder::new().mode(0o700).create(parent)
        }
        Err(err) => Err(err),
    }
}

fn unsafe_ipc_parent(parent: &std::path::Path) -> std::io::Error {
    std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        format!("refusing unsafe ipc directory {}", parent.display()),
    )
}

fn current_uid() -> u32 {
    unsafe extern "C" {
        fn getuid() -> u32;
    }

    unsafe { getuid() }
}

async fn handle_ipc_client(
    stream: tokio::net::UnixStream,
    tx: mpsc::UnboundedSender<ShellMessage>,
) -> Result<(), std::io::Error> {
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);

    while let Some(command) = read_ipc_command(&mut reader).await? {
        let Some(command) = command else {
            writer.write_all(b"error command-too-long\n").await?;
            break;
        };

        let command = command.trim();
        if command.is_empty() {
            continue;
        }

        match parse_ipc_command(command) {
            Some(request) => {
                tx.send(ShellMessage::Ipc(request)).map_err(|_| {
                    std::io::Error::new(std::io::ErrorKind::BrokenPipe, "shell is not running")
                })?;
                writer.write_all(b"ok\n").await?;
            }
            None => {
                writer
                    .write_all(format!("error unknown-command {command}\n").as_bytes())
                    .await?;
            }
        }
    }

    Ok(())
}

async fn read_ipc_command<R: tokio::io::AsyncRead + Unpin>(
    reader: &mut BufReader<R>,
) -> Result<Option<Option<String>>, std::io::Error> {
    let mut line = Vec::new();

    loop {
        let (consumed, done, too_long) = {
            let available = reader.fill_buf().await?;
            if available.is_empty() {
                if line.is_empty() {
                    return Ok(None);
                }
                return Ok(Some(Some(String::from_utf8_lossy(&line).into_owned())));
            }

            let newline = available.iter().position(|byte| *byte == b'\n');
            let take = newline.map(|pos| pos + 1).unwrap_or(available.len());
            let payload_len = newline.unwrap_or(take);
            let too_long = line.len().saturating_add(payload_len) > MAX_IPC_COMMAND_BYTES;
            if !too_long {
                line.extend_from_slice(&available[..payload_len]);
            }
            (take, newline.is_some(), too_long)
        };

        reader.consume(consumed);
        if too_long {
            return Ok(Some(None));
        }
        if done {
            return Ok(Some(Some(String::from_utf8_lossy(&line).into_owned())));
        }
    }
}

pub(super) fn parse_ipc_command(command: &str) -> Option<CoreRequest> {
    if let Some(surface_id) = command.strip_prefix("shell:show_surface:") {
        return Some(CoreRequest::ShowSurface {
            surface_id: surface_id.to_string(),
        });
    }
    if let Some(surface_id) = command.strip_prefix("shell:hide_surface:") {
        return Some(CoreRequest::HideSurface {
            surface_id: surface_id.to_string(),
        });
    }
    if let Some(surface_id) = command.strip_prefix("shell:toggle_surface:") {
        return Some(CoreRequest::ToggleSurface {
            surface_id: surface_id.to_string(),
        });
    }
    if let Some(scenario_id) = command.strip_prefix("shell:debug_benchmark:") {
        return Some(CoreRequest::RunDebugBenchmark {
            scenario_id: scenario_id.to_string(),
        });
    }
    match command {
        "shell:debug_overlay" => Some(CoreRequest::ToggleDebugOverlay),
        "shell:debug_profiling" => Some(CoreRequest::ToggleDebugProfiling),
        "shell:debug_cycle_tab" => Some(CoreRequest::CycleDebugTab),
        "shell:shutdown" => Some(CoreRequest::Shutdown),
        _ => None,
    }
}

fn is_private_tmp_ipc_dir(path: &std::path::Path) -> bool {
    path.parent()
        .is_some_and(|parent| parent == std::path::Path::new("/tmp"))
        && path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("mesh-"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::{FileTypeExt, PermissionsExt, symlink};
    use std::time::{SystemTime, UNIX_EPOCH};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    fn unique_tmp_mesh_parent(label: &str) -> std::path::PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::path::PathBuf::from(format!("/tmp/mesh-{label}-{}-{unique}", std::process::id()))
    }

    #[test]
    fn ipc_server_refuses_to_replace_non_socket_path() {
        let dir = tempfile::tempdir().unwrap();
        let socket_path = dir.path().join("mesh.sock");
        std::fs::write(&socket_path, "not a socket").unwrap();
        let runtime = Runtime::new().unwrap();
        let (tx, _rx) = mpsc::unbounded_channel();

        let err = spawn_ipc_server(&runtime, socket_path, tx).unwrap_err();

        assert_eq!(err.kind(), std::io::ErrorKind::AlreadyExists);
    }

    #[test]
    fn ipc_server_creates_owner_only_socket() {
        let dir = tempfile::tempdir().unwrap();
        let socket_path = dir.path().join("mesh.sock");
        let runtime = Runtime::new().unwrap();
        let (tx, _rx) = mpsc::unbounded_channel();

        spawn_ipc_server(&runtime, socket_path.clone(), tx).unwrap();

        let metadata = std::fs::symlink_metadata(socket_path).unwrap();
        assert!(metadata.file_type().is_socket());
        assert_eq!(metadata.permissions().mode() & 0o777, 0o600);
    }

    #[test]
    fn ipc_server_creates_missing_private_tmp_parent_as_owner_only() {
        let parent = unique_tmp_mesh_parent("missing-parent-test");
        let socket_path = parent.join("mesh.sock");
        let runtime = Runtime::new().unwrap();
        let (tx, _rx) = mpsc::unbounded_channel();

        spawn_ipc_server(&runtime, socket_path.clone(), tx).unwrap();

        let parent_metadata = std::fs::symlink_metadata(&parent).unwrap();
        let socket_metadata = std::fs::symlink_metadata(&socket_path).unwrap();
        assert!(parent_metadata.file_type().is_dir());
        assert_eq!(parent_metadata.permissions().mode() & 0o777, 0o700);
        assert!(socket_metadata.file_type().is_socket());
        std::fs::remove_file(socket_path).unwrap();
        std::fs::remove_dir(parent).unwrap();
    }

    #[test]
    fn ipc_server_refuses_non_private_tmp_parent() {
        let parent = unique_tmp_mesh_parent("nonprivate-parent-test");
        std::fs::create_dir(&parent).unwrap();
        std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o755)).unwrap();
        let socket_path = parent.join("mesh.sock");
        let runtime = Runtime::new().unwrap();
        let (tx, _rx) = mpsc::unbounded_channel();

        let err = spawn_ipc_server(&runtime, socket_path, tx).unwrap_err();
        std::fs::remove_dir(parent).unwrap();

        assert_eq!(err.kind(), std::io::ErrorKind::AlreadyExists);
    }

    #[test]
    fn ipc_server_refuses_symlinked_private_tmp_parent() {
        let target = tempfile::tempdir().unwrap();
        let parent = unique_tmp_mesh_parent("symlink-test");
        symlink(target.path(), &parent).unwrap();
        let socket_path = parent.join("mesh.sock");
        let runtime = Runtime::new().unwrap();
        let (tx, _rx) = mpsc::unbounded_channel();

        let err = spawn_ipc_server(&runtime, socket_path, tx).unwrap_err();
        std::fs::remove_file(&parent).unwrap();

        assert_eq!(err.kind(), std::io::ErrorKind::AlreadyExists);
    }

    #[test]
    fn ipc_client_rejects_oversized_command_line() {
        let runtime = Runtime::new().unwrap();
        runtime.block_on(async {
            let (mut client, server) = tokio::net::UnixStream::pair().unwrap();
            let (tx, _rx) = mpsc::unbounded_channel();
            let task = tokio::spawn(handle_ipc_client(server, tx));

            let mut payload = vec![b'x'; MAX_IPC_COMMAND_BYTES + 1];
            payload.push(b'\n');
            client.write_all(&payload).await.unwrap();
            let mut response = String::new();
            client.read_to_string(&mut response).await.unwrap();
            task.await.unwrap().unwrap();

            assert_eq!(response, "error command-too-long\n");
        });
    }
}
