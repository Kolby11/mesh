# Requirements: MESH v1.14 Unified Luau Import Contract

**Defined:** 2026-05-24
**Core Value:** MESH should let plugin authors build distinctive shell UI and service integrations while the shell stays observable, deterministic, and responsive on real interaction paths.

## v1 Requirements

### Import Model

- [ ] **LUAIMP-01**: Frontend and backend Luau runtimes share one documented `require(...)` resolution contract.
- [ ] **LUAIMP-02**: `require(...)` supports canonical shell API modules for current global `mesh` sub-APIs, including locale, logging, events, UI/redraw, popover, service, and module object access where capabilities allow.
- [ ] **LUAIMP-03**: `require(...)` supports service/interface proxies with version constraints, preserving current `require("mesh.audio@>=1.0")` behavior.
- [ ] **LUAIMP-04**: `require(...)` failures have consistent pcall-safe errors and diagnostics across frontend and backend contexts.
- [ ] **LUAIMP-05**: `require(...)` defines a canonical module scope import for module-specific variables, persistent state, exports, and lifecycle metadata without requiring global `mesh`.

### Runtime Parity

- [ ] **LUART-01**: Backend Luau scripts can use the same canonical shell API and service/interface import paths as frontend scripts where their capabilities allow.
- [ ] **LUART-02**: Frontend Luau scripts can use the same canonical shell API and service/interface import paths as backend scripts where their capabilities allow.
- [ ] **LUART-03**: Capability checks for imported shell APIs and interfaces are enforced at require time and remain visible through diagnostics.
- [ ] **LUART-04**: Imported APIs preserve existing module object state/export/event semantics without requiring global `mesh`.
- [ ] **LUART-05**: Module-specific state imported through the canonical module scope has identical behavior in frontend and backend contexts for reads, writes, subscriptions/events, and lifecycle reload boundaries.
- [ ] **LUART-06**: Module-scoped variables are isolated by module identity so two modules cannot accidentally read or mutate each other's state through shared imports.

### Component Imports

- [ ] **LUACOMP-01**: `.mesh` frontend modules can import local component files through the unified require model.
- [ ] **LUACOMP-02**: `.mesh` frontend modules can import module-provided components through the unified require model.
- [ ] **LUACOMP-03**: Existing `import Alias from "..."` component syntax remains compatible during migration.
- [ ] **LUACOMP-04**: Component import diagnostics identify missing local files, missing module components, duplicate aliases, and unsupported require targets.

### Compatibility And Migration

- [ ] **LUACOMPAT-01**: Existing global `mesh` access continues to work during this milestone.
- [ ] **LUACOMPAT-02**: Existing explicit interface import syntax and compiled interface globals continue to work during this milestone.
- [ ] **LUACOMPAT-03**: Migration diagnostics and docs steer authors from global `mesh` and `.mesh import` syntax toward explicit `require(...)`.
- [ ] **LUACOMPAT-04**: Shipped navigation, audio popover, and backend providers continue to pass current runtime behavior while demonstrating the new import style.

### Documentation And Proof

- [ ] **LUADOC-01**: Author docs define canonical require namespaces, accepted module specifiers, version syntax, capability behavior, and pcall error handling.
- [ ] **LUADOC-02**: Frontend docs show component requires, service requires, shell API requires, and migration examples from old import/global syntax.
- [ ] **LUADOC-03**: Backend docs show the same require model for service APIs, shell host APIs, module object access, and library modules.
- [ ] **LUADOC-04**: Regression tests prove parser/compiler/runtime behavior on both synthetic fixtures and shipped modules.
- [ ] **LUADOC-05**: Author docs show how module-specific variables and state should be imported, named, persisted, observed, and reset across reloads.

## Future Requirements

### Named Imports

- **LUANAME-01**: A future syntax layer may support named imports if it can be expressed without making `.mesh` scripts diverge from normal Luau semantics.
- **LUANAME-02**: Tooling may provide autocomplete or linting for destructured require tables.

### Package Distribution

- **LUAPKG-01**: Remote package resolution and third-party dependency fetching remain future work.
- **LUAPKG-02**: Language-server import completion remains future work after the runtime contract is stable.

## Out of Scope

| Feature | Reason |
|---------|--------|
| Removing global `mesh` immediately | Existing scripts and shipped modules need a compatibility window. |
| Inventing JavaScript-style `import { x } from ...` syntax | The milestone should converge on Luau-native `require(...)` semantics first. |
| Remote package manager behavior | Runtime import semantics must be stable before distribution. |
| Compositor-global shortcuts | Unrelated to scripting imports. |
| Replacing module object state/export/event semantics | v1.12 established this runtime contract; v1.14 should import it, not redesign it. |

## Traceability

| Requirement | Phase | Status |
|-------------|-------|--------|
| LUAIMP-01 | Phase 74 | Pending |
| LUAIMP-02 | Phase 74 | Pending |
| LUAIMP-03 | Phase 74 | Pending |
| LUAIMP-04 | Phase 74 | Pending |
| LUAIMP-05 | Phase 74 | Pending |
| LUART-01 | Phase 75 | Pending |
| LUART-02 | Phase 75 | Pending |
| LUART-03 | Phase 75 | Pending |
| LUART-04 | Phase 75 | Pending |
| LUART-05 | Phase 75 | Pending |
| LUART-06 | Phase 75 | Pending |
| LUACOMP-01 | Phase 76 | Pending |
| LUACOMP-02 | Phase 76 | Pending |
| LUACOMP-03 | Phase 76 | Pending |
| LUACOMP-04 | Phase 76 | Pending |
| LUACOMPAT-01 | Phase 77 | Pending |
| LUACOMPAT-02 | Phase 77 | Pending |
| LUACOMPAT-03 | Phase 77 | Pending |
| LUACOMPAT-04 | Phase 77 | Pending |
| LUADOC-01 | Phase 78 | Pending |
| LUADOC-02 | Phase 78 | Pending |
| LUADOC-03 | Phase 78 | Pending |
| LUADOC-04 | Phase 78 | Pending |
| LUADOC-05 | Phase 78 | Pending |

**Coverage:**
- v1 requirements: 24 total
- Mapped to phases: 24
- Unmapped: 0

---
*Requirements defined: 2026-05-24*
