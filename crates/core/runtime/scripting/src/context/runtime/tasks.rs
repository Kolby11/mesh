//! The per-instance task scheduler behind the `task` library.
//!
//! Each `ScriptContext` owns one deadline queue. Entries hold registry
//! references to a Luau function (`task.delay`) or a suspended coroutine
//! (`task.spawn` + `task.sleep`), so a pending task never crosses into the
//! shell: the shell only asks for the earliest deadline and, once it passes,
//! calls [`ScriptContext::run_due_tasks`]. Cancelling is a local queue removal.
//! A task that awaits a service call (`ticket:await()`) is parked by call id
//! instead of by deadline and becomes due when
//! [`ScriptContext::complete_service_call`] delivers the result.
//! Tasks belong to the context and are dropped with it on unmount or reload.

use super::super::ScriptError;
use super::super::lookup::{lua_err, map_lua_error};
use super::*;
use mlua::{Function, MultiValue, RegistryKey, Table, Thread, ThreadStatus, Value as LuaValue};
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Pending tasks one instance may hold. A component needs a handful; the cap
/// bounds what a runaway loop can queue.
pub(crate) const MAX_PENDING_TASKS: usize = 256;

/// The longest delay `task.delay` and `task.sleep` accept, in milliseconds.
pub(crate) const MAX_TASK_DELAY_MS: f64 = 24.0 * 60.0 * 60.0 * 1000.0;

pub(crate) enum TaskBody {
    Call(RegistryKey),
    Resume(RegistryKey),
}

#[derive(Default)]
pub(crate) struct TaskQueue {
    next_id: u64,
    by_deadline: BTreeMap<(Instant, u64), TaskBody>,
    deadlines: HashMap<u64, Instant>,
    /// Coroutines waiting on a service call, keyed by call id.
    awaiting: HashMap<u64, (u64, RegistryKey)>,
    /// Task id to the call id it awaits, so a handle can cancel it.
    awaiting_calls: HashMap<u64, u64>,
}

impl std::fmt::Debug for TaskQueue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TaskQueue")
            .field("pending", &self.deadlines.len())
            .field("next_deadline", &self.next_deadline())
            .finish()
    }
}

impl TaskQueue {
    fn allocate_id(&mut self) -> mlua::Result<u64> {
        if self.len() >= MAX_PENDING_TASKS {
            return Err(mlua::Error::external(format!(
                "too many pending tasks (limit {MAX_PENDING_TASKS})"
            )));
        }
        self.next_id += 1;
        Ok(self.next_id)
    }

    fn insert(&mut self, deadline: Instant, body: TaskBody) -> mlua::Result<u64> {
        let id = self.allocate_id()?;
        self.by_deadline.insert((deadline, id), body);
        self.deadlines.insert(id, deadline);
        Ok(id)
    }

    /// Park a coroutine until `call_id` completes. A call has one awaiter; a
    /// second task awaiting the same call is an error rather than a lost wakeup.
    fn park(&mut self, id: Option<u64>, call_id: u64, thread: RegistryKey) -> mlua::Result<u64> {
        if self.awaiting.contains_key(&call_id) {
            return Err(mlua::Error::external(format!(
                "service call {call_id} is already awaited by another task"
            )));
        }
        let id = match id {
            Some(id) => id,
            None => self.allocate_id()?,
        };
        self.awaiting.insert(call_id, (id, thread));
        self.awaiting_calls.insert(id, call_id);
        Ok(id)
    }

    /// Make the task awaiting `call_id`, if any, due now.
    pub(crate) fn wake(&mut self, call_id: u64) {
        self.wake_at(call_id, Instant::now());
    }

    pub(crate) fn wake_at(&mut self, call_id: u64, deadline: Instant) {
        if let Some((id, thread)) = self.awaiting.remove(&call_id) {
            self.awaiting_calls.remove(&id);
            self.reinsert(id, deadline, TaskBody::Resume(thread));
        }
    }

