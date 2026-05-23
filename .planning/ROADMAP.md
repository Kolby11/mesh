# Roadmap: MESH

## Milestones

- [ ] **v1.12 Module Object Contract** - Phases 65-69 planned
- [x] **v1.11 Surface Keybind Completion** - Phases 60-64 shipped 2026-05-23
- [x] **v1.10 Painter Engine** - Phases 51-59 shipped 2026-05-23
- [x] **v1.9 Renderer Library Integration** - Phases 46-50 shipped 2026-05-21
- [x] **v1.8 Rendering Engine Architecture** - Phases 42-45 shipped 2026-05-18

## Intent

Implement the module-object contract identified in Spike 003. Backend services and frontend modules should feel like Luau class instances with normal field, method, and event access while the Rust shell keeps ownership of routing, validation, permissions, replay, lifecycle, and diagnostics.

The target authoring model is "Lua objects over typed runtime lanes": durable data flows through replayable state/export snapshots, calls flow through typed methods with results, and transient facts flow through typed events with explicit subscriptions.

## Phase Summary

| # | Phase | Goal | Requirements | Success Criteria |
|---|-------|------|--------------|------------------|
| 65 | Module Instance Registry | Register backend and frontend modules as stable runtime object instances. | MOBJ-01, MOBJ-02, MOBJ-03 | Complete 2026-05-23 |
| 66 | State And Export Read Model | Make backend `state` and frontend `exports` first-class replayable object fields. | MSTATE-01, MSTATE-02, MSTATE-03, MSTATE-04 | Complete 2026-05-23 |
| 67 | Method Call Result Lane | Route object method calls through typed runtime calls with visible acknowledgements/results. | MMETH-01, MMETH-02, MMETH-03, MMETH-04 | `module:<method>(...)` routes through shell checks, reaches the target instance, and exposes result/failure data beyond tracing. |
| 68 | Typed Event Subscription Lane | Implement declared event emission, validation, subscription, and cleanup. | MEVT-01, MEVT-02, MEVT-03, MEVT-04 | `module.events.Name:subscribe(fn)` receives validated transient events and cleans up safely on runtime/module teardown. |
| 69 | Shipped Module Object Proof | Prove the completed object model on bundled audio/navigation modules and document the author contract. | MPROOF-01, MPROOF-02, MPROOF-03, MPROOF-04 | Shipped modules use canonical object syntax for state, exports, methods, and events with tests and docs. |

## Execution Rules

- Preserve the existing service model; evolve it into explicit module instances rather than adding one-off callback APIs.
- Keep Rust responsible for capabilities, schema validation, routing, replay, lifecycle cleanup, provider selection, and diagnostics.
- Standardize author examples on `module.state.field`, `module.exports.field`, `module:<method>(...)`, and `module.events.Name:subscribe(fn)`.
- Keep compatibility aliases only where needed for existing shipped modules, and diagnose or document migration paths.
- Prove behavior on real bundled modules, not only synthetic fixtures.

## Phases

### Phase 65: Module Instance Registry

**Goal:** Register backend services and frontend modules as stable runtime object instances with inspectable metadata.

**Requirements:** MOBJ-01, MOBJ-02, MOBJ-03

**Status:** Complete

**Success criteria:**
1. Backend service providers and frontend modules have stable object identities in a shell-owned registry.
2. Registry entries expose module id, instance id, interface/version, capabilities, lifecycle state, and active provider status where relevant.
3. Diagnostics and debug state can list registered instances without coupling to service-specific code.

### Phase 66: State And Export Read Model

**Goal:** Make backend `state` and frontend `exports` durable, replayable object fields.

**Requirements:** MSTATE-01, MSTATE-02, MSTATE-03, MSTATE-04

**Status:** Complete

**Success criteria:**
1. Backend state remains the authoritative replayable service read model.
2. Frontend modules can expose public export values through the same replayable snapshot semantics.
3. Runtime creation, surface show, reload, and subscription boundaries replay latest snapshots into the consuming Luau runtime.
4. Author-facing examples use canonical `module.state.field` and `module.exports.field` reads.

### Phase 67: Method Call Result Lane

**Goal:** Route object method calls through typed shell-managed calls and expose acknowledgements/results.

**Requirements:** MMETH-01, MMETH-02, MMETH-03, MMETH-04

**Status:** Planned

**Success criteria:**
1. `module:<method>(...)` and existing generated proxy methods route through one shell method/call lane.
2. Calls are capability-checked, contract-checked, and target the correct active module instance.
3. Backend/frontend method handlers can return structured result or failure data.
4. Debug state records recent method calls, target instance, result, failure, and coalescing behavior where applicable.

### Phase 68: Typed Event Subscription Lane

**Goal:** Turn declared interface/module events into a real runtime subscription mechanism.

**Requirements:** MEVT-01, MEVT-02, MEVT-03, MEVT-04

**Status:** Planned

**Success criteria:**
1. Interface and module event declarations are normalized into runtime event metadata.
2. Backend and frontend module instances can emit typed events validated against declarations.
3. Consumers can subscribe with `module.events.Name:subscribe(fn)` and receive event payloads in Luau.
4. Subscriptions are capability-checked and cleaned up on runtime, surface, or module teardown.

### Phase 69: Shipped Module Object Proof

**Goal:** Prove the full object contract on bundled audio/navigation modules and publish author guidance.

**Requirements:** MPROOF-01, MPROOF-02, MPROOF-03, MPROOF-04

**Status:** Planned

**Success criteria:**
1. Bundled audio modules use canonical object syntax for state reads, method calls, and at least one typed event proof.
2. Bundled navigation/frontend modules expose at least one frontend export or event through the object model.
3. Regression tests cover state replay, method results, event delivery, subscription cleanup, and capability denial.
4. Author docs describe backend and frontend modules as class-like object instances over typed runtime lanes.

## Backlog

### Future: Compositor-Global Shortcuts

Compositor-global shortcuts remain deferred until focused-surface and module-instance contracts are stable. Future work needs platform/session permission design, diagnostics separate from focused surfaces, and likely XDG Desktop Portal or compositor-specific integration.

### Future: Keybind Settings UI

A full user-facing remapping UI remains deferred. v1.11 validates override schema and runtime behavior so a later settings surface can safely inspect and modify overrides.

### Future: Generated Access Keys

Automatic locale-aware access-key generation remains deferred. v1.11 keeps authors responsible for explicit localized trigger defaults.
