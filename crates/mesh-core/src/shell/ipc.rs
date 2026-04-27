use super::types::{CoreRequest, ShellMessage};
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
    }

    if socket_path.exists() {
        let _ = std::fs::remove_file(&socket_path);
    }

    let _guard = runtime.enter();
    let listener = UnixListener::bind(&socket_path)?;
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

fn parse_ipc_command(command: &str) -> Option<CoreRequest> {
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
    match command {
        "shell:debug_overlay" => Some(CoreRequest::ToggleDebugOverlay),
        "shell:debug_cycle_tab" => Some(CoreRequest::CycleDebugTab),
        "shell:shutdown" => Some(CoreRequest::Shutdown),
        _ => None,
    }
}
