---
phase: 104
slug: retained-taffytree
status: gaps_found
verified: 2026-06-18
gap_closure_pass: 2026-06-22
---

# Phase 104 Verification

## Status

`gaps_found` — gap-closure pass on 2026-06-22 took the `mesh-core-shell` suite
from **54 → 7 failing** (`347 passed; 7 failed`). The 7 remaining are not
fixture drift; they are behavior-level regression suspects in subsystems Phase
104 and the shared-VM work touched, left failing on purpose for a dedicated
debug pass (see "Open Regression Suspects" below). No Phase 104 retained-layout
parity test regressed — all geometry/parity assertions pass.

Retained TaffyTree implementation is present and the focused retained-layout proof passes.

## Gap-Closure Pass (2026-06-22)

The original 54 failures were dominated by **stale fixtures from the shipped
navigation-bar / audio-popover rewrites**, plus shared test-harness gaps. Fixed:

- **Embed-handler key drift** — shipped surfaces moved handlers into child
  components, so keys became `__mesh_embed__::@mesh/navigation-bar/local:VolumeButton::onAudioToggle`
  (was `…::onToggleAudioSurface`). Updated all call sites.
- **Test harness missing child components** — `real_frontend_module_component`
  did not register the nav-bar's `BrightnessButton`/`ClockButton`/`NowPlaying`/
  `WindowTitle`/`WorkspaceList` (+ `QuickSettings` module import). Unregistered
  children rendered "no explicit component import" error text and overflowed the
  bar, pushing right-cluster buttons off-surface (un-hit-testable). Registered them.
- **Test harness missing interface catalog** — added `navigation_bar_catalog()`
  with minimal contracts for `mesh.brightness`/`mesh.hyprland`/`mesh.media` so the
  nav-bar children resolve their interfaces instead of error-rendering.
- **Test harness missing i18n wiring** — `real_frontend_module_component` now
  loads each module's `config/i18n/*.json` (mirrors the shell's graph i18n), so
  `t(...)` resolves (including across the locale switch the tooltip test exercises).
- **Debug-inspector seed-flow** — the inspector runtime only observes a
  `mesh.debug` event once an initial paint has tracked its state fields (the real
  shell seeds cached payloads at mount). Debug tests now paint once before
  dispatching the event.
- **Audio popover redesign** — popover is now a vertical slider + percent label
  (mute/volume buttons removed). Rewrote slider drag geometry (vertical: top=max),
  switched value range to 0–100, and deleted tests for removed UI.
- **Real product bug fixed** — the audio popover slider was stuck at 0 because
  `value={slider_value or 0}` (unquoted expression) never paints; switched the
  shipped `modules/frontend/audio-popover/src/main.mesh` to `value="{slider_value}"`.
- **Config completion** — `config/icons.toml` was missing direct XDG icon
  mappings (`preferences-system`, `preferences-desktop-locale`,
  `media-playback-start`, `window-close`) the rewritten nav-bar now declares.
- **Deprecated keybind migration** — a keybind test used the removed legacy
  `settings.keyboard.shortcuts` form; moved it to the supported `mesh.keybinds`
  manifest declaration.
- **Obsolete tests deleted** — `status-pulse`/`status-accent` keyframe-shape
  animation tests (feature removed from the nav-bar) and removed-UI popover tests.

## Open Regression Suspects (7, left failing by decision)

These are **not** fixtures — each is a behavior change in a subsystem Phase 104
or the shared-VM consolidation touched. They need a dedicated debug pass to
decide "real regression → fix source" vs "intentional change → adjust test".

| Test | Subsystem | Symptom |
|------|-----------|---------|
| `invalidation::narrow_script::threshold_fallback_exceeds_half` | retained narrow-diff | `narrow_script_diff` reports 4 affected nodes where 3 expected |
| `invalidation::narrow_script::threshold_narrow_below_half` | retained narrow-diff | reports 4 affected where 1 expected (1 text changed) |
| `interaction::diagnostics::raw_service_state_update_schedules_repaint_without_proxy_tracking` | service-observation gating | `runtime_observes_service_event` now skips repaint for components with no tracked service fields, even though `last_service_update` is still set |
| `restyle::metrics::restyle_metrics_reflect_post_restyle_bounds` | live element refs | `refs.btn.width` reads 0 from script state after paint |
| `interaction::reflow::container_size_restyle_preserves_runtime_and_local_state` | container-query restyle | narrow container style (`#222`) not applied after resize (stays `#eee`) |
| `invalidation::profiling::phase26_real_surface_baseline_emits_canonical_proof_measurements` | profiling proof | text/glyph + icon/image raster cache activity not reported active |
| `frontend_settings_load_surface_display_transition_defaults_and_overrides` | settings loader | in-test manifest declares `display_transition.default.show_ms = 90`, loader returns 0 |

## Original gap context (pre-closure)

Retained TaffyTree implementation is present and the focused retained-layout proof passes. Full shell test verification was blocked by the dirty worktree: shipped navigation/module tests failed against local navigation-bar and service/module changes already present outside this phase.

## Passed Checks

| Check | Result |
|-------|--------|
| `cargo test --package mesh-core-elements -- retained_layout_parity` | passed, 5/5 |
| `cargo test --package mesh-core-elements -- layout` | passed, 32/32 |
| `nix develop -c cargo build --package mesh-core-shell` | passed |

## Failed / Blocked Checks

| Check | Result |
|-------|--------|
| `cargo build --package mesh-core-shell` outside Nix | blocked: missing system `xkbcommon.pc` |
| `nix develop -c cargo test --package mesh-core-shell` | failed: 278 passed, 54 failed |

## Gap Details

- The full shell suite compiles under Nix, but runtime assertions fail in existing real-surface, service, navigation, and module graph tests.
- A focused shipped-surface layout test still expects old navigation-bar structure/content while the dirty worktree has rewritten `modules/frontend/navigation-bar/src/main.mesh` from `status-cluster`/`control-cluster` to `left-cluster`/`right-cluster` plus new clock/brightness/quick-settings/theme-selector surfaces.
- These failures are not suitable to fix inside Phase 104 without overwriting or redesigning user-visible module work that predates this retained-layout patch.

## Requirement Coverage

| Requirement | Coverage |
|-------------|----------|
| LAYOUT-01 retained `TaffyTree` state | covered |
| LAYOUT-02 STYLE/LAYOUT dirty routing | covered by `compute_incremental` tests |
| LAYOUT-03 `_mesh_key` structural identity | covered by add/remove/reorder parity tests |
| LAYOUT-04 post-order subtree removal | covered |
| LAYOUT-05 retained vs fresh parity | covered for five planned scenarios |

## Recommended Next Step

Gap-closure pass done (2026-06-22): 54 → 7 failing, all fixture/harness drift
from the shipped-module rewrites resolved. The remaining 7 are behavior-level
regression suspects (table above) — run a focused `gsd:debug` pass on the
narrow-diff counts and service-observation gating first, as those are the most
likely real Phase 104 / shared-VM regressions.

