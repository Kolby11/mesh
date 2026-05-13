---
phase: 31-smoothness-proof-and-cpu-render-tuning
phase_number: 31
status: clean
depth: standard
files_reviewed: 10
findings:
  critical: 0
  warning: 0
  info: 0
  total: 0
reviewed_at: 2026-05-13T20:13:34+02:00
reviewed_commits:
  - bfc6cd4
  - 6e0dc0a
  - 9497b8d
---

# Phase 31 Code Review

## Scope

- `modules/frontend/navigation-bar/src/main.mesh`
- `modules/frontend/audio-popover/src/main.mesh`
- `crates/core/shell/src/shell/mod.rs`
- `crates/core/shell/src/shell/discovery.rs`
- `crates/core/shell/src/shell/runtime/request.rs`
- `crates/core/shell/src/shell/runtime/service_state.rs`
- `crates/core/shell/src/shell/component/tests/common.rs`
- `crates/core/shell/src/shell/component/tests/interaction/navigation.rs`
- `crates/core/shell/src/shell/component/tests/interaction/policy.rs`
- `crates/core/shell/src/shell/tests.rs`

## Result

No open issues remain after review.

## 31-04 Review Addendum

Reviewed the focused 31-04 changes in `9497b8d`:

- `modules/frontend/navigation-bar/src/main.mesh`
- `modules/frontend/audio-popover/src/main.mesh`
- `crates/core/shell/src/shell/component/tests/interaction/navigation.rs`
- `crates/core/shell/src/shell/component/tests/interaction/policy.rs`

No new issues found. The trigger close path now emits an explicit hide request, and the popover mute display no longer competes with shell-normalized optimistic `mesh.audio.muted` state.

## Review Fix Applied

The review found one stale-state edge case in `normalize_service_event`: inactive or terminal audio providers could pass through optimistic mute normalization before `record_latest_service_state` rejected the event. That could clear `pending_audio_muted` using a stale provider update, allowing the next active stale event to flip the UI back.

Fixed in `6e0dc0a` by applying the inactive-provider and terminal-provider gate before optimistic audio mute reconciliation, then adding a regression assertion that inactive providers neither deliver audio state nor clear pending mute state.

## Verification

- PASS: `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo fmt --check`
- PASS: `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell set_muted_command_broadcasts_optimistic_audio_state_until_backend_confirms`
- PASS: `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell audio_popover`
- PASS: `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell navigation_bar`
- PASS: `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell profiling`
- PASS: `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell navigation_bar_same_hover_volume_trigger_closes_popover_immediately`
- PASS: `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell audio_popover_mute_renders_shell_normalized_state`
- PASS: `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell real_surfaces`
