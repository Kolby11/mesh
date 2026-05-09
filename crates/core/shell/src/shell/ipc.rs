use super::types::{CoreRequest, ShellMessage};
use std::os::unix::fs::{FileTypeExt, PermissionsExt};
use std::path::PathBuf;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;
use tokio::runtime::Runtime;
use tokio::sync::mpsc;

pub(super) fn spawn_ipc_server(
    runtime: &Runtime,
    socket_path: PathBuf,
    tx: mpsc::UnboundedSender<ShellMessage>,
) -> Result<(), std::io::Error> {
    if let Some(parent) = socket_path.parent() {
        std::fs::create_dir_all(parent)?;
        if is_private_tmp_ipc_dir(parent) {
            std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700))?;
        }
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

async fn handle_ipc_client(
    stream: tokio::net::UnixStream,
    tx: mpsc::UnboundedSender<ShellMessage>,
) -> Result<(), std::io::Error> {
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let mut line = String::new();

    loop {
        line.clear();
        let read = reader.read_line(&mut line).await?;
        if read == 0 {
            break;
        }

        let command = line.trim();
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
    use std::os::unix::fs::{FileTypeExt, PermissionsExt};

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
}
