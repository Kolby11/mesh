---
phase: 31
plan: 02
title: Close live audio surface UAT gaps
status: complete
started: "2026-05-13T17:01:41Z"
completed: "2026-05-13T17:17:33Z"
---

# Summary 31-02 - Close Live Audio Surface UAT Gaps

## Accomplishments

- Fixed stale shell-preserved slider values so script-owned button and backend updates can move the visible audio slider after prior pointer/keyboard interaction.
- Changed the navigation volume trigger close path to use the portal visibility flow instead of publishing duplicate hide requests.
- Added pending mute confirmation behavior in the audio popover so stale backend updates do not visually invert a just-requested mute state.
- Added configurable surface display transition settings and a shell hide lifecycle that keeps nonzero-duration surfaces mapped until the exit transition completes.
- Configured the audio popover with a short display transition and CSS exiting state.

## Files Changed

- `crates/core/frontend/host/src/lib.rs`
- `crates/core/surface-config/src/lib.rs`
- `crates/core/shell/src/shell/component.rs`
- `crates/core/shell/src/shell/component/input/mod.rs`
- `crates/core/shell/src/shell/component/input/widgets.rs`
- `crates/core/shell/src/shell/component/rendering.rs`
- `crates/core/shell/src/shell/component/runtime_tree.rs`
- `crates/core/shell/src/shell/component/shell_component.rs`
- `crates/core/shell/src/shell/runtime/mod.rs`
- `crates/core/shell/src/shell/runtime/request.rs`
- `crates/core/shell/src/shell/types.rs`
- `crates/core/shell/src/shell/tests.rs`
- `modules/frontend/audio-popover/module.json`
- `modules/frontend/audio-popover/src/main.mesh`
- `modules/frontend/navigation-bar/src/main.mesh`

## Verification

- PASS: `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo fmt --check`
- PASS: `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell audio_popover`
- PASS: `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell navigation_bar`
- PASS: `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell surface`
- PASS: `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell slider`
- PASS: `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell real_surfaces`
- PASS: `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell profiling`
- PASS: `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell phase26_real_surface_baseline_emits_canonical_proof_measurements -- --nocapture`

## Deviations from Plan

None - plan executed as written. Final live visual acceptance remains pending because UAT rows 2, 3, and 5 require user confirmation in the running shell.
