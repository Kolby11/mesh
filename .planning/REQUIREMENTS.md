# Requirements: MESH v1.12 Module Object Contract

**Defined:** 2026-05-23
**Core Value:** MESH should let plugin authors build distinctive shell UI and service integrations while the shell stays observable, deterministic, and responsive on real interaction paths.

## v1 Requirements

### Module Instances

- [ ] **MOBJ-01**: Backend service providers are represented as stable runtime module object instances with module id, instance id, interface/version, lifecycle, capabilities, and active-provider metadata.
- [ ] **MOBJ-02**: Frontend modules are represented as stable runtime module object instances with module id, instance id, lifecycle, capabilities, exports, methods, and events.
- [ ] **MOBJ-03**: Debug or diagnostic state can inspect registered module instances without service-specific Rust branches.

### State And Exports

- [ ] **MSTATE-01**: Backend service `state` is the canonical durable read surface exposed as `module.state.<field>`.
- [ ] **MSTATE-02**: Frontend module public values are exposed as `module.exports.<field>` with the same replayable snapshot semantics as backend state.
- [ ] **MSTATE-03**: Latest state/export snapshots are replayed into newly created, shown, or reloaded runtimes so consumers do not depend on a future update to avoid nil fields.
- [ ] **MSTATE-04**: Existing compatibility aliases such as direct `audio.field` reads are either preserved behind the canonical model or diagnosed with an explicit migration path.

### Methods

- [ ] **MMETH-01**: Object method syntax such as `module:<method>(...)` routes through the shell-owned method/call lane.
- [ ] **MMETH-02**: Method calls are capability-checked, contract-checked, and routed to the correct active backend or frontend module instance.
- [ ] **MMETH-03**: Method handlers can return structured success or failure data that is visible to callers or at least to debug state.
- [ ] **MMETH-04**: Existing generated service proxy command methods continue to work while converging on the object method lane.

### Events

- [ ] **MEVT-01**: Interface and module event declarations are normalized into runtime metadata rather than remaining documentation-only.
- [ ] **MEVT-02**: Backend and frontend module instances can emit typed events that are validated against their declarations.
- [ ] **MEVT-03**: Consumers can subscribe with constrained object syntax such as `module.events.Name:subscribe(fn)`.
- [ ] **MEVT-04**: Event subscriptions are capability-checked and cleaned up deterministically on runtime, surface, or module teardown.

### Shipped Proof And Docs

- [ ] **MPROOF-01**: Bundled audio modules prove backend state reads, method calls, method result visibility, and at least one typed event path using canonical object syntax.
- [ ] **MPROOF-02**: Bundled navigation or another frontend module proves frontend exports or events through the same object-instance model.
- [ ] **MPROOF-03**: Regression tests cover state/export replay, method routing, result/failure visibility, event delivery, subscription cleanup, and capability denial.
- [ ] **MPROOF-04**: Author docs teach modules as class-like Luau object instances backed by typed runtime lanes for state/exports, methods, and events.

## Out of Scope

| Feature | Reason |
|---------|--------|
| Arbitrary cross-module shared memory | The contract should stay typed and shell-routed for observability and permission checks. |
| Unvalidated event payloads | Event declarations should become real runtime contracts. |
| General-purpose async framework | Method result handling should be enough for module calls without committing to a broad coroutine/task API. |
| Compositor-global shortcuts | Platform shortcut permissions are separate from the module object contract. |

## Traceability

| Requirement | Phase | Status |
|-------------|-------|--------|
| MOBJ-01 | Phase 65 | Planned |
| MOBJ-02 | Phase 65 | Planned |
| MOBJ-03 | Phase 65 | Planned |
| MSTATE-01 | Phase 66 | Planned |
| MSTATE-02 | Phase 66 | Planned |
| MSTATE-03 | Phase 66 | Planned |
| MSTATE-04 | Phase 66 | Planned |
| MMETH-01 | Phase 67 | Planned |
| MMETH-02 | Phase 67 | Planned |
| MMETH-03 | Phase 67 | Planned |
| MMETH-04 | Phase 67 | Planned |
| MEVT-01 | Phase 68 | Planned |
| MEVT-02 | Phase 68 | Planned |
| MEVT-03 | Phase 68 | Planned |
| MEVT-04 | Phase 68 | Planned |
| MPROOF-01 | Phase 69 | Planned |
| MPROOF-02 | Phase 69 | Planned |
| MPROOF-03 | Phase 69 | Planned |
| MPROOF-04 | Phase 69 | Planned |

**Coverage:**
- v1 requirements: 19 total
- Mapped to phases: 19
- Unmapped: 0

---
*Requirements defined: 2026-05-23*
