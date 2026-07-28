---
phase: 06
slug: milestone-planning-reconciliation
status: draft
nyquist_compliant: true
wave_0_complete: true
created: 2026-05-05
---

# Phase 06 - Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Static markdown verification via `grep` and manual file review |
| **Config file** | none |
| **Quick run command** | `grep -n "mesh.exec_shell\\|milestone_complete" .planning/ROADMAP.md .planning/REQUIREMENTS.md .planning/STATE.md || true` |
| **Full suite command** | `grep -n "BSVC-01\\|BSVC-02\\|BSVC-04\\|BSVC-05\\|BDIAG-01\\|BDIAG-02\\|BDIAG-03\\|BDIAG-04\\|BREF-01\\|BREF-02\\|BREF-03" .planning/REQUIREMENTS.md && grep -n "mesh.exec_shell" .planning/ROADMAP.md .planning/REQUIREMENTS.md || true && grep -n "milestone_complete" .planning/STATE.md || true` |
| **Estimated runtime** | ~10 seconds |

---

## Sampling Rate

- **After every task commit:** Run the plan-local quick command.
- **After every plan wave:** Run the full suite command above.
- **Before `$gsd-verify-work`:** Full suite must be green.
- **Max feedback latency:** 10 seconds.

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 06-01-01 | 01 | 1 | N/A | T-06-01 | Requirement checkboxes and traceability rows match the verified milestone evidence | static | `grep -n "BSVC-01\\|BSVC-02\\|BSVC-04\\|BSVC-05\\|BDIAG-01\\|BDIAG-02\\|BDIAG-03\\|BDIAG-04\\|BREF-01\\|BREF-02\\|BREF-03" .planning/REQUIREMENTS.md` | yes | pending |
| 06-01-02 | 01 | 1 | N/A | T-06-02 | Planning docs no longer advertise public `mesh.exec_shell` as the MVP contract | static | `grep -n "mesh.exec_shell" .planning/ROADMAP.md .planning/REQUIREMENTS.md || true` | yes | pending |
| 06-01-03 | 01 | 1 | N/A | T-06-03 | State file no longer claims milestone completion before archive closeout | static | `grep -n "milestone_complete" .planning/STATE.md || true` | yes | pending |

*Status: pending until execution.*

---

## Wave 0 Requirements

Existing infrastructure covers all phase requirements.

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Reconciled milestone narrative reads consistently across planning files | N/A | Wording quality and archive readiness require human review in addition to `grep` checks | Read `.planning/ROADMAP.md`, `.planning/REQUIREMENTS.md`, and `.planning/STATE.md` together after execution and confirm they tell the same Phase 03 and milestone-close story. |

---

## Validation Sign-Off

- [x] All tasks have automated verify commands or existing Wave 0 infrastructure.
- [x] Sampling continuity: no 3 consecutive tasks without automated verify.
- [x] Wave 0 covers all MISSING references.
- [x] No watch-mode flags.
- [x] Feedback latency < 10s.
- [x] `nyquist_compliant: true` set in frontmatter.

**Approval:** approved 2026-05-05
