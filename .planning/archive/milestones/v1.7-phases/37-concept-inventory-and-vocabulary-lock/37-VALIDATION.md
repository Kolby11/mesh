---
phase: 37
slug: concept-inventory-and-vocabulary-lock
status: draft
nyquist_compliant: true
wave_0_complete: true
created: 2026-05-17
---

# Phase 37 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Shell file checks and ripgrep |
| **Config file** | none |
| **Quick run command** | `test -f docs/module-vocabulary.md && rg -n "D-01|D-02|D-03|D-04|D-20|D-21" docs/module-vocabulary.md` |
| **Full suite command** | `rg -n "CONC-01|CONC-02|CONC-03" .planning/phases/37-concept-inventory-and-vocabulary-lock/*-PLAN.md && test -f docs/module-vocabulary.md` |
| **Estimated runtime** | ~5 seconds |

---

## Sampling Rate

- **After every task commit:** Run the quick command for the plan's target files.
- **After every plan wave:** Run the full suite command plus each plan's verification commands.
- **Before `$gsd-verify-work`:** Full suite must be green.
- **Max feedback latency:** 10 seconds.

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 37-01-01 | 01 | 1 | CONC-01 | T-37-01-01 | N/A | file/grep | `test -f docs/module-vocabulary.md` | yes | pending |
| 37-01-02 | 01 | 1 | CONC-02 | T-37-01-02 | N/A | grep | `rg -n "Old term or shape|Canonical replacement|Disposition" docs/module-vocabulary.md` | yes | pending |
| 37-01-03 | 01 | 1 | CONC-03 | T-37-01-03 | N/A | grep | `rg -n "v1.1|v1.6|provider selection|keybind" docs/module-vocabulary.md` | yes | pending |
| 37-02-01 | 02 | 2 | CONC-01 | T-37-02-01 | N/A | grep | `rg -n "module-centered|module vocabulary|module.json" docs/module-system.md docs/extensibility.md docs/modules/README.md` | yes | pending |
| 37-02-02 | 02 | 2 | CONC-02 | T-37-02-02 | N/A | grep | `rg -n "public alias|compatibility alias|treat.*synonym" docs/module-system.md docs/extensibility.md docs/modules/README.md docs/modules/backend/core/README.md docs/health.md docs/theming/icons.md` | yes | pending |
| 37-03-01 | 03 | 2 | CONC-03 | T-37-03-01 | N/A | grep | `rg -n "ModulePackageManifest|RootPackageManifest|PackageSection|provides|implements|localized_triggers" docs/module-vocabulary.md` | yes | pending |

*Status: pending / green / red / flaky*

---

## Wave 0 Requirements

Existing infrastructure covers all phase requirements. This phase is documentation and planning-artifact focused.

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| End-user wording is understandable | CONC-01, CONC-02 | Readability cannot be fully proven by grep | Read `docs/module-vocabulary.md` end-user wording examples and confirm they avoid Rust/internal graph jargon. |
| Innovation remains allowed | CONC-03 | Requires conceptual review | Confirm the doc still allows base, extension, and independent interfaces plus new providers/resources/libraries. |

---

## Validation Sign-Off

- [x] All tasks have automated verify or Wave 0 dependencies
- [x] Sampling continuity: no 3 consecutive tasks without automated verify
- [x] Wave 0 covers all missing references
- [x] No watch-mode flags
- [x] Feedback latency < 10s
- [x] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
