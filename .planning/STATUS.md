# Status

**Updated:** 2026-08-20

This page describes the present. History lives in [`log/`](log/); open work
lives in [`docs/BACKLOG.md`](../docs/BACKLOG.md).

## Now

Section 01 improvement 1 is complete: module capability declarations now resolve
against a closed catalog and persisted root-graph approvals before activation;
required grants fail without approval, optional grants default to denied, and
frontend/backend runtimes receive only the resolved grant set.

Section 01 improvement 2 is complete: shell operations now pass through one
typed registry that validates payloads, required capabilities, and caller
constraints. Raw shell events, locale writes, and popover helpers share that
authorization path; malformed, unauthorized, unknown, and dropped requests
produce structured diagnostics.

Section 01 improvement 3 is complete: shared settings field declarations now
enforce numeric bounds and canonicalize enum values before deserialization.
Rejected values fall back per field, preserving valid siblings; the runtime
and LSP settings schema consume the same declarations.

Section 01 improvement 4 is complete: diagnostics are keyed by module,
instance, and stable issue code with severity, timestamps, counts, and active/
resolved state. Health aggregates active issues without severity downgrades;
collector registration replaces identities, unregister removes stale instance
state, snapshots are deterministic, and backend recovery resolves lifecycle
issues explicitly.

Section 01 improvement 5 is complete: the unused foundation `EventBus` and its
misleading publish/subscribe contract were removed from the workspace. Live
extension events continue through the typed scripting queue and shell service
router, leaving one operational event path.

Section 01 improvement 6 is complete: public debug inspection and profiling DTOs
now derive the versioned telemetry wire shape, including the invalidation fields
that had drifted out of the shell payload. The shell adds the schema version and
keeps only overlay/controller state plus the established module-graph compatibility
adapter outside the DTO boundary.

The Section 11 Luau runtime and sandbox audit is complete. Its logical process
tree, confirmed findings, broader feature direction, and regression matrix are
in [`sections/11-luau-runtime-and-sandbox/improvements.md`](log/sections/11-luau-runtime-and-sandbox/improvements.md);
the resulting open work is in `docs/BACKLOG.md`.

The Section 12 rendering and paint audit is complete. Its end-to-end process
tree, confirmed correctness and safety findings, unconstrained render-frame
direction, and regression matrix are in
[`sections/12-rendering-and-paint/improvements.md`](log/sections/12-rendering-and-paint/improvements.md);
the resulting open work is in `docs/BACKLOG.md`.

The Section 13 surface policy and configuration audit is complete. Its process
tree, role/settings/manifest findings, policy-compiler direction, and
regression matrix are in
[`sections/13-surface-policy-and-configuration/improvements.md`](log/sections/13-surface-policy-and-configuration/improvements.md);
the resulting open work is in `docs/BACKLOG.md`.

The Section 14 Wayland platform and presentation audit is complete. Its
end-to-end lifecycle tree, confirmed protocol, popup, SHM, region, input, and
test-fidelity findings, transactional presentation direction, and regression
matrix are in
[`sections/14-wayland-platform-and-presentation/improvements.md`](log/sections/14-wayland-platform-and-presentation/improvements.md);
the resulting open work is in `docs/BACKLOG.md`.

The Section 16 developer and authoring tools audit is complete. Its end-to-end
CLI/LSP process tree, package-transaction and path-safety findings, canonical
authoring-snapshot direction, LSP protocol findings, and regression matrix are
in [`sections/16-developer-and-authoring-tools/improvements.md`](log/sections/16-developer-and-authoring-tools/improvements.md);
the resulting open work is in `docs/BACKLOG.md`.

The Section 15 shell core and orchestration audit is complete. Its end-to-end
instruction tree, confirmed activation, backend lifecycle, generation,
scheduler, watcher, failure-isolation, and shutdown findings, revisioned shell
activation direction, and regression matrix are in
[`sections/15-shell-core-and-orchestration/improvements.md`](log/sections/15-shell-core-and-orchestration/improvements.md);
the resulting open work is in `docs/BACKLOG.md`.

The Nix development shell now recommends the optimized release shell command,
keeps the debug command explicit for debugging, and the shell logs its Cargo
build profile at startup. Production paints no longer build the focused proof
snapshot; the proof remains available to the shell regression tests.

Animation-only frames now reuse the targeted restyle decision and skip an
unrelated full style walk when no interaction state changed. Slider `change`
and `release` handlers now use narrow script invalidation plus retained slider
repainting instead of forcing a full tree rebuild.

Animation transition state and previous visual snapshots now key on stable
`NodeId`s. The transition pass no longer allocates a mesh-key string and then
clones it into a live set for every node; authored keyframe names remain
string-based where they are part of the CSS-facing identity.

