---
created: 2026-06-20T00:00:00.000Z
title: Shared surface VM for live cross-component references (Option B)
area: scripting
related_phases:
  - v1.17-vm-consolidation
files:
  - crates/core/runtime/scripting/src/pool.rs
  - crates/core/runtime/scripting/src/context/runtime.rs
  - crates/core/runtime/scripting/src/context/proxy.rs
  - crates/core/shell/src/shell/component/runtime.rs
  - crates/core/shell/src/shell/component/composition.rs
  - crates/core/shell/src/shell/component.rs
  - docs/llm-context.md
---

## Goal

Make `bind:this={child}` a **genuine live reference** to another component
instance, with DOM / Qt-same-thread / GObject semantics: reading a child field
reads the live value, calling a child function runs synchronously and returns
its real value, and the parent can subscribe to the child's events — all with
**no snapshot, no marshalling, no copy**.

This is "Option B": all component instances inside a single frontend surface
share **one Lua VM**, each component living in its own `_ENV` table. A live
cross-reference is then just `parent_env[binding] = child_public_view` — a real
table reference inside one shared heap, exactly how `el = document.querySelector`
works inside one JS realm, or how a `QObject*` works on one Qt thread.

Cross-*surface* communication (panel <-> launcher) is a separate, genuinely
isolated trust boundary and stays a marshalled bus — out of scope here.

## Why this is now small

v1.17 (Scripting VM Consolidation, phases 92-95) already built the foundation:
- Components isolate via per-instance `_ENV` tables with `__index = globals()`.
- Host API (`self`, `module`, `mesh`, `require`) installs into the `_ENV`, not
  globals — already per-component.
- Reactive public members are bare assignments landing in the `_ENV` table.

The ONLY thing preventing live references today is **VM ownership**: each
`ScriptContext` calls `pool::checkout()` and gets its *own* `Lua`. Two `_ENV`
tables in two different VMs cannot reference each other, which forces the current
snapshot-and-queue proxy. Collapse a surface's components onto one shared `Lua`
and the `_ENV` tables become mutually referenceable. `mlua` is built with the
`send` feature, so `Lua` is `Send + Clone` and clones share one VM — the handle
mechanism is already available.

## Current behavior being replaced

`install_bound_instance_proxy` (runtime.rs ~472) serializes the child's public
members to JSON, copies them into a parent-side Lua table refreshed once per
render, and turns child functions into stubs that push `BoundInstanceCall`s onto
a queue drained AFTER the parent handler finishes (`process_bound_instance_calls`
in component/runtime.rs ~498). Result: reads are up to one frame stale, calls are
fire-and-forget with no return value, and there is no child->parent event path.

## Plan (phased, each independently testable)

### Phase A.0 — Relocate per-component VM-global state to per-`_ENV` (PREREQUISITE)
Discovered during research: several per-component values are stashed on
`lua.globals()`, which works only because each component currently owns a private
VM. Under a shared VM these collide. Must move to the per-instance `_ENV` first.
- `__mesh_self_event_channels` (runtime.rs `self_event_channel`) — keyed by
  `module_id`; two instances of the SAME component (e.g. repeated `ItemRow` in a
  `{#for}`) would share `self.Changed`. **Must be per-instance** → store registry
  in `_ENV`.
- `__mesh_interface_event_channels` (proxy.rs `interface_event_channel`) — keyed
  by `service_name`; a shared channel would capture the FIRST subscriber's
  per-context `subscribed_interface_events` registry, breaking the routing work
  just shipped. **Must be per-instance** → store registry in `_ENV`.
- Thread the component `_ENV` `Table` into `self_event_channel`,
  `interface_event_channel`, `create_events_proxy`, `create_interface_proxy`,
  `create_service_proxy` (and the `__index` closures, which can capture the env
  Table) so registries are read/written on `_ENV`, not `globals()`.
- `__mesh_svc_*` and `__mesh_locale_current` are surface-wide-identical, so
  sharing is benign — leave as-is (they already read through `_ENV.__index ->
  globals`). `__mesh_request_redraw` is already per-`_ENV`.
- **Verify (local):** add a test putting two ScriptContexts on one shared VM and
  confirm their `self.Changed` channels and interface-event subscriptions are
  independent. `cargo test -p mesh-core-scripting`.

