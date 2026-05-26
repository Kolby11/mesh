# Requirements: MESH v1.14 Unified Luau Scripting Runtime

**Defined:** 2026-05-24
**Revised:** 2026-05-24
**Core Value:** MESH should let plugin authors build distinctive shell UI and service integrations while the shell stays observable, deterministic, and responsive on real interaction paths.

## v1 Requirements

### Scripting Context Core

- [ ] **LUACTX-01**: Frontend lifecycle hooks receive runtime-provided `self` as `render(self)`, `mount(self)`, and `unmount(self)`.
- [ ] **LUACTX-02**: Backend lifecycle hooks receive runtime-provided `self` as `start(self)` and `stop(self)`.
- [ ] **LUACTX-03**: `self.meta` exposes module id, component/provider id, runtime kind, runtime instance identity, and diagnostics identity.
- [ ] **LUACTX-04**: Legacy `module`, global `mesh`, `init`, `onRender`, and current handler behavior continue working during migration with diagnostics where appropriate.

### Require Resolver And Host APIs

- [ ] **LUAREQ-01**: Frontend and backend Luau runtimes share one documented `require(...)` resolution contract.
- [ ] **LUAREQ-02**: `require(...)` supports canonical shell API modules for current global `mesh` sub-APIs where already supported by host capabilities.
- [ ] **LUAREQ-03**: `require(...)` supports service/interface proxies with version constraints, preserving current `require("mesh.audio@>=1.0")` behavior.
- [ ] **LUAREQ-04**: `require(...)` supports Luau library modules and frontend component definitions without adding JavaScript-style import syntax.
- [ ] **LUAREQ-05**: Unsupported imports and capability denials produce consistent pcall-safe errors plus diagnostics across frontend and backend contexts.

### Public And Private Members

- [ ] **LUAMEM-01**: Lua `local` variables and functions are treated as private implementation details.
- [ ] **LUAMEM-02**: Non-local variables and functions are treated as public object members available to valid module/component consumers.
- [ ] **LUAMEM-03**: Lifecycle hooks are reserved runtime hooks, visible for diagnostics but not exposed as ordinary public API members.
- [ ] **LUAMEM-04**: Existing reactive global syncing remains compatible while docs and diagnostics rename the author-facing model to public members.

### Frontend Components And Binding

- [ ] **LUACOMP-01**: `.mesh` frontend modules can import local component files through `local Component = require("./component.mesh")`.
- [ ] **LUACOMP-02**: `.mesh` frontend modules can import module-provided frontend component definitions through the unified require model.
- [ ] **LUACOMP-03**: Existing `import Alias from "..."` component syntax remains compatible during migration and produces migration guidance.
- [ ] **LUACOMP-04**: Markup instantiates component definitions returned by require, while markup attributes write direct public fields on the mounted child instance.
- [ ] **LUACOMP-05**: `bind:this={name}` stores the mounted child instance reference so scripts can read public fields and call public functions.
- [ ] **LUACOMP-06**: Diagnostics identify definition-versus-instance misuse, missing files, missing module components, duplicate aliases, and unsupported require targets.

### Named Event Channels

- [ ] **LUAEVT-01**: Interface events are exposed as direct named channel objects on service proxies for declared PascalCase event names, for example `audio.VolumeChanged:on(fn)`.
- [ ] **LUAEVT-02**: Component/provider local events are exposed as named channel objects on `self`, for example `self.Changed:fire(payload)`.
- [ ] **LUAEVT-03**: Channel objects support `:on(fn)` subscriptions and `:fire(payload)` emission where emission is allowed by the local/provider contract.
- [ ] **LUAEVT-04**: Event subscriptions are lifecycle-bound and automatically cleaned up on component unmount or backend stop.
- [ ] **LUAEVT-05**: Existing `proxy.events.Name:subscribe`, `module.events`, `mesh.events.publish`, and backend `mesh.service.emit_event(...)` remain compatibility paths during v1.14.
- [ ] **LUAEVT-06**: Event names that conflict with methods, state fields, or public members produce diagnostics and require contract renaming.

### Automatic Rerendering

- [ ] **LUARERENDER-01**: Runtime render dependency tracking covers service state fields read during render.
- [ ] **LUARERENDER-02**: Runtime render dependency tracking covers locale and theme reads used during render.
- [ ] **LUARERENDER-03**: Runtime render dependency tracking covers bound public field reads used during render.
- [ ] **LUARERENDER-04**: Storage read dependency tracking is defined as part of the authoring contract but implemented with the persistent storage milestone.
- [ ] **LUARERENDER-05**: Affected frontend components rerender automatically when tracked dependencies change, without requiring normal authors to call explicit invalidation APIs.
- [ ] **LUARERENDER-06**: Existing explicit redraw/invalidation APIs remain available only as compatibility and debug escape hatches.

