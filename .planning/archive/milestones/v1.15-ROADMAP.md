# Roadmap: MESH

## Milestones

- ✅ **v1.15 Persistent Storage System** — Phases 81-85 complete 2026-05-26
- ⏭️ **v1.16 Elements Improvements** — queued after v1.15
- ✅ **v1.14 Unified Luau Scripting Runtime** — Phases 74-80 shipped 2026-05-26 ([archive](milestones/v1.14-ROADMAP.md))
- ✅ **v1.13 Manifest I18n Contract** — Phases 70-73 shipped 2026-05-24 ([archive](milestones/v1.13-ROADMAP.md))
- ✅ **v1.12 Module Object Contract** — Phases 65-69 shipped 2026-05-23 ([archive](milestones/v1.12-ROADMAP.md))
- ✅ **v1.11 Surface Keybind Completion** — Phases 60-64 shipped 2026-05-23 ([archive](milestones/v1.11-ROADMAP.md))

## Intent

Implement `self.storage` as shell-backed, component/provider instance-scoped
persistent key-value storage using atomic JSON files under the MESH/XDG data
area.

This milestone builds directly on v1.14's `self.meta` identity and render
dependency foundations. Storage is deliberately scoped as a small JSON-like
document primitive, not a settings schema, shared database, or remote sync
system.

## Phase Summary

| # | Phase | Goal | Requirements | Success Criteria |
|---|-------|------|--------------|------------------|
| 81 | Storage Foundation | Add the shell-owned scoped storage subsystem, path scoping, document operations, atomic writes, and corrupt-file recovery. | STORECORE-01, STORECORE-02, STORECORE-03, STORECORE-04, STORECORE-05, STORECORE-06 | ✅ 6/6 |
| 82 | Luau Self Storage Binding | Expose `self.storage` to frontend/backend runtimes with table-like reads, writes, deletes, snapshots, and invalid value diagnostics. | STOREAPI-01, STOREAPI-02, STOREAPI-03, STOREAPI-04, STOREAPI-05, STOREAPI-06 | ✅ 6/6 |
| 83 | Lifecycle Persistence | Load storage before lifecycle user code, coalesce writes, flush on teardown/shutdown, and preserve instance isolation. | STORELIFE-01, STORELIFE-02, STORELIFE-03, STORELIFE-04, STORELIFE-05 | ✅ 5/5 |
| 84 | Storage Rerender Integration | Track render-time storage reads and rerender only components whose watched keys changed. | STORERENDER-01, STORERENDER-02, STORERENDER-03, STORERENDER-04 | ✅ 4/4 |
| 85 | Storage Proof And Docs | Prove storage through tests, shipped UI/provider usage, diagnostics, and author documentation. | STOREPROOF-01, STOREPROOF-02, STOREPROOF-03, STOREPROOF-04, STOREPROOF-05, STOREPROOF-06 | ✅ 6/6 |

## Execution Rules

- Use v1.14 `self.meta` identity as the source of storage scope. Do not invent a parallel identity model.
- Keep storage shell-owned. Luau code sees `self.storage`; Rust owns persistence, validation, diagnostics, and lifecycle flushing.
- Store only JSON-like values: nil, boolean, number, string, arrays, and plain objects.
- Reject functions, userdata, component definitions, component instances, event channels, and other non-serializable values with non-fatal diagnostics.
- Persist with temp-file plus rename. Never leave partial writes as the canonical document.
- Treat corrupt files as recoverable diagnostics; do not crash the shell or module runtime.
- Keep storage private to the owning module/component/provider/runtime scope.
- Integrate with the existing render dependency model instead of adding a separate frontend invalidation path.
- Prove behavior with shipped runtime paths, not only synthetic unit tests.

## Phases

- [x] Phase 81: Storage Foundation
- [x] Phase 82: Luau Self Storage Binding
- [x] Phase 83: Lifecycle Persistence
- [x] Phase 84: Storage Rerender Integration
- [x] Phase 85: Storage Proof And Docs

### Phase 81: Storage Foundation

**Goal:** Add the shell-owned scoped storage subsystem, path scoping, document operations, atomic writes, and corrupt-file recovery.

**Requirements:** STORECORE-01, STORECORE-02, STORECORE-03, STORECORE-04, STORECORE-05, STORECORE-06