### Phase A — Shared surface VM ownership (infra)
- Introduce a VM handle abstraction in `ScriptContext` so it can run on an
  injected shared `Lua` instead of always doing its own `pool::checkout()`.
  Sketch: `enum ScriptVm { Pooled(pool::PooledVm), Shared(Lua) }` with
  `fn lua(&self) -> &Lua` for both arms; `uninit()` only returns the VM to the
  pool for the `Pooled` arm (the shared arm is owned by the surface).
- Add `ScriptContext::new_with_shared_vm(module_id, caps, lua, storage_root)`.
- `FrontendSurfaceComponent` owns one `surface_vm: Lua` created/checked out once
  at mount. Every `create_runtime_for_component` / `create_runtime` path builds
  its `ScriptContext` against a clone of that handle.
- Each component still gets its OWN `_ENV` table in the shared VM (unchanged
  isolation). Sandbox(true) on the shared VM keeps stdlib read-only.
- Backend modules and unit tests keep the pooled/owned path — no behavior change
  for them (backends are a separate trust boundary, correct to keep isolated).
- **Verify:** components in one surface share a VM; existing scripting tests stay
  green; `_ENV` isolation holds (component A cannot see component B's bare member
  unless explicitly handed a reference).

### Phase B — Live `bind:this` references
- Replace `install_bound_instance_proxy`'s snapshot/stub table with a
  **public-view proxy**: a small table whose metatable `__index` / `__newindex`
  forward to the child's live `_ENV`, gated to the child's known public member
  names (so host internals like `self`, `mesh`, `require` are not exposed). This
  preserves liveness (forwards to the real table, no copy) while keeping a
  curated surface — the Qt/GObject "property system" analog.
- `slider.percent` -> live read of child `_ENV.percent`.
- `slider.set_volume(50)` -> direct call of the child's real function, returning
  its real value synchronously (no queue).
- Because parent and child share the VM, no Rust round-trip is involved.
- **Verify:** parent reads a value the child mutated *within the same tick* and
  sees the new value; a bound call returns a value.

### Phase C — Child -> parent events
- Expose the child's self-named event channels (existing
  `create_event_channel`, `self.Changed:fire(...)`) through the public-view
  proxy so `slider.on("Changed", fn)` registers a real Lua closure that the
  child fires synchronously. No new marshalling — closures live in the shared VM.
- **Verify:** child `self.Changed:fire({...})` invokes the parent's registered
  callback in the same tick.

### Phase D — Remove dead machinery
- Delete `BoundInstanceCall`, `drain_bound_instance_calls`,
  `process_bound_instance_calls`, and the per-render snapshot path used only for
  binding. Keep `public_member_snapshot` only if still used elsewhere.
- Simplify `bind_child_instance` to install the live proxy once at mount /
  re-link on structural change rather than re-snapshotting every render.

### Phase E — Tests + docs
- Scripting-crate tests for: shared-VM isolation, live read, sync return, event
  subscription. (These run locally via `cargo test -p mesh-core-scripting`.)
- Shell-crate integration test for parent<->child liveness (compiles under
  `nix develop`; shell tests blocked locally by missing xkbcommon).
- Update `docs/llm-context.md` `bind:this` section to describe live-reference
  semantics. Add `project` memory.

## Constraints / watch-outs
- Single-threaded: all frontend script work runs on the shell event-loop thread
  (the `PooledVm` thread-affinity assert confirms this). The shared VM stays on
  that thread.
- Reload/teardown: removing a component must drop its `_ENV` from the shared VM
  to avoid leaks; reload rebuilds env tables but keeps (or recreates) the VM.
- Re-entrancy: with one VM there is no cross-VM locking; the current
  `Arc<Mutex<runtimes>>` borrow conflict that blocked Option A does NOT apply,
  because calls are direct Lua calls, not Rust re-entry into another runtime.
- Public-view proxy must not expose `self` / `module` / `mesh` / `require` or
  the `__functions` sentinel.

## Out of scope
- Cross-surface (surface-to-surface) live references — stays a marshalled bus.
- The `__newindex` dirty-tracking sync optimization (separate perf task).
