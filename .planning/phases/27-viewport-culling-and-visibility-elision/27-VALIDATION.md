---
phase: 27
slug: viewport-culling-and-visibility-elision
status: draft
nyquist_compliant: true
wave_0_complete: false
created: 2026-05-11
---

# Phase 27 - Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust built-in test harness |
| **Config file** | Workspace `Cargo.toml` |
| **Quick run command** | `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-render display_list` |
| **Full suite command** | `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell profiling -p mesh-core-render` |
| **Estimated runtime** | Focused render tests fast; shell profiling selectors moderate depending on rebuild state |

---

## Sampling Rate

- **After every task commit:** Run the focused command named in the task's `<verify>` block.
- **After the full plan:** Run render selectors plus shell profiling selectors that touch invalidation/debug payloads.
- **Before `$gsd-verify-work`:** `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-render -p mesh-core-shell profiling` must be green.
- **Max feedback latency:** Prefer focused selectors under one minute when dependencies are already built.

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 27-01-01 | 01 | 1 | CULL-02 | T-27-01 | Explicit hidden semantics remain distinct from generic opacity and do not widen hidden-state heuristics | unit | `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-render display_list` | yes | pending |
| 27-01-02 | 01 | 1 | CULL-01, CULL-04 | T-27-01 | Viewport pruning omits only fully invisible subtrees under explicit clip/scroll authority and keeps partial intersections paintable | unit | `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-render display_list` | yes | pending |
| 27-01-03 | 01 | 1 | CULL-01, CULL-02, CULL-04 | T-27-01 | Aggregate pruning counters serialize through the existing invalidation/debug payload without adding per-node trace output | unit | `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell profiling` | yes | pending |

---

## Wave 0 Requirements

Existing render and shell profiling infrastructure already exists; no separate Wave 0 bootstrap is required.

---

## Manual-Only Verifications

None required for planning. Human review may still compare Phase 27 proof numbers against the Phase 26 baseline artifact during execution, but acceptance should be automated through render and shell tests.

---

## Validation Sign-Off

- [x] All tasks have automated verify commands or existing test infrastructure.
- [x] Sampling continuity: no 3 consecutive tasks without automated verify.
- [x] Wave 0 covers all missing references.
- [x] No watch-mode flags.
- [x] Feedback latency target is defined.
- [x] `nyquist_compliant: true` set in frontmatter.

**Approval:** drafted 2026-05-11
