use std::{
    cell::RefCell, collections::HashMap, io::Write, os::unix::net::UnixStream, rc::Rc, sync::Arc,
};

use pipewire::{
    self as pw,
    proxy::{Listener, ProxyT},
    spa::support::system::IoFlags,
    types::ObjectType,
};

use super::{
    exec_stream::{StreamEvent, StreamEventKind, StreamHandle, StreamState},
    host_sources::{finish, spawn_source},
};

type Objects = HashMap<u32, (Box<dyn ProxyT>, Box<dyn Listener>)>;
const MAX_OBJECTS: usize = 512;

pub(super) fn spawn(state: &Arc<StreamState>) -> std::io::Result<StreamHandle> {
    let (reader, writer) = UnixStream::pair()?;
    spawn_source(
        state,
        "pipewire".into(),
        "default",
        move |state, stream, stop| async move {
            // Dropping this future closes the socket and wakes the native loop too.
            let mut writer = writer;
            let worker_state = state.clone();
            let worker_stream = stream.clone();
            let mut worker =
                tokio::task::spawn_blocking(move || run(worker_state, worker_stream, reader));
            let result = tokio::select! {
                result = &mut worker => result,
                _ = stop => {
                    let _ = writer.write_all(&[1]);
                    worker.await
                }
            };
            let failure = match result {
                Ok(Ok(())) => None,
                Ok(Err(error)) => Some(error),
                Err(error) => Some(error.to_string()),
            };
            finish(&state, stream, failure);
        },
    )
}

fn emit(state: &StreamState, stream: &StreamHandle, event: &str, id: u32) {
    state.push_event(StreamEvent {
        stream: stream.clone(),
        kind: StreamEventKind::Line(serde_json::json!({ "event": event, "id": id }).to_string()),
    });
}

fn run(state: Arc<StreamState>, stream: StreamHandle, stop: UnixStream) -> Result<(), String> {
    pw::init();
    let mainloop = pw::main_loop::MainLoopRc::new(None).map_err(|error| error.to_string())?;
    let weak = mainloop.downgrade();
    let _stop =
        mainloop
            .loop_()
            .add_io(stop, IoFlags::IN | IoFlags::HUP | IoFlags::ERR, move |_| {
                if let Some(mainloop) = weak.upgrade() {
                    mainloop.quit();
                }
            });
    let context =
        pw::context::ContextRc::new(&mainloop, None).map_err(|error| error.to_string())?;
    let core = context
        .connect_rc(None)
        .map_err(|error| error.to_string())?;
    let failure = Rc::new(RefCell::new(None));
    let failed = failure.clone();
    let weak = mainloop.downgrade();
    let started_state = state.clone();
    let started_stream = stream.clone();
    let _core = core
        .add_listener_local()
        .info(move |_| {
            started_state.push_event(StreamEvent {
                stream: started_stream.clone(),
                kind: StreamEventKind::Started,
            })
        })
        .error(move |id, _, _, message| {
            if id == 0 {
                *failed.borrow_mut() = Some(message.to_string());
                if let Some(mainloop) = weak.upgrade() {
                    mainloop.quit();
                }
            }
        })
        .register();
    let registry = core.get_registry_rc().map_err(|error| error.to_string())?;
    let weak_registry = registry.downgrade();
    let objects: Rc<RefCell<Objects>> = Rc::default();
    let added = objects.clone();
    let removed = objects.clone();
    let remove_state = state.clone();
    let remove_stream = stream.clone();
    let failed = failure.clone();
    let weak = mainloop.downgrade();
    let _registry = registry
        .add_listener_local()
        .global(move |global| {
            if !matches!(global.type_, ObjectType::Node | ObjectType::Metadata) {
                return;
            }
            if added.borrow().len() >= MAX_OBJECTS {
                *failed.borrow_mut() = Some("PipeWire object limit reached".into());
                if let Some(mainloop) = weak.upgrade() {
                    mainloop.quit();
                }
                return;
            }
            let Some(registry) = weak_registry.upgrade() else {
                return;
            };
            let id = global.id;
            let event_state = state.clone();
            let event_stream = stream.clone();
            let object: Result<(Box<dyn ProxyT>, Box<dyn Listener>), pw::Error> = match global.type_
            {
                ObjectType::Node => registry.bind::<pw::node::Node, _>(global).map(|node| {
                    let listener = node
                        .add_listener_local()
                        .param(move |_, _, _, _, _| {
                            emit(&event_state, &event_stream, "node-params", id)
                        })
                        .register();
                    node.subscribe_params(&[pw::spa::param::ParamType::Props]);
                    (
                        Box::new(node) as Box<dyn ProxyT>,
                        Box::new(listener) as Box<dyn Listener>,
                    )
                }),
                ObjectType::Metadata => {
                    registry
                        .bind::<pw::metadata::Metadata, _>(global)
                        .map(|metadata| {
                            let listener = metadata
                                .add_listener_local()
                                .property(move |_, _, _, _| {
                                    emit(&event_state, &event_stream, "metadata", id);
                                    0
                                })
                                .register();
                            (
                                Box::new(metadata) as Box<dyn ProxyT>,
                                Box::new(listener) as Box<dyn Listener>,
                            )
                        })
                }
                _ => unreachable!(),
            };
            match object {
                Ok(object) => {
                    added.borrow_mut().insert(id, object);
                    emit(&state, &stream, "added", id);
                }
                Err(error) => {
                    *failed.borrow_mut() = Some(error.to_string());
                    if let Some(mainloop) = weak.upgrade() {
                        mainloop.quit();
                    }
                }
            }
        })
        .global_remove(move |id| {
            if removed.borrow_mut().remove(&id).is_some() {
                emit(&remove_state, &remove_stream, "removed", id);
            }
        })
        .register();
    mainloop.run();
    let result = failure.borrow_mut().take().map_or(Ok(()), Err);
    result
}

#[cfg(test)]
mod tests {
}
