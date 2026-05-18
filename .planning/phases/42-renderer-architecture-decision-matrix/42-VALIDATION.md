---
phase: 42
slug: renderer-architecture-decision-matrix
status: draft
nyquist_compliant: true
wave_0_complete: true
created: 2026-05-18
---

# Phase 42 - Validation Strategy

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Document gates with `rg` and `gsd-sdk` |
| **Config file** | `.planning/config.json` |
| **Quick run command** | `rg -n "REND-01|REND-02|REND-03" .planning/phases/42-renderer-architecture-decision-matrix/*-PLAN.md` |
| **Full suite command** | `gsd-sdk query check.decision-coverage-plan .planning/phases/42-renderer-architecture-decision-matrix .planning/phases/42-renderer-architecture-decision-matrix/42-CONTEXT.md` |
| **Estimated runtime** | ~5 seconds |

## Sampling Rate

- **After every task commit:** Run the task-specific `rg` checks listed below.
- **After every plan wave:** Run the full decision coverage command.
- **Before `$gsd-verify-work`:** All document existence and `rg` checks must pass.
- **Max feedback latency:** 10 seconds.

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 42-01-01 | 01 | 1 | REND-01 | T-42-01 | Source-backed evidence only | docs | `rg -n "Blitz|Taffy|Parley|AnyRender|Skia|Stylo|Winit|AccessKit|Muda|html5ever|xml5ever" .planning/phases/42-renderer-architecture-decision-matrix/42-SOURCE-INVENTORY.md` | Yes | pending |
| 42-01-02 | 01 | 1 | REND-02 | T-42-02 | Hard blockers separate from weighted tradeoffs | docs | `rg -n "determinism|retained invalidation|profiling|diagnostics|accessibility|Wayland shell fit|build cost|binary/dependency risk|migration effort" .planning/phases/42-renderer-architecture-decision-matrix/42-DECISION-MATRIX.md` | Yes | pending |
| 42-02-01 | 02 | 2 | REND-03 | T-42-03 | Outcomes are explicit and auditable | docs | `rg -n "Blitz|Skia/rust-skia|Stylo|Taffy|Parley|AnyRender|Winit|AccessKit|Muda|html5ever|xml5ever" .planning/phases/42-renderer-architecture-decision-matrix/42-DECISION-MATRIX.md` | Yes | pending |
| 42-02-02 | 02 | 2 | REND-01 | T-42-04 | Direct adoption blocked if overhead is too high | docs | `rg -n "Wayland shell model fit|browser-engine-level performance overhead|hard blocker" .planning/phases/42-renderer-architecture-decision-matrix/42-DECISION-MATRIX.md` | Yes | pending |
| 42-03-01 | 03 | 3 | REND-01 | T-42-05 | Phase 43 scope remains constrained | docs | `rg -n "navigation bar|audio popover|throwaway harness|visual output|interaction shape" .planning/phases/42-renderer-architecture-decision-matrix/42-PHASE43-HANDOFF.md` | Yes | pending |
| 42-03-02 | 03 | 3 | REND-01, REND-02, REND-03 | T-42-06 | Final package covers all decisions | docs | `gsd-sdk query check.decision-coverage-plan .planning/phases/42-renderer-architecture-decision-matrix .planning/phases/42-renderer-architecture-decision-matrix/42-CONTEXT.md` | Yes | pending |

## Wave 0 Requirements

Existing infrastructure covers all phase requirements.

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Final architectural judgement is defensible | REND-01 | The decision requires engineering judgement after source review | Read `42-DECISION-MATRIX.md`; verify the selected path follows the hard blockers and scorecard evidence. |

## Validation Sign-Off

- [x] All tasks have automated `rg` or `gsd-sdk` verification.
- [x] Sampling continuity: no 3 consecutive tasks without automated verify.
- [x] Wave 0 covers all MISSING references.
- [x] No watch-mode flags.
- [x] Feedback latency < 10s.
- [x] `nyquist_compliant: true` set in frontmatter.

**Approval:** approved 2026-05-18
