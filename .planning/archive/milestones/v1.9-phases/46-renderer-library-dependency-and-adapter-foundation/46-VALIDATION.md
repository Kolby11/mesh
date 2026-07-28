---
phase: 46
slug: renderer-library-dependency-and-adapter-foundation
status: verified
nyquist_compliant: true
wave_0_complete: true
created: 2026-05-18
---

# Phase 46 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust cargo test/check through Nix dev shell |
| **Config file** | `Cargo.toml`, `crates/core/frontend/render/Cargo.toml`, `flake.nix` |
| **Quick run command** | `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo check -p mesh-core-render --features renderer-libraries` |
| **Full suite command** | `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test --workspace` |
| **Estimated runtime** | ~30-900 seconds depending on dependency cache and workspace breadth |

---

## Sampling Rate

- **After every task commit:** Run the task's focused `cargo check`, `cargo test`, or `rg` command.
- **After every plan wave:** Run the focused command group for that wave.
- **Before `$gsd-verify-work`:** Default and enabled feature checks plus focused proof/shell tests must be green or have explicit environment blockers recorded.
- **Max feedback latency:** 15 minutes for full workspace; 2 minutes for focused checks once dependencies are cached.

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 46-01-01 | 01 | 1 | LIBS-01 | T-46-01-01 | Default build remains current renderer | cargo check | `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo check -p mesh-core-render` | ✅ | ✅ green |
| 46-01-02 | 01 | 1 | LIBS-01 | T-46-01-02 | Enabled feature path compiles selected optional deps | cargo check | `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo check -p mesh-core-render --features renderer-libraries` | ✅ | ✅ green |
| 46-02-01 | 02 | 2 | LIBS-02 | T-46-02-01 | Adapter status exposes enabled/disabled state without changing behavior | unit | `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-render renderer_library` | ✅ | ✅ green |
| 46-02-02 | 02 | 2 | LIBS-02 | T-46-02-02 | Existing proof and shipped-surface behavior survives default path | regression | `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-render proof` | ✅ | ✅ green |
| 46-03-01 | 03 | 3 | LIBS-03 | T-46-03-01 | Dependency/Nix/rollback risk record exists | docs | `rg -n "renderer-taffy|renderer-parley|renderer-accesskit|renderer-anyrender|renderer-vello-encoding|Rust 1.88|rollback path" docs/renderer-migration.md docs/renderer-ownership.md crates/core/frontend/render/Cargo.toml Cargo.toml` | ✅ | ✅ green |
| 46-03-02 | 03 | 3 | LIBS-03 | T-46-03-02 | Shipped Phase 44 regressions remain green | regression | `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell phase44` | ✅ | ✅ green |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

Existing Rust and Nix test infrastructure covers all phase requirements. No Wave 0 test framework installation is required.

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Binary/build risk summary | LIBS-03 | Exact dependency fan-out and cache/disk impact require interpreting `cargo tree`/build output. | Review `cargo tree -p mesh-core-render --features renderer-libraries` output and the dependency record added to `docs/renderer-migration.md`. |

---

## Validation Audit 2026-05-18

| Metric | Count |
|--------|-------|
| Gaps found | 0 |
| Resolved | 0 |
| Escalated | 0 |

Focused validation rerun confirmed all mapped automated commands are green. No generated test files were needed.

---

## Validation Sign-Off

- [x] All tasks have `<automated>` verify or existing infrastructure dependencies.
- [x] Sampling continuity: no 3 consecutive tasks without automated verify.
- [x] Wave 0 covers all missing references.
- [x] No watch-mode flags.
- [x] Feedback latency target recorded.
- [x] `nyquist_compliant: true` set in frontmatter.

**Approval:** verified 2026-05-18
