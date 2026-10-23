//! `ext_workspace_manager_v1` records for `mesh.wayland_subscribe("workspace")`.
//! See [backend host events](../../../../../../docs/modules/backend-events.md).

use std::{os::fd::AsFd, sync::Arc};

use tokio::io::unix::AsyncFd;
use wayland_client::{
    Connection, Dispatch, Proxy, QueueHandle, WEnum,
    protocol::{wl_callback, wl_registry},
};
use wayland_protocols::ext::workspace::v1::client::{
    ext_workspace_group_handle_v1 as group, ext_workspace_handle_v1 as workspace,
    ext_workspace_manager_v1 as manager,
};

use super::{
    exec_stream::{StreamEvent, StreamEventKind, StreamHandle, StreamState},
    host_sources::{finish, spawn_source},
};

/// Bound on workspaces plus groups one connection tracks.
const MAX_HANDLES: usize = 1024;

pub(super) fn spawn(state: &Arc<StreamState>) -> std::io::Result<StreamHandle> {
    spawn_source(
        state,
        "wayland:workspace".into(),
        "workspace",
        move |state, stream, stop| async move {
            let result = tokio::select! {
                _ = stop => Ok(()),
                result = run(&state, &stream) => result,
            };
            finish(&state, stream, result.err().map(|error| error.to_string()));
        },
    )
}

struct Observer {
    state: Arc<StreamState>,
    stream: StreamHandle,
    manager: Option<manager::ExtWorkspaceManagerV1>,
    initial: bool,
    finished: bool,
    handles: usize,
    failure: Option<String>,
}

impl Observer {
    fn emit(&mut self, value: serde_json::Value) {
        if value.to_string().len() > super::exec_stream::MAX_STREAM_LINE_BYTES {
            self.failure = Some("Wayland event exceeds stream record limit".into());
            return;
        }
        self.state.push_event(StreamEvent {
            stream: self.stream.clone(),
            kind: StreamEventKind::Line(value.to_string()),
        });
    }

    fn track(&mut self) -> bool {
        self.handles += 1;
        if self.handles > MAX_HANDLES {
            self.failure = Some("Wayland workspace limit reached".into());
            return false;
        }
        true
    }
}

async fn run(state: &Arc<StreamState>, stream: &StreamHandle) -> std::io::Result<()> {
    let connection = Connection::connect_to_env().map_err(std::io::Error::other)?;
    let mut queue = connection.new_event_queue::<Observer>();
    let qh = queue.handle();
    let display = connection.display();
    let _registry = display.get_registry(&qh, ());
    let _sync = display.sync(&qh, ());
    let fd = AsyncFd::new(connection.as_fd())?;
    let mut observer = Observer {
        state: state.clone(),
        stream: stream.clone(),
        manager: None,
        initial: false,
        finished: false,
        handles: 0,
        failure: None,
    };
    loop {
        queue
            .dispatch_pending(&mut observer)
            .map_err(std::io::Error::other)?;
        if let Some(error) = observer.failure.take() {
            return Err(std::io::Error::other(error));
        }
        if observer.finished {
            return Err(std::io::Error::other("Wayland workspace manager finished"));
        }
        if observer.initial && observer.manager.is_none() {
            return Err(std::io::Error::other(
                "compositor does not advertise ext-workspace",
            ));
        }
        loop {
            match connection.flush() {
                Ok(()) => break,
                Err(wayland_client::backend::WaylandError::Io(error))
                    if error.kind() == std::io::ErrorKind::WouldBlock =>
                {
                    let mut ready = fd.writable().await?;
                    match ready.try_io(|_| connection.flush().map_err(io_error)) {
                        Ok(result) => {
                            result?;
                            break;
                        }
                        Err(_) => continue,
                    }
                }
                Err(error) => return Err(io_error(error)),
            }
        }
        let Some(read) = queue.prepare_read() else {
            continue;
        };
        let mut ready = fd.readable().await?;
        match ready.try_io(|_| read.read().map_err(io_error)) {
            Ok(Ok(_)) | Err(_) => {}
            Ok(Err(error)) => return Err(error),
        }
    }
}

