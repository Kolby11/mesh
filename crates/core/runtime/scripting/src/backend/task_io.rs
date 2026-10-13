use crate::context::tasks::{AwaitedCall, realm_await_function};
use mlua::{Function, Lua, LuaSerdeExt, Table};
use serde_json::Value;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
};
use tokio::sync::Notify;

static ACTIVE_HOST_WORKERS: AtomicUsize = AtomicUsize::new(0);
static NEXT_OPERATION: AtomicU64 = AtomicU64::new(1);
const MAX_HOST_WORKERS: usize = 64;

type Outcome = Result<Value, String>;

#[derive(Default)]
pub(super) struct TaskIo {
    closed: AtomicBool,
    completed: Mutex<Vec<u64>>,
    pub(super) wakeup: Arc<Notify>,
}

impl TaskIo {
    pub(super) fn close(&self) {
        self.closed.store(true, Ordering::Release);
        self.completed.lock().unwrap().clear();
    }

    pub(super) fn drain_completed(&self) -> Vec<u64> {
        std::mem::take(&mut *self.completed.lock().unwrap())
    }

    pub(super) fn ready(
        lua: &Lua,
        value: Value,
        resources: crate::policy::ResourceBudget,
    ) -> mlua::Result<Table> {
        ticket(
            lua,
            NEXT_OPERATION.fetch_add(1, Ordering::Relaxed),
            Arc::new(Mutex::new(Some(Ok(value)))),
            resources,
        )
    }

    pub(super) fn spawn(
        self: &Arc<Self>,
        lua: &Lua,
        resources: crate::policy::ResourceBudget,
        operation: impl FnOnce() -> Outcome + Send + 'static,
    ) -> mlua::Result<Table> {
        if self.closed.load(Ordering::Acquire) {
            return Err(mlua::Error::runtime("backend task I/O is shut down"));
        }
        ACTIVE_HOST_WORKERS
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |active| {
                (active < MAX_HOST_WORKERS).then_some(active + 1)
            })
            .map_err(|_| mlua::Error::runtime("backend host I/O worker limit reached"))?;
        let id = NEXT_OPERATION.fetch_add(1, Ordering::Relaxed);
        let outcome = Arc::new(Mutex::new(None::<Outcome>));
        let result = Arc::clone(&outcome);
        let owner = Arc::clone(self);
        let spawn = std::thread::Builder::new()
            .name(format!("mesh-host-io-{id}"))
            .spawn(move || {
                struct WorkerGuard;
                impl Drop for WorkerGuard {
                    fn drop(&mut self) {
                        ACTIVE_HOST_WORKERS.fetch_sub(1, Ordering::AcqRel);
                    }
                }
                let _guard = WorkerGuard;
                if owner.closed.load(Ordering::Acquire) {
                    return;
                }
                let value = std::panic::catch_unwind(std::panic::AssertUnwindSafe(operation))
                    .unwrap_or_else(|_| Err("backend host I/O worker panicked".into()));
                let mut completed = owner.completed.lock().unwrap();
                if owner.closed.load(Ordering::Acquire) {
                    return;
                }
                *result.lock().unwrap() = Some(value);
                completed.push(id);
                drop(completed);
                owner.wakeup.notify_one();
            });
        if let Err(error) = spawn {
            ACTIVE_HOST_WORKERS.fetch_sub(1, Ordering::AcqRel);
            return Err(mlua::Error::external(error));
        }
        ticket(lua, id, outcome, resources)
    }
}

fn ticket(
    lua: &Lua,
    id: u64,
    outcome: Arc<Mutex<Option<Outcome>>>,
    resources: crate::policy::ResourceBudget,
) -> mlua::Result<Table> {
    let ticket = lua.create_table()?;
    ticket.set("__mesh_awaited_call", lua.create_userdata(AwaitedCall(id))?)?;
    let snapshot = Arc::new(Mutex::new(None::<Table>));
    ticket.set(
        "poll",
        lua.create_function(move |lua, ()| {
            if let Some(snapshot) = snapshot.lock().unwrap().as_ref() {
                return Ok(snapshot.clone());
            }
            let mut outcome = outcome.lock().unwrap();
            let result = lua.create_table()?;
            result.set("done", outcome.is_some())?;
            if let Some(value) = outcome.take() {
                match value {
                    Ok(value) => {
                        let bytes = resources
                            .validate_json(&value, "backend host operation")
                            .map_err(|error| mlua::Error::external(error.to_string()))?;
                        resources
                            .reserve_output(bytes)
                            .map_err(|error| mlua::Error::external(error.to_string()))?;
                        result.set("result", lua.to_value(&value)?)?;
                    }
                    Err(error) => result.set("error", error)?,
                }
                *snapshot.lock().unwrap() = Some(result.clone());
            }
            Ok(result)
        })?,
    )?;
    Ok(ticket)
}

/// Suspend a task on a host operation; synchronous callback callers retain
/// their existing behavior. Worker results are converted only on the VM thread.
pub(super) fn yielding_host_function(
    lua: &Lua,
    blocking: Function,
    start: Function,
) -> mlua::Result<Function> {
    let await_fn = realm_await_function(lua)?;
    lua.load(
        r#"
local blocking, start, await = ...
return function(...)
    if not coroutine.isyieldable() then return blocking(...) end
    local result = await(start(...))
    if result.error then error(result.error, 2) end
    return result.result
end
"#,
    )
    .set_name("=backend.host-operation")
    .call((blocking, start, await_fn))
}

pub(super) fn read_file(path: &str, limit: usize) -> Value {
    use std::io::Read;
    use std::os::unix::fs::OpenOptionsExt;
    let result = (|| -> Result<String, String> {
        let file = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NONBLOCK | libc::O_CLOEXEC)
            .open(path)
            .map_err(|error| error.to_string())?;
        if !file
            .metadata()
            .map_err(|error| error.to_string())?
            .is_file()
        {
            return Err("mesh.read_file requires a regular file".into());
        }
        let mut bytes = Vec::new();
        file.take(limit.saturating_add(1) as u64)
            .read_to_end(&mut bytes)
            .map_err(|error| error.to_string())?;
        if bytes.len() > limit {
            return Err(format!("file exceeds {limit} bytes"));
        }
        String::from_utf8(bytes).map_err(|error| error.to_string())
    })();
    match result {
        Ok(contents) => serde_json::json!({"success": true, "contents": contents, "error": ""}),
        Err(error) => serde_json::json!({"success": false, "contents": "", "error": error}),
    }
}
