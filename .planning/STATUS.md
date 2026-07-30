# Status

**Updated:** 2026-07-30

This page describes the present and is meant to be overwritten. History lives in
[`log/`](log/); open work lives in [`docs/BACKLOG.md`](../docs/BACKLOG.md).

## Now

**Settings scrollbars are conditional.** The settings page uses `overflow-y:
auto`, so its scrollbar is rendered only when the page content exceeds its
viewport (2026-07-30). Regression:
`settings_scrollbar_is_conditional_on_overflow`.

**Spanning layer surfaces stay spanning after CSS measurement.** Top/bottom
surfaces now keep protocol width zero, preserving layer-shell's edge-to-edge
semantics instead of feeding a content width back to compositors that center
it. Left/right surfaces retain their measured height unless they are docked
rails with a positive exclusive zone (2026-07-30). Regressions:
`navigation_bar_keeps_layer_width_dynamic_after_css_measurement` and
`floating_side_surface_keeps_its_measured_height`.

**Wayland surface configure is nonblocking.** Presentation no longer polls for
up to 500ms when a layer surface or window is waiting for its compositor
configure (2026-07-30). Size resolution dispatches only already-available
events, and present returns a typed `NotReady` result. The shell retains the
painted buffer damage, excludes the surface from ready render work so the main
loop sleeps on the Wayland fd instead of spinning, and commits the retained
frame after configure arrives. Record:
[`log/performance-log.md`](log/performance-log.md).

**Live frontend catalogs are one atomic snapshot.** `Shell` owns the versioned
catalog generation used by every surface (2026-07-30). Enabling or disabling a
surface, component, or widget now publishes one generation and rebinds existing
hosts; source reload uses the same path. Reverse import/slot dependency tracking
invalidates only affected surfaces, drops changed embedded runtimes and prepared
styles, and keeps unrelated Lua state. Slot contributions follow the installed
graph's enabled state. Record: [`log/2026-07.md`](log/2026-07.md).

**Component host globals are isolated.** Every frontend component still shares
one thread-local Luau realm, but module- and instance-specific values now live
in that component's `_ENV` (2026-07-30). Creating a second module can no longer
replace the `this` descriptor seen by an already-live module's template
expressions or handlers. The regression creates both contexts concurrently and
checks both execution paths after the second descriptor is installed. Record:
[`log/2026-07.md`](log/2026-07.md).

**Settings are one file.** `config/settings.json` is the single store for every
user decision — `shell` for core preferences, a namespace per module or
interface for everything else (2026-07-30). It replaced `shell-settings.json`,
`settings-default.json`, and the per-module `config/settings.json` files that
were writing user overrides into shipped module source. The store is sparse:
defaults stay in code and module manifests, and `mesh-shell config eject
<module-id>` materializes a module's effective surface placement when you want
a block to hand-edit. Record: [`log/2026-07.md`](log/2026-07.md).

Stored values are now **validated on the way in** (2026-07-30). A wrong type, a
bad enum value, or a misspelled key used to change nothing and say nothing; each
now produces a diagnostic naming the namespace, the key path, the value found,
and what to do, while the shell keeps running on declared defaults — a bad
settings file is never fatal. `mesh-shell config doctor` runs the same checks
without starting a shell (exit 1 on any error) and reports namespaces whose
module is gone. `props.*` stays unvalidated until `<props>` exists.

Two side effects worth knowing: the old shell-settings merge was section-wise
and therefore lossy (a user file naming only `theme` reset tooltip and keyboard);
merging is now per key. And keyboard input no longer stats the filesystem —
`current_keyboard_settings` reads the shared store, closing the backlog item
under *Next* item 2 below.

**Window surfaces are done through phase 4.** `mesh.surface.role: "window"`
maps a frontend module as an `xdg_toplevel` (2026-07-29), the compositor's
toplevel states arrive as `:fullscreen` / `:maximized` / `:activated` / `:tiled`
(2026-07-29), and a `promotable` surface can now be moved between chrome and a
window at runtime without losing component state (2026-07-30).

`@mesh/settings` is the demonstration: it opens as chrome anchored to the right
edge with a "pop out" control in its header, becomes an ordinary Hyprland window,
and shows "dock back" instead — same VM, same page, same scroll position.
Triggered from script (`shell.set-surface-role`), from IPC
(`shell:toggle_surface_role:<id>`, for a compositor keybind), or by a user
settings override. Records: [`log/2026-07.md`](log/2026-07.md).

Verified against live Hyprland over IPC with `hyprctl` as the independent check —
promote yields a real `mesh.settings` toplevel that Hyprland tiles, demote
restores the 920×900 layer surface. The live run is also what found the one real
bug (a demoted surface stranded at 1×1 because the old compositor object was torn
down too late); fixed and regression-tested.

Open remainder is phase 5, per-widget promotion, in `docs/BACKLOG.md` — it needs
the shared surface VM first.

The longer-running stream is performance checkpoints across the retained UI
pipeline, run as a series of measured, individually gated changes rather than a
milestone.

The current stream has two threads:

- **Interning and typed representation.** Attributes are interned and flat,
  module ids and element tags are shared, template bindings keep their JSON type
  instead of stringifying. Recorded in
  [`log/backlog-archive-2026-07-28.md`](log/backlog-archive-2026-07-28.md)
  under *P2 — typing & interning*.
- **Interaction identity.** Scroll, checked, input, slider, hover, and focus
  state now key off stable `NodeId` rather than structural strings. Completion
  record: [`log/interaction-identity-2026-07-28.md`](log/interaction-identity-2026-07-28.md).

Cumulative effect on the representative 456-node tree build: 0.504–0.511ms
before the interning and theme-defaults checkpoints, 0.390–0.397ms after
(≈1.28x). Style resolution fell from 45% to 39% of the build.

Latest completed checkpoint: disjoint damage rectangles now share one selected
display-command traversal, raster canvas session, and profiling reset. The
representative four-region workload improved 3.31–3.48x and sixteen regions
improved 5.58–5.79x; one region remains on the original path. The full record
and checked gates are in
[`log/performance-log.md`](log/performance-log.md).

## Next

From the attack order at the end of
[`docs/BACKLOG.md`](../docs/BACKLOG.md):

1. Retained text-measure state.
2. Subscriber-proportional service delivery.

## Blocked

Nothing blocked.

## Verification

**Release measurement works.** Two backlog items recorded on 2026-07-28 that
measurement was "pending because the local Rust linker wrapper is unavailable".
Re-checked the same day under `nix develop` (rustc/cargo 1.94.0): the release
test binary builds, links, and runs, and a gate executes end to end —

```
cargo test --release -p mesh-core-elements --lib \
  shared_theme_defaults_beat_hashed_deep_clone -- --ignored --nocapture
→ 400,000 node resolutions: 35.8ms hashed+deep-clone vs 19.96ms cached, 1.79x
```

That gate passes. Note it came in at **1.79x against the 1.84–1.98x range
recorded when the checkpoint landed** — inside gate tolerance, but if the next
run also lands low, the recorded range is the thing to re-derive, not the gate.

The aggregate run also supplied the previously missing release measurements for
`node_id_slider_values_speedup` (1.720x), `node_id_hover_path_speedup` (1.675x),
and `node_id_focus_state_speedup` (2.978x). The remaining implemented work with
no recorded release measurement is:

| Gate | Item |
| --- | --- |
| typed declaration application | typed style declarations |

**Full shell suite re-established.** On 2026-07-30 the shell suite reports 570
passing, 18 failing, 122 ignored — the same 18 failures as the 2026-07-28
baseline of 556/18/123, with 12 more tests passing after the settings-store
work, one more from settings validation, and one catalog-generation regression.
On 2026-07-28,
`cargo test -p mesh-core-shell --lib` under `nix develop` reported 556 passing,
18 failing, and 123 ignored. The child-display-list checkpoint's complete
13-test `child_surface` slice passes; none of the 18 broader failures exercises
the 64-entry cap or eviction path. Treat 570/18 as the current shell baseline
until those failures are triaged.

**Nonblocking configure is covered at both boundaries.** The presentation
suite reports 58 passed / 12 ignored, including the typed `NotReady` result and
configured retry. The shell regression proves pending damage survives while
the surface is unconfigured, the scheduler does not treat it as ready work,
and the retained frame presents after configure. The full shell suite reports
572 passed / 17 failed / 122 ignored; the 17 failures are in the existing
style, shipped-surface, interaction, and debug-snapshot baseline areas, while
the new regression passes.

**Shared-Lua isolation is covered at both boundaries.** The complete
`mesh-core-scripting` suite reports 179 passing / 27 ignored, including a new
two-module regression that failed before the fix because the first module read
the second module's `this`. The focused real-shell descriptor slice passes 2/2,
and the full shell suite remains exactly 569 passing / 18 failing / 122 ignored
with the same baseline failures by name.

**Live catalog propagation is covered through composition and lifecycle.** The
catalog-generation regression proves a slot host retains its root Lua runtime,
drops the removed widget runtime, and rebuilds without the stale contribution.
The shipped live frontend activation/deactivation tests pass 2/2. The full
shell suite is 570 passing / 18 failing / 122 ignored, with exactly the same 18
baseline failures by name.

**Performance gate suite has unrelated failures.** The multi-damage benchmark
passes its direct checked thresholds: at least 2.805x for four regions and
4.7175x for sixteen. The aggregate script aborted earlier in command order when
`stable_child_id_reuse_beats_rewriting_slots` measured 1.051x against its
1.25x self-assertion. The previously recorded
`shared_theme_revision_cache_speedup` failure (3.021x against a 4.40x
threshold) also remains unresolved. Neither baseline was weakened.

The retained-update `SmallVec` candidate was also measured and reverted: it
removed one allocation but improved the complete scoped path by only 1.005x.
That result is now in the rejected-experiments table.

## Standing constraints

- Every performance change lands with a representative benchmark and, where the
  win is structural, a checked relative gate.
- Correctness suites pass before a performance claim counts.
- Rejected approaches get recorded with their measurements — see the
  rejected-experiments table in [`log/performance-log.md`](log/performance-log.md).