impl Dispatch<wl_registry::WlRegistry, ()> for Observer {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global {
            name,
            interface,
            version,
        } = event
            && interface == "ext_workspace_manager_v1"
            && state.manager.is_none()
        {
            state.manager = Some(registry.bind(name, version.min(1), qh, ()));
        }
    }
}

impl Dispatch<wl_callback::WlCallback, ()> for Observer {
    fn event(
        state: &mut Self,
        _: &wl_callback::WlCallback,
        _: wl_callback::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        state.initial = true;
        if state.manager.is_some() {
            state.state.push_event(StreamEvent {
                stream: state.stream.clone(),
                kind: StreamEventKind::Started,
            });
        }
    }
}

impl Dispatch<manager::ExtWorkspaceManagerV1, ()> for Observer {
    fn event(
        state: &mut Self,
        _: &manager::ExtWorkspaceManagerV1,
        event: manager::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            manager::Event::WorkspaceGroup { workspace_group } => {
                if state.track() {
                    let id = workspace_group.id().protocol_id();
                    state.emit(serde_json::json!({ "event": "group-added", "id": id }));
                }
            }
            manager::Event::Workspace { workspace } => {
                if state.track() {
                    let id = workspace.id().protocol_id();
                    state.emit(serde_json::json!({ "event": "added", "id": id }));
                }
            }
            manager::Event::Done => state.emit(serde_json::json!({ "event": "done" })),
            manager::Event::Finished => state.finished = true,
            _ => {}
        }
    }
    wayland_client::event_created_child!(Observer, manager::ExtWorkspaceManagerV1, [
        0 => (group::ExtWorkspaceGroupHandleV1, ()),
        1 => (workspace::ExtWorkspaceHandleV1, ()),
    ]);
}

impl Dispatch<group::ExtWorkspaceGroupHandleV1, ()> for Observer {
    fn event(
        state: &mut Self,
        object: &group::ExtWorkspaceGroupHandleV1,
        event: group::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let id = object.id().protocol_id();
        let value = match event {
            group::Event::WorkspaceEnter { workspace } => serde_json::json!({
                "event": "group-workspace-enter", "id": id,
                "workspace": workspace.id().protocol_id(),
            }),
            group::Event::WorkspaceLeave { workspace } => serde_json::json!({
                "event": "group-workspace-leave", "id": id,
                "workspace": workspace.id().protocol_id(),
            }),
            group::Event::Removed => {
                object.destroy();
                state.handles = state.handles.saturating_sub(1);
                serde_json::json!({ "event": "group-removed", "id": id })
            }
            _ => return,
        };
        state.emit(value);
    }
}

impl Dispatch<workspace::ExtWorkspaceHandleV1, ()> for Observer {
    fn event(
        state: &mut Self,
        object: &workspace::ExtWorkspaceHandleV1,
        event: workspace::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let id = object.id().protocol_id();
        let value = match event {
            workspace::Event::Id { id: stable } => {
                serde_json::json!({ "event": "workspace-id", "id": id, "value": stable })
            }
            workspace::Event::Name { name } => {
                serde_json::json!({ "event": "name", "id": id, "value": name })
            }
            workspace::Event::Coordinates { coordinates } => serde_json::json!({
                "event": "coordinates", "id": id,
                "value": coordinates
                    .chunks_exact(4)
                    .map(|bytes| u32::from_ne_bytes(bytes.try_into().unwrap()))
                    .collect::<Vec<_>>(),
            }),
            workspace::Event::State { state: flags } => {
                let bits = match flags {
                    WEnum::Value(flags) => flags.bits(),
                    WEnum::Unknown(bits) => bits,
                };
                serde_json::json!({ "event": "state", "id": id, "value": bits })
            }
            workspace::Event::Removed => {
                object.destroy();
                state.handles = state.handles.saturating_sub(1);
                serde_json::json!({ "event": "removed", "id": id })
            }
            _ => return,
        };
        state.emit(value);
    }
}

fn io_error(error: wayland_client::backend::WaylandError) -> std::io::Error {
    match error {
        wayland_client::backend::WaylandError::Io(error) => error,
        error => std::io::Error::other(error),
    }
}
