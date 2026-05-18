---
phase: 45
slug: renderer-migration-plan-and-author-contract
status: draft
nyquist_compliant: true
wave_0_complete: true
created: 2026-05-18
---

# Phase 45 - Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Documentation grep checks |
| **Config file** | none |
| **Quick run command** | `rg -n "MIGR-01|MIGR-02|MIGR-03|Feature flag|Linux/Nix|Binary|Rollback" docs/renderer-migration.md docs/renderer-ownership.md docs/frontend/renderer-contract.md` |
| **Full suite command** | `rg -n "authoritative|adapter-owned|replacement candidate|NodeId|typed invalidation|damage|profiling|diagnostics|AccessKit|theme-owned selection|renderer contract|renderer migration" docs/renderer-migration.md docs/renderer-ownership.md docs/frontend/renderer-contract.md docs/frontend/mesh-syntax.md docs/module-system.md` |
| **Estimated runtime** | ~2 seconds |

---

## Sampling Rate

- **After every task commit:** Run the task's `rg` verification commands.
- **After every plan wave:** Run the full suite command above.
- **Before `$gsd-verify-work`:** Full suite must be green.
- **Max feedback latency:** 2 seconds.

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 45-01-01 | 01 | 1 | MIGR-01 | T-45-01-01 | No broad rewrite approval | docs-grep | `rg -n "MIGR-01|phased|reversible|whole-renderer rewrite" docs/renderer-migration.md` | W0 | pending |
| 45-01-02 | 01 | 1 | MIGR-03 | T-45-01-02 | Dependency and rollback risks visible | docs-grep | `rg -n "Feature flag|Linux/Nix|Binary|Rollback|workspace tests" docs/renderer-migration.md` | W0 | pending |
| 45-02-01 | 02 | 1 | MIGR-02 | T-45-02-01 | Ownership classifications explicit | docs-grep | `rg -n "authoritative|adapter-owned|replacement candidate" docs/renderer-ownership.md` | W0 | pending |
| 45-02-02 | 02 | 1 | MIGR-02 | T-45-02-02 | Current code paths mapped | docs-grep | `rg -n "crates/core/frontend/render/src/render_object.rs|crates/core/frontend/render/src/display_list.rs|crates/core/presentation/src/lib.rs" docs/renderer-ownership.md` | W0 | pending |
| 45-03-01 | 03 | 2 | MIGR-01 | T-45-03-01 | Author contract avoids browser overpromise | docs-grep | `rg -n ".mesh is not HTML|Blitz is not the production authoring model|DOM/web platform behavior is not promised" docs/frontend/renderer-contract.md` | W0 | pending |
| 45-03-02 | 03 | 2 | MIGR-03 | T-45-03-02 | Existing docs route authors to contract | docs-grep | `rg -n "renderer contract|renderer migration" docs/frontend/mesh-syntax.md docs/module-system.md` | W0 | pending |

---

## Wave 0 Requirements

Existing documentation files and grep tooling cover all phase requirements.

---

## Manual-Only Verifications

All Phase 45 behaviors have automated grep verification. Human review may still check wording quality before merge.

---

## Validation Sign-Off

- [x] All tasks have automated verify commands.
- [x] Sampling continuity: no 3 consecutive tasks without automated verify.
- [x] Wave 0 covers all required references.
- [x] No watch-mode flags.
- [x] Feedback latency < 2s.
- [x] `nyquist_compliant: true` set in frontmatter.

**Approval:** approved 2026-05-18
