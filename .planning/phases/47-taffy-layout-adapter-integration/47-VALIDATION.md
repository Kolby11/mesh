---
phase: 47
slug: taffy-layout-adapter-integration
status: draft
nyquist_compliant: true
wave_0_complete: true
created: 2026-05-18
---

# Phase 47 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust built-in test harness |
| **Config file** | `Cargo.toml` |
| **Quick run command** | `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-elements layout` |
| **Full suite command** | `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell phase47` |
| **Estimated runtime** | ~120 seconds |

---

## Sampling Rate

- **After every task commit:** Run the task's focused `cargo test` or `cargo check` command.
- **After every plan wave:** Run `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-elements layout`.
- **Before `$gsd-verify-work`:** Full Phase 47 command set must be green or exact blockers must be recorded.
- **Max feedback latency:** 180 seconds for focused commands.

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 47-01-01 | 01 | 1 | LAYT-01 | T-47-01-01 | Taffy dependency belongs to the layout-owning crate, not unrelated renderer-only behavior | compile | `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo check -p mesh-core-elements` | ✅ | ⬜ pending |
| 47-01-02 | 01 | 1 | LAYT-03 | T-47-01-02 | Unsupported layout cases emit diagnostics instead of hidden old-engine fallback | unit | `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-elements taffy_diagnostic` | ✅ | ⬜ pending |
| 47-02-01 | 02 | 2 | LAYT-01 | T-47-02-01 | Taffy writes retained MESH `LayoutRect` values without replacing `NodeId` identity | unit | `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-elements taffy_layout` | ✅ | ⬜ pending |
| 47-02-02 | 02 | 2 | LAYT-01 | T-47-02-02 | Text measurement remains bounded by the injected `TextMeasurer` | unit | `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-elements intrinsic` | ✅ | ⬜ pending |
| 47-03-01 | 03 | 3 | LAYT-02 | T-47-03-01 | Required row/column/stack/fixed/gap/padding/absolute/container cases have automated coverage | unit | `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-elements layout` | ✅ | ⬜ pending |
| 47-03-02 | 03 | 3 | LAYT-01, LAYT-03 | T-47-03-02 | Shipped navigation/audio geometry remains proven under the Taffy layout path | integration | `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell phase47` | ✅ | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

Existing Rust test infrastructure covers this phase. No new test framework or watch-mode setup is required.

---

## Manual-Only Verifications

All Phase 47 acceptance criteria have automated verification targets. Manual review is limited to reading diagnostics and summary records if a platform/Nix command cannot run.

---

## Validation Sign-Off

- [x] All tasks have `<automated>` verify or existing test infrastructure dependencies
- [x] Sampling continuity: no 3 consecutive tasks without automated verify
- [x] Wave 0 covers all MISSING references
- [x] No watch-mode flags
- [x] Feedback latency < 180s for focused commands
- [x] `nyquist_compliant: true` set in frontmatter

**Approval:** approved 2026-05-18

