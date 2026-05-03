# Phase 1: Backend Host API Contract - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-05-01
**Phase:** 1-Backend Host API Contract
**Areas discussed:** API shape, command results and errors, config and logging, service emission and polling

---

## API Shape

| Option | Description | Selected |
|--------|-------------|----------|
| Preserve current public names | Keep the API names used by bundled backend plugins and stabilize behavior around them. | yes |
| Rename APIs before MVP | Change the public Luau surface before external users adopt it. | |
| Split compatibility and future APIs | Keep old names while introducing new canonical names immediately. | |

**User's choice:** Inferred from PROJECT.md and bundled plugin usage.
**Notes:** Existing plugin code already uses `mesh.exec_shell`, `mesh.service.emit`, and `mesh.service.set_poll_interval`; breaking these names would undercut the milestone's stabilization goal.

---

## Command Results and Errors

| Option | Description | Selected |
|--------|-------------|----------|
| Return structured command result tables | Preserve stdout/stderr/status/success for plugin logic. | yes |
| Throw on any nonzero command | Simpler but makes normal system command failures harder to handle in Luau. | |
| Return strings only | Too little information for backend services and diagnostics. | |

**User's choice:** Inferred from existing backend plugin behavior and requirements HOST-01/HOST-02.
**Notes:** Normal command failure should be data. API misuse and runtime failures should be visible as diagnostics or explicit errors.

---

## Config and Logging

| Option | Description | Selected |
|--------|-------------|----------|
| Implement milestone-required calls and preserve aliases | Support `mesh.config()` and `mesh.log(level, msg)` while keeping useful current method forms if present. | yes |
| Only keep current method forms | May fail the milestone's explicit API contract. | |
| Replace all aliases immediately | Risks unnecessary churn in bundled plugins. | |

**User's choice:** Inferred from requirements and current code comments.
**Notes:** Planning should reconcile the mismatch between requirements and current host API notes.

---

## Service Emission and Polling

| Option | Description | Selected |
|--------|-------------|----------|
| Stabilize `emit` and poll interval first | Matches HOST-05/HOST-06 and keeps Phase 1 backend-bound. | yes |
| Pull frontend proxy delivery into Phase 1 | Scope creep; belongs to Phase 2. | |
| Rewrite service event architecture | Too broad for Phase 1 unless planning proves it is required. | |

**User's choice:** Inferred from ROADMAP.md phase boundaries.
**Notes:** Phase 1 may test that backend updates are produced, but frontend proxy behavior is deferred.

---

## the agent's Discretion

- Exact Rust module split, diagnostic enum naming, and test organization.
- Whether to preserve `emit_json` and `emit_unavailable` as adjacent compatibility APIs beyond the explicit HOST requirements.

## Deferred Ideas

- Service proxy behavior, frontend reactivity, real surfaces, icon rendering, and documentation/reference plugin work are intentionally deferred to later roadmap phases.
