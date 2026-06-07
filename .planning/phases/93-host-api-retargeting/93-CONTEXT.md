# Phase 93: Host API Re-targeting - Context

**Gathered:** 2026-06-07
**Status:** Ready for planning

<domain>
## Phase Boundary

Mechanical refactoring: `install_host_api()` in `ScriptContext` (and `BackendScriptContext`) accepts a caller-supplied `&Table` target instead of hardcoding `lua.globals()`. Callers pass `lua.globals()` for now — no observable behavior change. Additionally, `LuaVmPool` captures a `pool_baseline_globals` snapshot (`Arc<HashSet<String>>`) at construction so `sync_state_from_lua` can distinguish stdlib entries from user-defined reactive state.

This phase does NOT change component behavior, does NOT wire `_ENV` isolation (Phase 94), and does NOT touch `ScriptContext` field layout.

</domain>

<decisions>
## Implementation Decisions

### install_host_api signature refactor

- **D-01:** Change `fn install_host_api(&mut self) -> Result<(), ScriptError>` in `crates/core/runtime/scripting/src/context/runtime.rs` to accept `target: &mlua::Table` and install all keys (`self`, `module`, `mesh.*`, `__mesh_svc_*`, `__mesh_request_redraw`, `__mesh_locale_current`) into `target` instead of `self.lua.globals()`. The single call site `install_host_api()` in the same file passes `&self.lua.globals()`.
- **D-02:** Functions registered in the host API that internally call `lua.globals()` (e.g., `mesh_ui_api.request_redraw`, `mesh_locale.current`) keep reading from `lua.globals()` at call time — these closures are NOT changed in this phase. The `target` parameter only controls where the API table keys are installed during setup. This is correct: Phase 94 will change those closure internals when `_ENV` is live.
- **D-03:** Apply the same signature refactor to `fn install_host_api(&mut self) -> mlua::Result<()>` in `crates/core/runtime/scripting/src/backend/runtime.rs`. Backend's call site passes `&self.lua.globals()`.

### pool_baseline_globals (ISO-04)

- **D-04:** Add `baseline_globals: Arc<HashSet<String>>` field to `LuaVmPool` in `crates/core/runtime/scripting/src/pool.rs`. Capture it once in `LuaVmPool::new()` by constructing a temporary VM, calling `sandbox(true)`, iterating its globals table, collecting all keys into a `HashSet<String>`, then discarding the temporary VM. The pool floor VMs are a separate set — do NOT capture from a floor VM (it would be consumed). The baseline is then stored as `Arc::new(set)`.
- **D-05:** Expose `pub fn baseline_globals(&self) -> Arc<HashSet<String>>` on `LuaVmPool` so Phase 94 can wire it into `ScriptContext`. No changes to `ScriptContext` in this phase.
- **D-06:** The existing `builtin_globals: HashSet<String>` field in `ScriptContext` is populated by a per-context walk after `install_host_api` runs. It captures stdlib + host API keys. `pool_baseline_globals` captures only stdlib keys (before host API). They serve related but distinct roles — do NOT remove or change `builtin_globals` in this phase.

### Scope boundaries

- **D-07:** Do NOT change any `ScriptContext` fields or constructor in this phase. The `lua: Lua` field stays as-is. That's Phase 94's job.
- **D-08:** All existing tests must pass unchanged — this is a pure structural refactor with no behavioral effect.

### Claude's Discretion

- Method visibility: `install_host_api` can remain `fn` (private) — no need to change visibility since it's always called internally.
- Error type: frontend context returns `Result<(), ScriptError>`, backend returns `mlua::Result<()>` — keep each as-is, just add the `target` parameter.

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Requirements

- `.planning/REQUIREMENTS.md` — ISO-02 (foundation work), ISO-04 — the two requirements this phase addresses

### Source files to modify

- `crates/core/runtime/scripting/src/context/runtime.rs` — `ScriptContext::install_host_api` (line ~563), call site (line ~160), `sync_state_from_lua` (line ~1003), `builtin_globals` field (line ~51)
- `crates/core/runtime/scripting/src/backend/runtime.rs` — `BackendScriptContext::install_host_api` (line ~466), call site (line ~133)
- `crates/core/runtime/scripting/src/pool.rs` — `LuaVmPool` struct to add `baseline_globals` field

### Already-built foundation (Phase 92)

- `crates/core/runtime/scripting/src/pool.rs` — `LuaVmPool`, `PooledVm` (Phase 92 deliverable)

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets

- `LuaVmPool::new(floor)` in `pool.rs` — already creates sandboxed VMs; the baseline globals capture uses the same `Lua::new() + sandbox(true)` pattern on a temporary VM
- `builtin_globals: HashSet<String>` in `ScriptContext` — similar pattern to the baseline capture; see lines 275-290 in `context/runtime.rs` for how it is populated post-install

### Established Patterns

- All host API writes currently go through `let globals = self.lua.globals();` at the top of `install_host_api` — the refactor changes this single binding to use the `target` parameter
- `Arc<HashSet<String>>` for shared immutable sets — used elsewhere in the codebase for capability sets; same pattern for `baseline_globals`

### Integration Points

- `install_host_api` is called from `initialize_script` in `context/runtime.rs` (line ~160) and from `BackendScriptContext::initialize` in `backend/runtime.rs` (line ~133)
- `sync_state_from_lua` reads `self.builtin_globals` — unchanged in this phase, but Phase 94 will complement it with the pool baseline
- The `thread_local! POOL` in `pool.rs` is per-thread; `baseline_globals` is process-wide (same pool init VM, deterministic stdlib)

</code_context>

<specifics>
## Specific Ideas

- The baseline globals capture should use a dedicated temporary `Lua::new()` + `sandbox(true)` call separate from the floor VMs, ensuring the floor VMs are all available for checkout. Create it, walk globals, drop it.
- The `install_host_api` refactor is straightforward: add `target: &Table` parameter, replace `globals.set(...)` calls with `target.set(...)`. Internal closure bodies (`lua.globals().get(...)`) are NOT touched.

</specifics>

<deferred>
## Deferred Ideas

- Wiring `pool_baseline_globals` into `ScriptContext` for use in `sync_state_from_lua` → Phase 94
- Replacing `self.lua.globals()` inside closure bodies with `_ENV` lookups → Phase 94
- `ensure_initialized()` lazy-init pattern → Phase 94

</deferred>

---

*Phase: 93-Host API Re-targeting*
*Context gathered: 2026-06-07*
