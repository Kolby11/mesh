---
phase: 92
slug: vm-pool-foundation
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-06-07
---

# Phase 92 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust built-in (`#[test]`) |
| **Config file** | none — inline `#[cfg(test)]` modules at bottom of source files |
| **Quick run command** | `cargo test -p mesh-core-scripting` |
| **Full suite command** | `cargo test -p mesh-core-scripting` |
| **Estimated runtime** | ~10 seconds |

---

## Sampling Rate

- **After every task commit:** Run `cargo test -p mesh-core-scripting`
- **After every plan wave:** Run `cargo test -p mesh-core-scripting`
- **Before `/gsd:verify-work`:** Full suite must be green
- **Max feedback latency:** 30 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|-----------------|-----------|-------------------|-------------|--------|
| 92-01-01 | 01 | 1 | POOL-01 | N/A | unit | `cargo test -p mesh-core-scripting pool` | ❌ Wave 0 | ⬜ pending |
| 92-01-02 | 01 | 1 | POOL-02 | N/A | unit | `cargo test -p mesh-core-scripting pool` | ❌ Wave 0 | ⬜ pending |
| 92-01-03 | 01 | 1 | POOL-03 | N/A | unit | `cargo test -p mesh-core-scripting pool` | ❌ Wave 0 | ⬜ pending |
| 92-01-04 | 01 | 1 | POOL-04 | N/A | unit | `cargo test -p mesh-core-scripting pool` | ❌ Wave 0 | ⬜ pending |
| 92-02-01 | 02 | 1 | CACHE-01 | N/A | unit | `cargo test -p mesh-core-scripting chunk_cache` | ❌ Wave 0 | ⬜ pending |
| 92-02-02 | 02 | 1 | CACHE-02 | N/A | unit | `cargo test -p mesh-core-scripting chunk_cache` | ❌ Wave 0 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `crates/core/runtime/scripting/src/pool.rs` — `LuaVmPool`, `PooledVm` with `#[cfg(test)]` block covering POOL-01 through POOL-04
- [ ] `crates/core/runtime/scripting/src/chunk_cache.rs` — `ChunkCache` with `#[cfg(test)]` block covering CACHE-01 and CACHE-02

*New source files — test modules must be added as part of implementation.*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Existing surfaces render identically | Success Criterion 5 | Requires running shell visually | Launch MESH with navigation-bar and audio-popover; verify both surfaces render and respond normally |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 30s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