    fn reinsert(&mut self, id: u64, deadline: Instant, body: TaskBody) {
        self.by_deadline.insert((deadline, id), body);
        self.deadlines.insert(id, deadline);
    }

    pub(crate) fn cancel(&mut self, id: u64) -> bool {
        if let Some(call_id) = self.awaiting_calls.remove(&id) {
            self.awaiting.remove(&call_id);
            return true;
        }
        let Some(deadline) = self.deadlines.remove(&id) else {
            return false;
        };
        self.by_deadline.remove(&(deadline, id));
        true
    }

    pub(crate) fn next_deadline(&self) -> Option<Instant> {
        self.by_deadline
            .keys()
            .next()
            .map(|(deadline, _)| *deadline)
    }

    pub(crate) fn pop_due(&mut self, now: Instant) -> Option<(u64, TaskBody)> {
        let (&(deadline, id), _) = self.by_deadline.first_key_value()?;
        if deadline > now {
            return None;
        }
        let body = self.by_deadline.remove(&(deadline, id))?;
        self.deadlines.remove(&id);
        Some((id, body))
    }

    pub(crate) fn clear(&mut self) {
        self.by_deadline.clear();
        self.deadlines.clear();
        self.awaiting.clear();
        self.awaiting_calls.clear();
    }

    pub(crate) fn len(&self) -> usize {
        self.deadlines.len() + self.awaiting.len()
    }
}

pub(crate) type SharedTaskQueue = Arc<Mutex<TaskQueue>>;

fn delay_ms(value: LuaValue, what: &str) -> mlua::Result<Duration> {
    let ms = match value {
        LuaValue::Integer(value) => value as f64,
        LuaValue::Number(value) => value,
        LuaValue::Nil => 0.0,
        _ => {
            return Err(mlua::Error::external(format!(
                "{what} expects a delay in milliseconds"
            )));
        }
    };
    if !ms.is_finite() || ms < 0.0 {
        return Err(mlua::Error::external(format!(
            "{what} delay must be a finite, non-negative number of milliseconds"
        )));
    }
    if ms > MAX_TASK_DELAY_MS {
        return Err(mlua::Error::external(format!(
            "{what} delay must not exceed {MAX_TASK_DELAY_MS} ms"
        )));
    }
    Ok(Duration::from_secs_f64(ms / 1000.0))
}

/// The values a coroutine yields to ask the scheduler to resume it later:
/// after a delay, or once a service call completes. Private to the realm, so
/// no script can forge either request.
const SLEEP_TOKEN_KEY: &str = "__mesh_task_sleep";
const AWAIT_TOKEN_KEY: &str = "__mesh_task_await";

fn task_handle(lua: &mlua::Lua, queue: &SharedTaskQueue, id: u64) -> mlua::Result<Table> {
    let handle = lua.create_table()?;
    handle.raw_set("id", id)?;
    let queue = Arc::clone(queue);
    handle.raw_set(
        "cancel",
        lua.create_function(move |_lua, _self: LuaValue| Ok(queue.lock().unwrap().cancel(id)))?,
    )?;
    Ok(handle)
}

/// Where [`drive`] left a coroutine.
pub(crate) enum Driven {
    /// It returned these values.
    Finished(MultiValue),
    /// It is queued or parked under this task id.
    Suspended(u64),
}

