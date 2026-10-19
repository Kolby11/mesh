use std::{os::fd::AsFd, sync::Arc};

use tokio::io::unix::AsyncFd;
use wayland_client::{
    Connection, Dispatch, QueueHandle,
    protocol::{wl_callback, wl_output, wl_registry},
};
use wayland_protocols_wlr::foreign_toplevel::v1::client::{
    zwlr_foreign_toplevel_handle_v1 as handle, zwlr_foreign_toplevel_manager_v1 as manager,
};

use super::{
    exec_stream::{StreamEvent, StreamEventKind, StreamHandle, StreamState},
    host_sources::{finish, spawn_source},
};

pub(super) fn spawn(state: &Arc<StreamState>, protocol: &str) -> std::io::Result<StreamHandle> {
    if protocol != "foreign-toplevel" {
        return Err(std::io::Error::other(
            "supported Wayland source: foreign-toplevel",
        ));
    }
    spawn_source(
        state,
        "wayland:foreign-toplevel".into(),
        protocol,
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
    manager: Option<manager::ZwlrForeignToplevelManagerV1>,
    finished: bool,
    initial: bool,
    handles: Vec<handle::ZwlrForeignToplevelHandleV1>,
    failure: Option<String>,
}

impl Observer {
    fn emit(&self, value: serde_json::Value) {
        self.state.push_event(StreamEvent {
            stream: self.stream.clone(),
            kind: StreamEventKind::Line(value.to_string()),
        });
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
        handles: Vec::new(),
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
            return Err(std::io::Error::other("Wayland toplevel manager finished"));
        }
        if observer.initial && observer.manager.is_none() {
            return Err(std::io::Error::other(
                "compositor does not advertise foreign-toplevel",
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
        {
            if interface == "zwlr_foreign_toplevel_manager_v1" && state.manager.is_none() {
                state.manager = Some(registry.bind(name, version.min(3), qh, ()));
            }
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

impl Dispatch<manager::ZwlrForeignToplevelManagerV1, ()> for Observer {
    fn event(
        state: &mut Self,
        _: &manager::ZwlrForeignToplevelManagerV1,
        event: manager::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            manager::Event::Toplevel { toplevel } => {
                if state.handles.len() >= 1024 {
                    state.failure = Some("Wayland toplevel limit reached".into());
                    return;
                }
                state.emit(
                    serde_json::json!({ "event": "added", "id": toplevel.id().protocol_id() }),
                );
                state.handles.push(toplevel);
            }
            manager::Event::Finished => state.finished = true,
            _ => {}
        }
    }
    wayland_client::event_created_child!(Observer, manager::ZwlrForeignToplevelManagerV1, [0 => (handle::ZwlrForeignToplevelHandleV1, ())]);
}

impl Dispatch<handle::ZwlrForeignToplevelHandleV1, ()> for Observer {
    fn event(
        state: &mut Self,
        object: &handle::ZwlrForeignToplevelHandleV1,
        event: handle::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let id = object.id().protocol_id();
        let value = match event {
            handle::Event::Title { title } => {
                serde_json::json!({ "event": "title", "id": id, "value": title })
            }
            handle::Event::AppId { app_id } => {
                serde_json::json!({ "event": "app-id", "id": id, "value": app_id })
            }
            handle::Event::State { state } => {
                serde_json::json!({ "event": "state", "id": id, "value": state.chunks_exact(4).map(|bytes| u32::from_ne_bytes(bytes.try_into().unwrap())).collect::<Vec<_>>() })
            }
            handle::Event::Done => serde_json::json!({ "event": "done", "id": id }),
            handle::Event::Closed => {
                state.handles.retain(|handle| handle != object);
                object.destroy();
                serde_json::json!({ "event": "closed", "id": id })
            }
            _ => return,
        };
        if value.to_string().len() > super::exec_stream::MAX_STREAM_LINE_BYTES {
            state.failure = Some("Wayland event exceeds stream record limit".into());
        } else {
            state.emit(value);
        }
    }
}

impl Dispatch<wl_output::WlOutput, ()> for Observer {
    fn event(
        _: &mut Self,
        _: &wl_output::WlOutput,
        _: wl_output::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}
use wayland_client::Proxy;

fn io_error(error: wayland_client::backend::WaylandError) -> std::io::Error {
    match error {
        wayland_client::backend::WaylandError::Io(error) => error,
        error => std::io::Error::other(error),
    }
}

#[cfg(test)]
mod tests {
}
