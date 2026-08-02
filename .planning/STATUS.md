# Status

**Updated:** 2026-08-02

This page describes the present and is meant to be overwritten. History lives in
[`log/`](log/); open work lives in [`docs/BACKLOG.md`](../docs/BACKLOG.md).

## Now

**Transient popups and overlays are shipped.** In-tree `<popover>` content is
promoted to content-sized, anchored `xdg_popup` child surfaces with custom
content, routed input, hover/grab dismissal, exclusivity, and compositor-dismiss
sync. Completion verification fixed child-bound handler arguments crossing a
component prop and stale Luau expression-cache values across multiple live
binding synchronizations. Automatic overflow derivation, legacy-popover
migration, and per-widget toplevel promotion remain open (2026-08-02). Record:
[`log/2026-08.md`](log/2026-08.md).

**Widget-tree and retained painting share one command builder.** Compatibility
`render_tree*` callers now build and replay a transient display list, and the
parallel recursive `WidgetNode` painter has been removed. The retained builder
is authoritative for clipping, ordering, controls, and filter/effect scopes
(2026-08-02). Record: [`log/2026-08.md`](log/2026-08.md).

**The settings frontend exposes the installed graph per module.** Each module
row expands to required and optional interface bindings, provided interfaces,
module dependencies, required/optional capabilities, resource packs, native
binary availability, i18n catalogs, keybinds, and combined health/diagnostics.
Production `interface=provider` bindings now populate the provider controls as
well as the details view (2026-08-02). Record:
[`log/2026-08.md`](log/2026-08.md).

**Core-triggered sound and backend profiling no longer bypass interface
routing.** `mesh.audio.play_sound` is a declared contract method, startup
dispatch follows the same capability-checked request path as module calls, and
the backend-update profiler selects active/sample-producing providers without
matching an interface name. Built-in debug and theme/locale service state
remain open (2026-08-02). Record: [`log/2026-08.md`](log/2026-08.md).

**Module props have a generated settings surface.** Installed frontend props
are projected into the module graph as typed schemas; the settings frontend
renders controls and reset actions, and privileged writes are validated before
updating sparse profile-scoped overrides. Hand-edited global and per-instance
prop overrides now go through the same declarations on startup/reload and in
`config doctor`; invalid values are ignored so declared defaults win. `config
eject` materializes effective exposed props alongside surface placement. Custom
`settings_ui` mounting and per-instance targeting remain open (2026-08-02). Record:
[`log/2026-08.md`](log/2026-08.md).

**External interface contracts are typed through the editor boundary.** The
shared canonical manifest loader resolves module-relative `contract.json`
references for runtime and tooling callers alike. The LSP consumes the
validated `InterfaceContract`, so service proxy completion carries exact state,
method, return, and event-payload types rather than guessing from a backend
script (2026-08-02). Record: [`log/2026-08.md`](log/2026-08.md).

**The retained widget tree owns the render fingerprint pass.** Each retained
node snapshot now carries the paint fingerprint and produces the detailed
render dirty summary/node set while its normal layout/style/attribute diff is
already visiting that node. Surface components no longer retain or traverse a
parallel `RenderObjectTree`; display-list sparse reconciliation consumes the
authoritative retained result. The 1,365-node release gate measured
1.077–1.085x across repeated interleaved runs (2026-08-02). Record:
[`log/performance-log.md`](log/performance-log.md).

**Direct service updates re-evaluate only affected static template subtrees.**
Changed service fields resolve through `NodeServiceFieldDependencies`; clean
native branches retain their copy-on-write widget payloads, while component
references continue through component memoization. Templates with structural
directives or render hooks conservatively keep full evaluation. The 1,026-node
end-to-end gate measured 1.018–1.030x across repeated release runs
(2026-08-01). Record: [`log/performance-log.md`](log/performance-log.md).

**Scoped retained updates keep their fresh `Vec`.** A reintroduced inline
`SmallVec` candidate removed one allocation per 13-node sparse-update frame,
but repeated end-to-end gains were only 1.011–1.075x and could not sustain the
5% gate. The candidate was reverted, corroborating the 2026-07-28 rejected
experiment; the stale backlog item is closed (2026-08-01). Record:
[`log/performance-log.md`](log/performance-log.md).

**Component composition metadata is typed end-to-end.** `WidgetNode` handler
records now carry the local script handler and owning embedded instance as a
`HandlerTarget`, so compiler namespacing, interaction lookup, scheduling, and
shell dispatch no longer build or parse the `__mesh_embed__::` wire string.
Legacy runtime-provided strings are decoded only at the compatibility edge.
Component values, prop bindings, `bind:this`, and promoted-popover metadata use
their existing typed channels (2026-08-01). Record:
[`log/2026-08.md`](log/2026-08.md).

