# Roadmap: MESH v1.1 Backend Plugin MVP

**Created:** 2026-05-03
**Milestone:** v1.1 Backend Plugin MVP
**Granularity:** standard
**Phase numbering:** reset to Phase 1 after archiving v1.0 planning artifacts

## Milestone Goal

Make backend plugins stable enough for MVP by first introducing a unified shell-owned plugin package manifest, then using it to drive backend lifecycle, provider selection, host APIs, service contracts, and diagnostics.

## Phases

### Phase 1: Plugin Package Manifest Foundation

**Goal:** Create the central package.json-like installed-plugin manifest and normalized plugin graph that frontend/backend plugin lifecycle and provider selection will use.

**Requirements:** PINST-01, PINST-02, PINST-03, PINST-04, PINST-05, PINST-06

**Success Criteria:**
1. A shell-owned package manifest lists user-installed frontend and backend plugins.
2. Frontend plugin entries can declare backend dependencies or required service categories.
3. Backend plugin entries declare category/service metadata and provider identity.
4. Multiple backend providers in the same category can be represented with an active user choice.
5. The shell can parse the manifest into a normalized installed-plugin graph for later lifecycle/provider selection phases.
6. The design remains local-first and does not require implementing remote download, signing, or marketplace behavior.

**Dependencies:** None
**UI hint:** no

### Phase 2: Backend Lifecycle Foundation

**Goal:** Use the installed-plugin graph to make backend plugin discovery, manifest validation, runtime creation, initialization, polling, and stop/restart behavior deterministic.

**Requirements:** BPLUG-01, BPLUG-02, BPLUG-03, BPLUG-04, BPLUG-05

**Success Criteria:**
1. Backend service plugin manifests with entrypoints validate before launch.
2. The shell creates exactly one active backend runtime per enabled provider choice.
3. `init()` runs once before polling or command dispatch.
4. Poll cadence honors plugin-controlled interval changes.
5. Runtime stop/restart leaves no stale poll loops or command receivers.

**Dependencies:** Phase 1
**UI hint:** no

### Phase 3: Backend Host API Contract

**Goal:** Stabilize the backend Luau host APIs needed by MVP service plugins: command execution, shell execution, config, logging, and poll interval control.

**Requirements:** BHOST-01, BHOST-02, BHOST-03, BHOST-04, BHOST-05

**Success Criteria:**
1. `mesh.exec(cmd, args)` returns stdout, stderr, and exit status for structured invocations.
2. `mesh.exec_shell(cmd)` returns stdout, stderr, and exit status for shell-style invocations.
3. `mesh.config()` returns plugin settings as a Luau table.
4. `mesh.log(level, msg)` emits plugin-scoped structured logs.
5. `mesh.service.set_poll_interval(ms)` affects subsequent backend polling without shell restart.

**Dependencies:** Phase 2
**UI hint:** no

### Phase 4: Service Provider Contract

**Goal:** Connect backend providers to service interfaces generically so state emission and command dispatch work without service-specific Rust branches.

**Requirements:** BSVC-01, BSVC-02, BSVC-03, BSVC-04, BSVC-05

**Success Criteria:**
1. Backend providers declare service interface/provider identity in manifest and interface metadata.
2. `mesh.service.emit(payload)` publishes JSON-compatible state under the correct provider.
3. The shell stores latest provider state for downstream consumers.
4. Service command requests route to backend Luau handlers generically.
5. Command success and failure results are visible through caller-facing results or diagnostics.

**Dependencies:** Phase 3
**UI hint:** no

### Phase 5: Backend Diagnostics and MVP Proof

**Goal:** Make backend plugin failures visible and prove the MVP backend contract with a fresh reference service plugin plus tests.

**Requirements:** BDIAG-01, BDIAG-02, BDIAG-03, BDIAG-04, BREF-01, BREF-02, BREF-03

**Success Criteria:**
1. Invalid manifests, missing entrypoints, missing contracts, init failures, poll failures, emit failures, and command failures produce clear diagnostics.
2. Backend plugin failures degrade health without crashing the shell where recovery is possible.
3. Repeated backend failures do not spam diagnostics every poll frame.
4. A fresh reference backend plugin exercises config, logging, polling, emission, and command handling.
5. Automated tests prove the reference backend plugin path and a short backend MVP reference note documents the pattern.

**Dependencies:** Phase 4
**UI hint:** no

## Traceability

| Requirement | Phase |
|-------------|-------|
| PINST-01 | Phase 1 |
| PINST-02 | Phase 1 |
| PINST-03 | Phase 1 |
| PINST-04 | Phase 1 |
| PINST-05 | Phase 1 |
| PINST-06 | Phase 1 |
| BPLUG-01 | Phase 2 |
| BPLUG-02 | Phase 2 |
| BPLUG-03 | Phase 2 |
| BPLUG-04 | Phase 2 |
| BPLUG-05 | Phase 2 |
| BHOST-01 | Phase 3 |
| BHOST-02 | Phase 3 |
| BHOST-03 | Phase 3 |
| BHOST-04 | Phase 3 |
| BHOST-05 | Phase 3 |
| BSVC-01 | Phase 4 |
| BSVC-02 | Phase 4 |
| BSVC-03 | Phase 4 |
| BSVC-04 | Phase 4 |
| BSVC-05 | Phase 4 |
| BDIAG-01 | Phase 5 |
| BDIAG-02 | Phase 5 |
| BDIAG-03 | Phase 5 |
| BDIAG-04 | Phase 5 |
| BREF-01 | Phase 5 |
| BREF-02 | Phase 5 |
| BREF-03 | Phase 5 |

**Coverage:**
- v1.1 requirements: 28 total
- Mapped to phases: 28
- Unmapped: 0

## Backlog

No backlog items yet.

---
*Roadmap updated: 2026-05-03 after Phase 1 package manifest pivot*