/// Run `thread` until it finishes or yields. A `task.sleep` yield is queued
/// for resumption under `id` (or a new id) and a `ticket:await()` yield is
/// parked on its call; any other yield ends the task with an error.
pub(crate) fn drive(
    lua: &mlua::Lua,
    queue: &SharedTaskQueue,
    thread: Thread,
    args: MultiValue,
    id: Option<u64>,
) -> mlua::Result<Driven> {
    let yielded = thread.resume::<MultiValue>(args)?;
    if thread.status() != ThreadStatus::Resumable {
        return Ok(Driven::Finished(yielded));
    }
    let mut values = yielded.into_iter();
    let first = values.next().unwrap_or(LuaValue::Nil);
    let is_token = |key: &str| -> mlua::Result<bool> {
        let token: LuaValue = lua.named_registry_value(key)?;
        Ok(!token.is_nil() && first.to_pointer() == token.to_pointer())
    };
    if is_token(AWAIT_TOKEN_KEY)? {
        let call_id = match values.next() {
            Some(LuaValue::UserData(call)) => call.borrow::<AwaitedCall>()?.0,
            _ => return Err(mlua::Error::external("ticket:await() lost its call id")),
        };
        let key = lua.create_registry_value(thread)?;
        return queue
            .lock()
            .unwrap()
            .park(id, call_id, key)
            .map(Driven::Suspended);
    }
    if !is_token(SLEEP_TOKEN_KEY)? {
        return Err(mlua::Error::external(
            "a task may only yield through task.sleep or ticket:await()",
        ));
    }
    let wait = delay_ms(values.next().unwrap_or(LuaValue::Nil), "task.sleep")?;
    let key = lua.create_registry_value(thread)?;
    let mut queue = queue.lock().unwrap();
    let deadline = Instant::now() + wait;
    match id {
        Some(id) => {
            queue.reinsert(id, deadline, TaskBody::Resume(key));
            Ok(Driven::Suspended(id))
        }
        None => queue
            .insert(deadline, TaskBody::Resume(key))
            .map(Driven::Suspended),
    }
}

const SLEEP_FUNCTION_KEY: &str = "__mesh_task_sleep_fn";

/// `task.sleep`, compiled once per realm. It has to be Luau because only Luau
/// code can yield; sharing it keeps every context from compiling its own copy.
fn realm_sleep_function(lua: &mlua::Lua) -> mlua::Result<Function> {
    if let Some(sleep) = lua.named_registry_value::<Option<Function>>(SLEEP_FUNCTION_KEY)? {
        return Ok(sleep);
    }
    let token = lua.create_table()?;
    lua.set_named_registry_value(SLEEP_TOKEN_KEY, &token)?;
    let sleep = lua
        .load(
            r#"
            local token = ...
            return function(ms)
                if not coroutine.isyieldable() then
                    error("task.sleep must run inside task.spawn", 2)
                end
                coroutine.yield(token, ms)
            end
            "#,
        )
        .set_name("=task.sleep")
        .call::<Function>(token)?;
    lua.set_named_registry_value(SLEEP_FUNCTION_KEY, &sleep)?;
    Ok(sleep)
}

const AWAIT_FUNCTION_KEY: &str = "__mesh_task_await_fn";

/// A ticket's call id, carried opaquely through the `await` yield. Frontend
/// call ids start at 2^63, beyond what a Luau number holds exactly.
pub(crate) struct AwaitedCall(pub(crate) u64);

impl mlua::UserData for AwaitedCall {}

/// The `await` of a service call ticket, compiled once per realm and bound to
/// each ticket. Inside `task.spawn` it suspends the task until the call
/// completes; elsewhere (a handler cannot yield) it returns the current
/// snapshot without blocking, as `poll` does.
pub(crate) fn realm_await_function(lua: &mlua::Lua) -> mlua::Result<Function> {
    if let Some(await_fn) = lua.named_registry_value::<Option<Function>>(AWAIT_FUNCTION_KEY)? {
        return Ok(await_fn);
    }
    let token = lua.create_table()?;
    lua.set_named_registry_value(AWAIT_TOKEN_KEY, &token)?;
    let await_fn = lua
        .load(
            r#"
            local token = ...
            return function(ticket)
                local snapshot = ticket.poll()
                if snapshot.done or not coroutine.isyieldable() then
                    return snapshot
                end
                repeat
                    coroutine.yield(token, ticket.__mesh_awaited_call)
                    snapshot = ticket.poll()
                until snapshot.done
                return snapshot
            end
            "#,
        )
        .set_name("=ticket.await")
        .call::<Function>(token)?;
    lua.set_named_registry_value(AWAIT_FUNCTION_KEY, &await_fn)?;
    Ok(await_fn)
}