**Runtime style diagnostics reuse retained-tree generations.** Diagnostic-enabled
rebuilds no longer hash every node a second time merely to decide whether to
repeat the diagnostic restyle. The gate now combines the authoritative retained
generation with rules, props, and container dimensions (2026-07-31). Record:
[`log/performance-log.md`](log/performance-log.md).

**Style caches evict cold entries individually.** Inline-style parsing, shared
theme defaults (revision and per-revision), and lowered theme declarations now
use bounded LRU eviction instead of clearing every hot entry at capacity
(2026-07-31). The 300-frame churn gate measures p95 latency and protects the
cache-cliff improvement. Record:
[`log/performance-log.md`](log/performance-log.md).

**Compiled frontend modules are shared across surface instances.** Catalog
entries and mounted profile roots retain copy-on-write `Arc` handles, so
multiple instances no longer clone compiled templates, scripts, or styles. A
source reload publishes a replacement in the next catalog generation while
existing surfaces retain their previous immutable snapshot until rebind
(2026-07-31). Record: [`log/performance-log.md`](log/performance-log.md).

**Interaction targets are `NodeId`s end-to-end.** Pointer-down capture, active
slider state, and keyboard button activation no longer retain structural-key
strings. Cleanup preserves a nested target across a keyed-list reorder and
clears it on removal; keys remain only at Lua handler/ref boundaries and in
diagnostics (2026-07-31). Record: [`log/2026-07.md`](log/2026-07.md).

**Authoring source diagnostics are explicit.** Undeclared icon, translation,
keybind-subscription, and raw event-publish checks no longer parse every
frontend source file during normal graph construction. `mesh-shell config
doctor` runs them intentionally, and `MESH_AUTHORING_DIAGNOSTICS=1` enables
them for development graph builds (2026-07-31). Record:
[`log/2026-07.md`](log/2026-07.md).

**Keyboard targets resolve in one traversal.** Keyboard payload construction
and button activation carry a borrowed node plus transformed bounds through
dispatch, rather than separately finding the node, bounds, and handler
(2026-07-31). Record: [`log/2026-07.md`](log/2026-07.md).

**Navigation popover triggers are push-driven.** The language and theme
popovers notify their bound trigger when state changes, so trigger render hooks
only establish the live binding and no longer poll child or service state
(2026-07-31). Record: [`log/2026-07.md`](log/2026-07.md).

**Dead Luau source caching is gone.** `ChunkCache` never supplied a compiled
chunk (or even read its stored source) and could retain every historical script
version. Script loading and source reload now execute directly (2026-07-31).
Record: [`log/2026-07.md`](log/2026-07.md).

**Graph diagnostics parse frontend source once per file.** The icon, Luau, and
keybind checks share one parsed component AST (2026-07-31). Record:
[`log/performance-log.md`](log/performance-log.md).

**Retained-tree identity collisions are release-safe.** A duplicate live
`NodeId` now fails loudly with a diagnostic in release as well as debug,
instead of silently aliasing retained snapshots (2026-07-31). Record:
[`log/performance-log.md`](log/performance-log.md).

**Luau translation calls use live module catalogs.** `mesh.i18n.t()` now
resolves through each script context's effective module-scoped locale catalog,
instead of echoing keys. New contexts receive the catalog before `init()`, and
locale switches refresh existing imported `t` functions (2026-07-31). Record:
[`log/2026-07.md`](log/2026-07.md).

**Named shells switch live as one transaction.** Profiles now scope sparse
shell/module/instance preferences, mount every named root instance, apply
per-instance surfaces and Luau storage identity, and retain identical service
providers across switches (2026-07-31). A changed provider and all new roots
are prepared before `active-profile` changes; candidate failure aborts the
staged runtimes and leaves the running shell untouched. `mesh-shell profile
use` switches over IPC when running and selects the next startup otherwise;
`profile set/unset` edits scoped preferences. Typed profile/package services
for the replaceable settings frontend remain open. Record:
[`log/2026-07.md`](log/2026-07.md).

**Named profiles now own opt-in composition.** An `active-profile` pointer and
`profiles/<id>.json` provide positive root instances, background services,
provider bindings, and resource selections without turning installed source
into an allow-list (2026-07-31). The graph infers declared module/resource
dependencies, interface modules, and sole providers; absent `active-profile`
preserves the legacy root graph. `mesh-shell profile` manages profiles, local
`mesh-shell install <path>` stages and validates modules with capability gates,
and direct frontend installs create an active `#default` instance while leaving
manifest placement/props inherited. Settings-surface enable/disable writes the
active profile atomically. Git installation and typed package/profile services
remain in the backlog. Record:
[`log/2026-07.md`](log/2026-07.md).

**Module script scanning parses Luau instead of searching for substrings.**
The graph's authoring diagnostics (translation keys, published channels,
backend events) walk a real `full_moon` AST (2026-07-31). `content.find("t(")`
matched the tail of `format(` and `assert(`, so shipped modules were reported
as using `%d%%` as a translation key; comments and string literals counted as
code. Costs 20–35ms on graph load for the shipped modules, which is why moving
authoring checks off the startup path is now a backlog item. Record:
[`log/2026-07.md`](log/2026-07.md).

