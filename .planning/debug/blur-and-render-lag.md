---
status: resolved
trigger: "The blur does not really work now; determine whether MESH uses Hyprland blur and fix the accompanying severe lag."
created: "2026-07-15T21:40:00+02:00"
updated: "2026-07-16T00:18:00+02:00"
---

# Blur and Render Lag

## Symptoms

- Expected behavior: translucent shell surfaces visibly blur content behind them and remain responsive during interaction.
- Actual behavior: blur appears ineffective and the shell is noticeably laggy.
- Error messages: none supplied.
- Timeline: present in the current live build after recent navigation/brightness work; prior working state is not yet established.
- Reproduction: run the shell under Hyprland and interact with translucent navigation/popover surfaces.

## Current Focus

- hypothesis: confirmed — missing Hyprland namespace rules caused ineffective compositor blur, while a re-enabled CPU Skia backdrop pass caused severe paint latency
- test: compositor-only backdrop handling passes focused/full renderer tests and compositor-region tests; physical blur requires the documented Hyprland rule
- expecting: the rebuilt shell no longer spends CPU time on an impossible behind-surface blur; Hyprland visibly blurs MESH after the user enables the documented @mesh layer rule
- next_action: rebuild/restart and physically verify responsiveness plus blur with the documented rule
- reasoning_checkpoint:
- tdd_checkpoint:

## Evidence

- timestamp: 2026-07-15T23:55:00+02:00
  observation: Live Hyprland 0.55.4 reports decoration:blur:enabled=true, size=10, passes=3.
  implication: Global compositor blur is enabled; the visual failure is not caused by decoration blur being disabled.
- timestamp: 2026-07-16T00:02:00+02:00
  observation: The active Hyprland rules configure blur for gtk, launcher, notifications, bar*, and quickshell:* namespaces, but no rule matches @mesh/navigation-bar or other @mesh namespaces.
  implication: Hyprland does not enable layer-surface blur for MESH even though MESH supplies a KDE blur-region hint.
- timestamp: 2026-07-16T00:04:00+02:00
  observation: Commit 4fe6f10e changed push_backdrop_filter_command from a no-op to PainterCommand::ApplyFilter, executing Skia image_filters::blur through save_layer on every selected frosted node.
  implication: The navigation bar's blur(20px) performs CPU work over its own SHM buffer but cannot sample desktop pixels behind the Wayland surface; this is a direct lag regression with no useful shell visual output.
- timestamp: 2026-07-16T00:05:00+02:00
  observation: A direct cargo test build outside the Nix development environment failed at link time because freetype/fontconfig libraries were unavailable.
  implication: Verification must run through nix develop; the failure is environmental, not a code/test failure.
- timestamp: 2026-07-16T00:11:00+02:00
  observation: Release real-surface profiling at 960x80 measured navigation backend-update paint/traversal at 77-79 ms while related layout took 15-26 us.
  implication: Paint traversal exceeded the live 120 Hz frame budget of 8.33 ms by roughly 9x; layout and input dispatch were not the dominant lag source.
- timestamp: 2026-07-16T00:15:00+02:00
  observation: After restoring compositor-owned backdrop handling, five focused backdrop tests, four compositor blur-region tests, and 175 of 177 full renderer tests passed; the two full-suite failures were shared image-fixture races and passed together in a focused rerun (3/3).
  implication: The renderer change preserves flat client pixels and blur metadata without introducing a focused regression.
- timestamp: 2026-07-16T00:24:00+02:00
  observation: The exact release profiling test could not be rerun after the fix because the current release test target has pre-existing compile errors in shell runtime debug probes and icon font requirement fields; these files are outside this fix and debug-mode tests compile.
  implication: A numeric post-fix comparison is unavailable from that harness. The command-level regression instead proves the blur ApplyFilter is absent, removing the measured hot operation rather than estimating its speedup.
- timestamp: 2026-07-16T00:27:00+02:00
  observation: Hyprland 0.55.4 `--verify-config` returned `config ok` for the documented Lua layer rule with namespace `^@mesh/.*$`, blur, blur_popups, and ignore_alpha fields.
  implication: The repository documentation uses syntax accepted by the user's live Hyprland version without mutating the active configuration.


## Eliminated

- hypothesis: Hyprland compositor blur is globally disabled.
  evidence: hyprctl reports decoration:blur:enabled=true with configured size and passes.
- hypothesis: Recent brightness scroll dispatch is the primary render-lag cause.
  evidence: Git history localizes the expensive Skia backdrop pass to the earlier 4fe6f10e render commit; brightness commits do not touch the painter or damage path.


## Resolution

- root_cause: Hyprland had global blur enabled but no blur layer rule matching @mesh namespaces. Independently, commit 4fe6f10e re-enabled Skia save-layer backdrop filtering over client SHM pixels, which cannot contain the desktop backdrop and measured 77-79 ms in the navigation backend-update paint path.
- fix: Restored compositor-owned backdrop filtering by preventing direct and retained painters from lowering backdrop-filter to a CPU ApplyFilter command. Preserved compositor blur-region metadata and documented the required Hyprland 0.55 @mesh layer/popup rule without modifying user configuration.
- verification: nix develop focused renderer backdrop tests 5/5; shell compositor-region tests 4/4; renderer library 175 passed with 2 unrelated parallel fixture failures, then both failures passed in focused rerun (3/3); command regression proves zero CPU backdrop ApplyFilter; Hyprland 0.55.4 config verifier accepted the documented rule; git diff --check passed. Exact post-fix release profiling is blocked by unrelated release-target compile errors.
- files_changed: crates/core/frontend/render/src/surface/painter.rs, crates/core/frontend/render/src/surface/painter/tree.rs, crates/core/frontend/render/src/surface/painter/tests.rs, docs/modules/frontend/core/navigation-bar/README.md, .planning/debug/blur-and-render-lag.md