pub(crate) fn create_task_library(
    lua: &mlua::Lua,
    shared_queue: &SharedTaskQueue,
) -> mlua::Result<Table> {
    let sleep = realm_sleep_function(lua)?;
    let task = lua.create_table()?;

    let queue = Arc::clone(shared_queue);
    task.set(
        "delay",
        lua.create_function(move |lua, (ms, callback): (LuaValue, Function)| {
            let wait = delay_ms(ms, "task.delay")?;
            let key = lua.create_registry_value(callback)?;
            let id = queue
                .lock()
                .unwrap()
                .insert(Instant::now() + wait, TaskBody::Call(key))?;
            task_handle(lua, &queue, id)
        })?,
    )?;

    let queue = Arc::clone(shared_queue);
    task.set(
        "spawn",
        lua.create_function(move |lua, (callback, args): (Function, MultiValue)| {
            let thread = lua.create_thread(callback)?;
            match drive(lua, &queue, thread, args, None)? {
                Driven::Suspended(id) => task_handle(lua, &queue, id).map(LuaValue::Table),
                Driven::Finished(_) => Ok(LuaValue::Nil),
            }
        })?,
    )?;

    task.set("sleep", sleep)?;

    let queue = Arc::clone(shared_queue);
    task.set(
        "cancel",
        lua.create_function(move |_lua, handle: LuaValue| {
            let id = match handle {
                LuaValue::Table(handle) => handle.raw_get::<Option<u64>>("id")?,
                LuaValue::Nil => None,
                _ => {
                    return Err(mlua::Error::external(
                        "task.cancel expects a handle from task.delay or task.spawn",
                    ));
                }
            };
            Ok(id.is_some_and(|id| queue.lock().unwrap().cancel(id)))
        })?,
    )?;

    Ok(task)
}

impl ScriptContext {
    pub(super) fn install_task_api(&mut self, globals: &Table) -> Result<(), ScriptError> {
        let lua = self.lua().clone();
        let task = create_task_library(&lua, &self.tasks).map_err(lua_err)?;
        globals.set("task", task).map_err(lua_err)
    }

    /// The earliest deadline among this instance's pending tasks.
    pub fn next_task_deadline(&self) -> Option<Instant> {
        self.tasks.lock().unwrap().next_deadline()
    }

    /// Pending tasks, for tests and diagnostics.
    pub fn pending_task_count(&self) -> usize {
        self.tasks.lock().unwrap().len()
    }

    /// Run every task whose deadline is at or before `now`, each under its own
    /// callback budget, then sync state and side channels once as a handler
    /// call does. A failing task is dropped and its error returned; the others
    /// still run. Tasks queued while this runs wait for the next call.
    pub fn run_due_tasks(&mut self, now: Instant) -> Vec<ScriptError> {
        let mut errors = Vec::new();
        if self
            .tasks
            .lock()
            .unwrap()
            .next_deadline()
            .is_none_or(|due| due > now)
        {
            return errors;
        }
        if let Err(error) = self.ensure_initialized() {
            self.tasks.lock().unwrap().clear();
            errors.push(error);
            return errors;
        }
        let lua = self.lua().clone();
        loop {
            let Some((id, body)) = self.tasks.lock().unwrap().pop_due(now) else {
                break;
            };
            let _budget = self.realm_policy.begin_callback();
            let result = match body {
                TaskBody::Call(key) => lua
                    .registry_value::<Function>(&key)
                    .and_then(|callback| lua.create_thread(callback))
                    .and_then(|thread| {
                        drive(&lua, &self.tasks, thread, MultiValue::new(), Some(id)).map(|_| ())
                    }),
                TaskBody::Resume(key) => lua.registry_value::<Thread>(&key).and_then(|thread| {
                    drive(&lua, &self.tasks, thread, MultiValue::new(), Some(id)).map(|_| ())
                }),
            };
            if let Err(error) = result {
                errors.push(map_lua_error(error));
            }
        }
        self.sync_state_from_lua();
        self.sync_side_channels();
        errors
    }
}
