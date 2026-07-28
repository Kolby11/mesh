---
phase: 28
slug: incremental-paint-command-retention
status: draft
nyquist_compliant: true
wave_0_complete: false
created: 2026-05-11
---

# Phase 28 - Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust built-in test harness |
| **Config file** | Workspace `Cargo.toml` |
| **Quick run command** | `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-render display_list` |
| **Full suite command** | `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-render render_object -p mesh-core-render display_list -p mesh-core-shell profiling` |
| **Estimated runtime** | Focused render selectors fast; shell profiling selector moderate depending on rebuild state |

---

## Sampling Rate

- **After every task commit:** Run the focused command named in the task's `<verify>` block.
- **After the full plan:** Run render selectors plus shell profiling selectors that touch retained invalidation/debug payloads.
- **Before `$gsd-verify-work`:** `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-render render_object -p mesh-core-render display_list -p mesh-core-shell profiling` must be green.
- **Max feedback latency:** Prefer focused selectors under one minute when dependencies are already built.

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 28-01-01 | 01 | 1 | PIPE-01 | T-28-01 | Retained command ownership becomes subtree-local and broad-dirty cases still record a conservative full fallback instead of forcing unsafe reuse | unit | `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-render display_list -p mesh-core-render render_object` | yes | pending |
| 28-01-02 | 01 | 1 | PIPE-01, PIPE-02 | T-28-01 | Transform-, scroll-, and reorder-only updates preserve unrelated descendant command payloads and do not trigger whole-surface command recollection | unit | `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-render display_list` | yes | pending |
| 28-01-03 | 01 | 1 | PIPE-01, PIPE-02 | T-28-01 | Aggregate subtree reuse, subtree rebuild, and fallback counters serialize through the existing invalidation/debug payload without adding a second trace system | unit | `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell profiling` | yes | pending |

---

## Wave 0 Requirements

Existing render and shell profiling infrastructure already exists; no separate Wave 0 bootstrap is required.

---

## Manual-Only Verifications

None required for planning. Human review during execution may still compare Phase 28 proof against the Phase 26 baseline artifact, but acceptance should remain driven by retained render and shell profiling tests.

---

## Validation Sign-Off

- [x] All tasks have automated verify commands or existing test infrastructure.
- [x] Sampling continuity: no 3 consecutive tasks without automated verify.
- [x] Wave 0 covers all missing references.
- [x] No watch-mode flags.
- [x] Feedback latency target is defined.
- [x] `nyquist_compliant: true` set in frontmatter.

**Approval:** drafted 2026-05-11