Showing and hiding a surface is one clean map/unmap on the wire. A layer-surface
configure is never sent carrying a zero dimension no anchor backs — the
protocol has to clamp those (1x1 at the anchor corner for a floating surface),
which is what made Settings collapse to a sliver and pop back on every show and
hide. Reopening also works at all again: hiding unmaps the surface and returns
it to the unconfigured state, and the render loop no longer waits for a
configure event before sending the configure that provokes it. Measured live
against Hyprland: five open/close cycles, one `openlayer`/`closelayer` pair and
one 920x900 configure each, zero protocol clamps.

Settings opening and closing has one idempotent visibility lifecycle. A
duplicate show cannot replay the entrance, a duplicate hide cannot shortcut an
active exit, and the navigation launcher suppresses Quick Settings while the
full Settings surface is being opened. Focused navigation and shell lifecycle
regressions pass.

Hyprland socket events now update provider state field-by-field instead of
running three `hyprctl` queries for every title change. Under a 10 Hz live
terminal-title workload with Settings scrolling, combined shell-plus-child CPU
fell from 66.8% to 15.9% of one core (4.20x); title events themselves now spawn
nothing, emit no redundant workspace event, and log below the default info
level. The 2-second full-state safety poll remains.

Geometry-only retained updates now recompute only the layout fingerprint.
Style and attribute hashes, child ids, and state are reused for clean nodes
that moved through layout propagation, while authoritative dirty roots retain
the full diff. The isolated 256-row release gate improves from
192.294–193.961ms to 62.866–62.967ms across three runs (3.06–3.08x).

Service-backed scroll and drag commands now use a 100ms backend-command budget
for coalescable setters. Navigation brightness scroll emits the absolute
coalescable setter; successful audio writes rely on live streams and optimistic
state instead of synchronous read-backs, while brightness publishes the
requested level and lets its safety poll confirm rounding or clamping. The
original live capture remains the baseline; mocked command-path regressions
cover the pacing, stream read-back suppression, and optimistic state paths.

A live capture against the running shell reframes the frame-cost work below:
the render loop is not what costs. At rest the shell process uses 0.20% of one
core, while its polling subprocesses (`hyprctl`, `wpctl`, `brightnessctl`, ~3
`fork`+`exec` per second) use 6.40% — 32x the rest of the shell, almost all of
it dynamic-linker process startup. Hyprland's own CPU is unchanged by the bar.
Per-frame render cost still matters under interaction, but it is not the idle
cost, and no idle frame-rate problem is reproducible.

The always-on navigation bar now has a frame-cost baseline of its own
(`navigation_frame_cost_profile`), alongside the existing Appearance profile.
Settled at 1920x56 with 107 nodes it costs 0.41ms for a plain repaint, 1.36ms
for a pointer move, 1.60ms for a service poll nothing on the bar reads, and
2.58ms for one it does. Sampling puts ~27% of a steady-state frame in the
allocator and under 1% in Skia's pixel fill, so the software rasterizer is not
the limit — per-frame `String` allocation and hashing in shell bookkeeping is.
Transition identity is now `NodeId`-keyed; remaining animation string work is
limited to authored keyframe names. Measurements are in the performance log.

Background service polls no longer re-instantiate frontend components that
cannot observe them. Read capability is per module, so every component instance
of a module receives every payload the module may read; a runtime whose
recorded template read sets show the update reaches it through neither the
service proxy, the service state member, nor `last_service_update` now takes
the value non-reactively. Declaring `render()` no longer disables the selective
service build either — the hooks run before the frame snapshots its dirty
flags, and the gate is what they actually wrote. A Settings Appearance frame
driven by an unrelated audio poll went from 12.2–12.4ms to 8.2–8.6ms.

The existing settings device page now has a typed `mesh.device` provider. The
new `@mesh/device-info` Linux backend reports OS identity, kernel and
architecture, desktop session, CPU, memory, graphics, hostname, and uptime
through the generic backend runtime; optional `lspci` support enriches graphics
details without making the provider unavailable.

Author-declared visual customization is now live as a module-system feature.
Named `mode="customizable"` slots select ordered public component placements
from profile schema 3, while ordinary slots keep automatic extension-point
behavior. Core provides generic discovery, validation, optimistic generation
checks, sparse persistence, and mounting through `mesh.composition`; it owns no
editor-specific UI.

Contribution roots now retain their authored layout and interaction styles
through click/hover repaints, including when a contributor comes from a
different module than the host. The navigation control cluster is intrinsically
measured again instead of reserving a fixed width.

Parent pointer dispatch now preserves content geometry for spanning surfaces,
and the render loop converts compositor-reported padded sizes back to content
before remeasuring, so activating a navigation control cannot feed the
tooltip-padded buffer size back into the retained layout.

The navigation bar proves the path with start, center, and end slots and author
defaults. The replaceable `@mesh/composition-editor` window is mounted by the
desk composition and can add, remove, reorder, move, reset, and configure
public scalar props. Structural, behavioral, and style changes remain normal
`.mesh`, Luau, and CSS source edits.

