# Phase 45: Renderer Migration Plan and Author Contract - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md - this log preserves the alternatives considered.

**Date:** 2026-05-18
**Phase:** 45-Renderer Migration Plan and Author Contract
**Areas discussed:** Migration sequence, renderer ownership classification, `.mesh` author contract, build/CI/release guardrails, deferred todo handling

---

## Migration Sequence

| Option | Description | Selected |
|--------|-------------|----------|
| Phased reversible adapter expansion | Expand the Phase 44 focused proof boundary into later production migration steps while current renderer ownership remains authoritative until gates pass. | Yes |
| Module-by-module replacement first | Start by replacing renderer modules directly before stabilizing adapter seams and gates. | |
| Broad renderer rewrite | Treat Phase 45 as approval for a whole-renderer rewrite path. | |

**User's choice:** User selected all discussion areas; recommended default selected.
**Notes:** This matches Phase 44's constrained proof result and keeps MIGR-01 reversible.

---

## Renderer Ownership Classification

| Option | Description | Selected |
|--------|-------------|----------|
| Authoritative/adapter/replacement classification | Classify existing MESH runtime/render/presentation as authoritative, Phase 44 proof boundaries as adapter-owned, and candidate crates as future replacements. | Yes |
| Candidate-crate-first classification | Treat focused crates as authoritative immediately and classify current MESH modules as legacy. | |
| Single flat list | Document modules without migration ownership status. | |

**User's choice:** User selected all discussion areas; recommended default selected.
**Notes:** This directly supports MIGR-02 and prevents the contract from implying author-facing behavior that has not shipped.

---

## `.mesh` Author Contract

| Option | Description | Selected |
|--------|-------------|----------|
| Stability contract with explicit non-goals | Preserve current `.mesh` authoring behavior while documenting renderer migration effects, stable semantics, and unsupported browser-like expectations. | Yes |
| New renderer API contract | Expose Phase 44 proof snapshots or crate-specific concepts as public author APIs. | |
| Minimal internal-only note | Document migration only for core maintainers and omit plugin author impact. | |

**User's choice:** User selected all discussion areas; recommended default selected.
**Notes:** The contract should explain what the renderer decision means for shipped shell surfaces and plugin-authored `.mesh` UI without promising Blitz/browser semantics.

---

## Build, CI, Release, And Rollback Guardrails

| Option | Description | Selected |
|--------|-------------|----------|
| Gate every migration step | Require feature/rollback path, dependency notes, Nix/Linux implications, binary/build risk, workspace tests, shipped-surface regressions, and observability parity. | Yes |
| Test-only acceptance | Rely on passing tests without explicit dependency, release, or rollback documentation. | |
| Dependency-first rollout | Add candidate crates broadly, then work backward to tests and rollback criteria. | |

**User's choice:** User selected all discussion areas; recommended default selected.
**Notes:** Phase 42 explicitly included performance, build cost, binary size, and dependency burden as renderer architecture concerns.

---

## Deferred Todo Handling

| Option | Description | Selected |
|--------|-------------|----------|
| Review and mostly defer | Keep audio transition polish and module installer requirement resolution out of Phase 45, while citing Blitz dependency evidence already handled by Phase 42/43. | Yes |
| Fold all matched todos | Expand Phase 45 into audio transition polish, module installer resolution, and renderer migration planning. | |
| Ignore matched todos | Omit discussion of pending matched todos entirely. | |

**User's choice:** User selected all discussion areas; recommended default selected.
**Notes:** The module install requirement todo may be referenced only where the author contract mentions module requirements. The audio polish todo remains unrelated to renderer migration planning.

---

## the agent's Discretion

- The user chose `all` after gray areas were presented, so all recommended defaults were accepted.
- Exact migration-plan structure, contract file name, and section ordering are left to planning.
- The context favors conservative source-backed planning because Phase 45 is a documentation/planning phase before broad renderer adoption.

## Deferred Ideas

- Audio popover transition delay polish remains deferred as accepted Phase 31 polish debt.
- Module install requirement resolution remains a separate pending module-system task.
