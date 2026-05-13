---
phase: 29
slug: damage-indexed-paint-execution-and-repaint-policy
status: draft
nyquist_compliant: true
wave_0_complete: false
created: 2026-05-11
---

# Phase 29 - Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust built-in test harness |
| **Config file** | Workspace `Cargo.toml` |
| **Quick run command** | `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-render display_list` |
| **Full suite command** | `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-render display_list`; `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-render painter_`; `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell profiling` |
| **Estimated runtime** | Focused render selectors fast; shell profiling selector moderate depending on rebuild state |

---

## Sampling Rate

- **After every task commit:** Run the focused command named in the task's `<verify>` block.
- **After the full plan:** Run render display-list and painter selectors plus shell profiling selectors that touch retained invalidation/debug payloads.
- **Before `$gsd-verify-work`:** `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-render display_list`, `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-render painter_`, and `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell profiling` must be green.
- **Max feedback latency:** Prefer focused selectors under one minute when dependencies are already built.

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 29-01-01 | 01 | 1 | CULL-03, PIPE-03 | T-29-01 | Repaint policy and span metadata are explicit, observable, and fall back to full-surface repaint when filtering cannot prove correctness | unit | `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-render display_list` | yes | pending |
| 29-01-02 | 01 | 1 | PIPE-03, PIPE-04 | T-29-01 | Partial-damage paint traversal receives an ordered filtered command view and preserves clipping, ordering, scrollbars, and tooltip overlay behavior | unit | `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-render display_list`; `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-render painter_` | yes | pending |
| 29-01-03 | 01 | 1 | CULL-03, PIPE-03, PIPE-04 | T-29-01 | Filtered execution, policy selection, and fallback counters serialize through existing debug payloads and are compared against canonical shipped-surface proof | unit/integration | `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell profiling` | yes | pending |

---

## Wave 0 Requirements

Existing render and shell profiling infrastructure already exists; no separate Wave 0 bootstrap is required.

---

## Manual-Only Verifications

None required for planning. Phase 31 remains responsible for final visible-smoothness acceptance, but Phase 29 must still record benchmark evidence against the canonical shipped-surface scenario IDs.

---

## Validation Sign-Off

- [x] All tasks have automated verify commands or existing test infrastructure.
- [x] Sampling continuity: no 3 consecutive tasks without automated verify.
- [x] Wave 0 covers all missing references.
- [x] No watch-mode flags.
- [x] Feedback latency target is defined.
- [x] `nyquist_compliant: true` set in frontmatter.

**Approval:** drafted 2026-05-11