### Proof And Migration

- [ ] **LUAPROOF-01**: Shipped navigation and audio frontend examples use the new require/self/public-member/component-binding/event syntax where applicable.
- [ ] **LUAPROOF-02**: Shipped backend providers use `start(self)`, `stop(self)`, canonical require imports, public members, and named event channels where applicable.
- [ ] **LUAPROOF-03**: Compatibility examples remain only where explicitly labeled as compatibility or migration material.
- [ ] **LUAPROOF-04**: Author docs and LLM context describe the unified scripting runtime, migration paths, and compatibility window.
- [ ] **LUAPROOF-05**: Regression tests cover resolver behavior, self injection, public/private members, component binding, named events, automatic rerendering, and compatibility paths.

## Future Requirements

### Persistent Storage Milestone

- **LUASTORE-01**: `self.storage` becomes a component/provider instance-scoped persistent JSON-like key-value object.
- **LUASTORE-02**: Storage values allow only nil, boolean, number, string, arrays, and objects; functions, userdata, component definitions, component instances, and event channels are rejected with diagnostics.
- **LUASTORE-03**: Storage persists through atomic JSON files under the MESH/XDG data area with scope isolation by module id, component/provider identity, and runtime instance identity.
- **LUASTORE-04**: Storage loads before `mount/start`, flushes on `unmount/stop` and orderly shell shutdown, and recovers from corrupt JSON with diagnostics and empty scoped storage.
- **LUASTORE-05**: Storage writes integrate with render dependency tracking so watched storage values rerender affected components only when changed.

### Package Distribution

- **LUAPKG-01**: Remote package resolution and third-party dependency fetching remain future work.
- **LUAPKG-02**: Language-server import completion remains future work after the runtime contract is stable.

## Out of Scope

| Feature | Reason |
|---------|--------|
| Full persistent `self.storage` implementation in v1.14 | Storage is the next milestone after the scripting runtime contract lands. |
| Removing global `mesh` immediately | Existing scripts and shipped modules need a compatibility window. |
| Inventing JavaScript-style import syntax | The milestone should converge on Luau-native `require(...)` semantics first. |
| Remote package manager behavior | Runtime import semantics must be stable before distribution. |
| Compositor-global shortcuts | Unrelated to scripting runtime authoring. |

## Traceability

| Requirement | Phase | Status |
|-------------|-------|--------|
| LUACTX-01 | Phase 74 | Pending |
| LUACTX-02 | Phase 74 | Pending |
| LUACTX-03 | Phase 74 | Pending |
| LUACTX-04 | Phase 74 | Pending |
| LUAREQ-01 | Phase 75 | Pending |
| LUAREQ-02 | Phase 75 | Pending |
| LUAREQ-03 | Phase 75 | Pending |
| LUAREQ-04 | Phase 75 | Pending |
| LUAREQ-05 | Phase 75 | Pending |
| LUAMEM-01 | Phase 76 | Pending |
| LUAMEM-02 | Phase 76 | Pending |
| LUAMEM-03 | Phase 76 | Pending |
| LUAMEM-04 | Phase 76 | Pending |
| LUACOMP-01 | Phase 77 | Pending |
| LUACOMP-02 | Phase 77 | Pending |
| LUACOMP-03 | Phase 77 | Pending |
| LUACOMP-04 | Phase 77 | Pending |
| LUACOMP-05 | Phase 77 | Pending |
| LUACOMP-06 | Phase 77 | Pending |
| LUAEVT-01 | Phase 78 | Pending |
| LUAEVT-02 | Phase 78 | Pending |
| LUAEVT-03 | Phase 78 | Pending |
| LUAEVT-04 | Phase 78 | Pending |
| LUAEVT-05 | Phase 78 | Pending |
| LUAEVT-06 | Phase 78 | Pending |
| LUARERENDER-01 | Phase 79 | Pending |
| LUARERENDER-02 | Phase 79 | Pending |
| LUARERENDER-03 | Phase 79 | Pending |
| LUARERENDER-04 | Phase 79 | Pending |
| LUARERENDER-05 | Phase 79 | Pending |
| LUARERENDER-06 | Phase 79 | Pending |
| LUAPROOF-01 | Phase 80 | Pending |
| LUAPROOF-02 | Phase 80 | Pending |
| LUAPROOF-03 | Phase 80 | Pending |
| LUAPROOF-04 | Phase 80 | Pending |
| LUAPROOF-05 | Phase 80 | Pending |

**Coverage:**
- v1 requirements: 36 total
- Mapped to phases: 36
- Unmapped: 0

---
*Requirements revised: 2026-05-24*