**Child→parent `bind:this` events reach the parent's template.** A child's
`self.<Event>:fire()` into a parent's `:on` closure now marks the parent for
resync, so the pushed value lands in the parent's reactive state (2026-07-31).
The live-binding proxy returns a wrapper channel whose `on`/`subscribe` flags
the parent when a callback actually runs — previously only the parent touching
the child set a flag, in either direction, so the push half was unusable.
Untouched neighbours are still skipped. Record: [`log/2026-07.md`](log/2026-07.md).

**Size constraints are real dimensions.** `min-width`, `max-width`,
`min-height`, and `max-height` are `Dimension` like `width`/`height`, so they
take percentages, `auto`, `fit-content`, and `none` instead of lengths only
(2026-07-31). `max-width: 100%` now clamps a fixed root to its surface; it used
to parse to `0` and collapse the element, as did `max-width: none`. The four
also joined the typed-literal fast path and interpolate through
`lerp_dimension`. Record: [`log/2026-07.md`](log/2026-07.md).

**The editor understands `settings.json`.** `mesh-tools-lsp` now serves the
settings store: namespace and key completion, hover documentation, and
diagnostics, with the schema derived from `SHELL_SETTINGS_FIELDS` and
`MODULE_NAMESPACE_FIELDS` rather than restated (2026-07-31). Themes, locales,
icon packs, and installed module/interface ids are discovered from the
workspace and offered as suggestions; enum values stay enforced. The
manifest/settings machinery now shares one `json/` engine. Record:
[`log/2026-07.md`](log/2026-07.md).

**Element blur covers the subtree.** `filter: blur()` lowers into
`PushFilterLayer`/`PopFilterLayer` around the element and its descendants, so
the whole subtree blurs as one image and spills past the element box
(2026-07-30). Blur layers are atomic for partial repaint and expand damage to
their whole region; nesting is capped at four; quality is
`shell.render.blur.{passes,max_radius}`. A blurred 420x420 subtree costs
~1.3ms/repaint against 0.05ms unblurred. Records:
[`log/2026-07.md`](log/2026-07.md),
[`log/performance-log.md`](log/performance-log.md).

**Bubble rings window three of N.** Both the language ring (18 locales, flag
emoji) and the theme ring (7 themes) show three options and scroll the rest
through, turning along the arc; wheel and two-finger trackpad both drive it
(2026-07-30). Record: [`log/2026-07.md`](log/2026-07.md).

**Promoted popovers are selectable again.** Child-surface input now applies the
same padded `content_offset` as paint, so pointer coordinates in a popup buffer
are no longer skewed by the blur/transform padding (~50px for the bubble
popovers) into spurious `pointerleave` dismissals (2026-07-30). Bubble options
sit in a shallower arc, and scrolling turns the ring instead of being reverted
by the next render. Record: [`log/2026-07.md`](log/2026-07.md).

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

**Text measurement contexts are retained by node.** `PerSurfaceLayoutState`
now owns clean text inputs for the retained Taffy tree, updating an entry only
when content or measurement-affecting style changes (2026-07-30). The focused
512-text-node release gate is 1.58–1.72x faster than rebuilding the contexts;
the earlier scratch-map rejection predated dirty-node style scoping. Record:
[`log/performance-log.md`](log/performance-log.md).

**Component memo hits now share immutable build payloads.** A memo cache entry
and its live hit share each node's authored attributes, handlers, service reads,
and child topology through copy-on-write `Arc`s (2026-07-30). Runtime style,
layout, interaction, and accessibility overlays remain independently mutable;
only the changed branch is copied. The 273-node wide release gate measured
2,527–2,777x faster clone time and 418,323→120 exclusive retained heap bytes;
the 97-node deep shape measured 803–826x and 147,826→120 bytes. Record:
[`log/performance-log.md`](log/performance-log.md).

**Navigation volume localization and popover hover are stable.** The volume
tooltip now translates its dynamic key in the template, rather than preserving
the key from Luau. Language and theme triggers use component-specific style
classes, so they do not inherit the settings button's hover lift/scale.
Child-popup pointer input also preserves the parent surface's authoritative
dimensions instead of temporarily relaying the popup's size into the navigation
layout (2026-07-30). Record: [`log/2026-07.md`](log/2026-07.md).

**Service delivery is subscriber-proportional.** The shell refreshes sorted,
unique service subscriber lists only when component observation summaries
change, then routes updates and named interface events directly through those
lists (2026-07-30). An epoch marker prevents overlapping update/cache lists
from delivering twice without per-event target cloning or normalization.
Duplicate declarations are collapsed during index rebuild, including named
events. The 20,000-event/256-component release gate measured 12.24–16.78x
against a full component scan. Record:
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

1. Narrow invalidation and affected-subtree template re-evaluation.

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