Overflow child surfaces now derive automatically for nested absolutely
positioned content that escapes the parent buffer, while explicit popover
subtrees and intentionally clipped regions remain owned by their existing
surface.

The production audio controls now live in the navigation bar's shared component
VM as an in-tree promoted popover. Core slider capture keeps drag updates alive
while the pointer crosses the popup boundary; no standalone audio surface is
enabled by the shipped graph.

Promoted legacy popovers now measure their first frame against the known parent
surface bounds while keeping the axes intrinsically sized, so the `(1, 1)` popup
positioner placeholder cannot permanently collapse cross-axis content.

The component settings boundary is clean: frontend runtimes expose declared
`props.*` and the typed `mesh.settings` service, without a raw top-level
`settings` namespace. The regression and props integration suite pass.

The retained display-list renderer is the production paint path. Legacy
PixelBuffer icon/glyph entrypoints and recursive widget helpers are restricted
to tests, eliminating the corresponding production dead-code warnings.

Pure paint-only scroll frames now reuse computed styles and retained layout, so
large Appearance resource lists do not rerun the full style walk on every tick.
The Appearance scroll regression covers the bounded icon/font catalog path.

The Phase26 real-surface proof is deterministic under the default parallel
shell suite. Its icon-cache assertion now covers both raster-backed and
font-glyph-backed semantic icon packs. The broader shell baseline still has
known pre-existing failures; do not treat them as caused by this work.

Component memoization now validates descendant state through a parent/child
runtime-generation index with aggregate subtree stamps, avoiding the former
all-runtime descendant scan while preserving nested-state invalidation.

Retained layout now receives sparse owned snapshots for layout-dirty nodes and
updates their Taffy styles/text contexts directly. The persistent Taffy mapping
and structural/unkeyed reconciliation fallback are now authoritative across
incremental frames.

The typed `mesh.packages` provider now owns module installation and removal as
well as provider selection, enablement, and profile switching. Local and Git
sources update the installed graph, lock, and profile state through the same
core path used by the CLI.

## Verification

- Hyprland event-specific updates: structural subprocess/event regression
  passed; full `mesh-core-scripting` suite passed (178 active, 27 ignored).
- Live optimized title-storm/Settings-scroll capture: 66.8% → 15.9% combined
  CPU (4.20x); title events dropped from three subprocesses to zero.

- Geometry-only fingerprint reuse: retained-tree suite passed (25 active),
  propagated-layout/full-diff parity and paint-only scroll parity passed.
- Release `geometry_only_fingerprint_speedup`: 3/3 runs passed at 3.06–3.08x;
  `scroll_retained_scope_speedup`: 3/3 runs passed at 9.30–9.45x.
- `nix develop -c cargo check -p mesh-core-shell` and `cargo fmt --all --
  --check`: passed.

- Node-slot parser/compiler, profile merge, module graph, and request mapping:
  passed.
- Shipped frontend compilation, navigation layout/raster/hover/keyboard/pointer
  behavior, and LSP manifest schema check: passed.
- `mesh-core-module`: 198 passed, 3 ignored.
- Full `mesh-core-shell --lib`: 667 passed, 128 ignored; six known fixture,
  shipped-navigation, and debug/theme manifest baseline failures remain.
- Animation and slider focused slices: 30 and 18 active tests passed; release
  retained-scope and narrow-slider gates passed across three runs.
- Release `animation_node_id_keys_beats_string_keys`: 3/3 runs passed on a
  1,024-node, 5,000-pass workload; owned-string bookkeeping took
  296.090–805.982ms versus 88.414–252.904ms for `NodeId` (3.19–3.47x per
  run).
- `cargo fmt --all -- --check` and `git diff --check`: passed.

- Runtime-generation index correctness: passed.
- Component-memo integration: 11 passed, 3 ignored.
- Retained layout parity and sparse dirty-node release gate: passed.
- Release index gate: 3/3 runs passed.
- `mesh.packages` install/uninstall mapping compiled; shell library check passed.
- `cargo fmt --all -- --check` and `git diff --check`: passed.
- Appearance bounded-list and retained-scroll regressions passed; the release
  paint-only gate passed 3/3 samples.
- The current `cargo check -p mesh-core-shell` attempt is blocked before Rust
  compilation because the environment lacks the native `xkbcommon.pc` file.
- Device-provider JSON/Lua/static checks passed; focused Cargo tests are blocked
  before compilation by the environment's missing Nix `ld-wrapper.sh`.

- `real_surfaces` slice: 44 passed, 3 failed, 3 ignored. The three failures are
  the already-recorded shipped-navigation layout and inspector fixture
  baseline; both frame-cost profiles are `#[ignore]` and did not run in that
  suite, so they cannot have caused them.
- `cargo fmt --all -- --check`: passed.