**Status:** Complete — 2026-05-26

**Success criteria:**
1. Storage scope derives from `self.meta` for frontend component instances and backend providers.
2. Storage documents are private to module/component/provider/runtime identity.
3. Storage paths are deterministic, sanitized, collision-resistant, and rooted under the MESH/XDG data area.
4. The shell storage subsystem supports load, get, set, remove, clear, and snapshot operations.
5. Writes persist through temp-file plus rename.
6. Corrupt or unreadable storage files recover non-fatally with diagnostics and empty in-memory storage.

### Phase 82: Luau Self Storage Binding

**Goal:** Expose `self.storage` to frontend/backend runtimes with table-like reads, writes, deletes, snapshots, and invalid value diagnostics.

**Requirements:** STOREAPI-01, STOREAPI-02, STOREAPI-03, STOREAPI-04, STOREAPI-05, STOREAPI-06

**Status:** Complete — 2026-05-26

**Success criteria:**
1. Frontend and backend lifecycle contexts expose `self.storage`.
2. `self.storage.key` and `self.storage["key"]` reads return persisted values or `nil`.
3. Assigning supported values updates in-memory storage and schedules persistence.
4. Assigning `nil` removes keys.
5. Supported values are limited to JSON-like values.
6. Unsupported values are rejected with non-fatal diagnostics.

### Phase 83: Lifecycle Persistence

**Goal:** Load storage before lifecycle user code, coalesce writes, flush on teardown/shutdown, and preserve instance isolation.

**Requirements:** STORELIFE-01, STORELIFE-02, STORELIFE-03, STORELIFE-04, STORELIFE-05

**Status:** Complete — 2026-05-26

**Success criteria:**
1. Storage loads before frontend `mount/render` and backend `start` can read it.
2. Storage flushes on frontend `unmount`, backend `stop`, and orderly shell shutdown.
3. Multiple writes in one runtime turn coalesce without losing latest in-memory values.
4. Persistence failures preserve in-memory state and emit diagnostics.
5. Two instances of the same component/provider keep isolated scoped storage unless they intentionally share identity.

### Phase 84: Storage Rerender Integration

**Goal:** Track render-time storage reads and rerender only components whose watched keys changed.

**Requirements:** STORERENDER-01, STORERENDER-02, STORERENDER-03, STORERENDER-04

**Status:** Complete — 2026-05-26

**Success criteria:**
1. Frontend render reads of `self.storage` keys are tracked as dependencies.
2. Writes to watched keys rerender only the affected components.
3. Writes to unwatched keys do not trigger unrelated rerenders.
4. Explicit redraw/invalidation APIs remain compatibility/debug escape hatches.

### Phase 85: Storage Proof And Docs

**Goal:** Prove storage through tests, shipped UI/provider usage, diagnostics, and author documentation.

**Requirements:** STOREPROOF-01, STOREPROOF-02, STOREPROOF-03, STOREPROOF-04, STOREPROOF-05, STOREPROOF-06

**Status:** Complete — 2026-05-26

**Success criteria:**
1. Regression tests cover scoping, atomic persistence, corrupt recovery, invalid diagnostics, and two-instance isolation.
2. Frontend runtime tests prove reads, writes, deletes, snapshots, and rerender dependencies.
3. Backend runtime tests prove provider storage, lifecycle flush, and invalid diagnostics.
4. A shipped UI preference or provider setting uses `self.storage`.
5. Author docs explain scope, value types, lifecycle timing, persistence location, and diagnostics.
6. Debug or health output exposes storage diagnostics without leaking private stored values.

## Queued Milestone: v1.16 Elements Improvements

**Goal:** Add common native markup controls that reduce custom component
workarounds and improve shipped UI behavior.

**Planned scope:**

- First-class `<select>` and `<option>` element support in MESH markup
- Visible dropdown/popup behavior with vertical option layout
- Keyboard navigation, focus, selection, disabled states, and accessibility metadata
- Value binding/change events suitable for Luau component state
- Styling hooks that fit the existing shell CSS profile without requiring browser CSS compatibility
- Shipped proof by replacing the navigation bar language selector's horizontal custom menu

## Backlog

### Future: Package Distribution

Remote package fetching, third-party dependency resolution, and LSP import
completion remain future work after the runtime import contract is stable.
