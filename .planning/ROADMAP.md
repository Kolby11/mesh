# Roadmap: MESH v1.1 Backend Plugin MVP

**Created:** 2026-05-03
**Milestone:** v1.1 Backend Plugin MVP
**Granularity:** standard
**Phase numbering:** reset to Phase 1 after archiving v1.0 planning artifacts

## Milestone Goal

Make backend plugins stable enough for MVP: core backend concepts work predictably, plugins can run service logic, emit state, receive config, log, execute host commands, expose service contracts, and surface failures clearly.

## Phases

### Phase 1: Backend Lifecycle Foundation

**Goal:** Make backend plugin discovery, manifest validation, runtime creation, initialization, polling, and stop/restart behavior deterministic.

**Requirements:** BPLUG-01, BPLUG-02, BPLUG-03, BPLUG-04, BPLUG-05

**Success Criteria:**
1. Backend service plugin manifests with entrypoints validate before launch.
2. The shell creates exactly one active backend runtime per enabled provider.
3. `init()` runs once before polling or command dispatch.
4. Poll cadence honors plugin-controlled interval changes.
5. Runtime stop/restart leaves no stale poll loops or command receivers.

**Dependencies:** None
**UI hint:** no

### Phase 2: Backend Host API Contract

**Goal:** Stabilize the backend Luau host APIs needed by MVP service plugins: command execution, shell execution, config, logging, and poll interval control.

**Requirements:** BHOST-01, BHOST-02, BHOST-03, BHOST-04, BHOST-05

**Success Criteria:**
1. `mesh.exec(cmd, args)` returns stdout, stderr, and exit status for structured invocations.
2. `mesh.exec_shell(cmd)` returns stdout, stderr, and exit status for shell-style invocations.
3. `mesh.config()` returns plugin settings as a Luau table.
4. `mesh.log(level, msg)` emits plugin-scoped structured logs.
5. `mesh.service.set_poll_interval(ms)` affects subsequent backend polling without shell restart.

**Dependencies:** Phase 1
**UI hint:** no

### Phase 3: Service Provider Contract

**Goal:** Connect backend providers to service interfaces generically so state emission and command dispatch work without service-specific Rust branches.

**Requirements:** BSVC-01, BSVC-02, BSVC-03, BSVC-04, BSVC-05

**Success Criteria:**
1. Backend providers declare service interface/provider identity in manifest and interface metadata.
2. `mesh.service.emit(payload)` publishes JSON-compatible state under the correct provider.
3. The shell stores latest provider state for downstream consumers.
4. Service command requests route to backend Luau handlers generically.
5. Command success and failure results are visible through caller-facing results or diagnostics.

**Dependencies:** Phase 2
**UI hint:** no

### Phase 4: Backend Diagnostics and MVP Proof

**Goal:** Make backend plugin failures visible and prove the MVP backend contract with a fresh reference service plugin plus tests.

**Requirements:** BDIAG-01, BDIAG-02, BDIAG-03, BDIAG-04, BREF-01, BREF-02, BREF-03

**Success Criteria:**
1. Invalid manifests, missing entrypoints, missing contracts, init failures, poll failures, emit failures, and command failures produce clear diagnostics.
2. Backend plugin failures degrade health without crashing the shell where recovery is possible.
3. Repeated backend failures do not spam diagnostics every poll frame.
4. A fresh reference backend plugin exercises config, logging, polling, emission, and command handling.
5. Automated tests prove the reference backend plugin path and a short backend MVP reference note documents the pattern.

**Dependencies:** Phase 3
**UI hint:** no

## Traceability

| Requirement | Phase |
|-------------|-------|
| BPLUG-01 | Phase 1 |
| BPLUG-02 | Phase 1 |
| BPLUG-03 | Phase 1 |
| BPLUG-04 | Phase 1 |
| BPLUG-05 | Phase 1 |
| BHOST-01 | Phase 2 |
| BHOST-02 | Phase 2 |
| BHOST-03 | Phase 2 |
| BHOST-04 | Phase 2 |
| BHOST-05 | Phase 2 |
| BSVC-01 | Phase 3 |
| BSVC-02 | Phase 3 |
| BSVC-03 | Phase 3 |
| BSVC-04 | Phase 3 |
| BSVC-05 | Phase 3 |
| BDIAG-01 | Phase 4 |
| BDIAG-02 | Phase 4 |
| BDIAG-03 | Phase 4 |
| BDIAG-04 | Phase 4 |
| BREF-01 | Phase 4 |
| BREF-02 | Phase 4 |
| BREF-03 | Phase 4 |

**Coverage:**
- v1.1 requirements: 22 total
- Mapped to phases: 22
- Unmapped: 0

## Backlog

No backlog items yet.

---
*Roadmap created: 2026-05-03 after v1.1 reset*
