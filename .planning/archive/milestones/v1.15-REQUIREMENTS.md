# Requirements: MESH v1.15 Persistent Storage System

**Defined:** 2026-05-26
**Core Value:** MESH should let plugin authors build distinctive shell UI and service integrations while the shell stays observable, deterministic, and responsive on real interaction paths.

## v1 Requirements

### Storage Foundation

- [x] **STORECORE-01**: Storage scopes are derived from `self.meta` for frontend component instances and backend provider instances.
- [x] **STORECORE-02**: Storage data is private to the owning module/component/provider/runtime identity and cannot be read across unrelated modules.
- [x] **STORECORE-03**: Storage files live under the MESH/XDG data area using deterministic, sanitized, collision-resistant paths.
- [x] **STORECORE-04**: Storage documents support load, get, set, remove, clear, and snapshot operations through a shell-owned storage subsystem.
- [x] **STORECORE-05**: Persistence writes use temp-file plus rename semantics so partial writes do not corrupt the last valid document.
- [x] **STORECORE-06**: Corrupt or unreadable storage files recover non-fatally with diagnostics and an empty in-memory document.

### Luau `self.storage` Binding

- [x] **STOREAPI-01**: Frontend `render/mount/unmount` and backend `start/stop` contexts expose `self.storage`.
- [x] **STOREAPI-02**: `self.storage.key` and `self.storage["key"]` reads return persisted values or `nil` for missing keys.
- [x] **STOREAPI-03**: Assigning JSON-like values to `self.storage` updates in-memory state and schedules persistence.
- [x] **STOREAPI-04**: Assigning `nil` to a storage key removes that key from the scoped document.
- [x] **STOREAPI-05**: Storage values accept only nil, boolean, number, string, arrays, and plain objects.
- [x] **STOREAPI-06**: Unsupported values such as functions, userdata, component definitions, component instances, and event channels are rejected with non-fatal diagnostics.

### Lifecycle And Persistence

- [x] **STORELIFE-01**: Storage is loaded before frontend `mount/render` and backend `start` user code can read it.
- [x] **STORELIFE-02**: Storage flushes on frontend `unmount`, backend `stop`, and orderly shell shutdown.
- [x] **STORELIFE-03**: Multiple writes in one runtime turn coalesce without losing the latest in-memory value.
- [x] **STORELIFE-04**: Failed persistence attempts preserve in-memory state and emit observable diagnostics.
- [x] **STORELIFE-05**: Two instances of the same component/provider maintain isolated scoped storage unless they intentionally share the same runtime identity.

### Rerender Integration

- [x] **STORERENDER-01**: Storage reads during frontend render are tracked as render dependencies.
- [x] **STORERENDER-02**: Writes to a watched storage key rerender only components that read that key.
- [x] **STORERENDER-03**: Writes to unwatched storage keys do not trigger unrelated frontend rerenders.
- [x] **STORERENDER-04**: Existing explicit redraw/invalidation escape hatches remain compatibility/debug-only behavior.

### Proof, Diagnostics, And Docs

- [x] **STOREPROOF-01**: Regression tests cover path scoping, atomic persistence, corrupt-file recovery, invalid value diagnostics, and two-instance isolation.
- [x] **STOREPROOF-02**: Frontend runtime tests prove `self.storage` reads, writes, removes, snapshots, and rerender dependency behavior.
- [x] **STOREPROOF-03**: Backend runtime tests prove provider `self.storage` reads, writes, lifecycle flush, and invalid value diagnostics.
- [x] **STOREPROOF-04**: A shipped UI preference or provider setting uses `self.storage` as real product proof.
- [x] **STOREPROOF-05**: Author docs explain storage scope, supported value types, lifecycle timing, persistence location, and diagnostics.
- [x] **STOREPROOF-06**: Debug or health output exposes storage diagnostics without leaking stored private values.

## Future Requirements

### Elements Improvements Milestone

- **UIELEM-SELECT-01**: MESH markup supports first-class `<select>` and `<option>` elements.
- **UIELEM-SELECT-02**: Select controls render a visible vertical dropdown/popup for options instead of forcing authors to build horizontal custom menus.
- **UIELEM-SELECT-03**: Select controls support pointer selection, keyboard navigation, focus behavior, disabled states, and accessibility metadata.
- **UIELEM-SELECT-04**: Select controls expose value binding/change events that Luau components can use without bespoke per-control state plumbing.
- **UIELEM-SELECT-05**: The shipped navigation language selector uses the native select/dropdown element as proof.

### Package Distribution

- **LUAPKG-01**: Remote package resolution and third-party dependency fetching remain future work.
- **LUAPKG-02**: Language-server import completion remains future work after the runtime contract is stable.

## Out of Scope

| Feature | Reason |
|---------|--------|
| Cross-module storage reads | v1.15 storage is private instance-scoped persistence, not a shared database. |
| Schema-backed settings UI | Storage is a primitive; settings schemas and UI can build on it later. |
| Remote synchronization | Local deterministic persistence comes first. |
| Arbitrary userdata/function persistence | Storage must stay JSON-like, portable, diagnosable, and safe to serialize. |
| Database query language or indexing | The milestone is scoped to small scoped documents, not a general datastore. |
| Encryption/keychain integration | Sensitive secret storage needs separate threat modeling. |

## Traceability

| Requirement | Phase | Status |
|-------------|-------|--------|
| STORECORE-01 | Phase 81 | Complete |
| STORECORE-02 | Phase 81 | Complete |
| STORECORE-03 | Phase 81 | Complete |
| STORECORE-04 | Phase 81 | Complete |
| STORECORE-05 | Phase 81 | Complete |
| STORECORE-06 | Phase 81 | Complete |
| STOREAPI-01 | Phase 82 | Complete |
| STOREAPI-02 | Phase 82 | Complete |
| STOREAPI-03 | Phase 82 | Complete |
| STOREAPI-04 | Phase 82 | Complete |
| STOREAPI-05 | Phase 82 | Complete |
| STOREAPI-06 | Phase 82 | Complete |
| STORELIFE-01 | Phase 83 | Complete |
| STORELIFE-02 | Phase 83 | Complete |
| STORELIFE-03 | Phase 83 | Complete |
| STORELIFE-04 | Phase 83 | Complete |
| STORELIFE-05 | Phase 83 | Complete |
| STORERENDER-01 | Phase 84 | Complete |
| STORERENDER-02 | Phase 84 | Complete |
| STORERENDER-03 | Phase 84 | Complete |
| STORERENDER-04 | Phase 84 | Complete |
| STOREPROOF-01 | Phase 85 | Complete |
| STOREPROOF-02 | Phase 85 | Complete |
| STOREPROOF-03 | Phase 85 | Complete |
| STOREPROOF-04 | Phase 85 | Complete |
| STOREPROOF-05 | Phase 85 | Complete |
| STOREPROOF-06 | Phase 85 | Complete |

**Coverage:**
- v1 requirements: 27 total
- Mapped to phases: 27
- Unmapped: 0

---
*Requirements defined: 2026-05-26*
