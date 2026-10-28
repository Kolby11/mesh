# MESH Performance Log

## 2026-09-30 — scroll and animation frames: wave 2 timed, wave 3 compositor blur via `ext-background-effect-v1`

`working tree` · area: paint-only frames, presentation blur protocols, backdrop policy, styling spec

Follows the "waves 1 and 2" entry below, which left wave 2 untimed.

**Wave 2 timing.** Release, desktop in use. Baseline is a worktree of `HEAD`
plus the wave-1 working tree as snapshotted before wave 2 began (its diff
against the current tree touches only wave-2 files and hunks), with today's
gate harness copied in. Four alternating runs per side, same machine:

| | scroll p50 | scroll p90 |
| --- | --- | --- |
| wave 1 | 11.57–13.59 ms | 12.81–15.11 ms |
| waves 1+2 | 8.18–8.42 ms | 9.31–9.56 ms |

`settings_scroll_frame_gate` still reports 90 frames, 90 passes, and no restyle
after the first frame on both sides.

`navigation_frame_cost_profile` with `MESH_BENCH_BACKDROP=in-surface`, default
60 events with the stage profiler on (not comparable to the 300-event
uninstrumented rows of the previous entry). Both sides painted 60 audio and
35 pointer frames; per-event cost:

| | paint only | pointer move | audio poll | render hooks alone |
| --- | --- | --- | --- | --- |
| wave 1 | 1.32–1.44 ms | 2.05–2.13 ms | 9.12–9.31 ms | 0.99–1.05 ms |
| waves 1+2 | 0.80–0.81 ms | 1.76–1.85 ms | 8.71–9.26 ms | 1.05–1.11 ms |

The audio poll is a restyle/relayout frame and is unchanged within noise, as
expected: wave 2 targets paint-only frames.

**Wave 3: `ext-background-effect-v1`.** The presentation backend binds
`ext_background_effect_manager_v1` next to the KDE manager
(`wayland_surface/backend/blur.rs`). The ext protocol is preferred while its
`capabilities` event carries `blur`, else KDE blur, else none; the choice is
re-read on every staging, so a capability that arrives late or is withdrawn
moves each surface's region onto the other protocol (or clears it).
`supports_compositor_backdrop_blur` now reports that live choice instead of the
KDE global's version, and the shell requests a paint on every component when
the backdrop policy flips, since an idle surface would otherwise keep display
lists built for the old policy. The per-surface object is still created lazily
and a region is only resent when it changes.

On Hyprland this selects `CompositorRegion`: the in-surface blur path is no
longer taken at all for the navigation bar, and the compositor blurs the
desktop behind the `.nav-shell-blur` region on its GPU. Live check, release
`mesh-shell start` for 8 s with `WAYLAND_DEBUG=1` (Hyprland, same session as
the entry below): `capabilities(1)` arrives, the bar gets one
`get_background_effect` and one `set_blur_region` with 19 rounded-corner band
rectangles (2880 px wide), and no protocol error. The visible result on screen
was not inspected, and the bar-hover session profile the plan called for was
not repeated.

Namespace decision (item 13): the `:blur` suffix stays when a protocol is
bound. Changing it would recreate the surface on a capability change, and it is
a name that user rules match; 04 §10 now tells users to leave the Hyprland
`layerrule` out on compositors with a protocol. 04 §10 was rewritten for the
protocol order, the runtime capability, and the no-protocol in-surface
fallback (outline only; the full contract remains in the in-surface blur
backlog item). The plan `2026-09-30-compositor-backdrop-blur.md` is removed.

**Verified.** New `blur_prefers_background_effect_only_while_it_can_blur`;
`mesh-core-presentation` passes (113). Debug `cargo test -p mesh-core-shell
-p mesh-core-render -p mesh-core-elements -p mesh-core-presentation
--no-fail-fast`: 42 failures. Against the wave-2 set, `phase98_*` passed and
`phase26_real_surface_baseline_emits_canonical_proof_measurements` plus three
`scheduler_*` tests failed; the scheduler tests pass alone (3/3) and `phase26_*`
fails alone but is in the pre-change baseline failure set. Clippy reports
nothing on the new code; rustfmt is clean on it.

## 2026-09-30 — scroll and animation frames, waves 1 and 2: one paint per frame, paint-only scroll ticks, cheaper paint-only frames

`working tree` · area: frame scheduling, scroll invalidation, backdrop blur, profiling harness, style animation, display-list pre-clip, layout, eligibility, runtime annotation

Implements the "Scroll and animation frames" backlog items filed by the
entry below. Wave 1 is measured; wave 2 is verified for correctness only.

**Wave 1.**

1. **Gate `settings_scroll_frame_gate`** (`appearance_profile.rs`, ignored,
   release). Real `@mesh/settings` Appearance page at 920x900 with a 240-entry
   resource catalog. The pointer hovers the page and transitions settle; then
   60 `TwoFingerScroll { dy: -4 }` frames and 30 momentum frames, 16 ms apart.
   Each frame runs the way `render_components_inner` does: paint only when
   `wants_render()`, plus one corrective pass when `wants_immediate_rerender()`.
   It fails when any frame paints twice, or any frame after the gesture's first
   restyles (the first frame claims scroll ownership, a one-time interaction
   restyle).
2. **No second pass for animation re-arm.** `next_frame_only_dirty` marks dirt
   that a live transition, keyframe, smooth scroll, or momentum tick schedules
   for the next frame. `wants_immediate_rerender` ignores it, and requires
   pending dirt, so an animating surface no longer re-runs a full pass before
   every present. Measurement and configure dirt still take the corrective pass.
3. **Paint-only scroll ticks.** Momentum and smooth-scroll ticks invalidate
   `PAINT | METRICS` when an offset moved, and `PAINT` alone while momentum
   waits out its 34 ms start delay (previously `VISUAL_REPAINT`).
4. **Empty in-surface backdrops are not blurred.** The painter skips
   `ApplyBackdropFilterInSurface` for a node that `compute_backdrop_regions`
   gave no read region, i.e. nothing is painted beneath it in the surface. The
   shipped navigation bar has exactly one such node (the `.nav-shell` root) and
   was blurring transparent pixels every frame. Pixels are unchanged.
5. **Profiling overhead.** The `#[cfg(test)]` focused proof snapshot is built
   only in debug test builds by default (`focused_proof_enabled`), so release
   benchmarks stop paying for it; tests that assert on it enable it. Per-rule
   restyle attribution now needs `MESH_PROFILE_STYLE_RULES=1`.

Scroll gate, release, desktop in use, alternating runs of the pre-change tree
(same harness) and the change:

| | frames | passes | restyled after first | frame p50 | frame p90 |
| --- | ---: | ---: | ---: | --- | --- |
| before (4 runs) | 71–89 | 142–177 | 70–88 | 13.24–30.12 ms | 17.65–34.58 ms |
| after (4 runs) | 90 | 90 | 0 | 8.10–11.60 ms | 12.36–14.40 ms |

"Before" frames often exceeded the 16 ms interval, so fewer fit in the run. The
machine drifted (one "before" run measured 13.24 ms, three measured
29.6–30.1 ms); the pass and restyle counts are not timing-dependent.

Navigation bar, `navigation_frame_cost_profile` with the new
`MESH_BENCH_BACKDROP=in-surface` switch (Hyprland's policy; the harness default
is the compositor policy, which paints no blur), 300 events × 3 runs,
alternating. Very noisy — the audio poll ranged 4.6–9.5 ms on each side, so no
audio claim is made. Pointer loop:

| | frames painted | per event | per painted frame |
| --- | ---: | --- | --- |
| before | 99–135 | 1.18–1.96 ms | 2.9–4.4 ms |
| after | 160 | 0.52–0.98 ms | 0.98–1.63 ms |

The "before" side painted fewer pointer frames; not investigated.

**Wave 2** (not timed yet — the release gates were not re-run).

6. **Style-animation pass skipped on paint-only frames with nothing live**
   (`should_run_style_animation_pass(styles_unchanged)`). Styles are not
   re-resolved on a paint-only retained frame, so no transition can start.
7. **Linear display-list pre-clip.** `should_preclip_child_subtree_with_transform`
   stops as soon as the growing union of visual bounds reaches the clip (the
   union only grows, so the answer is unchanged), and visual bounds come
   straight from the widget node instead of a full `DisplayPaintNode` per
   descendant. `early_exit_preclip_matches_the_full_subtree_union` compares it
   with the old union walk.
8. **`validate_widget_tree` skipped on no-op layout frames.** The incremental
   layout's existing "nothing affects geometry" return now happens before the
   validation walk; every frame that lays out still validates first.
9. **Local eligibility reads its attributes in one pass.** `NodeEligibility::local`
   did seven string-keyed binary searches (`hidden` and `inert` twice); one scan
   of the sorted list with a length-first string `match`, stopping after
   `"inert"`, reads all five.
10. **Scroll-only runtime annotation.** A paint-only retained frame whose
    annotation inputs (focus, hover path, pressed node, slider/input/checked
    values, window state, promoted windows, shortcut cache generation) equal
    the previous paint-only frame's writes only clamped scroll offsets into the
    retained metrics. A frame that ran layout never stores inputs, because
    annotation measures overflow before layout. A paint-only frame *can* change
    interaction state (a text selection clears the pressed node), which is why
    this compares inputs rather than trusting the dirty flags.
    `consecutive_scroll_frames_annotate_only_scroll_offsets` checks pixel
    equality against a twin forced through the full walk.

Item 11 (fingerprint only restyled nodes) was not done: its main trigger was
the momentum restyle removed in wave 1, and the remaining full-restyle frames
relayout, so a style-only dirty set would be incomplete. Backlog item reworded.

**Verified.** Debug `cargo test -p mesh-core-shell -p mesh-core-render
-p mesh-core-elements --no-fail-fast`, wave 1 against the pre-change tree in a
separate worktree and target dir: identical failure sets apart from the new
wave-1 tests (which fail on the old tree, as intended) and 14 render icon tests
that fail only in the worktree checkout. With wave 2, the same suites against
the wave-1 failure set: no new failures except
`phase98_pixel_equivalence_backend_update`, which passes 3/3 alone (a known
order-dependent test); `phase26_*` and three `scheduler_*` tests passed this
run. Clippy adds no warnings on the new code; rustfmt is clean on it.

## 2026-09-30 — settings scroll: every frame paints twice, and momentum ticks restore the full restyle

`working tree` · area: frame scheduling, scroll invalidation, display-list build, paint · investigation only, nothing landed

A live session capture with the new `./tools/profile-shell session` (CPU stacks
joined to the shell's input log and built-in stage timings; report by
`./tools/profile-report`). The operator idled, clicked through the navigation
bar, then scrolled the Settings Appearance page with a touchpad. Fixes are
filed in the backlog under "Scroll and animation frames"; the A/B numbers below
come from temporary patches that were reverted, not from rejected designs.

**Live session.** `profiling` build (release + line debuginfo), Hyprland,
`perf record -e cpu-clock -F 499 --call-graph dwarf,16384`, with the stage
profiler on (its per-rule style attribution adds ~5% `clock_gettime`, so
restyle shares are slightly inflated). Capture `profiles/session/20260930-142634`.

| activity | time | shell CPU (of one core) | renders/s | mean / max settings render |
| --- | ---: | ---: | ---: | --- |
| settings scroll (818 two-finger events) | 18.6 s | 58% (per-second peaks 72%) | 32.3 | 24.8 ms / 77.1 ms |
| navigation-bar clicks and hover | 11.2 s | 39% | 69.4 | — |
| idle | 4.7 s | 5% | 1.3 | — |

Scroll stage shares of wall time: full restyle 28.9%, paint 13.5%, paint
traversal 11.7% (text blits 1.29 s of 2.16 s), layout 8.1%, display-list update
7.6%, render-object sync 4.5%. Top self time: `retained_snapshot_with_render`
9.6%, `memcmp` 8.9% (mostly `interaction_contract::boolean_attribute` via
`NodeEligibility::local`), `memmove` 5.7%, `clock_gettime` 5.2% (profiler).

**Finding 1 — two full passes per presented frame.** Settings logged 1,294
paints for 652 presents; the navigation bar 976 paints for 520 presents. In
`render_components_inner` a surface is painted again before present whenever
`wants_immediate_rerender()` is true, and that is true whenever `wants_render()`
is and the pending work is not surface-configuration only. A live transition,
keyframe animation, smooth scroll, or momentum entry keeps `wants_render()`
true, so every animated frame is restyled, laid out, and painted twice before
the compositor sees either. The per-frame stage order in the activity
log shows the pair directly: restyle → layout → display list → paint, twice,
then one `present_commit`.

**Finding 2 — momentum and smooth-scroll ticks invalidate `STYLE`.** Direct
two-finger input invalidates `PAINT | METRICS`, which takes the paint-only path
from the 2026-08-08 entry "skip full style restyle on paint-only scroll frames".
`advance_scroll_inertia` and `advance_scroll_animations` invalidate
`VISUAL_REPAINT` (`STYLE | PAINT | ACCESSIBILITY | METRICS`). A momentum entry
is created on every finger event and ticks during its 34 ms start delay, so
every scroll frame — fingers down or not — carries `STYLE` and takes the
full-restyle branch, plus layout, full retained fingerprinting, and full
`normalize_accessibility`. That silently undid the 2026-08-08 fix; its gate
(`paint_only_frame_speedup`) invalidates `PAINT` directly and never sees
momentum.

**Headless reproduction.** Temporary ignored test on the real `@mesh/settings`
Appearance page, 920x900, large resource catalog: 60 frames of
`TwoFingerScroll { dy: -8 }` then 30 momentum frames, 16 ms apart, mirroring the
render loop's one extra pass. `cargo test --profile profiling`, three runs each:

| variant | passes / 90 frames | frame p50 | frame p90 |
| --- | ---: | --- | --- |
| current | 160–164 | 15.96–20.36 ms | 22.27–27.40 ms |
| no second pass | 90 | 15.32–15.40 ms | 24.22–24.45 ms |
| momentum ticks → `PAINT \| METRICS`, both passes | 162–167 | 12.30–16.64 ms | 16.60–25.79 ms |
| both | 90 | 11.63–11.86 ms | 22.67–24.35 ms |

The desktop was in use, so ranges are wide; the dirty flags are not noisy: with
the tick change every scroll frame reports `PAINT | METRICS` instead of
`STYLE | PAINT | ACCESSIBILITY | METRICS`, and the full restyle disappears.

**What a paint-only scroll frame still costs** (perf on the "both" variant,
samples inside `paint`, excluding the test's own idle-frame rebuilds):
display-list update 23.9% (`build_paint_subtree_with_transform` 14.3%, of which
the pre-clip bounds walk `subtree_bounds_at_with_transform` is most), painting
21.7% (text blits 14.4%), style-animation pass 11.6%
(`apply_style_animations_to_node` 8.6% + `collect_visual_styles_into` 3.0%),
`#[cfg(test)]` focused proof snapshot 10.6% (build + drop; test builds only),
`validate_widget_tree` inside layout 5.4%, runtime annotation 4.2%,
`normalize_accessibility_dirty` 2.3%. No glyph-atlas misses (fractional scroll
offsets do not defeat the cache).

**Finding 3 — navigation-bar interaction is dominated by in-surface blur.**
During bar clicks and hover, `ThreeBoxApproxPass::blurSegment` is 18.7% self
time and blur 21.9% inclusive, all from `.nav-shell-blur`'s
`backdrop-filter`. On Hyprland (no `org_kde_kwin_blur`) the policy is
`InSurfaceFilter`, so every damaged animation frame re-blurs the clipped
backdrop.

**Harness observations.** Direct `paint()` calls in the in-crate benchmarks
bypass `render_components_inner`, so none of them can see Finding 1, and the
`#[cfg(test)]` focused proof snapshot adds ~10% to every in-crate paint-only
frame measurement.

## 2026-09-30 — navigation bar: stop re-creating failed runtimes, scope style diagnostics, one-pass accessibility attributes

`working tree` · area: frontend runtime lifecycle, runtime style diagnostics, accessibility normalization, harness

Found by sampling `navigation_frame_cost_profile` under `perf` (release,
`nix develop`) rather than from the backlog. Three changes land; one experiment
was reverted; the harness had two defects of its own.

**1. A runtime that cannot be created is no longer re-created every frame.**
The bar's start-slot components (workspace list, window title) require
`mesh.wm`. With no provider, `create_runtime_for_component` built a whole Luau
context — host API install, storage open, i18n translator — then failed with
`InterfaceUnavailable`, dropped it, and rendered a placeholder. Nothing was
cached, so the next frame did it again: 411 creations over ~400 frames for two
instances, plus a `tracing::warn!` per frame. A surface's interface catalog and
capability grants are fixed for its lifetime, so `InterfaceUnavailable` and
`CapabilityDenied` creation failures are now remembered per instance key
(`failed_runtime_creations`) and cleared wherever the runtime map is rebuilt
(reload, frontend catalog change). The diagnostic issue stays open. This
affects any real session without a provider for a required interface, e.g. a
compositor with no `mesh.wm` backend.

**2. Runtime style diagnostics re-resolve only nodes the retained diff marked.**
`record_runtime_style_diagnostics` re-resolved every node's full style with
diagnostics after each rebuild, only to keep animation diagnostics — which all
land in the single `runtime` issue row. Its gate included
`retained_tree_generation`, which advances on every changed rebuild, so it ran
on every audio poll. When the only input that moved is the retained generation,
by exactly one step, and that update was non-structural, it now resolves only
nodes flagged `STYLE | ATTRIBUTES | STATE`. Each node is resolved in isolation
in both walks, so the per-node result is unchanged. Anything else takes the
full pass as before.

**3. Accessibility normalization reads a node's attributes in one pass.**
`normalized_info` did about 40 binary-search lookups per node (`aria-*` and
alias names, each a string compare chain). It now fills named slots in one scan
of the sorted attribute list and applies the same precedence chains.
`normalization_keeps_alias_precedence_and_empty_values` pins alias order, the
any-true `disabled` rule, and "empty `aria-role` does not fall back to `role`".

**Harness correction.** `navigation_frame_cost_profile` painted after every
event, whether or not the component wanted a frame, and its pointer loop ran
along `y = 40` — the bottom edge of the settled 40 px bar — so it never hovered
anything. A forced paint of an unchanged surface takes the conservative
full-rebuild path, which a session never does (`Shell::render_components`
checks `wants_render()`). The harness now paints only when asked, moves the
pointer through the bar's middle, leaves and settles hover transitions by wall
clock before the following loops, and reports how many events painted.
`MESH_BENCH_ONLY=<loop>` restricts a run to one loop for `perf`. Consequences
for earlier entries: the "media poll (unread)" costs (2.3–2.6 ms on 2026-09-21)
and every "pointer move" figure measured that forced-repaint path. In a session
an unread poll paints **0** frames. The harness catalog still has no `mesh.wm`
provider, so the start slot is two error placeholders here (see backlog).

**Measured.** Release under `nix develop`, interleaved runs of isolated builds.

Per change, as measured at the time (the first two on the old harness):

| change | workload (events × runs) | before | after |
| --- | --- | --- | --- |
| 1 | unread poll, old harness (200 × 3) | 2.05–2.17 ms | 1.53–1.55 ms |
| 1 | pointer, old harness (200 × 3) | 2.12–2.21 ms | 1.54–1.60 ms |
| 1 | audio poll, old harness (200 × 3) | 4.74–4.88 ms | 4.17 ms |
| 2 | audio poll, old harness (200 × 3) | 4.00 ms | 3.07–3.10 ms |
| 3 | audio poll, corrected harness (300 × 3) | 3.22–3.25 ms | 2.92–2.94 ms |

End to end, corrected harness on both sides, 300 events × 3 runs:

| workload | before | after |
| --- | --- | --- |
| audio poll (read; 300/300 paint) | 4.74–4.90 ms | **3.06–3.17 ms (1.53x)** |
| pointer across the bar (160/300 paint) | 0.49–0.52 ms | 0.46–0.48 ms |
| paint only | 0.49–0.50 ms | 0.47–0.49 ms (overlapping) |
| unread poll | 0 paints | 0 paints |

This machine drifted about 3% between batches (the same change-3 binary
measured 2.92–2.94 ms, then 3.03–3.10 ms), so compare only within a batch.

**Rejected — cheaper hashing in `validate_widget_tree`.** It runs a full-tree
walk on every build (~4% of an audio frame) with SipHash sets and a `String`
allocation per keyed node. Borrowed keys plus a multiply-rotate hasher:
3.03–3.07 ms vs 3.03–3.10 ms audio poll, fully overlapping. Reverted.

**Remaining profile (audio poll, after).** Template re-evaluation on the narrow
path is ~40% — mostly the three instances that read `mesh.audio`. Full-tree
`normalize_accessibility` is ~20%: narrow script frames cannot use
`normalize_accessibility_dirty`, because memo-reused subtrees are stored before
finalization and carry no normalized data. Tracked in the backlog.

**Verified.** New tests:
`unavailable_interface_runtime_failure_is_not_retried_every_frame` (a
remembered message is rendered verbatim on the next frame, so no retry occurred),
`animation_token_diagnostic_follows_a_non_structural_class_change` (fails with
the scoped branch disabled), and the precedence test above. Debug `cargo test
-p mesh-core-shell -p mesh-core-elements --no-fail-fast`: 10 and 31 failures,
the same counts as the same tree with these changes removed. The three names
that differed each way (`scheduler_*`, `phase26_*`, `phase98_*`,
`debug_snapshot_exposes_installed_module_graph_contracts`) behave identically
on both sides when run alone; `phase26_*` fails on both. rustfmt is clean on
the new code, and clippy adds no warnings.

## 2026-09-30 — a full-restyle style-share cache hits 99% and saves nothing

`archive/style-share-cache` (local branch, not landed) · area: style resolution

This targeted the backlog item "a tree-rebuild frame restyles memo-reused
subtrees". A `StyleShareCache` on each surface reused a prior `ComputedStyle`
whenever a node's resolution inputs were identical: tag, module, classes, id,
inline style, state, the parent's inherited style, and container size. Theme,
props, and rule identity formed the cache epoch. It was correct, and a
node-for-node equality test against the unshared restyle passed.

**Measured.** Release under `nix develop`, `appearance_frame_cost_profile` (real
`@mesh/settings` Appearance surface, 920x900, 60 unrelated `mesh.audio` frames),
three interleaved runs each against an isolated build of `0aff0b08`:
**15.51–15.91ms before, 15.13–16.07ms after** per service frame, fully
overlapping. The cache answered 39,428 of 39,720 lookups (99.3%), so the misses
were not the problem. Hashing and comparing the key, then cloning a whole
`ComputedStyle` into the node, costs about as much as resolving the style.

**What this rules out.** Value-level memoization of computed styles cannot beat
resolution while a hit still clones the full style. A win here needs the
restyle walk to *skip* nodes (a "styles still valid" mark on memo-reused
subtrees), or a cheaply shareable `ComputedStyle` (`Arc`/copy-on-write). The
backlog item stays open with that note.

Note: this harness's attributed second pass takes the `*_cached_profiled`
restyle path, which bypasses any unattributed restyle variant. Read only the
`MESH_PERF` wall-clock line when evaluating restyle changes.

## 2026-09-21 — the narrow service path engages, and sparse frames stop paying whole-stream costs

`working tree` · area: service invalidation, damage selection, display-list batch metrics

Completes the render-pipeline items opened on 2026-08-08. Four changes, plus a
correction to the harness that produced that entry's headline number.

**1. Render hooks no longer escalate a narrow service update.** The 2026-08-08
entry below diagnosed the cause correctly — shipped components read services in
Luau and assign derived template variables from `render()`, so `narrow_nodes` is
empty — but the fix is not to index script reads. It is that
`call_render_hooks` unconditionally called `invalidate_script_state()`
(`TREE_REBUILD` plus `surface_pixels_invalid`) whenever a hook wrote state. When
the hook is running *because* a service update already raised `SCRIPT_NARROW`,
it now keeps the frame on the narrow path: the template is still evaluated in
full, and `narrow_script_dirty_roots` plus the retained diff decide between a
scoped and a full synchronization. Structural changes still fall back.

**2. A `backdrop-filter` grows damage by the kernel reach, not to the whole
region.** `expand_damage_for_blur_regions` unioned any intersecting damage with
the entire blurred region, so a surface-spanning backdrop collapsed every
partial repaint to the full surface. Blur output at a pixel is a function of the
backdrop within the kernel reach of it, so the expansion is now
`inflate(damage, reach) ∩ region`. Each backdrop applies once, against the
damage as it stood before that backdrop — re-inflating a rect the same pass grew
cascades one reach at a time and refills the region, which is the bound being
avoided. `filter: blur()` layers keep the whole-region rule: that layer is
rasterized as a unit. `BackdropRegion` now carries its reach alongside the rect.

**3. Sparse display-list frames patch retained batch metadata.** Batch metrics
are a property of the whole ordered primitive stream, so `collect_display_entries`
walked every node and every primitive slot on every frame — including sparse
frames — purely to rebuild that stream. A new `BatchIndex` holds it as a segment
tree over `(node, slot)` leaves in paint order, each summary carrying its leading
and trailing run so two ranges join in constant time. A sparse frame descends to
the dirty nodes by recorded path and repairs `O(log n)` summaries. It is valid
only under the conditions `can_patch_sparse_entries` already requires (no
insert/remove/reorder/transform/clip/opacity/geometry change); anything broader
reseeds it. The old ordered-stream side channel is gone from the update path and
`compute_batch_metrics` survives only as the differential test oracle.

**4. Display-list command storage is segments, replayed directly.**
`PaintSequence` retains child sequences by `Arc` instead of flattening command
arrays into each ancestor. Replay consumes segments; the flat slice is built
lazily and only for the diagnostic accessors, which no production caller now
uses. (The eager reconstruction recorded as rejected below is not what this is.)

**Measured — damage.** Release profile under `nix develop`, the settled
1920x40 navigation bar, 60 sampled frames, `navigation_frame_cost_profile`,
three runs each.

| | before | after |
| --- | --- | --- |
| surface repainted per `mesh.audio` poll | 100.0% | 1.7% |
| frames at full surface | 60/60 | 1/60 |
| rects per frame | 1.00 | 0.05 |
| audio frame (ms) | 5.07–7.58 | 5.21–5.82 |

The one remaining full-surface frame is the first payload, which is a genuine
first-paint rebuild. Most later frames produce *no* damage at all: percent
40→41 changes the aria-label and the tooltip string, but the button renders an
icon whose bucket has not changed, so nothing visible moves. Wall-clock ranges
overlap — this harness paints into a local `PixelBuffer` and models neither the
SHM copy nor the compositor's re-blur of a damaged blurred layer surface, which
is where full-surface damage is actually paid.

**Measured — batch metrics.** Release profile,
`sparse_batch_metrics_beat_the_ordered_walk`, 4 dirty nodes of 4921 primitives,
2000 iterations, three runs: ordered walk 636–662ms, retained index 7.5–7.9ms,
**81.9–85.2x**. The same test asserts both paths report identical
`batch_count` / `batched_primitives` / `barrier_count`, and
`patched_batch_index_matches_a_full_ordered_scan` re-checks that equality against
a full scan across a sequence of sparse frames.

**Correction — the 2026-08-08 measurement was read through three harness bugs.**
The entry below reports "240/240 frames full-surface" from
`navigation_profile.rs`. That harness:

1. published `{"volume": N}` to `mesh.audio`, but the bar's `render()` reads
   `audio.percent` and returns early unless `audio.available` is set — so every
   loop in it, including the one labelled "audio poll (read)", was measuring a
   poll the bar ignores;
2. read `component.last_present_damage_rects` rather than taking it. A frame
   with zero damage leaves that accumulator untouched, so the loop re-reported
   the *previous* loop's full-surface rect and scored a frame that repainted
   nothing as 100%;
3. consequently reported "audio poll (read)" and "media poll (unread)" as
   near-identical costs, which is what an unread poll measured twice looks like.

The harness is fixed here. The conclusion survives: re-measured against clean
`main` with the corrected harness, the baseline really is 100.0% and 60/60
full-surface. The *magnitude* of the per-frame wall-clock claims in that entry
should not be reused — a genuine read now costs 5.0–6.0ms against 2.3–2.6ms for
an unread poll, where the old harness reported both as the same work.

**Correction — backdrop blur was not the latent amplifier it was recorded as.**
That entry lists `.nav-shell`'s `backdrop-filter` as a latent damage amplifier
waiting behind the narrow-path fix. With the narrow path engaged, the
navigation bar reports **zero** backdrop and filter-layer regions on these
frames, so it was not the next ceiling. Change 2 above is still correct and
still bounds the collapse — it is simply not what was holding this surface back.

**Not done — `FrameSnapshot::capture_dirty`.** The incremental semantic capture
is implemented and now proven: `dirty_capture_matches_a_full_capture` runs 33
label/layout/state mutations across 11 generated trees and asserts the
structurally shared result is indistinguishable from a full capture, node for
node, including the semantic diff; a second test asserts structural edits take
the fallback. It has no production caller, because its contract requires an
authoritative changed-node set "including layout propagation and interaction
state", and the only authoritative source — the retained tree diff — is computed
*after* `finalize_tree` captures the frame. Feeding it a non-exhaustive set
would silently publish stale accessibility data. Wiring it needs the frame
reordered so the retained diff precedes semantic capture, which is the
changed-node-fingerprint unification still open in the backlog.

**Verified.** Workspace tests under `nix develop`, `--no-fail-fast`, before and
after: 53 failures both, all pre-existing. The three membership differences
across runs are order-dependent or timing-dependent tests that reproduce on
clean `main` standalone
(`thread_vm_reclaims_dropped_context_environments`,
`skia_effect_linear_gradient_reuses_shader_for_moving_same_size_rects`,
`phase26_real_surface_baseline_emits_canonical_proof_measurements`, the last
depending on a process-global icon cache warmed by earlier tests). `cargo fmt`
clean; the two new files add no clippy warnings.

**Fixed along the way.** `annotate_runtime_tree_inner` removed `_mesh_focused`
from every unfocused node unconditionally. `attributes` lives inside the shared
authored payload, so a remove of an absent key still forced a copy-on-write
clone — breaking memo payload sharing for the whole subtree, and costing
allocation on exactly the frames the rest of this work makes cheap. Both
branches now check before writing.

---

## 2026-08-26 — enable Hyprland socket events in the active graph

`working tree` · area: Hyprland workspace reaction latency

The Hyprland backend already consumes `.socket2.sock` events, but the active
development graph approved only its `hyprctl` and socket-discovery commands.
Because `socat` and `nc` are optional capabilities and optional grants default
to denied, the backend could not start either stream and fell back to its
500 ms safety poll. The graph now explicitly approves both stream forms; the
backend will use whichever installed tool is available and retain polling only
when neither is available.

**Measured.** No live compositor before/after range is claimed in this
environment. The structural baseline is the configured 500 ms fallback; with
an approved stream, delivery is event-driven rather than gated by that poll.

## 2026-08-26 — skip unobserved theme-token fan-out and locale catalog recompilation

`working tree` · area: theme/locale switching

Theme revision publication now computes the changed-token identities once and
consults the existing service-delivery subscription index before constructing
individual `TokenChanged` payloads. Exact subscribers retain per-token events;
components without an authoritative observation summary remain conservative
fallback observers. Every revision still publishes the aggregate
`ThemeChanged.changed_tokens` payload.

Locale-only settings writes and settings-file reloads now replace the locale
selection while retaining the active immutable catalog `Arc`. Graph/profile
activation remains the owner of catalog snapshot preparation, so a source-set
change still reads, compiles, validates, and atomically replaces the snapshot.

**Measured.** Release profile under `nix develop`, three runs of
`unsubscribed_theme_token_event_gate_beats_fanout`: 100 batches of 215 token
changes, matching the shipped theme token count, with an indexed component
subscribed to `ThemeChanged` but not `TokenChanged`. Forced construction,
contract validation, and no-subscriber routing took 16.239–17.446ms; the exact
subscriber gate took 4.749–4.750us. This measures only the removed event work,
not an end-to-end theme-switch speedup. The ignored release gate requires the
subscriber path to finish before the forced-fan-out control.

**Verified.** The service-delivery slice passed 7 active tests; the focused
theme/locale regressions passed, including fallback-observer preservation,
individual-event delivery to a real subscriber, aggregate-only delivery without
one, durable locale revision commit, and catalog-snapshot pointer reuse. The
serialized theme slice passed 18 active tests and reproduced only the existing
`malformed_theme_reload_retains_last_known_good_snapshot` diagnostic-count
failure recorded in the current monthly log. Formatting and diff checks passed.
Workspace clippy remains blocked by the existing warning backlog, including
warnings in dependencies and 172 shell warnings with `--no-deps`.

---

## 2026-08-16 — key animation transition state by NodeId

`working tree` · area: animation identity and per-frame allocation

Transition state, previous visual snapshots, and the live transition set now
use the stable runtime `NodeId` assigned during tree annotation. The animation
walk no longer owns a mesh-key `String` and clones it into a live set for every
node on every frame. Keyframe registry entries remain string-based because
their names are authored CSS identities.

**Measured.** Release profile under `nix develop`, three runs of
`animation_node_id_keys_beats_string_keys`: 1,024 nodes and 5,000 passes over
the previous-style lookup plus live-set update. The former owned-string shape
took 296.090–805.982ms; the `NodeId` shape took 88.414–252.904ms, with paired
speedups of 3.19–3.47x. The checked ignored gate is
`animation_node_id_keys_beats_string_keys`.

**Verified.** `mesh-core-animation` passed 19 tests; the focused shell
animation slice passed 30 active tests with 4 ignored; formatting and diff
checks passed.

---

## 2026-08-15 — scope animation restyles and slider handler invalidation

area: retained style invalidation, slider input

Animation-only frames now enter the targeted restyle decision using the
existing `animation_only_frame` marker. When no interaction state changed, the
restyle walk is skipped while animated values are still applied and the
retained tree receives the animated dirty roots. Slider `change` and `release`
handlers likewise no longer force `TREE_REBUILD`; handler dispatch selects
`SCRIPT_NARROW` when reactive state changed, and the slider retains an
interaction repaint for handler-side effects that do not change script state.

**Measured.** Release profile under `nix develop`, three runs of the existing
`animation_scoped_retained_end_to_end_benchmark`: a 1,026-node tree with one
animated node over 250 frames. Forced full retained fingerprinting took
659.275–697.273ms; the scoped path took 351.863–398.864ms (1.748–1.874x).
The checked ignored gate is `animation_scoped_retained_end_to_end_benchmark`.

The new `slider_drag_handler_narrow_script_beats_full_rebuild` gate uses a
96-row surface and 200 drag frames per run. The former forced full rebuild took
373.980–939.031ms across three release runs; narrow script invalidation took
143.390–306.308ms (2.4–3.1x). The gate requires the narrow path to finish
before the forced rebuild control.

**Verified.** The focused animation and slider slices passed (30 and 18 active
tests respectively), including the new `waste:empty_restyle_avoided` proof and
the renamed narrow-invalidation regression. The full `mesh-core-shell --lib`
suite passed 667 tests with 6 already-recorded fixture/manifest/debug failures
and 128 ignored. `cargo fmt --all -- --check` passed.

---

## 2026-08-15 — pace service control writes and remove successful read-backs

area: runtime command dispatch, audio/brightness backends, service-backed input

Coalescable service setters now use a 100ms backend-command budget instead of
the former 16ms render-tick interval. The shell keeps leading-edge delivery
and last-wins trailing flushes, so optimistic state bindings keep controls
responsive without permitting external-process-backed setters to run at the
frame rate. Navigation brightness scroll now sends the absolute `set` value,
which makes it coalescable instead of issuing relative commands that cannot be
safely collapsed.

PipeWire and PulseAudio successful writes no longer synchronously reread state
when their monitor stream is active. The shell's optimistic binding is
confirmed by the stream or safety poll; failed writes and fallback polling
still reread state. The brightness provider likewise publishes a successful
requested level optimistically and leaves rounding/clamping correction to its
2-second safety poll.

**Baseline.** The live `profiling` capture from 2026-08-08 used a 16ms
interval, observed up to 62.5 volume commands/second, two `wpctl` launches per
command, 15.5ms CPU per launch, and 93.4% of one core in children versus 3.55%
for the shell process. That workload was on a live Hyprland session; the
current verification environment did not provide a compositor session, so no
live before/after CPU range is claimed here.

**Checked structural gate.** `coalescable_service_commands_use_a_backend_cost_budget`
proves leading-edge delivery, last-wins pending payloads, and trailing flush
after the 100ms budget. The bundled PipeWire and PulseAudio backend tests each
prove one successful setter produces only the write process when streaming;
the bundled brightness test proves a successful write publishes the requested
level without a read-back. The shipped navigation brightness tests cover wheel,
trackpad, configured sensitivity, and invalid-sensitivity fallback through the
coalescable absolute setter.

---

## 2026-08-08 — Hyprland stream events stop spawning full-state queries

area: backend host API, Hyprland provider, service events

The Hyprland backend now consumes authoritative `activewindow`, `workspace`,
and `workspacev2` socket payloads directly. Address-only or named-workspace
events query only their missing field, and structural workspace events query
only the workspace catalog. The 2-second full-state poll remains as a safety
net. State commits compare fields independently, so title-only changes emit
`WindowChanged` without the former redundant `WorkspaceChanged` event.

**Measured.** `profiling` build (optimized plus debuginfo, rustc/cargo 1.94.0),
live Hyprland session, active terminal title changing at approximately 10 Hz.
`perf record -F 997 -g` attached to the shell with child inheritance. Before,
a 30-second capture recorded 19,970 samples, or **66.8% of one core**; roughly
76% of samples were `hyprctl`, mostly dynamic-linker startup. The first fixed
20-second control recorded 3,708 samples, or 18.6% of one core, with `hyprctl`
down to roughly 16% of samples and primarily the intended safety poll. After
demoting title-only state logs from info to debug, a final 20-second Settings
scroll recorded 3,163 samples, or **15.9% of one core** including children: a
**4.20x** reduction from the original scroll capture. The event path itself
moved from three `hyprctl` launches per title change (about 30
launches/second in this workload) to zero.

The paired shell-only controls also showed why the symptom looked like scroll
cost: ordinary scrolling used 9.2% of one core while the no-scroll title-storm
control used 10.1%. Scrolling added no measurable shell CPU in that capture.

**Checked structural gate.** The
`hyprland_stream_events_query_only_missing_fields_and_emit_selectively`
regression requires zero subprocesses plus only `WindowChanged` for an
`activewindow` payload, zero subprocesses plus only `WorkspaceChanged` for a
`workspacev2` payload, and exactly one workspace-catalog query for an
`openwindow` event. The full `mesh-core-scripting` suite passed: 178 active,
27 ignored.

---

## 2026-08-08 — geometry-only retained snapshots skip style and attribute hashing

area: retained tree diff, scroll frames

Scoped retained updates now distinguish authoritative dirty roots from clean
nodes whose geometry changed through layout propagation. Dirty roots still
compute the complete layout, style, attribute, child, state, and render diff.
For geometry-only nodes, `retained_snapshot_with_render` refreshes the layout
fingerprint and reuses the retained style and attribute hashes, child ids, and
state. This removes the ~70-field `ComputedStyle` hash and full attribute-map
hash from each moved row without weakening full-diff detection at dirty roots.

**Measured.** `release` profile under `nix develop` (rustc/cargo 1.94.0), local
development host, three runs of 2,000 geometry-only frames with 256 moved rows.
Each row carries four children and representative class, resource-id, and
accessibility attributes. The control recomputes the former full retained
snapshot; the split path recomputes layout only. Full fingerprinting took
192.294–193.961ms; split fingerprinting took 62.866–62.967ms, a **3.06–3.08x**
speedup. The checked `geometry_only_fingerprint_speedup` gate requires 2.5x.

**Verified.** The retained-tree suite passed 25 active tests. Focused parity
tests passed for propagated layout plus simultaneous dirty-root style,
attribute, and state changes, and for the end-to-end paint-only scroll against
the forced-full control. `cargo check -p mesh-core-shell` passed in the Nix dev
shell; the direct host check remains blocked by the already-recorded stale Nix
`ld-wrapper.sh` path.

---

## 2026-08-08 — live scroll profile: retained fingerprinting is the hot path

area: retained tree diff, scroll frames

Corrects the entry below that attributed a live scroll's cost to subprocess
spawning. A verified 120-second capture of a real scroll on the **release**
build puts the cost in the shell's own render path.

**Measured.** `release` build, live Hyprland session, `perf record -F 997 -g`
on the shell and its children for 120s while a person scrolled, plus
`/proc/<pid>/stat` accounting:

| | |
| --- | --- |
| shell process | 38.50% of one core |
| its children | 12.90% of one core |
| samples: `mesh-shell` | **89.49%** |
| samples: `wpctl` | 7.35% |
| samples: `hyprctl` | 2.78% |

**The shell's own top self-time:**

```
13.68%  runtime_tree::fingerprint::retained_snapshot_with_render
 5.73%  __memmove_avx512_unaligned_erms
 5.13%  __memcmp_evex_movbe
 4.35%  mesh_core_elements::attributes::AttributeMap::get
 6.21%  malloc / free / cfree / malloc_consolidate (combined)
 1.52%  display_list::paint_node::build_paint_node_with_previous
 2.51%  core::hash::sip::Hasher::write (two entries)
 1.43%  animation::apply_style_animations_to_node
 2.65%  skia srcover_rgba_8888 + rect_memset32
 1.03%  taffy::compute::flexbox::compute_preliminary
```

`retained_snapshot_with_render` is the single hottest function in the process by
a factor of two. It rebuilds, per visited node per frame, a `LayoutFingerprint`,
a `style_fingerprint` over ~70 `ComputedStyle` fields, an `attributes_fingerprint`
over the whole attribute map, and a `child_ids` Vec. Scrolling a list moves
every visible row, so every row's layout fingerprint legitimately changes — but
its style and attributes did not, and both are re-hashed anyway. The
`memmove`/`memcmp`/`AttributeMap::get`/SipHash mass behind it is the same walk.

Actual rasterization is ~2.7% (`srcover_rgba_8888`, `rect_memset32`).

**Correction.** The 2026-08-08 entry titled "scroll-to-100%-CPU is subprocess
spawning" measured a scroll that put children at 93.4% and the shell at 3.55%.
That capture is not wrong, but it was not representative: it evidently caught a
scroll over the volume button, where `onVolumeScroll` → `audio.set_volume()`
drives two `wpctl` launches per command. This capture, over an ordinary scroll,
spawned 695 `wpctl` and 174 `hyprctl` processes in 120s (5.8/s and 1.5/s) for
only 7.35% and 2.78% of samples. Both costs are real; which dominates depends
on what the pointer is over. The spawn storm is the narrower case, and the
retained-diff cost is the general one.

**Magnitude, confirmed by the operator.** The shell averaged 38.5% of a core
across the whole 120s window because the scrolling was intermittent; `btop` on
the same pid showed `mesh-shell` peaking near **80% of one core** during active
scrolling, and nothing reaching 100%. The peak sits on the shell process, not
on its children, which is what the 89.49%-of-samples split already indicated.

---

## 2026-08-08 — the reported slowness was a debug build

area: developer workflow, dev-shell guidance

Every measurement in this log's 2026-08-08 entries was taken against the
`profiling` build (`inherits = "release"`). The shell actually running on the
development host was `target/debug/mesh-shell` — unoptimized.

**Measured.** Same host, same shipped graph, idle with no interaction,
20-second windows, `/proc/<pid>/stat`:

| build | shell process | its children |
| --- | --- | --- |
| `profiling` (optimized) | 0.20–0.27% of one core | 6.6% |
| `debug` | **5.55%** of one core | 14.7% |

**~21–28x** on the render loop at idle. Applied to the 3.55% the optimized
shell used during a live scroll, the debug build lands at roughly a full core —
which is what the reported `btop` reading showed, on the `mesh-shell` process
itself rather than on its children.

**Why it happened.** The `nix develop` banner prints
`Run: cargo run -p mesh-tools-cli --bin mesh-shell -- start`, which builds and
runs the debug profile. That line is the first thing shown on entering the dev
shell, so it is the default path into running the shell, and nothing downstream
distinguishes the resulting binary from a release one at runtime.

**This does not retract the other findings.** The subprocess-spawn storm
recorded below was measured on the *optimized* build and is build-independent:
`wpctl` costs 15.5ms of CPU per launch regardless of how MESH was compiled, and
a scroll over the volume button still drove children to 93.4% of a core there.
A debug build and a spawn storm are two separate problems that happened to
produce the same symptom. Fixing the build does not fix the spawn rate.

---

## 2026-08-08 — scroll-to-100%-CPU is subprocess spawning, not rendering

area: backend host API, command throttle, service method dispatch

A reported "scrolling pegs a core" reproduced against the live shell. The
renderer is not involved.

**Measured.** `profiling` build, live Hyprland session, 60-second
`perf record -F 997 -g --no-inherit` on the shell plus `/proc/<pid>/stat`
accounting, while a person scrolled continuously:

| | % of one core |
| --- | --- |
| shell process (all its threads, render loop included) | **3.55** |
| its reaped children | **93.40** |

The shell's own samples are dominated by Luau on the tokio workers —
`luau_execute` 10.2%, `luaS_newlstr` 3.5%, `luaM_newgco_` 2.0%,
`mlua::state::LuaGuard::new` 1.5%, `mesh_core_scripting::backend::exec_stream::spawn_stream`
1.1% — i.e. backend command dispatch, not painting. No render stage appears
above 1%.

**Mechanism.** `volume-button.mesh` binds `onscroll` / `ontwofingerscroll` to
`onVolumeScroll`, which calls `audio.set_volume()`. Commands are throttled to
`COMMAND_THROTTLE_INTERVAL` = 16ms, so a continuous scroll dispatches up to
62.5 commands per second. Each one costs **two** subprocess spawns:
`on_command_set_volume` runs `wpctl set-volume`, then `refresh_state()` →
`read_state()` → `wpctl get-volume`.

A `wpctl` invocation costs **15.5ms of CPU** on this host (0.31s for 20 runs,
0.16 user + 0.15 sys), almost all of it dynamic-linker work — the earlier
system profile put `do_lookup_x`, `strcmp`, `_dl_lookup_symbol_x` and
`_dl_relocate_object_no_relro` at the top for exactly these children. The
source comment at `read_state` estimates ~20ms per invocation, so the cost was
known; what was missing is that the throttle permits it 125 times a second.

```
 10 commands/s ->  20 spawns/s ->  31% of one core
 20 commands/s ->  40 spawns/s ->  62% of one core
 30 commands/s ->  60 spawns/s ->  93% of one core   <- matches the capture
 62 commands/s -> 125 spawns/s -> 194% of one core   <- what the throttle allows
```

The 16ms throttle is tuned for a *render* budget; it is roughly 30x too
permissive for an operation whose implementation is a process launch.

**Scope.** The same shape applies wherever a scroll or drag maps to a service
method backed by `mesh.exec`: brightness scroll (`brightnessctl`), and audio
slider drags in the popover. The `refresh_state()` round-trip after every write
doubles it, and is redundant while `pw-mon` is already streaming state.

**Context: this is not the idle cost.** At rest, and with Settings merely open
and untouched, children stay at 6.6% and the shell process at 0.2–0.3%.
Scrolling is what moves it, and only when the pointer is over a control whose
handler writes to a service.

---

## 2026-08-08 — live shell capture: the render loop is idle-cheap, backend polling is not

area: measurement only — running shell on a real Hyprland session

The frame profiles below measure `component.paint()` in isolation. This entry
measures the actual shipped shell running against the user's live compositor,
to test whether per-frame cost explains a perceived frame-rate problem. It does
not.

**Setup.** `profiling` build of `mesh-shell start`, shipped root graph
(`@mesh/navigation-bar` as layout entrypoint, pipewire-audio and
backlight-brightness providers), Hyprland session, 1920px output. CPU measured
from `/proc/<pid>/stat` — fields 14/15 for the process, 16/17 for reaped
children — over 20-second windows with no interaction.

| | % of one core |
| --- | --- |
| shell process itself (render loop, at rest) | **0.20** |
| its reaped children (`hyprctl`, `wpctl`, `brightnessctl`) | **6.40** |
| Hyprland, with the MESH bar running | 2.10 |
| Hyprland, with MESH stopped | 3.05 |

**The render loop is not the cost.** At rest the shell burns a fifth of a
percent of a core. A service poll only rebuilds when the payload actually
changed, so an unchanging volume/network/brightness reading costs nothing —
the full-surface rebuild recorded in the entry below fires on real changes, not
on every poll.

**Backend polling costs 32x the rest of the shell.** `perf record -F 997 -g` on
the shell and its children put ~45% of all samples in `hyprctl`, and most of
that in the dynamic linker (`do_lookup_x`, `strcmp`, `_dl_lookup_symbol_x`,
`_dl_relocate_object_no_relro`) — process startup, not work. A 15-second window
contained 24 distinct `hyprctl` processes, 15 `wpctl`, and 7 `brightnessctl`,
matching the shipped poll intervals (hyprland-wm 500ms, audio 1000ms,
brightness 2000ms) at roughly three `fork`+`exec` per second, forever. The
already-open backlog item for push-based backend primitives now has its
measurement: 6.4% of a core, continuously, for no state change.

Not every backend does this: `pw-mon` and `socat` appear as *persistent*
children across every sample, so the event-stream adoption path already works
where it is used.

**MESH costs the compositor nothing at rest.** Hyprland's own CPU was
unchanged — marginally lower — with the bar running. The concern that
full-surface damage on a `layerrule = blur` surface forces continuous
compositor re-blur is not observable at rest, because at rest the bar does not
damage. It remains untested under animation.

**A transient, not a leak.** An early window showed the shell process at
98–99% of a core for roughly a minute after startup, which did not reproduce in
any later window on the same or a fresh process; steady state is consistently
0.1–0.4%. Startup work (module compilation, icon rasterization, backend spawn)
is the likely explanation. Recorded because it was observed, not as a finding —
it was not isolated.

**Still unmeasured.** Interactive frame timing. Driving the shell over IPC
(`shell:debug_benchmark:<scenario>`) exercises the scenarios but their results
surface only in the debug inspector, and pointer interaction cannot be
synthesized from here. Whether a specific surface drops frames under a specific
interaction needs a capture taken while a person interacts with it.

---

## 2026-08-08 — every service poll repaints the whole navigation bar

area: measurement only — service invalidation, damage selection

Follow-up to the frame profile below, chasing a perceived frame-rate problem
that the per-frame CPU numbers do not explain: 0.4–2.6ms of work per surface is
several hundred frames per second of headroom, so something forces work rather
than costing it.

**Measured.** `profiling` profile under `nix develop` (rustc/cargo 1.94.0), the
settled 1920x56 navigation bar, 240 sampled frames per run. A `mesh.audio`
volume change — the most ordinary update the bar receives — produces:

```
dirty = SCRIPT|STATE|STYLE|LAYOUT|PAINT|TEXT|ACCESSIBILITY|METRICS  (TREE_REBUILD)
damage = 1 rect, 1920x56 = 100.0% of the surface, 240/240 frames full-surface
```

Every poll is a full Luau tree rebuild, a full style walk, a full-surface
repaint, a full SHM copy, and full compositor damage. The narrow path exists
(`SCRIPT_NARROW`, `invalidate_service_template_nodes`) and never engages.

**Why.** `handle_service_event` takes the narrow path only when
`node_service_field_deps.nodes_reading_field(service, field)` is non-empty, and
that index is populated from `WidgetNode::service_field_reads` — i.e. only when
a **template** interpolates the service field directly. Every shipped navigation
component instead does the idiomatic thing: `local audio = require("mesh.audio")`
in Luau, deriving `icon_name` / `audio_tooltip_value` for the template to bind.
Those nodes record no service field read, `narrow_nodes` is empty, and the code
falls back to `invalidate_script_state()` → `TREE_REBUILD` →
`requires_tree_rebuild` → full-surface damage. The narrow service path is
effectively dead for the module-authoring pattern the project recommends.

**Poll rates** in the shipped graph: audio 1000ms (250ms while a stream is
active), network 1000ms, hyprland-wm 500ms, brightness 2000ms, power 10s, media
5s, theme 5s — roughly 4 full-surface bar repaints per second at rest, ~7 during
audio activity.

**Ruled out by measurement.**

- *Slots.* The customizable `<slot>` elements added in 56016c9e were the first
  suspect via `template_has_dynamic_structure`. Restoring the pre-slot HEAD~1
  template gave the identical result (`narrow_supported=false`, 100% damage,
  2.451–2.468ms audio frames vs 2.575–2.586ms), so slots are not the cause.
- *Backdrop blur.* `.nav-shell` carries `backdrop-filter: blur()` at the bar
  root, and `expand_damage_for_blur_regions` unions any intersecting damage with
  the whole blurred region — so it *would* collapse partial damage to the full
  surface. But damage is already 100% without it: flipping the `blur_enabled`
  prop default to false changed neither the damage (100.0%, 240/240) nor the
  frame cost (2.383–2.419ms vs 2.575–2.586ms). It is a latent amplifier, not the
  current cause.

**Not measured here.** These numbers are `component.paint()` into a
`PixelBuffer` only. The SHM copy, the Wayland commit, serial paint of every
surface in one loop iteration, and the compositor's own cost for a damaged
blurred layer surface are all outside them. On Hyprland the bar opts into
`layerrule = blur` via its `:blur` namespace, so each full-surface damage also
forces a compositor re-blur of the whole bar. Quantifying that needs a live
`./tools/profile-shell` capture against a running shell.

---

## 2026-08-08 — navigation-bar frame profile and paint-path hotspot survey

area: measurement only — frame cost attribution across the shipped surfaces

No behavior changed. This entry records a baseline for the always-on surface,
which had none, plus a sampling profile of where a steady-state frame actually
goes.

**New gate.** `navigation_frame_cost_profile` (release-only, ignored) drives the
real `@mesh/navigation-bar` module the way the session does: a poll of a service
it reads (`mesh.audio`), a poll of one it does not (`mesh.media`), a pointer
crossing the bar, and a plain repaint. `MESH_BENCH_FRAMES` scales the loops for
sampling. The existing `appearance_frame_cost_profile` gained per-rule and
per-command attribution output.

**Settle first.** A surface painted at an extent its content does not measure to
re-raises `STYLE|LAYOUT|PAINT|METRICS` and never reaches a fast path. The bar
measures 1920x56; the harness adopts that before measuring, and prints the
measured size and the resulting dirty flags so an unsettled run is visible
rather than silently reported as steady state.

**Measured.** `profiling` profile (release + debuginfo, rustc/cargo 1.94.0)
under `nix develop`, local development host, three runs of 4,000 frames per
loop against the 107-node settled bar at 1920x56:

| frame kind | ms/frame |
| --- | --- |
| `mesh.audio` poll (the bar reads it) | 2.575–2.586 |
| `mesh.media` poll (nothing reads it) | 1.601–1.606 |
| pointer move | 1.357–1.373 |
| paint only | 0.413–0.421 |
| render hooks alone | 0.309–0.314 |

Instrumented stage split of the audio-poll frame (60 frames; attribution
inflates absolute values, so read the shares): style_restyle 1.93ms, tree_build
1.22ms, paint 0.46ms (paint_traversal 0.43ms), render_object_sync 0.12ms,
layout 0.07ms, retained_display_list_update 0.010ms, text_shaping 0.000ms.

**Where the cycles go.** `perf record -F 997`, 26K samples, instrumented passes
excluded so per-rule attribution does not appear as a production cost. Self
time: the glibc allocator totals ~27% (`_int_malloc` 8.9, `_int_free_chunk`
4.9, `cfree` 3.4, `malloc_consolidate` 3.0, `__libc_malloc2` 3.0, `malloc` 2.1,
`unlink_chunk` 1.3), plus `memmove` 4.3% and `memcmp` 3.7% from string traffic.
Named work: `HashMap::clone` 4.6% inclusive, `format!` 4.0% inclusive,
`collect_focused_nodes` 2.1% inclusive, `style_fingerprint` 2.0%,
`attributes_fingerprint` 2.0%, taffy flexbox 3.1%, Luau GC 2.5%, SipHash 2.1%,
`AttributeMap::get` 1.6%, `ComputedStyle::clone` 1.3%. Skia's
`blit_row_color32` — the actual pixel fill — is 0.75%. **Rasterization is not
the bottleneck; per-frame allocation and string hashing in shell bookkeeping
is.**

**A/B: the per-paint focused proof snapshot.** `build_focused_proof_snapshot`
runs on every paint and is read only by tests. It allocates several `String`s
per node per frame (`node.id.to_string()` twice, `accesskit_node_id_for`'s
`format!`, cloned `role`/`aria-label`, and a
`"parley_text::{content}::…"` format per text node). Skipping it behind a
temporary env gate, three runs each on the 556-node Appearance surface:
paint-only 1.977–2.204ms → 1.603–1.698ms (**~19%**), resource-list scroll
2.613–2.715ms → 2.279–2.365ms (**~13%**), unrelated service frame
8.182–8.673ms → 7.898–8.258ms (~4%). On the 107-node bar the same gate is worth
~4–8%. The gate was reverted; nothing in the tree carries it.

**A live animation defeats targeted restyle.** While any transition or keyframe
animation is unfinished anywhere in a surface,
`invalidate_animation_style_path` raises `VISUAL_REPAINT`, which carries no
`STATE` bit — and `STATE` is exactly what selects the targeted
interaction-restyle branch. Every frame for the animation's duration therefore
restyles the whole tree rather than the animating nodes. Measured on the bar:
frames taken while a hover transition is live cost 0.87ms paint-only versus
0.413–0.421ms once nothing is animating, a **2.1x** difference on identical
content. `.bubble-option-core`'s `animation-iteration-count: infinite` was
tested as a cause and ruled out — making it finite changed nothing.

**Not reproduced as a problem.** Text shaping and icon rasterization are
0.000–0.006ms per frame; the glyph and raster caches are doing their job.
`retained_display_list_update` is 0.010ms. Neither needs work.

---

## 2026-08-08 — service fan-out stops invalidating components that cannot see it

area: service updates, component render memoization, render hooks

The Settings Appearance page stuttered under ordinary background service
traffic. Profiling a real `@mesh/settings` surface showed a frame triggered by
an unrelated `mesh.audio` poll costing 12.4ms against 2.1ms for a paint-only
frame, with component memoization hitting **zero** times: every one of the six
page instances was re-instantiated on every tick. Two independent causes:

1. **Read capability is declared per module**, so all seven runtimes of
   `@mesh/settings` received every payload the module may read. The
   `ScriptState` write marked each instance dirty and advanced the
   `mutation_generation` that the memo's subtree stamp is keyed on, so an audio
   poll invalidated Appearance, Wi-Fi, Bluetooth, Device and Advanced alike.
   A runtime that has completed a template evaluation now has authoritative
   read sets: if the update reaches it through neither the service proxy, the
   `<service>` state member, nor the `last_service_update` trace, the value is
   installed non-reactively (`set_unobserved_value_with_fingerprint`) — correct
   for a later first read, but not a mutation.
2. **Declaring `render()` disabled the selective service build outright.** The
   hooks now run in `paint` before the dirty flags are snapshotted, and the
   gate is what a hook actually wrote: a hook whose writes no template reads
   leaves the retained and selective paths available, using the same
   `dirty_state_affects_template` rule that event-handler dispatch already
   applies. Running hooks before the snapshot also folds their invalidation
   into the current frame instead of costing an extra one.

`changed_public_members` is now deduplicated on push: a frame that never
rebuilds no longer clears it, so blind pushes could grow it without bound.

**Measured.** Release under `nix develop` (rustc/cargo 1.94.0), three repeated
runs of the `appearance_frame_cost_profile` workload — a real `@mesh/settings`
surface at 920x900 showing Appearance over a 240-icon-theme plus
240-font-family catalog (556-node tree), 60 frames each of an unrelated
`mesh.audio` payload change plus paint. Before: 12.186–12.376ms per frame.
After: 8.181–8.593ms per frame, a **1.42–1.51x** improvement from the observed
ranges. Stage breakdown per frame moved from tree_build 6.9ms / restyle 2.9ms
to tree_build 3.4ms / restyle 2.8ms, and the render hooks themselves from
0.793–0.800ms to 0.584–0.633ms. Paint-only frames (2.06→1.94ms) and
resource-list scroll frames (2.65→2.65ms) were unchanged within noise. The
checked gate is behavioural, not timed:
`audio_polls_reuse_every_settings_page_appearance_does_not_read` pins five
memo reuses per audio poll and asserts Appearance content still paints.

**Not fixed.** The rebuild frame still restyles the memo-reused subtrees,
because memo entries are stored pre-restyle to stay position-independent.
That is 2.8ms of the remaining 8.6ms and is now a backlog item.

**Verified.** `mesh-core-shell --lib`: 659 passed, 6 failed, 126 ignored — the
six are the pre-existing shipped-navigation, inspector, backend-lifecycle and
debug-fixture failures present on `main` before this change (baseline: 656
passed, same 6 failed). `mesh-core-scripting` 177 passed, `mesh-core-elements`
213 passed, `mesh-core-frontend` 64 passed. `cargo fmt --all -- --check` and
`git diff --check` passed.

Two tests encoded the superseded rules and were replaced rather than deleted:
`service_template_with_render_hook_keeps_full_evaluation_fallback` split into
an inert-hook case (reuse expected) and a writing-hook case (fallback
expected), and `immediate_rerender_preserves_present_damage_from_first_pass`
now tolerates the second pass being unnecessary while still requiring present
damage and the hook-derived label in the same frame.

## 2026-08-08 — skip full style restyle on paint-only scroll frames

area: retained component rendering, Appearance scrolling

Paint-only invalidations now reuse the existing computed styles and retained
layout. Scroll ticks still update runtime scroll metrics and run the retained
display-list damage/paint path, while script, state, style, layout, and text
invalidations keep their existing restyle behavior. This removes a full style
resolver walk from every scroll frame in large settings trees.

**Measured.** Release under `nix develop` (rustc/cargo 1.94.0), three repeated
samples of the existing `paint_only_frame_speedup` workload: a 2,050-node
retained tree with 150 paint-only frames per sample, with optimized and legacy
versions measured in separate controlled batches. The legacy scoped path took
353.298–364.946ms; the paint-only style-reuse path took 236.227–245.235ms
(1.44–1.55x faster from the observed ranges). The gate passed all three
optimized samples. The Appearance regression also uses a 240-icon-theme plus
240-font-family catalog and verifies the bounded 48-row tree remains scrollable
and paints after a retained scroll update.

**Verified.** Focused Appearance integration test, bounded Appearance resource
list regression, release retained paint benchmark, `cargo fmt --all -- --check`,
and `git diff --check` passed.

## 2026-08-08 — sparse retained-layout dirty-node handoff

area: retained Taffy layout synchronization

Layout-dirty frames now receive owned copy-on-write snapshots for the nodes
whose layout, style, or text inputs changed. The retained layout engine applies
their Taffy styles and text contexts directly, avoiding a second clean-tree
walk to resolve IDs; structural and unkeyed reconciliation still use the full
safe path. The parity suite covers keyed updates and structural add/remove/
reorder fallbacks.

**Measured.** Release with the local linker fallback, three runs of 2,000
one-leaf width updates on a 1,365-node `4^0..5` retained tree: the ID-based
tree-walk path took 318.384–320.724ms, while owned dirty snapshots took
234.558–242.892ms (1.31–1.37x). The
`retained_layout_dirty_snapshots_beat_tree_walk` gate passed all three runs.
An earlier broad-subtree shape was slower because cloning a 341-node dirty
subtree outweighed the walk; the shipped path is intended for sparse dirty
sets, which is the retained-tree contract.

**Verified.** `cargo test -p mesh-core-elements retained_layout --lib` passed
(7 active tests), and the release gate passed.

## 2026-08-04 — bounded Appearance resource materialization

area: settings frontend, component tree size

The Appearance page now materializes installed icon themes and font families
in 24-row increments. Each list keeps the complete service catalog available
through a localized **Show more** control, while the initial Settings tree pays
style, layout, retained-diff, and paint costs only for the first visible-sized
batch. A regression publishes 240 icon themes and 240 font families and checks
that the initial tree contains exactly 48 resource option rows.

**Measured.** Release under `nix develop` (rustc/cargo 1.94.0), five samples of
24 Settings page switches across all six pages after publishing the 240+240
resource catalog. Eager materialization took 2.738–2.791s (median 2.764s) and
produced a 2,710-node tree; bounded materialization took 174.038–264.745ms
(median 177.284ms) and produced a 550-node tree. Median page-switch throughput
improved 15.59x and the retained tree was 4.93x smaller. The checked
`settings_appearance_resource_lists_are_bounded` gate pins the 24+24 initial
row bound.

**Verified.** The bounded-catalog regression and the shipped Settings
backend/Appearance integration test pass in release. The latter covers theme,
icon-theme, and font selection requests with the richer service catalog.

## 2026-08-02 — Frontend catalog compilation reuse

`working tree` · area: frontend catalog

Live frontend activation, deactivation, and profile switches now retain an
existing compiled frontend module when its normalized manifest and complete
watched `.mesh` source fingerprint match. The catalog continues to rebuild
slot-contribution and validation indexes against the new graph; narrowing those
indexes to graph deltas remains open in `docs/BACKLOG.md`.

**Not measured.** The checkout's configured Nix linker wrapper references a
missing store path, preventing `cargo check -p mesh-core-shell` from linking
dependencies. The change has a pointer-identity regression test; benchmark it
once the toolchain is repaired before making a speedup claim.

Archived record of the performance work tracked in `todo.md`, moved here on
2026-07-13 to keep the backlog readable. This file is **history, not backlog**:
the canonical open-item list lives in `todo.md`. Checkbox state, progress
narratives, measurements, and section letters (A–V) below are preserved
verbatim as of the archive date; open items listed here also have a current,
slimmed entry in `todo.md`.

Every entry keeps its benchmark numbers so future work can compare against the
recorded baselines. Sections M–V were focused per-subsystem deep dives; see
`PERFORMANCE_SECTIONS.md` for the subsystem map.

## 2026-08-02 — retained tree becomes the single render fingerprint pass

area: retained tree, render objects, display list (N)

`RetainedNodeSnapshot` now owns the detailed render fingerprint alongside its
layout, style, attribute, child, and state fingerprints. When the broad retained
diff marks a node, that same visit computes the paint-slot delta and publishes
the authoritative `RenderObjectDirtySummary` plus dirty `NodeId` set. The shell
no longer owns a parallel `RenderObjectTree`, its duplicate node map/child-id
storage, or its second full-tree traversal. The retained style and attribute
fingerprints were extended to cover background paint, shadows, filters, and
direct accessibility metadata so every render input reliably opens the detailed
paint diff. Existing display-list sparse entry reconciliation consumes the
retained result unchanged; structural updates retain its full fallback.

**Measured.** Release under `nix develop` (rustc/cargo 1.94.0), three
interleaved runs of 2,000 non-structural frames on a 1,365-node five-way tree,
alternating one deep text leaf between two values. The legacy control performs
the retained fingerprint walk followed by the independent render-object map
walk; the new path performs one retained walk and computes the detailed render
fingerprint only for broadly changed nodes. Separate passes took
0.727–1.272s; the single pass took 0.675–1.177s, a 1.077–1.085x improvement.
The checked `retained_single_fingerprint_speedup` gate requires 1.05x.

**Verified.** A parity regression compares the retained-owned dirty summary and
node set with the former render-object implementation across isolated and
simultaneous geometry, material, text, and accessibility changes. The 47-test
invalidation suite, 24 active retained-tree tests, and 13 active render-object
tests pass.
Per-node scoping of the retained tree's own traversal and wider generation
shortcuts remain in the next backlog item.

## 2026-08-01 — affected static template re-evaluation

area: template evaluation, retained render pipeline

Direct service-field updates now carry the exact `NodeId` set from
`NodeServiceFieldDependencies` into `narrow_script_update`. The compiler walks
only the affected nodes and their ancestors, reusing clean native subtrees via
the widget tree's copy-on-write payloads. Component references still execute
their normal resolver so occurrence identity and memo side effects remain
authoritative. The static-shape eligibility check is cached at component
construction and source reload rather than scanning the AST on each update.

Dynamic `{#if}` / `{#for}` / slot templates and surfaces with `render()` hooks
stay on full evaluation because their structural and indirect state
dependencies are not represented by the node service-field index. The service
diff now also uses the normalized service name (`audio`, not `mesh.audio`),
fixing the prior mismatch between event fields and dependency keys.

**Measured.** Release under `nix develop` (rustc/cargo 1.94.0), three repeated
runs of 100 updates on a statically shaped 1,026-node surface with one direct
`audio.percent` expression and 1,024 clean text siblings. Each run alternated
full and selective updates. Full template evaluation plus the complete
layout/render/present pipeline took 5.090–6.205s; affected evaluation took
5.001–6.070s, a 1.018–1.030x end-to-end improvement. The checked
`affected_template_eval_speedup` gate requires 1.01x; its structural regression
also proves at least 1,024 clean nodes reused their authored payloads.

**Verified.** The focused narrow-script suite passes 11 tests with 3 release
benchmarks ignored. Dedicated regressions cover direct-field output and reuse,
dynamic-structure fallback, render-hook fallback, retained scoped diffing, and
pixel equivalence in the benchmark control. The frontend compiler suite reports
62 passed, 1 pre-existing shipped-navigation fixture failure, and 20 ignored.

## 2026-08-01 — retained-update inline scratch revalidation rejected

area: retained render tree (N)

A stale backlog item caused the 2026-07-28 `SmallVec<[&WidgetNode; 8]>`
candidate to be revalidated after it had already been rejected. The inline
buffer removed one candidate-list allocation per sparse update, but did not
produce a stable end-to-end frame win. The candidate was reverted again to the
fresh `Vec`; early returns remain local and cannot retain borrowed tree nodes.
This entry reaffirms the existing 2026-07-28 rejected-experiments row rather
than superseding it.

**Measured.** Release with allocation profiling under `nix develop`
(rustc/cargo 1.94.0), three repeated runs of 100,000 one-leaf-dirty updates on
a 13-node tree, using ten alternating 10,000-frame batches per run. The fresh
`Vec` took 208.536–214.576ms and made 200,000 allocations; inline `SmallVec`
took 199.644–206.299ms and made 100,000 allocations, a noisy 1.011–1.075x.
Two of three runs missed the proposed 1.05x gate, consistent with the earlier
40-node result of 1.005x, so the allocation reduction is not a reliable frame
improvement.

## 2026-07-31 — style diagnostics reuse the retained generation

area: style diagnostics, retained invalidation

Diagnostic-enabled rebuilds no longer traverse the complete widget tree to
hash tag, class, id, module, state, and child inputs after the normal restyle.
The diagnostic pass now runs after the authoritative retained-tree diff and
keys its reuse decision on that generation together with the style-rule
generation, resolved props, and container dimensions. Module identity joined
the retained attribute fingerprint because it affects module-scoped theme
defaults and was the one former diagnostic input not already represented by
the retained snapshots. Source/theme cache resets still clear the diagnostic
gate, and the first rebuilt tree still records diagnostics.

**Measured.** Release under `nix develop` (rustc/cargo 1.95.0), five repeated
runs of 2,000 unchanged diagnostic-enabled rebuild gates over a 156-node tree.
Both sides include the existing retained-tree diff. The legacy path plus the
extra full-tree fingerprint took 336.331–339.755ms; the retained-generation
path took 271.427–275.034ms, a 1.235–1.250x improvement. The checked
`runtime_style_diagnostic_generation_speedup` gate uses a 1.20x baseline with
5% tolerance.

**Verified.** Focused tests prove retained generations change for tag, class,
id, state, module, and structural inputs; the independent fingerprint fields
cover rules, props, and both container dimensions. The module-identity
regression directly checks the retained attribute fingerprint. The full shell
suite reports 590 passed, 19 failed, and 123 ignored; the failures are
outside the new gate in real-surface fixtures, interaction/reflow expectations,
profiling proofs, and backend debug state; all new diagnostic-generation tests
pass.

## 2026-07-31 — style caches use bounded eviction

area: style resolution, cache tail latency

The inline-style parse cache, shared theme-default revision and entry caches,
and lowered theme-declaration cache now evict one least-recently-used entry at
their existing bounds. Capacity churn no longer clears unrelated hot styles.
The shared O(1) LRU gained mutable lookup so nested revision caches update
recency without removing and reinserting values. Lowered declarations retain
their source defaults and validate them on lookup, preventing allocator address
reuse from turning a retained pointer-key entry into a stale hit.

**Measured.** Release under `nix develop` (rustc/cargo 1.95.0), five runs of a
300-frame inline-style churn workload with 96 hot and 12 rotating declarations
per frame at capacity 128. Flush-all p95 was 335,936–341,872ns; individual LRU
p95 was 46,295–49,856ns, a conservative 6.74x improvement. The checked
`bounded_style_cache_p95_speedup` gate uses a 5.00x baseline with 30% tolerance
(minimum 3.50x).

**Verified.** The capacity regression covers inline entries, shared-theme
revision and per-revision entries, and lowered declarations, proving that a
refreshed hot entry survives while the oldest cold entry leaves. The elements
suite reports 207 passed, 2 failed, and 56 ignored; both failures reproduce in
the current baseline (`indexed_theme_defaults_reuse_lowered_declarations_per_revision`
expects the reverse of declaration insertion order, and the shipped navigation
hover fixture expects a transform no longer present in its source).

## 2026-07-31 — share compiled frontend modules across surface instances

area: frontend catalog, shell component lifecycle

Catalog entries and surface components now retain a copy-on-write `Arc` handle
to their immutable `CompiledFrontendModule`. Multiple profile roots no longer
clone compiled templates, scripts, or style rules; a source reload compiles one
replacement and publishes it in the next catalog generation, leaving prior
snapshots valid until their live surfaces adopt the change. Test fixtures keep
their concise manifest setup through copy-on-write mutation.

**Verified.** `cargo check -p mesh-core-shell` and `cargo test -p
mesh-core-shell --lib --no-run`; the catalog regression asserts that a cloned
top-level surface entry and its catalog entry dereference to the same compiled
module. The release-only `shared_compiled_handle_clone_beats_deep_clone` gate
pins the ownership improvement against the former deep-clone operation.

## 2026-07-31 — graph diagnostics parse each `.mesh` source once

area: module graph diagnostics

The frontend authoring scan now parses each source once, then shares that
component AST across static icon, translation/event, and keybind extraction.
Standalone extractors retain their parse-own-source API for targeted tests.

**Measured.** Debug `shipped_module_luau_scan_cost`, 38 shipped `.mesh` files
(7,876 lines), one sample: legacy triple parse 1.812s versus shared parallel
parse 519.7ms (3.49x). The gate verifies equal extracted facts (139).

## 2026-07-31 — retained `NodeId` collisions fail safely in release

area: retained render tree

The retained tree's identity index is keyed by a hash-chained `NodeId`. Two
live nodes with the same value were guarded only by `debug_assert_ne!`, so a
release build could overwrite one snapshot with the other. The traversal epoch
check is now an unconditional assertion, making the impossible identity
violation fail with its node id/key diagnostic in every build instead of
silently aliasing cache entries. The test-only snapshot collector uses the
same unconditional check.

**Verified.** The forced root/child collision regression passes in both debug
and release: `cargo test -p mesh-core-shell --lib
retained_update_rejects_duplicate_live_node_ids_in_release_too` and `cargo
test -p mesh-core-shell --release --lib
retained_update_rejects_duplicate_live_node_ids_in_release_too`.

## Rejected experiments — do not retry without new evidence

Each of these was prototyped, measured, and reverted. Full context lives in the
section noted.

| Date | Experiment | Section | Result |
| --- | --- | --- | --- |
| 2026-08-01 | Revalidate `SmallVec<[&WidgetNode; 8]>` for scoped retained-update candidates | N | Halved measured allocations on a 13-node sparse frame, but only 1.011–1.075x over three runs and missed the 1.05x gate twice; reverted again |
| 2026-07-03 | Component-local copied tracked-field summary for the service event gate | C | Only 1.1x (2.544ms → 2.221ms/100k) and refresh had to clone tracked maps; shell-side subscription index is the viable design |
| 2026-07-03 | Fractional-scale physical-damage rect fix | D | Byte-for-byte correct but no CPU-side win (28.066ms vs 28.367ms one-box); revisit with compositor/upload damage instrumentation |
| 2026-07-04 | Fuse surface + child enter/exit class annotation into one full-tree traversal | D | 0.6x — 87.249ms fused vs 52.903ms targeted walks; fusion must not scan unrelated branches |
| 2026-07-04 | Retain/clear scratch `node_map`/`text_nodes` allocations in layout | F/T | 0.99x (77.502ms vs 77.033ms); full style sync dominates |
| 2026-07-04 | Render-object child IDs `Vec` → `SmallVec` | D | Slower (11.285ms Vec vs 21.992ms SmallVec) |
| 2026-07-04 | Private Lua-table cache for scalar `self.storage` reads | I | 0.8x (50.238ms vs 39.202ms); extra Lua lookup outweighed saved Rust work |
| 2026-07-05 | `Arc<str>` for display payload strings | I | 6.6% regression on two-string payload clone (67.591ms vs 63.400ms/5M); atomics on every command clone |
| 2026-07-05 | One full-tree inherited-state walk replacing per-key closing-popover searches | J | Slower for the realistic small-key case (1.035s vs 855.098ms/2k) |
| 2026-07-05 | `SmallVec` for `RenderObjectTree` child ids | N | Regressed the six-child case 28.448ms → 37.879ms; retained-`Vec` clear/refill won instead (4.2x) |
| 2026-07-10 | `Vec::drain` instead of the `VecDeque::from` adapter for emitted requests | V | 0.9x (82.505ms vs 74.102ms/1M four-request batches) |
| 2026-07-12 | Scratch `PixelBuffer` reuse for rotation transforms | P | 341.011ms scratch vs 2.586ms fresh alloc+clear over 2M 96x64 buffers; fresh zeroed allocation is much cheaper |
| 2026-07-12 | Hand-rolled inline clip/layer stacks in the Skia backend | P | 350.183ms inline vs 31.812ms `Vec` over 8M batches; later solved with `SmallVec` (2026-07-13) |
| 2026-07-12 | `tree.style(id) == new_style` equality guard before Taffy `set_style` | T | Slower than unconditional `set_style` (51.642ms vs 46.115ms); needed the retained dirty-bit feed, which landed 2026-07-12 |
| 2026-07-15 | Rust recursive detached-Lua-table cache for nested storage reads | I | Regressed 1.221s current to 1.815s cached over 100k reads (0.67x); deep-copy construction outweighed saved conversion/locking |
| 2026-07-15 | Luau `table.clone` plus recursive array replacement for nested storage reads | I | Regressed 1.237s current to 1.611s cached over 100k reads (0.77x); exact detached-value semantics still require too much table reconstruction |
| 2026-07-28 | `SmallVec<[&WidgetNode; 8]>` for scoped retained-update candidates | N | Removed one allocation per sparse update, but the complete 40-node path improved only 1.005x (63.918ms → 63.631ms/50k); prototype reverted |
| 2026-07-30 | Downsample → blur → upsample image-filter chain for `filter: blur()` layers | P | Slower at every radius (512x512 layer, 200 frames: r8 254ms full-res vs 562ms quarter-res; r32 287ms vs 479ms; r64 310ms vs 488ms). Skia's raster blur already resamples internally for wide kernels, so an explicit chain only adds two transforms and an intermediate. The `downscale` setting was removed rather than shipped as a slower default |
| 2026-09-30 | Borrowed keys + multiply-rotate hasher in `validate_widget_tree` | — | Audio poll 3.03–3.07ms vs 3.03–3.10ms (300 events × 3); the per-build validation walk is not hash-bound enough to show. Reverted |
| 2026-09-30 | Full-restyle `StyleShareCache` reusing cloned `ComputedStyle`s by node inputs | E | 99.3% hit rate but no frame win: 15.51–15.91ms before vs 15.13–16.07ms after on `appearance_frame_cost_profile`; key hash/compare plus a full style clone costs as much as resolution. Archived on local branch `archive/style-share-cache` |
| 2026-10-18 | Separate `Value::hash`/`RandomState` pass for validated backend state | runtime boundary | Unchanged 128-row, 12,626-byte snapshots, release, 2,000 iterations: 177.458–314.765 ms baseline vs 226.492–274.504 ms candidate over two attempted runs. Missed the 1.05x gate (1.15x then 0.78x); replaced by a fingerprint fused into validation's serialization pass |
| 2026-10-18 | Early return for the flat interior of the liquid-glass SkSL lens | rendering | Release, three interleaved runs of ten rounded 180×40 pills: 9.269–9.338 ms baseline vs 10.524–10.660 ms candidate (0.87–0.89x). One 360×240 card probe reached only 1.08x, missing the 1.2x gate. Reverted; unrefracted wallpaper image-shader fast path retained |
| 2026-10-18 | Clip-invariant rounded shapes: one tinted A8 image per corner, full runs in the body path, region under a canvas translate, device-space cached regions, region boundary path | rendering | Release microbenchmark, 300 translucent 80×26 radius-8 fills: 2.5 / — / 1.23 / 1.37 / 1.39 ms vs 1.23 ms for the shipped region + single `draw_atlas` (direct `draw_rrect` 0.79–0.84 ms). Body-path runs cost about 10% end to end on appearance paint. Per-corner images also left a 1/255 seam against solid fills |
| 2026-10-18 | FNV-1a fingerprint fused into bounded JSON serialization | runtime boundary | Three controlled first-sample attempts on unchanged 128-row / 12,626-byte state, release, 2,000 iterations: 178.085–191.996 ms baseline vs 202.623–204.492 ms fused (0.87–0.95x); each failed the 1.05x gate. Reverted; fast keyed `ahash` over the typed value is the measured alternative |
| 2026-10-18 | `Arc<[TransitionStyle]>` with a shared default instead of `Vec` in `ComputedStyle` | style | Appearance service frame restyle 2.673–2.723 ms vs 2.672–2.705 ms without it (three runs each, release): no measurable change. The one-entry default `Vec` clone is not a restyle cost driver. Reverted |
| 2026-10-18 | Finalized memo styles while retaining compiler preliminary layout | component memo | Release, 550-node Appearance, three interleaved unpinned 60-update samples: 10.665–11.395 ms/frame baseline vs 25.345–26.575 ms/frame prototype; intrinsic shaping/font fallback returned in compiler layout. Final implementation defers that pass |
| 2026-10-20 | Pin raster replay uploads by retaining `TextureFromImage` aliases | GPU residency | Rejected by backend-texture identity test before timing: the API creates uncached uploads; aliases do not pin the picture playback cache. Use immutable raster/page identities and Ganesh cache headroom |

## 2026-07-30 — element `filter: blur()` subtree layers

Workload: release `cargo test -p mesh-core-render --release -- blur_layer_pass_cost
--ignored`; a 512x512 surface whose 420x420 blurred subtree holds 16 children,
200 repaints of the full retained command list per sample.

| Configuration | 200 frames | Per frame |
| --- | --- | --- |
| Same tree, no `filter` | 9.049ms | 0.045ms |
| radius 8, 1 pass (shipped default) | 253.841ms | 1.269ms |
| radius 32, 1 pass | 335.039ms | 1.675ms |
| radius 64, 1 pass | 327.557ms | 1.638ms |
| radius 8, 2 passes | 704.624ms | 3.523ms |
| radius 32, 2 passes | 963.720ms | 4.819ms |
| radius 64, 2 passes | 1.142s | 5.711ms |

Two things this pins down. **Radius is nearly free, layers are not**: going
from an 8px to a 64px kernel costs 1.3x, while adding the layer at all costs
28x over the unfiltered tree — the offscreen allocation and composite dominate,
not the convolution. That is why the cap that matters is on nesting depth
(four) and why damage is expanded to the whole layer rather than subdivided.
**Passes cost what they say**: a second pass is 2.8-3.5x, so the shipped
default is one and `shell.render.blur.passes` is an opt-in for smoother
falloff, not a default worth carrying.

The gate `blur_layer_pass_cost` asserts the shipped default stays the cheapest
configuration at each radius.

## 2026-07-16 — completed performance checkpoints archived from `todo.md`

The active backlog was cleaned after the following checkpoints passed their
correctness suites and release performance gates. This section preserves the
completion evidence that was removed from `todo.md`.

### L — measurement, live diagnostics, and regression gates

- The canonical profiling catalog now includes idle, pointer move, text
  update, scroll, icon grid, animation, theme reload, and resize workloads.
- The native per-surface HUD paints the last 16 frame samples, redraw and dirty
  counters, retained-entry rebuilds, damage area, full-damage warnings, and
  HiDPI-aware paint flashing without perturbing text/glyph caches. Its release
  gate measured 201.31us per observed 640x80 frame, including save/restore.
- Profiling snapshots expose a deterministic cumulative top-10 attribution
  list for component-instance builds, matched style rules, and primitive,
  text, icon, control, and scrollbar paint classes. The profiling-off paint
  path retains its original clock-free loop; style resolution performs no
  clock reads and only one profiling option check per node.
- `wasted_work_avoided` counts memoized component builds and empty interaction
  restyles. Existing retained-paint metrics report skipped pixels and filtered
  commands.
- `mesh.debug.profiling_stream` remains bounded and order-stable, exports
  Chrome-trace/Perfetto complete events, and deduplicates surface samples from
  shell roll-ups.
- CI now runs `tools/check-performance` in Nix against
  `config/performance-baseline.tsv`. The completion run passed all gates:
  template dependency lookup 20.263x, unbound handler dispatch 105.844x,
  sparse versus full handler repaint 2.180x, handler expression reuse 1.371x,
  and native HUD 201.31us/frame.

### B/E/I — narrow invalidation, restyle, and component reuse

- Handler-driven public-member writes retain the members read by evaluated
  full-Luau template expressions. Unobserved writes update script state without
  rebuilding or painting; visible writes enter the retained narrow-script path
  and preserve sparse display-list damage. Structural changes still promote to
  the authoritative full-tree fallback. The final 200-update release gate
  measured zero paints for the unbound member and a 105.844x speedup over the
  rendered-member path.
- Template dependency probes moved from a Lua aggregate table to the Rust
  expression cache. One million probes measured 254.8ms through Lua versus
  12.57ms through Rust (20.263x).
- Interaction restyles use selector dependency bits for every supported pseudo
  state, root-to-leaf hover-path tail diffs, and direct retained node IDs.
  Background-only targeted changes avoid descendant rule resolution unless an
  inherited text field changes. A 2,049-node benchmark measured 550.6ms full
  descendant resolution versus 24.9ms targeted (22.1x); direct deep-node scope
  conversion measured 94.99us versus 9.93ms walking (104.6x).
- Component memoization now assigns source-stable occurrence identities to
  repeated local/import aliases and positional identities to loop instances.
  Each occurrence owns an independent runtime, handler namespace, generation,
  and memo entry. Four 200-frame runs over 12 repeated instances measured
  1.63-1.66x end-to-end with all 2,400 expected hits per run.

### D/N/P/T/U — retained render, layout, popup, and damage work

- Runtime-state annotation and post-order overflow processing were fused into
  one finalization traversal. A 1,365-node, 2,000-pass release benchmark
  measured 271.5ms separate versus 219.2ms fused (1.2x).
- Structural layout reconciles keyed nodes in the per-surface `TaffyTree`,
  preserving IDs and cached layout across insertion, removal, and reorder. A
  1,365-node, 200-reorder benchmark measured 148.3ms rebuilding versus 67.5ms
  retained (2.2x), with identical geometry.
- Promoted child surfaces own origin-aware retained display lists, consume
  local damage for clipped clears/replay, expand backdrop-filter damage, and
  forward the same rectangles to presentation. A 61-node popup measured
  38.0ms immediate versus 30.9ms retained (1.23x); sparse one-descendant paint
  measured 17.5ms versus 4.83ms (3.61x). Independent subtree generations also
  reduced 10,000 unrelated parent-frame cache checks from 4.290ms and 10,000
  clears to 3.562us and one clear.
- Fractional-scale logical damage now maps to physical edges with floor/ceil
  rules shared by painting and Wayland presentation. At 1.5x on 1920x1080,
  100 release uploads measured 70.4ms full-buffer copying versus 9.5us sparse
  copying (7,413x), with the physical 37x31 damage rectangle verified at both
  boundaries.

### B/C/S/U/V — dispatch, contracts, input identity, and startup

- Local, imported, and slot instance paths are interned as shared `Arc<str>`
  values and reused by runtime lookup, bindings, portals, memo generations, and
  handler dispatch. Scheduled handlers precompute their full target; 5.12M
  due-handler preparations measured 351.4ms formatted versus 207.8ms cached
  (1.7x), while borrowed root dispatch measured about 1.3x faster.
- Shell service delivery uses a dirty-refresh subscription index rather than
  acquiring every component mutex. The 20,000-event/256-component workload
  measured 202.8ms scanning versus 20.3ms indexed (about 10x); a later fresh
  validation measured 204.4ms versus 15.3ms (13.4x).
- Contract validation caches compiled field types and event-name maps by
  resolved contract identity. Validating 100,000 payloads against a 64-event
  contract measured 23.5ms scanning versus 10.7ms cached (2.2x).
- Presentation and shell input routing share `Arc<str>` surface IDs. Cloning
  two million representative IDs measured 10.74ms owned versus 6.57ms shared
  (1.6x); moving the shared ID through 500,000 split/route events measured
  8.89ms versus 10.18ms allocating (1.15x).
- Installed-graph and legacy discovery parse ordered manifests in parallel.
  The exact 17 shipped manifests measured 2.60-2.86x faster for the manifest
  stage with identical IDs, paths, sources, and diagnostics. The complete
  installed-graph startup improved only 1.01-1.02x because validation and graph
  assembly dominate, which is the recorded whole-system result.

### M — composition correctness and allocation cleanup

- Template `and`/`or` now follows Lua: operators short-circuit, return operand
  values, only `false` and `nil` are falsy, and `and` binds tighter than `or`.
- Runtime prop publication borrows unchanged host JSON, local component style
  cascades and selector indexes are cached, validation/CSS projection borrow
  source values, and matched-call/slot buffers reserve known capacities.
  Representative subpath measurements were 3.6x for the unchanged prop gate,
  1.7x for borrowed nested-value conversion, and complete elimination of
  repeated 64-rule cascade preparation.
- Duplicate service-field reads check the observed set by borrowed `&str`
  before allocating. One million duplicate reads measured 31.5ms
  allocate-before-insert versus 16.5ms borrowed (1.9x).

### R — one Luau VM per execution thread

- The four-slot VM pool and per-surface VM construction were replaced by one
  lazily initialized sandboxed Luau realm per execution thread. Frontend
  surfaces, embedded component instances, standalone contexts, and backend
  contexts clone a cheap handle to that realm; each `ScriptContext` retains a
  distinct `_ENV`, so globals, host channels, interface subscriptions, and
  live bindings preserve their existing isolation semantics.
- Context teardown explicitly clears the `_ENV` and its metatable before
  releasing handles. A two-wave, 64-context regression test forces full Lua
  collection and verifies that dead environment graphs do not accumulate in
  the long-lived thread realm.
- The release benchmark includes the first real VM initialization for a fresh
  thread, not only steady-state clones. Creating 256 fresh sandboxed realms
  measured 14.45ms versus 454.7us for one thread realm plus 255 clones (31.8x).
  Thirty-two fresh realms used 10,903,040 bytes of Lua heap versus 340,720 bytes
  for the shared realm (32.0x less aggregate heap). The checked CI baseline now
  includes `thread_vm_checkout_speedup`.

## 2026-07-14 follow-up — five measured hot-path changes

- Retained narrow/layout analysis caps its initial result-set capacity at 256;
  a 4,096-node release microbenchmark measured 2.327s growing versus 2.238s
  reserved (1.04x), avoiding unbounded sparse-frame over-allocation.
- Surface shortcut resolution retains the preformatted accessibility index;
  1,000 release probes measured 3.297ms rebuilding versus 2.4µs borrowing
  the cached map.
- Element-metrics publication moves the refs JSON value into script state after
  borrowing it for the live proxy, eliminating a full snapshot clone; 20,000
  256-entry release snapshots measured 1.601s clone versus 996.7ms move (1.6x).
- Scroll overflow annotation reserves the reusable root key-path buffer;
  20,000 release passes measured 796.1ms unreserved versus 769.5ms reserved
  (1.03x).
- Service-field reverse dependencies use a nested borrowed lookup instead of
  allocating a `(String, String)` key per query; 1M release lookups measured
  33.7ms tuple allocation versus 27.7ms nested lookup (1.2x).

## 2026-07-14 follow-up — retained/render scratch reuse

- Clean `RenderObjectTree` updates now skip the full stale-entry scan when the
  visited count and retained map length prove that no structure changed; a
  4,096-entry release benchmark measured 65.3µs retain scanning versus 10.7µs
  conditional skip (6.1x).
- Retained display-list dirty-ancestor collection reuses its path vector and
  ancestor set; 50,000 sparse release walks measured 6.39ms fresh versus
  4.38ms reused (1.46x).
- Animation passes reuse the live-key sets and previous-style snapshot map;
  release benchmarks measured 2.35x and 1.68x versus fresh allocations.
- Element-metrics publication reuses the ref-name → node-key map backing
  storage between paints; 20,000 512-entry release maps measured 1.368s fresh
  versus 719ms reused (1.90x).
- The shell service-delivery index now starts dirty so its first event lazily
  builds from the registered component set instead of being dropped, then the
  built index is marked clean. A fresh release validation measured 204.4ms
  full scan versus 15.3ms indexed across the existing 20,000-event/256-component
  workload (~13.4x) while the accepted-delivery regression test passes.
- Unchanged embedded-component prop publication now borrows host-owned JSON
  values, cloning only when a prop changes. A 100,000-write release benchmark
  measured 15.5ms for the owned unchanged gate versus 4.3ms for the borrowed
  gate (~3.6x); eager Lua writes measured 130.3ms.
- Local component style cascades now cache the merged host/component rule
  vector and its selector index per host-module/alias pair. Over 20,000
  iterations of a 64-rule cascade, rebuilding took 706.0ms while borrowing the
  prepared entry took 10.7µs (~66,069x for the eliminated preparation work).
- Runtime validation and CSS projection convert JSON props by reference rather
  than deep-cloning values that the caller retains. A 20,000-iteration nested
  value benchmark measured 418.9ms owned versus 247.1ms borrowed (~1.7x); CSS
  projection maps also reserve their declared prop count up front.

## 2026-07-15 — canonical profiling workloads

- The typed debug benchmark contract now exposes idle, pointer move, text
  update, scroll, icon grid, animation, theme reload, and resize profiles with
  stable shipped targets and stage priorities. Inspector fallback rows and IPC
  scenario parsing use the same IDs; existing interaction scenarios remain for
  compatibility. Profile guidance requires a fresh profiling session per
  workload so accumulated summaries remain comparable.

## 2026-07-15 — cached runtime diagnostic class tokens

- Runtime style diagnostics now resolve live `WidgetNode`s directly through
  their restyle-populated class-token cache instead of splitting the `class`
  attribute into a fresh `Vec<String>` for every rebuilt node. The indexed
  allocating and cached paths have parity coverage. A release benchmark over
  200,000 diagnostic resolutions measured 79.0ms for per-node splitting versus
  55.0ms for cached node tokens (1.44x faster for the diagnostic-resolution
  subpath).

## 2026-07-15 — parallel diagnostic, handler, and popup invalidation wave

- Unchanged rebuilds now fingerprint runtime style-diagnostic inputs (rule
  generation, selector-facing tree, surface props, and container dimensions)
  and skip the second full style-resolution traversal when they match. A
  156-node release benchmark over 2,000 unchanged rebuilds measured 172.7ms
  full re-resolution versus 19.8ms for the fingerprint gate (8.7x; repeat runs
  ranged from 8.7x to 9.2x).
- Ordinary handler graph namespacing now constructs the embedded-instance
  prefix once per traversal and appends raw handler names with exact capacity.
  A 1,000-node release benchmark over 200 traversals measured 79.6ms formatting
  each complete handler versus 31.8ms with the shared prefix (2.50x; repeat
  2.53x).
- Retained paint subtrees now expose independent generations, allowing promoted
  child surfaces to remain cached across unrelated parent/sibling updates. A
  160x90 release lower-bound benchmark across 10,000 parent frames measured
  4.290ms and 10,000 clears using the broad generation versus 3.562us and one
  clear using the popup subtree generation (~1,204x for the gate plus avoided
  clear). Popup-descendant changes still advance the child generation.

## 2026-07-15 — retained popup replay and root dispatch borrowing

- Promoted child targets now own bounded, origin-aware retained display lists
  and replay their command stream instead of using the immediate tree painter.
  Pixel parity is covered. A 61-node popup across 400 root-opacity transition
  frames measured 38.0ms immediate versus 30.9ms retained (1.23x); a sparse
  one-descendant material workload measured 17.5ms versus 4.83ms (3.61x).
- Plain root handler dispatch now borrows the surface instance ID for runtime
  lookup and allocates it only when published events or live-binding neighbors
  need post-dispatch processing. Two 2M-lookup release runs measured
  41.4ms/41.0ms owned versus 30.9ms/31.4ms borrowed (1.34x/1.30x).

---

## Non-performance completed items

### Codebase cleanup — markup `{...}` as full Luau (completed 2026-07-05)

- [x] **Evaluate markup `{...}` as full Luau in the component instance
      environment.** The authoring contract is that braces contain a
      value-producing Luau expression in the same lexical/runtime scope as the
      component `<script lang="luau">`, including private locals, functions,
      standard operators, and `{#for}` locals. Today
      `frontend/compiler/src/expr.rs` is a hand-written subset evaluator and the
      compiler receives only a `VariableStore` JSON snapshot, so valid Luau is
      rejected or evaluated with divergent truthiness/`and`/`or` semantics.
      Compile/register expression closures inside the component script chunk
      itself (a second chunk sharing `_ENV` cannot see lexical locals), thread
      closure invocation through the tree-build/composition boundary by
      component instance key, pass `{#for}` locals as invocation bindings
      without mutating persistent globals, and isolate the legacy evaluator to
      VM-less preview/tooling paths.
      Completed 2026-07-05: runtime creation collects each component's template
      expressions and compiles them into closures appended to that component's
      Luau chunk, preserving lexical access to private locals/functions. Tree
      rendering invokes closures through the owning instance runtime; loop
      locals are supplied through a temporary locals-first function
      environment. Text, attributes, conditionals, loop iterables, component
      props, and handler-call arguments use typed VM results. The legacy
      evaluator remains only as a VM-less preview/test fallback. The markup
      preprocessor is now quote-aware inside brace expressions, including
      nested double-quoted Luau strings.

---

## Performance sections — archived detail (as of 2026-07-13)

## Performance — remaining open items

Items owned by a milestone are listed with their milestone reference.

### P0 — scheduling and invalidation (→ v1.18 / v1.19)

- [ ] Narrow script/service invalidation below tree-rebuild + pixel repaint; add typed state dependencies → v1.18
- [ ] Avoid full-tree restyle for safe interaction changes; use selector-dependency analysis → v1.18

### P0 — scripting (→ v1.17)

- [x] One `mlua::Lua` VM per ScriptContext; moved to a per-thread VM with `_ENV`
      isolation and explicit environment teardown (completed 2026-07-16; see R).
### P1 — renderer hot paths

- [ ] Interaction frames still re-apply string style declarations per node (`apply_declaration_no_diagnostics` + theme defaults maps dominate the post-2026-06-10 toggle profile); folds into the typed/compiled declaration work → v1.23 and narrower invalidation → v1.18. Progress 2026-07-05: `StyleRuleIndex` now precomputes no-diagnostics declaration metadata for cached restyle paths, so interaction/restyle frames reuse support/profile/deprecated-token classification instead of repeating it per matched declaration. Added parity coverage for cached vs uncached declaration application and a release-only microbenchmark showing indexed declaration application at ~1.4x faster (156.3ms → 109.1ms for 200k iterations on the local profile run).
- [ ] Avoid flattening retained display-list subtrees into a new flat command buffer on each update; move toward segment/rope-style command storage → v1.21. Progress 2026-07-05: `RetainedDisplayList::update_inner` now detects unchanged display entries for the same root/surface before rebuilding paint subtrees, preserving the existing flat command arrays on no-op updates. Added regression coverage for zero subtree/command rebuilds on unchanged trees and a release-only benchmark showing unchanged update at ~2.6x faster than fresh flat rebuild (349.0ms → 922.3ms for 1k iterations on the local profile run).
- [ ] Replace per-node string/hash-heavy style matching with interned/typed node keys; remaining after first pass: interned tags, classes, attribute keys → v1.23
- [ ] Retain Taffy node state across layout passes; `build_taffy_tree` rebuilds a fresh TaffyTree every layout → v1.21
- [ ] Affected-subtree template re-evaluation: `narrow_script_update` rebuilds the full tree (full template eval) then diffs; use `NodeServiceFieldDependencies` to re-evaluate only nodes whose tracked fields changed → v1.27. Progress 2026-07-14: narrow and layout analysis now walk the retained slotmap directly instead of building a temporary fresh snapshot map; a same-run release benchmark measured 396.1ms map-based versus 317.1ms direct over 2,000 passes (1.25x). The larger affected-subtree re-evaluation remains open.
- [ ] Generation-aware retained-tree diff: `RetainedWidgetTree::update` FNV-hashes every node's style + attribute strings per paint; skip clean subtrees using dirty bits → v1.27
- [ ] Fuse the five per-frame `finalize_tree` annotation walks into one traversal; move hot annotations from string attributes to typed `WidgetNode` fields → v1.27

### P1 — backend modules

- [ ] Investigate `pw-dump --monitor` as a real volume event source for the pipewire-audio backend — `pw-mon` emits no `changed:` block for volume changes (verified with and without `--hide-params`), so the stream currently only signals client/object lifecycle, and volume detection leans on the safety poll
### P1 — presentation and memory (→ v1.20)

- [ ] Add performance profiles for canonical shell workloads (idle, pointer move, text update, scroll, icon grid, animation, theme reload, resize) → v1.21
### P2 — architecture

- [ ] Introduce interned `Symbol` / `TagId` types before further string-key cleanups → v1.23
- [ ] Add allocator-level profile mode (allocation counts per render pass) → v1.23
- [ ] Consider typed runtime node representation for hot paths (`WidgetNode` tag/attrs/content as strings today) → v1.23
- [ ] GPU rendering — after retained layout, smart invalidation, and damage tracking ship → v1.25

---

## Performance improvements — 2026-07-02 deep scan

Findings from a full-codebase performance scan (data handling, component
communication, events, rendering) motivated by the gap to QtQuick/webview-class
shells. Each item cites `file:line` as of this scan; reverify before editing.
Items that overlap an existing milestone entry above say so instead of
duplicating it.

### A. Data handling — Rust ↔ Lua boundary is JSON-shaped and clone-heavy

- [ ] **Per-paint element metrics: build → deep-compare → JSON→Lua convert,
      every frame.** `publish_element_metrics`
      (`shell/component/interaction_state.rs:41-65`) serializes _every keyed
      node_ to a `serde_json::Map` per paint, `set_host_value` deep-compares
      it, then `apply_element_metrics`
      (`scripting/context/runtime.rs:414-428`) converts the whole object to a
      Lua table **and** reinstalls bound element proxies — per frame, even
      when nothing scripted reads geometry that frame. Make `refs.<name>`
      reads lazy: keep metrics in a Rust-side store and resolve fields on
      `__index` (the proxy machinery already exists in `element_ref.rs`),
      publishing only a generation bump per paint; drop the eager
      `elements`/`refs` state tables or gate them on actual template reads.
      Progress 2026-07-02: the state-side deep-compare portion is removed for
      unchanged metrics via full-JSON fingerprints (see previous item), but
      eager JSON construction remains. Progress 2026-07-03: the scripting
      runtime now caches the last successfully installed refs fingerprint, so
      unchanged paints skip JSON→Lua conversion and bound-proxy reinstallation.
      A release benchmark over 20k unchanged publications measured 90.711ms for
      the eager path versus 0.140ms for the fingerprint-gated path (~647x
      faster). Rust-side tree walking/JSON construction and lazy `refs` field
      resolution remain open. Progress 2026-07-04: metrics publication is now
      gated by the retained-tree diff. Paint/style/state-only frames skip the
      Rust tree walk, JSON maps, fingerprints, runtime lock, and proxy update;
      layout, attribute, child, insertion, and removal changes still publish.
      A 1,365-node release microbenchmark over 2,000 unchanged passes measured
      23.236s for rebuilding snapshots versus 1.188us for the dirty-summary
      gate. Lazy field resolution remains open for frames where metrics really
      changed. Progress 2026-07-05: metric usage is now split between
      `elements` and `refs`, so ref/id-only components keep publishing the
      public `refs` table and live proxies without also building the all-node
      `elements` snapshot. A 341-node release benchmark over 2,000 changed
      publications measured 6.069s for collect-both versus 3.823s for
      refs-only (1.6x faster). Progress 2026-07-05: the collector now builds
      full JSON snapshots lazily only for nodes that actually publish to
      `elements` or `refs`, while reading scroll offsets directly for traversal
      through unpublished ancestors. A sparse-ref 341-node release benchmark
      over 2,000 publications measured 1.872s for eager per-node snapshots
      versus 205.920ms for lazy snapshots (9.1x faster). Progress 2026-07-13:
      element snapshot scroll fields now read the runtime's typed
      `WidgetNode::scroll_metrics` via `resolved_scroll_metrics()` instead of
      parsing `_mesh_scroll_*` attributes when typed metrics are present, while
      preserving the legacy attribute fallback. A release microbenchmark over
      2M scroll metric reads measured 106.047ms for attribute parsing versus
      6.085ms for typed metrics (17.4x faster for that snapshot subpath). Lazy
      field resolution remains open for frames where metrics really changed.
      Progress 2026-07-13: `element_snapshot_json` now builds the public JSON
      object directly instead of building an `ElementSnapshot`, cloning
      attributes into a `BTreeMap`, and serializing it back through serde before
      adding tag-specific fields. Parity coverage compares the direct builder
      against the old serde shape; a release microbenchmark over 200k input
      snapshots measured 530.347ms for the serde roundtrip versus 459.070ms
      direct (1.2x faster). Progress 2026-07-13: the public
      `element_snapshot()` path now clones the node attribute map directly
      instead of rebuilding it through iterator collection; a release
      microbenchmark over 500k 16-attribute clones measured 360.630ms for
      collect-clone versus 193.524ms for `BTreeMap::clone()` (1.9x faster for
      that snapshot subpath).
- [ ] **Stringly-typed template expression values.** `eval_expr` returns
      `String` for everything (`frontend/compiler/src/expr.rs:26,162`);
      numeric ops re-`parse::<f64>` both sides per evaluation
      (`expr.rs:197`), `if` conditions compare against `"false"|"nil"|""|"0"`
      string literals, and every result is stored as an attribute `String`
      that downstream code re-parses. Introduce a small typed value enum
      (bool/number/string) for compiled-expression evaluation and only
      stringify at the attribute boundary — this also removes false
      attribute-hash dirtiness from float formatting.
  - [x] 2026-07-04: compiled expression evaluation now carries an internal
        bool/number/string value enum through boolean operators, ternaries,
        comparisons, concatenation, translation, and JSON variable reads, then
        stringifies only at the public `eval_expr` boundary. Numeric JSON
        comparisons avoid per-evaluation string allocation and `parse::<f64>`.
        A release benchmark over 500k numeric comparisons measured 36.848ms for
        the old string-parse shape versus 29.394ms for typed comparison (1.3x
        faster).
  - [ ] Attribute storage remains string-based until the downstream
        `WidgetNode`/style contracts are typed.

### B. Component communication & input

- [ ] **Handler dispatch overhead per event.** `call_namespaced_handler`
      locks the runtimes mutex, allocates 3 Strings for namespacing, and
      unconditionally runs `resync_binding_neighbors` over every linked
      instance after each handler (`shell/component/runtime.rs:494-560`).
      Track "did a cross-`_ENV` write actually happen" (a dirty bit set by
      the live-binding `__newindex`) and skip neighbor resync when clean;
      intern instance keys. Progress 2026-07-04: ordinary handler names now
      bypass legacy JSON-descriptor parsing unless the first byte is `{`;
      legacy pre-bound descriptors remain supported. A release benchmark over
      500k pointer-handler unpacks measured 43.866ms with failed JSON parsing
      versus 37.898ms with the syntax gate (1.2x faster). The binding-resync
      and instance-key allocation work remains open; a simple `__newindex`
      dirty bit is insufficient because Lua does not invoke it when replacing
      existing globals. Progress 2026-07-04: live `bind:this` proxies now set
      a per-runtime external-access flag only when another component writes
      through the proxy or calls a proxied function. Post-handler neighbor
      resync consumes that flag and skips untouched linked runtimes, while
      touched child-call semantics remain covered by
      `bind_this_event_handler_calls_child_live_and_resyncs_it`. A release
      benchmark over 2k untouched-neighbor checks measured 3.194ms for
      unconditional child resync versus 42.743us for the flag-gated skip
      (74.7x faster).
  - [x] 2026-07-04: plain handler argument unpacking now borrows the handler
        name and event args instead of cloning them into a fresh `String`/`Vec`
        on every dispatch; legacy JSON descriptors and typed pre-bound handler
        args still allocate only when merging is needed. A release benchmark
        over 500k plain handler transfers measured 40.768ms for clone-transfer
        versus 2.548ms for borrowed transfer (16.0x faster).
  - [ ] Instance-key interning remains open. Progress 2026-07-06:
        namespaced handler dispatch now borrows the parsed instance key and
        raw handler name directly from the already-namespaced handler string,
        and clones the component id only on the error/diagnostics path. A
        release benchmark over 500k target resolutions measured 44.668ms for
        clone-heavy resolution versus 10.224ms for borrowed resolution (4.4x
        faster). Full graph-wide instance-key interning remains open.
- [ ] **`bind:this`/live-binding writes mark the whole surface dirty.** Any
      handler that touches state invalidates via `invalidate_script_state()`
      → full template re-eval + tree rebuild (narrow path still rebuilds the
      full tree first — see v1.27 item). The typed state-dependency work
      (v1.18) should extend to handler writes: record which public members a
      template actually binds and skip rebuilds for writes nothing binds to.

### C. Events & service delivery

- [ ] **Per-event mutex churn in the observation gate.**
      `deliver_service_event` (`runtime/service_state.rs:167-184`) calls
      `observes_service_event` on every component per event, which locks that
      component's `runtimes` mutex and queries tracked-field maps
      (`shell_component.rs:261-268`); several tracked-field APIs clone whole
      maps/sets (`tracked_service_fields()`,
      `tracked_fields_for_service()` — `scripting/context/runtime.rs:478-497`).
      Maintain a shell-side subscription index (service → component indices),
      invalidated when a runtime's tracked fields/subscriptions change, so
      event routing is a lookup instead of N mutex acquisitions. Experiment
      2026-07-03: a component-local copied summary refreshed after paint/input
      was rejected and reverted. Its production-faithful single-runtime
      release benchmark improved the event gate only from 2.544ms to 2.221ms
      over 100k calls (1.1x), while refreshes had to clone the tracked maps.
      The shell-side index described above remains the viable design because it
      also eliminates the O(component count) scan rather than merely moving
      locks into refresh work.
- [ ] **Backend providers still exec-poll by default.** `spawn_backend_service`
      drives a tokio interval that re-runs `exec` subprocesses per tick
      (`runtime/backend/src/lib.rs:157-…`). The pipewire item above tracks one
      backend; the generic gap is push-based host API primitives (D-Bus
      signal subscribe, fd/socket watch, `pw-dump --monitor`-style stream
      adoption) so providers can be event-driven and the safety poll becomes
      the fallback, not the mechanism.

### D. Rendering & per-frame work

- [ ] **Fractional HiDPI forces full-surface repaint every frame.** `paint`
      sets `surface_pixels_invalid = true` whenever `scale` is non-integer
      (`shell/component/shell_component.rs:460-462`), so on a 1.25/1.5×
      output _every_ frame is a full clear+repaint+full-damage present —
      partial damage only exists at integer scales. Fix the underlying
      logical-vs-physical damage-clip mismatch (compute/clip damage in
      physical pixels through the painter) so the retained partial path works
      at fractional scale. Likely the single biggest win on fractional-scale
      setups. Experiment 2026-07-03: a physical-damage rect fix was correct
      byte-for-byte against forced-full repaint, but rejected as a performance
      change for now. End-to-end release benchmarks did not show a meaningful
      CPU-side win: one-box 1200×600 forced-full 28.066ms vs partial 28.367ms,
      and large 3600×1800 forced-full 74.918ms vs partial 73.910ms. Revisit
      with compositor/upload damage instrumentation before marking this done.
- [ ] **Per-frame full-tree fingerprinting even when clean.**
      `RetainedWidgetTree::update` re-walks every node, hashes ~50 style
      fields + every attribute/handler string (FNV byte-at-a-time), allocates
      a snapshot (with a `child_ids` Vec) per node into a scratch map, and
      clones snapshots on change (`runtime_tree.rs:98-163,293-392`). The
      v1.27 "generation-aware diff" item covers skipping clean subtrees; add:
      hash with a word-at-a-time hasher (fxhash), reuse snapshot allocations
      in place (index by slotmap key instead of rebuilding the `NodeId` map),
      and stop hashing shell-owned annotation attributes that already have
      typed change tracking (`_mesh_scroll_*`, `_mesh_key`). Partial
      2026-07-03: `RuntimeTreeHasher` now implements primitive `write_*`
      methods so numeric style fields are mixed word-at-a-time instead of
      falling back to byte-at-a-time hashing. A release benchmark over 500k
      style fingerprints measured 118.362ms for the old byte fallback versus
      63.946ms primitive-aware (1.9x faster). Snapshot allocation reuse and
      broader shell-owned annotation filtering remain open. Progress
      2026-07-04: `_mesh_key` is no longer included in the attribute hash
      because the same identity is already encoded by the retained `node.id`;
      structural movement still changes parent `child_ids`. A 10-level-key
      release microbenchmark over 2M fingerprints measured 98.724ms with the
      redundant key hash versus 44.799ms without it (2.2x faster). Scroll and
      other shell annotations remain hashed until they have equivalent typed
      change tracking. Progress 2026-07-13: retained attribute fingerprints now
      skip `_mesh_scroll_*` and `_mesh_content_*` annotations because
      `layout_fingerprint()` already tracks the same resolved typed scroll
      metrics. A standalone Rust benchmark matching the attribute-hash loop
      measured 475.5ms with redundant scroll/content attribute hashing versus
      32.9ms with the typed-annotation skip over 2M hashes (14.5x faster for
      that subpath); in-crate shell verification remains blocked locally by
      the missing `xkbcommon.pc` system dependency. Progress 2026-07-04:
      retained snapshot `child_ids` now
      use inline storage for up to eight children, eliminating the per-node
      heap allocation for normal UI trees while spilling safely for wider
      containers. A 4-child release microbenchmark over 2M snapshots measured
      9.811ms with fresh `Vec` allocation versus 2.810ms inline (3.5x faster).
      The transient retained dirty `SecondaryMap` now also swaps through a
      scratch slot instead of reallocating on each interaction update; a
      128-dirty-node release benchmark over 20k updates measured 6.622ms fresh
      versus 3.419ms reused (1.9x faster). Progress 2026-07-04: retained
      snapshot updates now remove stale nodes before draining the per-frame
      scratch map, then move changed/inserted `RetainedNodeSnapshot`s into
      slotmap storage instead of cloning them. Release benchmark:
      clone-transfer 216.847ms vs drain-move 177.698ms over 5.12M snapshot
      transfers (1.2x faster). Broader clean-subtree skipping and slotmap-keyed
      snapshot reuse remain open. Progress 2026-07-04: pre-bound event handler
      args in retained attribute fingerprints now hash `serde_json::Value`
      structure directly instead of allocating a serialized string for every
      arg. A release benchmark over 500k nested JSON arg fingerprints measured
      433.760ms for `to_string` hashing versus 92.355ms for direct typed
      hashing (4.7x faster). Progress 2026-07-10: `_mesh_focused` is no
      longer included in retained attribute fingerprints because the same
      state change is already tracked by the typed `ElementState` fingerprint.
      Added regression coverage and a release-only benchmark; the local
      dev-shell run measured 82.703ms with redundant hashing versus 58.788ms
      with the skip over 2M fingerprints (1.4x faster).
- [ ] **`WidgetNode` allocation profile.** Every node carries `tag: String`,
      `attributes: BTreeMap<String,String>`, `event_handlers:
    BTreeMap<String,String>` (`ui/elements/src/tree.rs:44-68`), rebuilt
      from the template on every script invalidation and deep-cloned by the
      input path. Interning (v1.23) plus a small-map type (attrs are
      typically <8 entries; `Vec<(Symbol, CompactString)>` beats a BTreeMap)
      and moving shell annotations (`_mesh_key`, scroll offsets, focus flags,
      selection coords — currently formatted floats in string attributes,
      `runtime_tree.rs:729-743`, `rendering.rs:697-728`) to typed fields
      would shrink both build and diff cost. Overlaps v1.23/v1.27; listed
      here because the _authoring_ of new annotations keeps growing the
      string surface.
- [ ] **`finalize_tree` runs ~8 full-tree walks per finalized frame** beyond
      the annotation fuse already tracked (v1.27): `annotate_runtime_tree`,
      `append_class_recursive` (exit/enter classes), `annotate_surface_shortcuts`,
      `annotate_overflow_tree`, `merge_runtime_primitive_defaults`,
      `collapse_promoted_popover_wrappers`, `constrain_error_placeholders`,
      `annotate_selection_tree` (`shell/component/rendering.rs:238-432`).
      Several are only relevant when a feature is active (no popovers → no
      collapse walk; no selection → skip). Gate the conditional walks on
      cheap presence flags and fold the unconditional ones into the fused
      traversal. Partial 2026-07-03: promoted-popover collapse and generated
      error-placeholder constraints are now guarded by component-level presence
      flags set at the actual marker creation sites; normal trees skip both
      marker walks. Surface shortcut annotation now also returns before loading
      keyboard settings when neither manifest nor legacy settings declare
      shortcuts. A release microbenchmark over 20k plain-tree finalizations
      measured 361.497ms for the two old marker walks versus 2.375us for the
      gated path (152k×). The always-needed annotation/restyle/layout walks and
      broader traversal fusion remain open. Rejected experiment 2026-07-04:
      fusing surface and child enter/exit class annotation into one full-tree
      traversal measured 87.249ms versus 52.903ms for the existing targeted
      searches/subtree walks (0.6x). The prototype was reverted; any future
      fusion needs to avoid scanning unrelated branches. Progress 2026-07-04:
      text-selection annotation now resolves the selected `_mesh_key` with the
      existing keyed node lookup and annotates only that node instead of running
      a selection-specific recursive tree walk. Release benchmark on a broad
      tree: recursive 4.072s vs keyed 3.857s over 10k iterations (1.1x faster).
      Progress 2026-07-10: targeted interaction restyle now carries both the
      full affected-descendant set for CSS matching and a root-only set for
      runtime primitive default merging, so hover/focus frames stop applying
      default fills across unrelated subtrees. Added regression coverage and a
      release-only benchmark; the local dev-shell run measured 2.593s for
      full-tree default merge versus 1.923s targeted over 5k synthetic
      interaction updates (1.3x faster). Progress 2026-07-12: the mandatory
      `annotate_runtime_tree` traversal no longer allocates a
      `source_element_tag(node).to_string()` for every node; it borrows the
      source tag only in the choice/select branches that need it. Added a
      release-only benchmark over 20k plain-tree walks; the local dev-shell run
      measured 250.396ms for eager allocation versus 58.000ms for lazy
      borrowed checks (4.3x faster for that annotation subpath). Progress
      2026-07-13: interaction changed-subtree collection now converts changed
      hover/focus runtime paths to stable `NodeId`s once, then matches
      `node.id` during the tree walk instead of probing string key sets for
      every node. A standalone Rust microbenchmark matching the collector shape
      measured 825.6ms for the string-key collector versus 498.4ms for the
      `NodeId` collector over 20k synthetic updates (1.66x faster); in-crate
      shell tests/benchmark remain blocked locally by the missing `xkbcommon.pc`
      system dependency.
- [ ] **Layout + display list**: Taffy tree rebuilt per layout pass and
      display-list subtree flattening per update are already tracked
      (v1.21). Reaffirmed as the dominant structural-frame costs behind
      restyle in this scan; no new sub-findings. Progress 2026-07-04:
      render-object sync now reuses the per-update `dirty_nodes` allocation
      and replaces the separate `visited` hash set with an epoch mark stored on
      each retained render object. Release benchmark: visited set 158.594ms vs
      epoch marks 89.300ms over 20k synthetic updates (1.8x faster). Rejected
      experiment: changing render-object child IDs from `Vec` to `SmallVec`
      measured slower (11.285ms `Vec` vs 21.992ms `SmallVec`) and was reverted.
      Progress 2026-07-10: dirty-ancestor collection for retained display-list
      subtree reuse now stops once all dirty nodes have been found instead of
      always walking the full tree. Added regression coverage for sparse dirty
      ancestor correctness and a release-only benchmark; the local dev-shell
      in-crate run measured 2.334s full-walk versus 5.292ms early-exit over
      the sparse-dirty workload (441.1x faster). Progress 2026-07-10:
      retained display-list
      damage diff now skips the previous-entry removal scan when the new entry
      set has no insertions and the map sizes match, because removals are then
      impossible. Added a release-only benchmark; the local dev-shell in-crate
      run measured 1.905s for the full previous-entry scan versus 185.220us
      for the guarded skip over 200k stable-key updates (10283.4x faster).
- [ ] **CPU Skia raster + SHM is the ceiling.** Painting is skia-safe CPU
      raster into `PixelBuffer` + SHM upload (`render/src/surface/painter/backend.rs`);
      blur/shadows/gradients are CPU per damaged pixel. GPU rendering is
      deferred (v1.25) — when it lands, prefer a `wgpu`/Skia-GPU surface per
      output with the retained display list as the command source, and keep
      SHM as fallback. Until then, the damage-path fixes above (especially
      fractional scale) are the effective lever.
### E. Style system — second-pass findings

- [ ] **Every declaration resolves through a String round-trip.** Theme
      tokens are stored as `TokenValue::Number` but resolution formats them
      (`format!("{n}")`, `resolve.rs:402`) and downstream re-parses
      (`parse_px`, `Color::from_hex` — `resolve.rs:446-461`); `var()`
      resolution walks embedded-reference string substitution per value.
      This is the inner loop of both build and restyle. Extends the v1.23
      typed-declaration item: resolve tokens to typed values
      (`Color`/`f32`/enum) once per theme load and make
      `apply_declaration` consume typed values, keeping strings only for
      diagnostics. Progress 2026-07-06: numeric style resolution now has a
      typed fast path for simple `var(...)`/`prop(...)` references and numeric
      theme tokens, avoiding `TokenValue::Number -> String -> parse_px`
      round-trips for numeric properties while preserving fallback string
      semantics for embedded CSS references. A release benchmark over 500k
      numeric token resolutions measured 83.468ms for the string round trip
      versus 47.757ms for the typed path (1.7x faster). Progress 2026-07-06:
      color style resolution now also has a borrowed fast path for simple
      literals, variables, props, and string theme tokens, avoiding a cloned
      resolved string before `Color::from_hex` when substitution is not needed.
      A release benchmark over 500k color token resolutions measured 44.999ms
      for the string-clone path versus 35.997ms for the borrowed path (1.3x
      faster). Progress 2026-07-06: keyword-style declarations now use the
      same borrowed simple-value fast path for direct comparisons across
      `display`, `position`, visibility, flex/alignment, text, and blend-mode
      properties. A release benchmark over 500k keyword token resolutions
      measured 38.788ms for the string-clone path versus 31.852ms for the
      borrowed path (1.2x faster). Progress 2026-07-06: dimension declarations
      (`width`, `height`, `flex-basis`) now parse borrowed simple values as
      well, avoiding allocation before `parse_dimension` for common tokenized
      sizes. A release benchmark over 500k dimension token resolutions measured
      48.995ms for the string-clone path versus 41.248ms for the borrowed path
      (1.2x faster). Progress 2026-07-06: overflow declarations
      (`overflow`, `overflow-x`, `overflow-y`) now parse borrowed simple
      values too. A release benchmark over 500k overflow token resolutions
      measured 57.668ms for the string-clone path versus 47.218ms for the
      borrowed path (1.2x faster). Progress 2026-07-06: transition/animation
      duration and delay declarations now parse borrowed simple values before
      `parse_first_time_ms`. A release benchmark over 500k time token
      resolutions measured 120.253ms for the string-clone path versus
      113.080ms for the borrowed path (1.1x faster). Progress 2026-07-06:
      `transition-property` now parses borrowed simple property lists before
      `parse_transition_properties`. A release benchmark over 500k transition
      property token resolutions measured 72.124ms for the string-clone path
      versus 65.417ms for the borrowed path (1.1x faster). Progress
      2026-07-06: `filter` and `backdrop-filter` now parse borrowed simple
      values before `parse_filter`. A release benchmark over 500k filter token
      resolutions measured 52.210ms for the string-clone path versus 43.185ms
      for the borrowed path (1.2x faster). Progress 2026-07-06:
      `background-image` now parses borrowed simple values before
      `parse_background_image`. A release benchmark over 500k gradient token
      resolutions measured 71.205ms for the string-clone path versus 62.134ms
      for the borrowed path (1.1x faster). Progress 2026-07-06: edge
      shorthands (`padding`, `margin`, `border-width`, `inset`) now parse
      borrowed simple values before `parse_edges_shorthand`. A release
      benchmark over 500k edge shorthand token resolutions measured 120.785ms
      for the string-clone path versus 112.609ms for the borrowed path (1.1x
      faster). Progress 2026-07-06: `border-radius` shorthand now parses
      borrowed simple values before `parse_corners_shorthand`. A release
      benchmark over 500k corner shorthand token resolutions measured 85.640ms
      for the string-clone path versus 82.162ms for the borrowed path (~4%
      faster). Progress 2026-07-06: `border` and `border-color` now parse
      borrowed simple values before `apply_border_shorthand` /
      `parse_border_color_shorthand`. A release benchmark over 500k border
      shorthand token resolutions measured 111.028ms for the string-clone path
      versus 96.613ms for the borrowed path (1.1x faster). Progress
      2026-07-06: `transform-origin` now parses borrowed simple values before
      `parse_transform_origin`. A release benchmark over 500k transform-origin
      token resolutions measured 137.328ms for the string-clone path versus
      129.423ms for the borrowed path (1.1x faster). Progress 2026-07-06:
      `transform` now parses borrowed simple values before `parse_transform`.
      A release benchmark over 500k transform token resolutions measured
      120.761ms for the string-clone path versus 104.237ms for the borrowed
      path (1.2x faster). Progress 2026-07-06: `box-shadow` now parses
      borrowed simple values before `parse_box_shadow`. A release benchmark
      over 500k box-shadow token resolutions measured 144.953ms for the
      string-clone path versus 130.210ms for the borrowed path (1.1x faster).
      Progress 2026-07-06: `flex` now resolves simple values through the
      borrowed path before applying shorthand semantics. A release benchmark
      over 500k flex token resolutions measured 80.558ms for the string-clone
      path versus 73.508ms for the borrowed path (1.1x faster). Progress
      2026-07-13: `apply_flex_shorthand` now reads the first three whitespace
      fields directly instead of collecting all tokens into a temporary `Vec`.
      A release microbenchmark over 2M flex shorthand parses measured
      150.266ms for `Vec` collect versus 129.296ms for iterator fields (1.2x
      faster for that parser subpath). Progress 2026-07-13:
      `parse_overflow_shorthand` now reads the first two whitespace fields
      directly instead of collecting into a temporary `Vec`. A release
      microbenchmark over 3M overflow shorthand parses measured 81.771ms for
      `Vec` collect versus 61.478ms for iterator fields (1.3x faster for that
      parser subpath). Progress 2026-07-13: `parse_transform_origin` now reads
      the first two whitespace fields directly instead of collecting into a
      temporary `Vec`. A release microbenchmark over 3M transform-origin
      parses measured 93.661ms for `Vec` collect versus 65.960ms for iterator
      fields (1.4x faster for that parser subpath). Progress 2026-07-13:
      animation keyword
      properties (`transition-timing-function`, `animation-name`,
      `animation-timing-function`, `animation-iteration-count`,
      `animation-direction`, `animation-fill-mode`, `animation-play-state`) now
      use the borrowed simple-value resolver before parsing first-list-item
      keywords. A release benchmark over 500k timing-function token
      resolutions measured 29.373ms for the string-clone path versus 25.064ms
      for the borrowed path (1.2x faster).
- [x] **Theme component defaults re-applied per node from string maps.**
      `apply_theme_component_defaults` parses `HashMap<String, String>`
      defaults on every node resolution (already visible in the
      post-2026-06-10 toggle profile note above). Pre-bake per-tag
      `ComputedStyle` prototypes once per theme change and start resolution
      from a memcpy of the prototype instead of re-applying string
      declarations.
  - [x] 2026-07-05: no-diagnostics theme default application now applies
        borrowed property names directly instead of constructing a temporary
        `Declaration` for every default on every node. A release benchmark over
        200k default applications measured 165.045ms for declaration allocation
        versus 154.446ms for direct property application (1.1x faster).
  - [x] 2026-07-09: each `StyleResolver` now pre-bakes and caches the resolved
        `ComputedStyle` plus custom-variable seed per `(module, tag)`, then
        clones that prototype before applying matched rules. Resolver-local
        caches naturally invalidate on theme/prop rebuilds and keep module
        defaults isolated. A release benchmark over 200k cache-hit resolutions
        measured 2.931s reapplying an eight-property string map versus 29.740ms
        from the prototype (~98.5x faster).
### F. Animation & layout per-frame overhead

- [x] **Retained Taffy layout still re-syncs every node's style per pass.**
      `compute_incremental` → `update_retained_node_styles` walks the whole
      tree rebuilding `taffy_style_for_node` and re-populating
      `node_map`/`text_nodes` HashMaps on every layout-dirty frame
      (`ui/elements/src/layout.rs:346-390`), even when one node changed.
      Feed the retained-tree dirty set (already computed in
      `RetainedWidgetTree::update`) into layout so only dirty nodes get
      `set_style` calls — Taffy caches internally, but MESH pays the full
      style-conversion walk. (Structural rebuild case is tracked at v1.21;
      this is the _non-structural_ per-frame cost.) Progress 2026-07-04 for the
      paint-only case: when available geometry and layout dirtiness are both
      unchanged, `compute_incremental` now returns before rebuilding node/text
      maps, converting styles, or calling Taffy `set_style`. A layout-dirty or
      resized frame still synchronizes the full retained tree immediately
      before layout, preserving deferred correctness. The 1,365-node release
      microbenchmark over 2,000 paint-only passes measured 378.430ms for the
      old synchronization walk versus 40.369us for the fast path (9,374x).
      Dirty-node-only synchronization within actual layout passes remains a
      possible follow-up once retained-tree dirty IDs are exposed here.
      Rejected experiment 2026-07-04: retaining and clearing the temporary
      `node_map`/`text_nodes` allocations made the end-to-end layout pass
      slightly slower (77.502ms scratch versus 77.033ms fresh, 0.99x), because
      full style synchronization and map clearing dominate. The prototype was
      reverted. Completed 2026-07-12 with the T-layout closure:
      non-structural layout frames now feed retained layout-relevant dirty
      `NodeId`s into `LayoutEngine::compute_incremental_with_dirty_nodes`, so
      Taffy `set_style`/`mark_dirty` is limited to dirty nodes while structural
      uncertainty still falls back to the full retained sync. `node_map` and
      text measurement context remain populated for all retained nodes because
      they are needed by layout output and measurement callbacks. Verification:
      `cargo test -p mesh-core-elements retained_layout` passes; shell crate
      verification remains blocked by the missing `xkbcommon.pc` system
      dependency.

### G. Lua runtime — state sync & handler overhead

### H. Presentation & memory

- [ ] **Extra full-buffer memcpy per present.** Skia paints into
      `PixelBuffer`, then `copy_bgra_to_canvas`/`copy_bgra_damage_to_canvas`
      memcpys into the SHM mapping (`presentation/src/wayland_surface/backend.rs:514-646`).
      The damage-scoped copy path is good, but full-present frames (first
      paint, resize, fractional scale until fixed) pay paint + full copy.
      Have Skia render directly into the mapped SHM canvas
      (`with_skia_canvas` over the pool slot) for the active buffer,
      keeping `PixelBuffer` only as the retained/compare copy — or adopt
      double-buffered direct paint once damage tracking is per-buffer.
- [ ] **SHM pool thrash on resize.** Any size change clears and re-creates
      all `SHM_BUFFER_POOL_DEPTH` buffers (`backend.rs:251-260`). A
      content-measured surface that animates its size (expanding popover,
      growing launcher list) reallocates the whole buffer set every frame.
      Round buffer allocation up to size classes (e.g. next-64px) and
      present with viewport crop, so gradual resizes reuse allocations.
- [ ] **Startup compiles modules serially.** Module discovery + `.mesh`
      parse + compile runs one directory at a time on the main thread
      (`shell/discovery.rs:126+`). Parse/compile are pure per-module —
      parallelize with rayon/spawn_blocking to cut shell start latency
      (matters for session startup perception vs. quickshell).
      Progress 2026-07-05: `FrontendCatalog::from_modules` now compiles the
      sorted frontend module set with an indexed Rayon pipeline, preserving
      deterministic collection/error order while running independent compile
      work concurrently. A release benchmark over 20 builds of the nine
      shipped frontend modules measured 63.48ms sequential versus 32.70ms
      parallel (~1.9x faster). Directory discovery and manifest parsing remain
      serial.

### I. Composition, display list & proxies — third-pass findings

- [ ] **No component-level render memoization — the strategic gap.** Every
      surface rebuild re-evaluates _every_ embedded/local component's
      template from scratch: `render_import`
      (`shell/component/composition.rs:12-100`) re-clones props into a fresh
      `HashMap<String, serde_json::Value>`, re-`format!`s instance keys,
      re-runs `bind_child_instance`, and re-renders the child subtree even
      when that instance's props and script state are untouched. This is why
      one reactive variable changing anywhere re-costs the whole surface.
      Each `EmbeddedFrontendRuntime` already has a
      `ScriptState::mutation_generation`; cache each instance's built
      subtree keyed by (props fingerprint, state generation, locale/theme
      generation) and reuse it wholesale on rebuild. This is the
      component-granular complement to the v1.27 node-level narrow re-eval
      and probably the single largest structural win for complex surfaces.
      Progress 2026-07-05: embedded/local runtime prop sync now uses a single
      runtime-map lock for existing instances and applies props directly to a
      newly-created runtime after its render hook, instead of inserting and
      looking it up again. A release microbenchmark over 1M existing-instance
      updates measured 26.851ms for `contains_key` + second `get_mut` lock
      versus 12.362ms for one `get_mut` lock (2.2x faster). Full subtree
      memoization remains open. Progress 2026-07-05: prop-bound handler
      matching now scans borrowed event-handler maps and only clones matched
      handler entries instead of cloning each node's full handler map before
      checking for matches. A 65-node no-match release benchmark over 50k
      passes measured 1.089s for clone-then-scan versus 877.216ms for borrowed
      scan (1.2x faster). Full subtree memoization remains open.
- [ ] **Display payload text still clones string attributes.** Display-entry
      comparison and paint-node creation still clone/deep-compare per-entry
      strings (`content`/`value`/`src`/`name`). Consider sharing node text via
      `Arc<str>` between `WidgetNode` and display entries, or introducing a
      compact interned attribute payload, after auditing the `WidgetNode` and
      renderer payload contract. Progress 2026-07-05: display primitive
      signatures now hash paint payload attributes by tag instead of hashing
      text/input/icon/slider attributes for every node. This also avoids
      display-list churn when irrelevant payload-like attributes change on
      generic nodes. A mixed 512-node release benchmark over 20k signature
      passes measured 925.336ms for all-payload-attrs hashing versus
      219.760ms for tag-aware hashing (4.2x faster). Actual string sharing
      remains open. Rejected experiment 2026-07-05: replacing payload-owned
      strings with `Arc<str>` made a two-string input-payload clone slower:
      67.591ms versus 63.400ms for owned `String` clones over 5 million
      iterations (6.6% regression). A useful solution must avoid node-to-payload
      allocation without adding atomic bumps to every command clone.
- [ ] **Storage reads clone per Lua access.** `self.storage.key` reads lock
      the storage mutex and clone the JSON value per access
      (`scripting/storage.rs:275-307`); render hooks that read storage pay
      this per frame. Minor today; becomes visible once handlers use
      storage more. Consider caching the storage table Lua-side and
      invalidating on write. Rejected experiment 2026-07-04: a private
      Lua-table cache for scalar values measured 50.238ms versus 39.202ms for
      the existing lock/clone/convert path (0.8x); the extra Lua lookup cost
      outweighed the saved Rust work, so the prototype was reverted. A future
      attempt should target shared immutable JSON values or lock avoidance
      without adding another Lua table lookup. Progress 2026-07-12:
      `self.storage.snapshot` is now installed as a table-owned Lua function
      once instead of being allocated from `__index` on every method lookup.
      This does not change scalar storage read cloning, but removes repeated
      function allocation for render hooks that call `self.storage:snapshot()`.
      Added coverage that snapshot lookup does not track a storage-key read
      plus a release-only benchmark; the local dev-shell run measured
      602.437ms for per-lookup function creation versus 548.181ms for the
      table-owned method over 100k snapshot calls (1.1x faster).
### J. Algorithmic complexity — quadratic hot-path patterns (fourth pass)

Targeted scan for accidentally-super-linear loops. These compound with each
other: an uncoalesced motion event multiplied by an O(depth × n) hover dispatch
multiplied by O(n) tree clones is where interaction latency actually goes.

- [ ] **Runtime key paths make deep trees O(n × depth).** Every node's key
      is the full slash-joined ancestor path built with
      `format!("{key}/{index}")` and FNV-hashed from scratch per node per
      frame (`runtime_tree.rs:616-622,281-291`) — a 10-deep list row hashes
      ~40-byte strings for every row every frame, and key length grows with
      depth. Derive ids by hash-chaining `(parent_id, child_index)` — O(1)
      per node, no string at all — and keep the string path only for debug
      builds / diagnostics. Progress 2026-07-04: runtime node IDs now hash-chain
      `(parent_id, child_index)` instead of rehashing each full ancestor path.
      IDs remain deterministic, nonzero, and sibling-distinct. A 10-level
      release microbenchmark over 500k iterations measured 36.394ms for full
      path hashing versus 5.755ms for parent chaining (6.3x faster). String
      paths are still built because interaction state and refs currently use
      them as public runtime keys; removing those allocations remains open.
      Progress 2026-07-10: runtime annotation now builds those public key
      paths with one mutable string buffer instead of allocating a formatted
      child key at every edge. Added key-string regression coverage and a
      release-only benchmark; the local dev-shell run measured 1.091s for
      `format!("{key}/{index}")` versus 421.848ms for append/truncate over
      20k broad-tree iterations (2.6x faster). Progress 2026-07-13: scroll
      overflow annotation now uses the same append/truncate path strategy and
      avoids cloning existing scroll-offset keys during steady-state lookups.
      A release benchmark over 20k broad-tree annotation passes measured
      988.379ms for recursive formatted child keys versus 710.105ms for the
      path buffer (1.4x faster).
- [ ] **`finalize_tree` closing-popover pass: O(closing-keys × tree)**
      `find_node_by_key_mut` per closing key (`rendering.rs:273-279`).
      Trivial count in practice; fold into the fused annotation walk (D)
      rather than fixing separately. Rejected experiment 2026-07-05: replacing
      the per-key searches with one full-tree inherited-state traversal was
      slower for the realistic small-key case (855.098ms existing per-key
      search vs 1.035s one-walk over 2k broad-tree iterations), so the
      prototype was reverted.
- [ ] **Slider drag worst case = every quadratic above at once.** Each
      uncoalesced motion during a drag runs slider-value tree walks ×3, a
      handler call (Lua + full `sync_state_from_lua`), then
      `invalidate_script_state()` → full template rebuild + restyle + layout + paint (`input/mod.rs:163-186`). With motion coalescing (above) plus
      routing slider drags through the STATE/interaction-restyle path
      instead of SCRIPT invalidation (the knob position is
      shell-owned state — `slider_values` — not script state), a drag frame
      should cost a targeted restyle, not a rebuild.
      Progress 2026-07-10: handlerless slider press/move frames now invalidate
      through interaction restyle instead of unconditional script rebuild,
      while sliders with `change`/`release` handlers preserve the script
      invalidation path so reactive labels still update. Added policy tests for
      both paths and a release-only repaint benchmark; the local dev-shell run
      measured 790.919ms for forced script rebuild versus 213.822ms retained
      interaction repaint over 200 handlerless drag frames (3.7x faster).

### K. Threading & repaint suppression (fifth pass)

MESH is effectively single-threaded for all UI work: script execution, tree
build, restyle, layout, Skia raster, and present for **every surface** run
serially inside `Shell::run` on the main thread (`shell/runtime/mod.rs:173+`).
The Tokio runtime (`runtime/mod.rs:182`) only hosts backend pollers and IPC.
QtQuick's render loop, by contrast, splits scene-graph sync from rendering.
The Lua VMs are `!Send` and must stay on the shell thread — but everything
after the display list is built does not.

- [ ] **Parallelize paint across surfaces.** After `finalize_tree`, painting
      is pure: display list + `PixelBuffer` in, pixels out. Surfaces are
      independent (own buffer, own damage). Restructure `render_components`
      into two phases — phase 1 (serial, VM-bound): script hooks, build,
      restyle, layout, display-list update per dirty surface; phase 2
      (parallel): `paint_pixel_regions` + SHM copy per surface via rayon
      scope. The painter's text/glyph/gradient caches are already
      `thread_local` (`painter/backend.rs:29`, `text.rs:28-48`), so worker
      threads get their own — verify cache hit rates don't crater with a
      pinned worker-per-surface mapping. Bar + popover + launcher painting
      concurrently roughly divides paint latency by the surface count.
- [ ] **Pipeline paint against the next frame's script work.** Even with one
      surface, phase 2 for frame N can overlap phase 1 of frame N+1 (double-
      buffer the `PixelBuffer`, hand the display list snapshot to a render
      thread, present from there). This is the classic guarded-render-loop
      design; it halves effective frame latency for rebuild-heavy frames.
      Bigger lift than per-surface parallelism — do that first.
- [ ] **Tile-parallel raster for large damage.** Within one buffer, split
      full-surface repaints (theme change, first paint, launcher open) into
      horizontal bands painted in parallel (disjoint `&mut [u8]` slices via
      `split_at_mut`; each band gets its own Skia canvas with a band clip).
      Only worth it above a damage-area threshold; measure with the v1.21
      profiles first.
- [ ] **Move blocking file IO off the shell thread.** `load_graph_i18n_catalogs`
      does `fs::read_to_string` per catalog on mount (`component/runtime.rs:136-171`),
      settings/theme reloads re-read files inline in the loop, and icon/SVG
      cache _misses_ rasterize on the paint path. Route one-shot IO through
      `spawn_blocking` with a completion event (the loop already wakes on
      eventfd), and make icon-cache misses paint a placeholder frame and
      fill in on the next wake instead of stalling the frame.
### L. Live performance debugging — design

Goal: see hotspots _live_ while interacting with the shell, with cause
attribution (which rule, which component, which invalidation), without the
measurement tool perturbing what it measures. Builds on what already exists:
`ProfilingStage` accumulators + `ProfilingSnapshot` (`runtime/profiling.rs`),
`ProfilingInvalidationSnapshot` (per-paint rebuild/retained/narrow/damage
counts), the `DebugOverlay` painter, `mesh.debug.*` IPC, and the
debug-inspector's profiling start/stop. Tiered by effort:

- [ ] **Tier 1 — in-shell perf HUD painted by the renderer, not a module.**
      A HUD that is itself a `.mesh` surface would pollute the numbers with
      its own rebuild/restyle cycle at every update. Instead extend the
      existing `DebugOverlay` (which already paints layout bounds directly
      into the buffer post-paint, `frontend/render/src/surface/debug_overlay.rs`)
      with a profiling mode, toggled by the existing `CoreRequest` debug
      path: - **frame waterfall strip**: last ~120 frames as stacked bars (script /
      build / restyle / layout / display-list / paint / SHM / present),
      color-coded, 16.6 ms budget line — the data is already in
      `ProfilingSurfaceSnapshot.recent_samples`, it just needs a ring
      buffer keyed by frame rather than by stage; - **live counters**: FPS, presents vs skipped, damage area % of
      surface, retained-path vs full-rebuild ratio, narrow-path hits — all
      already in `ProfilingInvalidationSnapshot`, currently only visible in
      the inspector module; - **paint flashing** (the Chrome/KWin repaint debugger): translucent
      colored overlay on each frame's damage rects, decaying over ~300 ms.
      This makes "we repainted the whole bar for a clock tick" _visible_
      instantly, and is the single best tool for the repaint-suppression
      work in K. Trivial to add: the damage rects are already in
      `last_present_damage_rects` when the overlay paints.
      HUD paint cost must be excluded from the recorded stages (paint it
      after `PaintTraversal` is recorded) and its damage must not feed back
      into the damage stats (flag its rects).
- [ ] **Tier 2 — cause attribution (top-N tables).** Stages say _what phase_
      is slow; attribution says _why_: - per-style-rule cumulative restyle time + match count (time
      `apply_declaration` per rule id in the cached index; report top 10
      selectors); - per-component-instance build time (wrap `render_import`/embedded
      instance eval — directly measures the memoization win in I); - per-node paint time bucketed by command kind (text/shadow/blur/
      gradient/icon) — the painter already returns `PaintMetrics` with
      shaping/raster micros, extend to per-kind totals; - wasted-work counters: rebuilds whose retained diff was empty,
      restyles with zero changed styles, service deliveries whose payload
      was identical (K), motion events coalesced vs dispatched (J).
      Surface these in the HUD's second page and in the IPC snapshot.
- [ ] **Tier 3 — streaming + offline analysis.** - `mesh.debug.profiling_stream`: push per-frame profiling records over
      the existing IPC bus so an external `mesh-tools-cli perf top`
      TUI can show live tables without any in-shell UI (and without the
      HUD's paint cost); - Chrome-trace/Perfetto JSON export of a captured window (the
      `ProfilingSample` ring buffers already hold timestamps+durations) for
      offline flamegraph comparison before/after each A–K fix; - wire the existing `DebugBenchmarkSnapshot`/`BenchmarkScenarioSnapshot`
      types to the canonical-workload profiles item (v1.21): scripted
      scenarios (idle 10 s, pointer sweep, slider drag, popover open/close,
      theme switch) that run headless and emit a JSON summary — this is the
      regression harness that keeps the wins from A–K from rotting.
      Compare runs in CI against a stored baseline with a tolerance band.

### M. Component composition & template evaluation — 2026-07-04 deep dive

Focused trace of compile → `build_tree_with_state` → `build_widget_node` →
`FrontendCompositionResolver::render_import` → finalize (see
`PERFORMANCE_SECTIONS.md` §1 for the section map). New findings not covered by
passes A–L; `file:line` as of this scan.

Performance:

- [x] **Full layout per embedded instance per rebuild.** `build_tree_with_state`
      always ends with `LayoutEngine::compute_with_measurer`
      (`frontend/compiler/src/lib.rs:203`) and `render_embedded_instance` calls
      it per embedded module instance mid-build; `finalize_tree` then re-lays-out
      the whole tree (`shell/component/rendering.rs:460`). Embedded subtrees get
      ≥3 layout passes per rebuild (+1 per nesting level). Verify nothing reads
      `node.layout` between build and finalize, then skip the build-time layout
      for `FrontendRenderMode::Embedded` (and likely the surface build too).
      Progress 2026-07-09: embedded builds now defer layout until the composed
      surface finalization pass; standalone surface/preview builds retain their
      existing eager layout contract. Added regression coverage for both modes
      and a release-only benchmark: a 513-node embedded tree over 200 builds
      measured 2.757s with eager layout versus 2.447s deferred (~1.13x faster).
- [x] **`{#for}` deep-clones the whole items array every rebuild.**
      `store.get(&for_node.iterable)` (`frontend/compiler/src/render.rs:429`)
      uses owned `get` although borrowed `get_ref` exists and is already used by
      `eval_path`. Switch to `get_ref`; trivial diff. Completed 2026-07-06:
      `{#for}` now borrows iterable arrays through `VariableStore::get_ref`
      when available, keeps an owned fallback for stores that only implement
      `get`, and stores loop items by reference in `LayeredStore`. Added
      regression coverage for the no-owned-root-clone path plus fallback
      coverage for owned-only stores. Release-only benchmark on a clone-heavy
      1k-item array showed borrowed iteration ~1.2x faster locally
      (5.12s -> 4.25s for 80 rebuilds); a small-payload full-render benchmark
      was layout/tree-build dominated and did not show a win, so this is
      specifically an allocation/clone-heavy iterable improvement.
- [x] **Post-hoc full-subtree walks per embedded instance.**
      `namespace_event_handlers` re-`format!`s every handler string on every
      rebuild (`ui/interaction/src/hit_test.rs:359`) even though
      `build_widget_node` already receives `instance_key` — namespace during
      `parse_attributes` instead and the walk disappears.
      `apply_prop_handler_calls` clones each node's whole `event_handlers` map
      and does an O(handlers × props) scan per node
      (`shell/component/composition.rs:213-239`).
      Progress 2026-07-09: imported and local embedded component handlers are
      now namespaced during attribute construction, removing both recursive
      post-build walks while preserving generic preview builds and prop-handler
      linkage. A 512-handler release benchmark over 2,000 rebuild-shaped tree
      clones measured 792.876ms with the post-build walk versus 697.654ms with
      inline namespacing (~1.14x faster). The `apply_prop_handler_calls`
      matching pass now builds one handler-value index per embedded subtree
      instead of repeating an O(handlers × props) scan at every node, while
      preserving first-prop-wins behavior for duplicate bindings. With 16
      handler props and 64 child nodes, 20k release iterations measured
      942.132ms repeated scan versus 524.108ms indexed (~1.8x faster). The
      dominant single-prop case bypasses hash-index construction and compares
      against one resolved handler directly; 50k release iterations over 64
      children measured 1.238s repeated map scans versus 1.159s specialized
      (~1.07x faster).
- [ ] **Per-rebuild prop churn.** `ensure_runtime`/`ensure_local_component_runtime`
      re-`set` every prop into script state per instance per rebuild with 2–3
      runtimes-mutex acquisitions (`shell/component/runtime.rs:408-415`);
      `render_import` rebuilds `props_json` maps and `format!`s instance keys per
      frame (`composition.rs:25-38,90-98`); host+component style-rule slices are
      re-cloned into a merged `Vec` per instance per rebuild
      (`render.rs:266-278`) — cacheable per (host, alias). Progress
      2026-07-06: local-component rendering now threads the already-resolved
      `ComponentFile` and host style-rule slice from `render_import` into
      `render_local_component`/`ensure_local_component_runtime`, removing the
      duplicate host-module and local-component catalog lookups from each local
      component rebuild. Prop map construction, prop state writes, and style-rule
      merge caching remain open. Progress 2026-07-09: embedded and local
      component prop synchronization now skips unchanged public-member writes,
      avoiding JSON-to-Lua conversion, `_ENV` mutation, and module-object
      rebuilding on steady-state parent rebuilds. A release benchmark over
      100k unchanged structured prop writes measured 119.964ms eager versus
      12.451ms equality-gated (~9.6x faster). Prop map construction and
      style-rule merge caching remain open. Progress 2026-07-09: CSS prop
      projection now borrows the runtime's `props` object and only falls back
      to an owned lookup for non-lending stores/live proxies, avoiding a deep
      clone of unrelated instance props per component build. With one declared
      prop in a 129-entry structured props object, 10k release projections
      measured 208.842ms owned versus 1.485ms borrowed (~140.6x faster).
      Progress 2026-07-10: imported/local component prop-map construction now
      uses a shared helper that pre-counts public props and allocates the
      runtime `HashMap` at the right capacity while consistently filtering
      internal binding channels. A release benchmark over 100k rebuild-shaped
      64-prop maps measured 591.288ms for filtered `collect` versus 429.191ms
      for the pre-sized helper (1.4x faster). Prop state writes and style-rule
      merge caching remain open.
- [ ] **Per-node build allocations.** `attach_module_id` inserts a fresh
      `_mesh_module_id` String on every node; `TrackingVariableStore` pushes two
      fresh Strings per dotted read per node; `resolve_event_handler_value` does
      an owned `store.get` per handler attribute. Folds into v1.23 interning but
      listed because composition keeps adding string attributes.
      Progress 2026-07-09: event-handler resolution now prefers
      `VariableStore::get_ref` and falls back to owned `get` only for stores
      that cannot lend a value (such as live proxies). A release benchmark over
      1M handler lookups measured 16.593ms with owned JSON cloning versus
      8.630ms borrowed (~1.9x faster). Consecutive duplicate service-field
      reads within one node are now coalesced before allocating service/field
      strings; 1M repeated reads measured 93.496ms eager versus 11.050ms
      coalesced (~8.5x faster). Progress 2026-07-09: non-consecutive duplicate
      service-field reads within one node are also coalesced before allocation,
      so interleaved expressions like `audio.percent`, `network.ssid`,
      `audio.percent` publish two dependencies instead of three. A release
      benchmark over 250k node-shaped read batches measured 22.424ms
      consecutive-only versus 21.995ms duplicate-scan (~1.0x faster) while
      reducing dependency entries from 1,000,000 to 750,000. Progress
      2026-07-12: `WidgetNode` now carries a typed `module_id` field with
      legacy `_mesh_module_id` attribute fallback, and frontend build/style
      resolution paths use the typed field for normal trees. This removes the
      fresh `_mesh_module_id` attribute insertion per built node while
      preserving hand-built test/tooling compatibility. A release benchmark
      over 500k build-shaped node clones with existing attributes measured
      152.730ms for attribute-map insertion versus 141.105ms for typed-field
      assignment (1.1x faster), before downstream savings from the smaller
      attribute map. Unique tracked-read string allocations remain open.

Structure / correctness:

- [ ] **`and`/`or` template expressions diverge from Lua semantics.**
      `eval_compiled` returns literal `"true"`/`"false"` for `And`/`Or`
      (`frontend/compiler/src/expr.rs:193-204`) instead of the operand values —
      `{name or "Anonymous"}` renders `true`/`false`; only the exact
      `cond and a or b` ternary shape is special-cased to work. Also
      `is_truthy` treats `"0"`/`""` as falsy (Lua does not), and
      `a or b and c` parses with inverted precedence (`and` split before `or`).
      Fix as part of the typed expression-value enum (section A, "stringly-typed
      template expression values") — that item is now correctness work, not just
      an optimization. Doc example using unsupported C-style `?:` fixed in
      `docs/frontend/mesh-syntax.md` 2026-07-04.
- [ ] **Build is not a pure function — prerequisite for render memoization.**
      `render_import` mutates shell state during build via RefCells
      (`pending_surface_states`, `portal_hidden_bindings`,
      `has_promoted_popover_wrappers`, live `bind:this` installation —
      `composition.rs:74-131`). Component-level memoization (section I) would
      silently skip these side effects when serving a cached subtree; make them
      explicit build outputs (a `BuildEffects` struct the caller applies) first.
- [ ] **Typed handler-call linkage matches by value equality.**
      `apply_prop_handler_calls` maps typed args onto child handlers by
      comparing resolved handler *values* to prop values
      (`composition.rs:221-235`); two props bound to the same handler name get
      the wrong args. Link by prop name through the child build instead.
- [x] **Remove the legacy JSON handler-descriptor path.** `unpack_handler_args`
      still parses `{"h":…,"a":…}` strings (`shell/component/runtime.rs:644-664`)
      after typed `EventHandlerCall` landed (section G). Per the
      no-backward-compat project rule, verify nothing produces them and delete.
      Completed 2026-07-06: verified the compiler/shell emit typed
      `EventHandlerCall`s and removed JSON descriptor parsing from dispatch;
      `unpack_handler_args` now always borrows the handler name and event args.
      Release benchmark over 200,000 pre-bound calls measured 95.353ms for the
      old JSON descriptor unpack versus 27.112ms for typed handler-call args
      (3.5x faster).
- [ ] **`{#if}`/`{#for}` always wrap children in a synthetic `column` node**
      (`render.rs:394,423`) — one extra node per conditional/loop paying layout,
      style, hash, and paint, and it forces column flow inside row parents.
      Needs a fragment/transparent-container concept.
- [ ] **No keyed list diffing.** `{#for}` identity is positional (`_mesh_key`
      paths), so any reorder/insert re-styles and re-hashes every following row.
      Add a `key=` attribute; pairs naturally with component memoization
      (section I) and the retained-tree diff work (v1.27).
- [ ] **Magic-string protocol at the composition boundary.**
      `__mesh_embed__::`, `__mesh_binding_*`, `__mesh_bind_this`,
      `_mesh_module_id`, the promoted-popover marker — stringly-typed channels
      between compiler and shell causing prefix parsing and false attribute
      dirtiness. The composition-boundary instance of v1.23 typed fields.
- [x] **Verify dynamic `class="{expr}"` bindings participate in build-time style
      resolution.** Completed 2026-07-09: build-time style matching now derives
      selector class/id identity from resolved dynamic attributes before calling
      `resolve_node_style_for_module_indexed`, so dynamic class styles are correct
      in the initial tree instead of depending on a corrective finalize restyle.
      Added `dynamic_class_participates_in_initial_style_resolution`.
- [x] Minor: `render_import`'s local-component branch does its catalog lookups
      twice (gate in `composition.rs:22-23`, again inside
      `render_local_component`, `runtime.rs:435-440`). Completed 2026-07-10:
      the local branch now threads the already-resolved `ComponentFile`,
      host manifest, and host style-rule slice into `render_local_component`,
      so the duplicate catalog lookup path is gone.

### N. Retained tree, render objects & display list — 2026-07-04 deep dive

Focused trace of annotate → `RetainedWidgetTree::update` → `RenderObjectTree`
→ `RetainedDisplayList` → damage (see `PERFORMANCE_SECTIONS.md` §2). New
findings beyond the D/I/J items; `file:line` as of this scan.

Performance:

- [x] **`ordered_entries` is built per display-list rebuild but consumed only in
      debug builds.** `collect_display_entries` pushes every `(key, entry)` pair
      into a Vec (`render/src/display_list.rs:770-774`) whose sole consumer is
      `compute_batch_metrics` behind `#[cfg(debug_assertions)]`
      (`display_list.rs:891-894`). Release builds pay a full per-entry Vec push
      every rebuild frame for nothing. Gate the collection itself (pass
      `Option<&mut Vec<_>>` or a debug-only sink). Free win.
      Completed 2026-07-05: release builds now compile out the ordered-entry
      scratch buffer and pass no debug sink during entry collection. Two
      release runs over 9.842 million collected entries measured 2.943s versus
      2.881s and 2.941s versus 2.930s (0.4-2.1% faster), with identical damage
      map entry counts.
- [x] **`RenderObjectTree` allocates per node per dirty frame.** `text_slot`
      clones the text `content` String (`render/src/render_object.rs:307`),
      `accessibility_slot` clones the label, `child_id_slot` allocates a fresh
      `Vec<NodeId>` per node (`render_object.rs:263-271`; the retained tree
      already uses an inline `SmallVec` for the same data), `geometry_slot`
      string-parses six `_mesh_scroll_*`/`_mesh_content_*` attributes per node
      (`render_object.rs:296-301`), and `update_inner` allocates two fresh
      `HashSet`s per update (`render_object.rs:97-98`) instead of scratch-reuse.
      This file predates the D-item optimizations and never got them.
      Progress 2026-07-05: geometry now consumes typed scroll metrics (34.5x
      faster than six string parses), dirty-node storage is scratch-reused, and
      text content, accessibility labels, and custom roles reuse retained
      `Arc<str>` allocations when unchanged. The retained-string benchmark
      measured 35.148ms for `String` clones versus 15.648ms for retained arcs
      over 5 million values (2.2x faster). An eight-entry `SmallVec` child-id
      experiment was rejected because it regressed the measured six-child case
      from 28.448ms to 37.879ms. Reusing the retained `Vec` allocation instead
      measured 54.663ms for fresh vectors versus 13.167ms for clear/refill over
      5 million six-child updates (4.2x faster), while preserving arbitrary
      child counts and reorder detection. All listed allocation sources are now
      addressed.
- [ ] **Triple full-tree fingerprinting on every dirty frame.** Three parallel
      diff systems each walk the whole tree and hash/compare overlapping data:
      `RetainedWidgetTree` snapshots (layout/style/attrs/children/state,
      `runtime_tree.rs:102-170`), `RenderObjectTree` paint-data slots
      (transform/clip/geometry/material/primitive/text, `render_object.rs:90-124`),
      and `RetainedDisplayList` per-(node, slot) entry signatures — which
      `collect_display_entries` recomputes for **every** node on any dirty frame
      (`display_list.rs:1384-1433`) even when the dirty set names one node.
      The retained-tree generation gates the clean-frame case only. Unify:
      make `RetainedWidgetTree` the single fingerprint pass and have the render
      object tree and display entries consume its per-node dirty flags,
      re-signing entries only inside dirty subtrees (plus scrolled/moved
      ancestors). This is the §2 complement of the v1.27 generation-aware diff.
      Progress 2026-07-05: display-list batch signatures now hash only the
      material fields relevant to each primitive slot, and entries that already
      carry a batch barrier skip batch-signature hashing entirely because the
      metric never compares them. A 512-node release benchmark over 50k
      background-slot signature passes measured 804.926ms for the previous broad
      material hash versus 69.806ms for the slot-aware hash (11.5x faster).
- [x] **Reused paint subtrees are cloned twice per clean node.**
      `build_paint_subtree`'s reuse path does `previous.clone()` then
      `next_subtrees.insert(id, reused.clone())`
      (`display_list.rs:1488-1491`) — Arc bumps plus span/kind vec copies for
      every clean node on every incremental rebuild. Insert once and return a
      cheap handle/index instead.
      Completed 2026-07-05: retained subtree maps now own whole-subtree `Arc`
      handles, so clean-node reuse clones one handle rather than cloning each
      shared command/kind/order field plus metadata. A release benchmark over
      10 million reuse clones measured 180.455ms fieldwise versus 28.002ms for
      the whole-subtree handle (6.4x faster).
- [x] **Two more full passes per display-list rebuild.**
      `build_command_spans(root, &subtrees)` walks the tree and
      `count_effect_overflow_commands` scans all commands
      (`display_list.rs:895-896`) on every rebuild; both derivable
      incrementally from the subtree reuse bookkeeping.
      Progress 2026-07-05: effect-overflow counts are now accumulated while
      building each retained subtree, composed from cached child counts, and
      reused directly on unchanged generations. A release benchmark over
      20,000 metric queries with nonzero effects measured 746.477ms for the
      former command scan versus 9.499us for retained reads, with identical
      totals. Command spans are now composed as subtree-relative metadata and
      exposed from the root through a shared handle, preserving exact span
      ordering while removing the second tree walk. A release benchmark over
      10,000 assemblies measured 608.869ms for traversal versus 36.806us for
      retained root handles, with identical 25.21 million aggregate spans.
- [x] **Scroll state round-trips float→string→float three times per node per
      frame.** Written as `"{:.2}"` strings in `annotate_runtime_tree`
      (`runtime_tree.rs:819-832`), re-parsed in `collect_display_entries`
      (`display_list.rs:1417-1426`), `build_paint_node` scrollbars (six
      `attr_f32` calls), and `geometry_slot` (six more). Also quantizes offsets
      to hundredths. The concrete §2 instance of the v1.23/v1.27 typed
      `WidgetNode` fields item.
      Completed 2026-07-05: `WidgetNode` now carries optional typed scroll
      metrics used by runtime annotation, overflow measurement, retained-tree
      invalidation, hit testing, render objects, display-list construction, and
      both painters. Legacy attributes remain a fallback for hand-built nodes.
      A release geometry benchmark over 2 million snapshots measured 186.504ms
      for six string parses versus 5.407ms for typed fields (34.5x faster), and
      offsets are no longer quantized to hundredths.
- [x] **Handler-call args re-serialize to JSON strings per fingerprint.**
      `attributes_fingerprint` does `arg.to_string()` per pre-bound arg per
      node per frame (`runtime_tree.rs:479`). Hash the `serde_json::Value`
      structurally instead.
      Completed 2026-07-05: typed handler arguments are hashed recursively by
      JSON variant and primitive value without allocating serialized strings.
      The release benchmark measured 415.585ms for `to_string` hashing versus
      94.309ms for structural hashing (4.4x faster).

Structure:

- [x] **The primitive-aware hasher improvement never reached the render crate.**
      `RuntimeTreeHasher` got word-at-a-time `write_*` methods (D, 1.9x), but
      `DisplaySignatureHasher` (`display_list.rs:1305-1325`) and
      `RenderObjectHasher` (`render_object.rs:51-70`) are still byte-at-a-time
      FNV copies. Either port the primitive methods or — better — share one
      hasher type; three hand-rolled FNV implementations is the maintenance
      smell that let this drift.
      Completed 2026-07-05: both render hashers now mix primitive values in one
      operation while preserving byte-wise hashing for strings and slices. A
      release benchmark over a representative primitive field mix measured
      3.072ms for the byte fallback versus 2.232ms word-at-a-time (1.4x
      faster across 5 million iterations).
- [x] **No `NodeId` collision detection.** Runtime ids are FNV/chained hashes
      of key paths (`runtime_tree.rs:346-365`) used as identity keys by all
      three retained systems and the display-list keys; a collision silently
      aliases two nodes (wrong reuse, wrong damage) with no diagnostic. Add a
      debug-build assertion where `node_keys` is populated.
      Completed 2026-07-09: retained snapshot collection now asserts in debug
      builds when two nodes produce the same `NodeId`, with regression coverage
      for duplicate-id detection. Release builds keep the previous zero-cost
      insert path.
- [x] **Identity travels as a string attribute.** `annotate_runtime_tree`
      writes `_mesh_key` into `attributes` (`runtime_tree.rs:711`) purely so
      interaction/refs/metrics can read identity back out of a string map,
      which in turn forced the `_mesh_key` hash-exclusion special case in
      `attributes_fingerprint`. Typed field on `WidgetNode` (v1.23) retires
      both.
      Completed 2026-07-12: `WidgetNode` now carries a typed `mesh_key`
      field with legacy `_mesh_key` attribute fallback for hand-built tests and
      tooling. Runtime annotation writes only the typed field, retained-tree
      fingerprints no longer special-case `_mesh_key`, and interaction,
      layout, style, animation, metrics/ref publication, keybind, widget
      navigation, and child-surface request paths read identity through
      `WidgetNode::mesh_key()`. A release microbenchmark over 500k assignments
      measured 48.154ms for `_mesh_key` BTreeMap insertion versus 37.245ms for
      typed field assignment (1.3x faster), before counting the downstream
      fingerprint/map-scan savings from the smaller attribute map.
- [ ] Minor: display-list `update_inner` is ~220 lines mixing diff, damage,
      and a ~30-field metrics struct assembly (`display_list.rs:742-961`);
      split when next touched.

### N addendum — 2026-07-04 second pass (display-list subtree internals)

- [ ] **Every rebuilt ancestor copies its entire descendant command list.**
      `PaintSubtreeBuilder::append_child` does
      `extend_from_slice(&child_subtree.commands)`
      (`display_list.rs:586-600`), so each ancestor's flat buffer holds copies
      of all descendant `DisplayPaintCommand`s, and `next_subtrees` retains a
      full flattened copy per node — O(n × depth) command storage and re-copy
      on every ancestor rebuild. This is the retained-memory face of the v1.21
      segment/rope item; fixing v1.21 should make per-node subtrees hold spans
      into shared storage, not owned flattened copies.
- [ ] **A dirty node rebuilds its entire subtree's paint segments.**
      `build_paint_subtree` passes `force_rebuild || node_is_dirty` down to all
      children (`display_list.rs:1563`), so a style-only change on a container
      (hover background) rebuilds every descendant's commands even though
      their geometry and content are unchanged. Only the dirty node's own
      commands need rebuilding when its layout/scroll/clip didn't change;
      children could be re-appended from the previous subtree. Progress
      2026-07-10: paint-only dirty parents now allow clean descendants to
      reuse retained subtrees, while layout/clip/transform/reorder dirty
      parents still force descendant rebuilds. Added regression coverage for
      both paths and a release-only benchmark; the local dev-shell run measured
      564.325ms for forced descendant rebuilds versus 226.123ms with clean
      descendant reuse over 1k paint-only dirty-parent rebuilds (2.5x faster),
      with rebuilt commands dropping from 514k to 2k.
- [x] **`DisplayPaintCommand` embeds a full cloned `DisplayPaintNode` per
      command.** `paint_node.clone()` per Node command
      (`display_list.rs:1524`), with the same node reused for the Scrollbars
      command — each clone copies text/placeholder Strings and the style
      block. Share via `Arc<DisplayPaintNode>` per node with per-command kind.
      Progress 2026-07-12: `DisplayPaintCommand` now shares
      `Arc<DisplayPaintNode>` handles, so node and scrollbar commands for the
      same widget reuse one paint-node payload and retained command-buffer
      clones copy an Arc handle instead of text/style payloads. The in-crate
      release benchmark is present, but this environment cannot link Skia test
      binaries because `freetype`/`fontconfig` are missing; a standalone
      release benchmark over 4M two-command clones measured 548.859ms for
      owned node clones versus 83.270ms for Arc-backed commands (6.6x faster).

### O. Style system & theming — 2026-07-04 deep dive

Focused trace of CSS parse → `StyleRuleIndex` → `StyleResolver` →
`ComputedStyle` (build, restyle, and diagnostics paths) plus theme defaults.
See `PERFORMANCE_SECTIONS.md` §3. `file:line` as of this scan.

Performance:

- [x] **Hidden second full restyle with per-node index construction on every
      rebuild frame.** `record_runtime_style_diagnostics` runs whenever a
      diagnostics sink is attached — which is always in production
      (`shell_component.rs:60`) — on every `"rebuild"`-trigger finalize
      (`rendering.rs:429-431`). It walks the whole tree re-resolving every
      node through the diagnostics path, which builds `StyleRuleIndex::new(rules)`
      **per node** (`resolve.rs:546`) — the exact O(nodes × rules) pattern the
      E-item fixed on the build path — plus a fresh `Vec<String>` classes clone
      and a fresh variables HashMap per node (`rendering.rs:584-590`,
      `resolve.rs:614`). Rebuild frames are the most common invalidation class
      (every service update / handler write). Fix in stages: thread the cached
      index through the diagnostics path; gate the pass on (rules generation,
      tree-structure generation) instead of every rebuild; long-term validate
      declarations once per rule at compile time and delete the runtime pass.
      Completed first stage 2026-07-06: added an indexed diagnostics resolver
      and threaded the existing cached `StyleRuleIndex` through the shell
      runtime style diagnostics walk, removing per-node index construction on
      rebuild diagnostics. Added parity coverage for indexed vs uncached
      diagnostics. Release-only microbenchmark showed cached-index diagnostics
      ~2.4x faster locally (652.3ms -> 270.3ms for 20k diagnostic resolutions
      over 80 rules). Remaining follow-up: gate diagnostics by rules/tree
      generation or move unsupported CSS validation fully static.
- [x] **Per-declaration static validation re-runs per node per pass.**
      `apply_declaration_no_diagnostics` runs `style_profile_status`,
      `is_supported_css_property`, `contains_deprecated_token_reference` (a
      string scan of the value), and `is_strict_animation_property` for every
      declaration of every matched rule on every node on every restyle
      (`resolve.rs:916-950`). All are pure functions of the declaration;
      precompute them once per rule into a validated/compiled declaration at
      rule-build time. Cheap first step toward the v1.23 typed-declarations
      item. Progress 2026-07-12: diagnostics resolution now consumes the same
      indexed declaration metadata as the no-diagnostics restyle path,
      preserving diagnostic messages while avoiding static property
      reclassification and per-declaration selector-string allocation on each
      matched node. Added parity coverage for unsupported properties and
      missing variables. Release benchmark over 200k diagnostic declaration
      applications measured 138.902ms for reclassification versus 76.267ms
      with indexed metadata (1.8x faster). Follow-up 2026-07-12:
      `StyleRuleIndex` now also precomputes selector diagnostic strings, so
      the diagnostics walk no longer formats compound selectors for every
      matched node. Release benchmark over 500k compound-selector lookups
      measured 45.176ms per-node formatting versus 214.902us from indexed
      strings (210.2x faster).
      Completed 2026-07-13: the remaining theme-default diagnostics and
      no-diagnostics paths now also apply through `IndexedDeclaration`
      metadata, and indexed style resolution uses one diagnostics-optional
      loop. Production restyle/default application no longer re-runs static
      declaration classification per matched node; the old uncached helper is
      retained only for direct declaration diagnostics/tests.
- [x] **`seed_module_theme_variables` allocates two Strings per module token
      per node per pass** — `format!("--{}", name.replace('.', "-"))`
      (`resolve.rs:857-876`). Precompute the CSS-variable-keyed token map once
      per theme load per module and seed by reference.
      Completed 2026-07-05: each `StyleResolver` now caches normalized
      CSS-variable keys and converted token values per module, preserving
      first-writer variable precedence. A release benchmark over 3.2 million
      token insertions measured 237.080ms for per-node normalization versus
      132.686ms from cached entries (1.8x faster).
- [x] **`seed_prop_variables` clones every prop key+value per node**
      (`resolve.rs:599-603`) even though props are per-instance constants for
      the whole pass. Seed once per pass or resolve through a layered lookup
      (props map consulted after scratch) instead of copying.
      Completed 2026-07-09: variable resolution now consults per-node scratch
      first and the resolver's immutable prop map second, preserving local
      custom-property override semantics without seeding clones. Embedded
      `var()`/`prop()` substitution uses the same layered lookup. With 32 props,
      200k release resolutions measured 582.988ms cloning per-node seeds versus
      20.304ms layered (~28.7x faster).
- [x] **`theme_reference_to_token_name` allocates and canonicalizes per
      `var()` reference per declaration per node** (`resolve.rs:1916-1922` +
      `css_custom_property_to_token_name` prefix tables). Double-key theme
      tokens by their CSS custom-property name at theme load, or intern the
      mapping, so hot lookups are a single hash probe.
      Progress 2026-07-09: `StyleResolver` now interns canonical token names
      per reference as `Arc<str>` and the simple string/color/number plus
      diagnostics paths reuse them. A release benchmark over 1M
      `--mesh-color-primary` mappings measured 49.820ms recanonicalizing versus
      15.449ms cached (~3.2x faster). Embedded multi-reference substitution
      now also routes through the resolver cache and the duplicate standalone
      helper was removed. A release benchmark over 300k embedded substitutions
      measured 96.285ms recanonicalizing versus 86.376ms cached (~1.1x faster).
      Follow-up 2026-07-12: `StyleResolver` now also caches resolved theme token
      values, so repeated global-token `var(--...)` paths skip both canonical
      name lookup and `Theme::token` lookup after the first hit. A release
      benchmark over 1M `--color-primary` lookups measured 33.179ms for
      cached-name + theme lookup versus 18.984ms from cached token values
      (1.7x faster). The corrected embedded-reference benchmark using real
      `--color-*` variables measured 202.594ms recanonicalizing versus
      158.496ms cached (1.3x faster). A theme-load double-keyed token map is no
      longer needed for the resolver hot path, but may still be useful if other
      call sites begin resolving CSS-variable token names directly.
- [x] Confirmed mechanism for the existing "pre-bake per-tag prototypes" item:
      `apply_theme_defaults_map_no_diagnostics` re-clones each default's
      property String and re-classifies its value per node per pass
      (`resolve.rs:901-914`), for "base" + tag + module-base + module-tag maps.
      Completed 2026-07-12: diagnostics style resolution now has the same
      per-tag/module theme-default prototype cache as the no-diagnostics path,
      including cached default diagnostics and seeded custom variables. A
      release benchmark over 200k diagnostic default resolutions measured
      1.304s replaying theme string maps versus 29.778ms from cached prototypes
      (43.8x faster).

Structure / correctness:

- [x] **Theme component defaults apply in nondeterministic order.**
      `ComponentDefaults = HashMap<String, String>`
      (`foundation/theme/src/lib.rs:12`) and `apply_theme_component_defaults`
      iterates it per node. A theme declaring an overlapping shorthand +
      longhand pair (e.g. `background` and `background-color`) on the same
      component resolves in random order per process run, and theme-CSS source
      declaration order is lost entirely at parse. Store defaults as an
      ordered `Vec<(String, String)>` preserving source order (CSS last-wins).
      Completed 2026-07-12: `ComponentDefaults` is now an ordered declaration
      collection with serde support for the existing map-shaped JSON/CSS theme
      format. CSS parsing preserves declaration order, duplicate properties
      move to their latest declaration position, and component-default
      iteration now applies authored CSS order deterministically.
- [x] **The diagnostics/no-diagnostics path duplication caused the drift.**
      Four near-identical function pairs (`resolve_node_style_with_attrs*`,
      `apply_theme_defaults_map*`, `apply_declaration_*`) exist so the
      diagnostics path could stay separate; that duplication is exactly where
      the per-node index rebuild survived. When fixing the first item, fold
      diagnostics into a sink parameter (`Option<&mut Vec<StyleDiagnostic>>`)
      on one path so the two cannot diverge again. Completed 2026-07-13:
      indexed style resolution now runs through one inner
      diagnostics-optional loop, indexed declaration application uses one
      optional diagnostics sink, and theme component-default application uses
      the same indexed declaration path for diagnostics and no-diagnostics
      prototype caches. The remaining uncached declaration-with-diagnostics
      helper is outside the production indexed restyle/default path. Verified
      with `cargo test -p mesh-core-elements indexed` and
      `cargo test -p mesh-core-elements theme_defaults`.
- [x] Design note (fine, but document): selector matching has no CSS
      specificity — candidate rules apply in source-index order (last wins),
      and descendant combinators are rejected at parse with a diagnostic
      (`ui/component/src/style.rs:100`). Worth one paragraph in
      `docs/spec/04-styling.md` so authors don't expect specificity semantics.
      Completed 2026-07-13: `docs/spec/04-styling.md` §6.1 already documents
      the supported selector subset, compile-time rejection of descendant/child
      combinators, and the no-specificity source-order cascade contract.

### P. Rendering & paint — 2026-07-04 deep dive

Focused trace of `paint()` → damage assembly → `paint_pixel_regions` →
display-list replay → Skia session → text/glyph/icon caches → buffer. See
`PERFORMANCE_SECTIONS.md` §4. `file:line` as of this scan.

Performance:

- [x] **File-backed icon draws stat() the filesystem every paint, even on
      cache hits.** Every draw computes `raster_file_key` → `file_freshness`
      → `std::fs::metadata` (`render/src/surface/icon.rs:134-145,179-190`),
      and SVG sources add a second freshness check via `svg_file_cacheability`
      (`icon.rs:211`). Freshness is part of the raster cache key, so a hit
      still pays the syscall; `cached_file_resource_opacity` (opaque-region
      derivation) stats again per present (`icon.rs:297-331`). A bar with ~10
      file icons at 60 Hz is 600–1800 blocking syscalls/s on the paint path,
      and a slow filesystem stalls the frame. Fix: TTL the freshness probe
      (re-stat at most every ~1s) or make invalidation event-driven through
      the shell's existing inotify hot-reload watcher, so steady-state paints
      do zero filesystem calls. Named-icon *font glyph* draws are unaffected
      (glyph caches key by path hash + axes). Completed 2026-07-06: file
      freshness probes now use a 1s TTL-backed LRU shared by image loading,
      raster-cache keys, SVG cacheability checks, and cached opacity lookups,
      so steady-state file-backed icon paints reuse the cached `(len, mtime)`
      instead of calling `metadata` per draw. Added focused regression coverage
      proving repeated raster keys reuse one freshness probe inside the TTL and
      still re-stat after expiry. Release-only benchmark over 50k raster keys
      measured 69.051ms with per-draw `metadata` versus 7.274ms with the TTL
      cache (9.5x faster, one real probe).
- [ ] **Child popup surfaces bypass the whole retained pipeline.**
      `paint_child_surface` (`shell/component/shell_component.rs:992-1027`)
      clears the entire child buffer and repaints the popover subtree through
      the immediate-mode `paint_frontend_tree_at_for_module` on every present,
      plus two full-tree walks (`find_node_by_key`, `find_node_bounds_by_key`)
      per child per frame. An open hover menu or quick-settings popover
      full-repaints at frame rate with no display list, no damage, no partial
      present. Route child targets through the same retained display-list +
      damage path as the parent (subtree-scoped), which also deletes the
      duplicate immediate-mode painter (structure item below).
- [ ] **Any non-clean frame bypasses all generation shortcuts.**
      `use_generation_shortcuts` requires `dirty_types.is_empty()`
      (`shell_component.rs:529-537,560-581`), so every interaction/animation/
      script frame runs `RenderObjectTree::update` and display-list entry
      collection as full-tree passes. This is the shell-side counterpart of
      the §N triple-fingerprint item — fixing §N must include widening this
      gate to per-node dirty scoping, not only the fully-clean case.
- [ ] **Rotation transforms allocate a temp `PixelBuffer` per node per
      frame** and recursively repaint the subtree into it before the rotated
      blit (`render/src/surface/painter/tree.rs:380-410`). Any animated
      rotation pays an allocation + full subtree repaint per frame; reuse a
      cached temp buffer keyed by size class. Low priority until rotation is
      used in shipped surfaces. Rejected experiment 2026-07-12: adding a
      reusable scratch `PixelBuffer` slot to `FrontendRenderEngine` preserved
      ownership/recursion safety, but an optimized standalone benchmark that
      mirrored `PixelBuffer` allocation/clear measured 2.586ms for fresh
      allocate+clear versus 341.011ms for scratch reuse over 2M 96x64 buffers.
      The retained buffer must be explicitly cleared every frame, while fresh
      zeroed allocation can be much cheaper in this workload. Do not retry this
      shape without measuring against real rotated subtree painting and memory
      bandwidth, or without a way to track dirty coverage inside the temp.
- [x] **Minor inner-loop allocations in the Skia backend.**
      `execute_commands_on_canvas` allocates clip/layer stacks per batch
      (`painter/backend.rs:479-480`); the gradient shader cache key includes
      absolute rect position (`backend.rs:18`), so an animated/moving gradient
      re-creates its shader every frame and can thrash the 64-entry LRU — key
      by size only and translate the canvas, or accept and document. Progress
      2026-07-06: linear-gradient shaders are now local to the gradient box
      and cached by `(from, to, width, height)`, with the canvas translated for
      drawing. Same-sized moving gradients now keep one cached shader while
      preserving sampled top/bottom colors. Release-only benchmark over 5k
      moving-gradient draws measured 13.653ms with position-churned shader
      creation versus 12.524ms with size-key reuse (1.1x faster, one shader
      creation). Rejected experiment 2026-07-12: replacing the per-batch
      `Vec::with_capacity` clip/layer stacks with a hand-rolled inline stack
      regressed isolated stack bookkeeping badly: 31.812ms for the existing
      Vec path versus 350.183ms inline over 8M four-clip/four-layer batches.
      Clip/layer stack allocation remains open; a future attempt should use a
      proven small-vector implementation or reuse scratch storage. Completed
      2026-07-13: `execute_commands_on_canvas` now uses `SmallVec` for the
      common shallow clip/layer stacks, with inline-capacity coverage and a
      release-only bookkeeping benchmark. `cargo check -p mesh-core-render`
      passes; render test binaries still cannot link in this environment
      because `freetype`/`fontconfig` are missing.

Structure:

- [ ] **Every widget is painted by two parallel implementations.** The
      immediate-mode path (`render_tree*`/`render_node_with_filter`,
      `render_input_node`, `render_slider_node`, `render_icon_node`,
      `render_scrollbars`) duplicates the display-list path
      (`render_display_*` twins in `painter/widgets.rs`, `painter/tree.rs`)
      for input, slider, icon, scrollbar, and text painting. Same
      pair-duplication hazard as §O's diagnostics split — behavior drift
      between parent surfaces (display list) and child popups/tooltips
      (immediate mode) is silent. Converge on the display-list path (unblocked
      by the child-surface item above) and delete the immediate-mode twins.
- [ ] Text stack is healthy (layout LRU + glyph atlas + ellipsis cache with
      `Cow` fast path); remaining text work is the cache-pressure visibility
      + locale-sensitive workload items already tracked from
      `TEXT_RENDERING_TODO.md`. No new text findings.

### Q. Interaction & input — 2026-07-04 deep dive

Focused trace of `handle_component_input` (pointer/scroll/keyboard),
hover/tooltip transitions, element actions, and focus/scroll helpers. This
section already absorbed the B/J optimization passes; findings below are what
remains. `file:line` as of this scan.

Performance:

- [x] **Keyboard input reads and JSON-parses settings files from disk on
      every key event.** `current_keyboard_settings()` calls
      `load_shell_settings()` (`input/keyboard.rs:340-344`), which does up to
      two `fs::read_to_string` + JSON parse + merge (`config/src/lib.rs:374-390`)
      — invoked per `KeyPressed`, per `KeyReleased`, and per `Char`
      (`keyboard.rs:41,167,516,531`, `input/mod.rs:343`). Typing in a launcher
      input costs 2–4 file reads + parses per keystroke, blocking the shell
      thread. Cache `KeyboardSettings` on the component (or shell) and
      invalidate through the existing settings hot-reload/inotify path — the
      same infra module settings reloads already use.
      `resolved_surface_shortcuts` (rebuilt per keypress with locale lookups)
      becomes cacheable the same way.
      Progress 2026-07-06: `FrontendSurfaceComponent::current_keyboard_settings`
      now caches the merged `KeyboardSettings` behind an mtime check of both
      the defaults and user settings paths, so unchanged files skip the
      `fs::read_to_string` + JSON parse + merge entirely and only pay two
      `stat()` calls. A release benchmark over 20,000 calls measured 218.6ms
      reloading every call versus 50.1ms mtime-cached (~4.4x faster).
      Completed 2026-07-06: `resolved_surface_shortcuts` now caches resolved
      keybinds by `KeyboardSettings` + active locale, so repeated key events
      reuse declaration/override/localized-trigger resolution. Release
      benchmark over 50,000 calls with 24 actions measured 693.588ms rebuilding
      each call versus 128.437ms cached (5.4x faster).
- [x] **Click press/release still runs ~5–8 separate full-tree walks.**
      Press: `selectable_text_target_key`, `pointer_event_target_key`,
      `find_node_bounds_by_key`, `find_focusable_at`, then per-kind probes
      (`is_slider_key`/`is_option_key`/`is_radio_key`/
      `is_checkable_choice_key`); release: `pointer_event_target_key`,
      `find_click_handler`, `build_click_event` (`input/mod.rs:52-179`).
      Clicks are rare so this is latency (not throughput), but on large trees
      it's the same pattern the motion path already fixed — extend
      `pointer_hit_test` to also return focusable/selectable/kind/handler info
      in its single traversal.
      Progress 2026-07-11: `pointer_event_target_key` already ran
      `find_focusable_at` once internally, and the press handler immediately
      called `find_focusable_at` a second time on the same point to pick the
      focus target — a duplicate full-tree walk every press. Replaced both
      call sites with a fused `pointer_event_target_with_focus` free function
      that returns the click-target key and the focusable key from one walk.
      Added parity tests (focusable-with-handler, click-only fallback,
      no-target) and a release-only benchmark; the local dev-shell run
      measured 3.156ms for the duplicate-walk path versus 1.438ms fused over
      20k presses on a 200x12 grid (2.2x faster). The remaining walks
      (`selectable_text_target_key`, `find_node_bounds_by_key`, per-kind
      probes, release-path `pointer_event_target_key`/`find_click_handler`)
      are still separate; full fusion into `pointer_hit_test` remains open.
      Progress 2026-07-11: `selectable_text_target_key` ran `find_node_path_at`
      once to get the depth-many key path, then called `find_node_by_key`
      separately for *each* key in that path — twice over (once for the
      interactive-ancestor check, once for the selectable-text search) —
      turning every press into an O(depth × tree) walk instead of O(depth).
      Rewrote it to resolve the whole path in one `find_nodes_by_keys` call
      (already used by hover-transition dispatch for the same reason) and
      converted it to a free function alongside the fused press-target lookup.
      Added correctness tests (plain selectable text, interactive-ancestor
      short-circuit, non-selectable text, and old-vs-new parity on a deep
      chain) and a release-only benchmark; the local dev-shell run measured
      169.032ms for the per-key walk versus 28.557ms fused over 2k lookups on
      a 40-deep chain with 200 padding siblings (5.9x faster).
      Progress 2026-07-12: pointer press targeting now uses a fused
      `pointer_press_hit` traversal that returns the press target, focusable
      target, target node, and bounds together. The press handler consumes the
      resolved target for pointer-down bounds, slider detection/change-handler
      checks, option/radio/checkable classification, and initial slider value
      calculation instead of immediately re-walking the tree for each. Added
      parity coverage for focusable targets and non-focusable clickable
      ancestors plus a release-only benchmark; the local dev-shell run measured
      3.844s for the old multi-walk press lookup versus 3.170ms fused over 20k
      clickable-grid presses (1212.6x faster). Remaining release-path
      `pointer_event_target_key`/`find_click_handler`, click-event node
      resolution, and some activation helpers still do separate walks.
      Follow-up 2026-07-12: pointer release capture now short-circuits when the
      pointer is still inside the stored press bounds, avoiding an unnecessary
      release hit-test on ordinary clicks. The click dispatch path also reuses
      stored press bounds and an already-resolved target node for menu/item
      classification, click-event construction, and click/activate handler
      checks. Added release-capture parity tests plus a release-only benchmark;
      the local dev-shell run measured 1.529ms for unconditional release
      hit-testing versus 168.599us with the bounds short-circuit over 20k
      releases (9.1x faster). Remaining: release outside press bounds still
      falls back to `pointer_event_target_key`, and some activation helpers
      still do their own ancestry/descendant walks.
      Follow-up 2026-07-12: release fallback outside the stored press bounds
      now uses the fused `pointer_press_hit` path instead of the legacy
      `pointer_event_target_with_focus` multi-walk. Added a release-only
      non-focusable clickable-grid benchmark; the local dev-shell run measured
      2.238s for the legacy multi-walk fallback versus 2.884ms with the fused
      press hit over 20k releases (775.9x faster). Remaining activation helper
      ancestry/descendant walks are still separate.
      Follow-up 2026-07-12: press targets now snapshot their release-dispatch
      metadata (bounds, tag/current-target payload, activation classification,
      click/activate handlers and pre-bound args) so ordinary same-bounds
      release dispatch avoids re-walking the tree for node lookup, bounds
      lookup, classification, and handler resolution. Added a release-only
      metadata benchmark; the local dev-shell run measured 2.503s for the old
      tree lookup/reconstruction path versus 18.997us from the press snapshot
      over 20k worst-case releases (131766.2x faster for that isolated
      metadata path). Remaining activation helper ancestry/descendant walks
      are still separate.
      Follow-up 2026-07-12: checkable switch/checkbox press activation now
      reads `checked` state from the already-resolved press target node instead
      of re-walking the tree by key inside `toggle_checked_value`. Added a
      release-only benchmark; the local dev-shell run measured 4.989s for the
      key lookup path versus 1.076ms with the resolved node over 200k
      worst-case toggles (4637.5x faster for that isolated state lookup).
      Remaining option/radio activation ancestry/descendant walks are still
      separate.
      Follow-up 2026-07-12: option/radio pointer activation now also consumes
      the already-resolved press target for the initial disabled/value read,
      while preserving the existing ancestor/descendant group behavior. Added
      a release-only benchmark; the local dev-shell run measured 5.068s for
      the key lookup path versus 3.942ms with the resolved node over 200k
      worst-case activations (1285.6x faster for that isolated target-state
      lookup). Broader option/radio group indexing remains a future structural
      optimization rather than part of the original press/release multi-walk
      hot path.
- [x] **Scroll events do two extra walks** — `find_scrollable_at` then
      `find_node_by_key` for limits (`input/mod.rs:307-309`); fold the
      scrollable ancestor + limits into the fused hit-test result.
      Progress 2026-07-10: added `find_scrollable_at_with_limits`, preserving
      the legacy key-only API while letting wheel input consume the scrollable
      key and max offsets from the same traversal. Added parity coverage and a
      release-only benchmark; the local dev-shell run measured 1.578s for
      key-then-lookup versus 311.309ms fused over 200k scroll hits (5.1x
      faster).
      Follow-up 2026-07-12: scroll-handler dispatch now uses a fused
      `pointer_event_handler_hit(..., "scroll")` traversal that returns the
      nearest handler node and bounds directly. This removes the path
      allocation, per-key `find_event_handler` tree walks, duplicate handler
      probe, node lookup, and bounds lookup before calling `onscroll`. Added
      parity coverage and a release-only benchmark; the local dev-shell run
      measured 2.610s for the old path/key-walk dispatch lookup versus 1.600ms
      fused over 20k scroll-handler hits (1630.5x faster).
- [x] Minor: `apply_element_actions` cloned the whole `ref_node_keys`
      HashMap per action batch (`interaction_state.rs:91`). Progress
      2026-07-10: action application now temporarily moves the ref lookup map
      out of the `RefCell` and restores it after resolving the drained batch,
      avoiding clone-per-batch while preserving the same resolver semantics.
      Added ref-table preservation coverage and a release-only benchmark; the
      local dev-shell run measured 1.837s for cloning a 512-entry map versus
      1.494ms for move/restore over 100k batches (~1230x faster).
- [x] Minor: hover-change path cloned `Vec<String>` paths
      (`input/mod.rs:214-240`). Progress 2026-07-10: pointer move now takes
      ownership of the path produced by `pointer_hit_test`, swaps it into
      component hover state, and pointer-leave takes the stored path instead
      of clone-then-clear.
      Added a release-only benchmark; the local dev-shell run measured 1.600s
      for the old clone shuffle versus 1.042s for move/replace over 500k path
      updates (1.5x faster).
      Follow-up 2026-07-12: hover transition dispatch now borrows the incoming
      `new_path` before moving it into component state, removing the remaining
      `current_path = self.hovered_path.clone()` snapshot while preserving
      state restoration before handler errors propagate. Updated the
      release-only benchmark; the local dev-shell run measured 1.574s for the
      old clone-shuffle path versus 495.624ms for move/replace without the
      dispatch snapshot over 500k path updates (3.2x faster).
- [x] Confirmation for the tracked slider-drag item (J): the unconditional
      `invalidate_script_state()` per coalesced drag motion is at
      `input/mod.rs:193-200` with a comment explaining why state-dirty
      detection was insufficient — the fix needs slider knob position painted
      from shell-owned `slider_values` via the STATE path plus a paint-only
      text update, exactly as the J item describes.
      Confirmed complete 2026-07-12: handlerless slider press/move paths now
      use interaction restyle, while sliders with change/release handlers keep
      script invalidation for reactive labels. Existing policy coverage asserts
      both paths, and the release-only benchmark reports 790.919ms forced
      script rebuild versus 213.822ms retained interaction repaint over 200
      handlerless drag frames (3.7x faster).

Structure:

- [ ] Interaction identity is string-keyed end to end (`hovered_path:
      Vec<String>`, `focused_key`, `scroll_offsets`, `input_values`,
      `slider_values` all keyed by `_mesh_key` strings). This is the consumer
      side that keeps the §N "identity travels as a string attribute" problem
      alive; the NodeId migration should convert these maps together with the
      metrics/refs publication so the string keys can finally disappear.
- [ ] Otherwise healthy: pointer-motion is fused single-traversal with
      coalescing, hover dispatch resolves all transitioning nodes in one walk,
      scroll animations early-out when idle, stale-target pruning is
      probe-based. No further structural findings.

### R. Script runtime & Lua boundary — 2026-07-04 deep dive

Focused trace of `call_handler` → `sync_state_from_lua` → `ScriptState` →
`refresh_module_object`, plus the VM pool and backend runtime. The G-item
optimizations (write-log discovery, side-channel flag, cached self table,
proxy seen-cache) are confirmed in place; findings below are what remains.
`file:line` as of this scan.

Performance:

- [x] **`refresh_module_object` re-serializes the entire component state per
      handler call for every proxy-bearing component.** Any component that
      `require`s a service interface registers state proxies, so
      `has_proxies()` is true and the generation skip never applies
      (`context/runtime.rs:1777-1793`). Every handler and render hook then
      pays: `state.snapshot()` with proxies — which **bypasses the snapshot
      cache and deep-clones every variable's JSON** plus invokes every proxy
      getter (`context/state.rs:222-231`) — followed by a full JSON→Lua
      conversion and a `module.state` table write. And per
      `docs/modules/frontend/core/README.md:64`, `module.state` is a legacy
      v1.12 compatibility lane; no shipped module reads it. Verify no internal
      consumer remains, then delete the refresh (and the lane) per the
      no-backward-compat rule — likely the single largest remaining boundary
      cost for service-connected components.
      Completed 2026-07-09: verified no shipped module consumes
      `module.state`/`module.exports`, removed both legacy tables and all
      refresh/export synchronization, and retained `module.events` as the
      supported named-event API. Host-seeded values remain direct component
      globals. A proxy-bearing 65-field release benchmark over 20k legacy
      mirrors measured 1.476s for snapshot + JSON→Lua serialization versus
      9.497µs of remaining generation bookkeeping (~155k× difference).
- [ ] **The sync "fast path" still round-trips every known global per
      handler.** For each user global: env read + `from_value` Lua→JSON
      conversion + `state.set` deep-compare, changed or not
      (`context/runtime.rs:1678-1687`). The write log fixed discovery only.
      Because Luau `__newindex` does not fire for existing table keys, a true
      per-write log needs `_ENV` to become a forwarding proxy (empty table
      with `__index`/`__newindex` to a backing store) — or invert ownership:
      keep values in Rust and expose globals through the proxy so there is no
      sync at all. Measure script read-through cost first; pairs with the
      v1.17 per-thread-VM work.
      Progress 2026-07-09: known scalar globals now compare borrowed Lua
      bool/integer/number/string values directly against current JSON state and
      skip `from_value` plus `ScriptState::set` when unchanged; tables and
      changed values retain the conservative conversion path. With 512
      unchanged numeric globals over 5k handler syncs, release time fell from
      468.867ms to 410.951ms (~1.14x faster). Avoiding the `_ENV` read itself
      still requires the forwarding-proxy architecture described above.
- [x] **`ScriptState::snapshot()` with proxies has no caching.** The
      non-proxy branch caches by generation; the proxy branch rebuilds and
      deep-clones everything on every call (`state.rs:196-231`). Even after
      the `module.state` deletion, remaining `snapshot()` callers pay this —
      cache the variables portion by generation and overlay proxy getters.
      Completed 2026-07-09: `snapshot()` now always obtains the local-variable
      object through the generation cache, then overlays fresh proxy getter
      values for proxy-bearing states. Added regression coverage proving proxy
      values remain live while local variables are preserved. A release
      benchmark over 20k proxy-bearing snapshots with 128 local values measured
      455.840ms rebuilding locals versus 385.254ms from the cached variable
      snapshot (~1.2x faster).
- [x] Minor: `sync_module_exports_from_lua` runs per sync (module table read
      + `from_value` + `set`) even for components that export nothing
      (`runtime.rs:1765-1775`); record "has exports" once at script load and
      skip. Removed with the legacy `module.exports` lane on 2026-07-09.

Structure:

- [x] **Legacy `module.state` / `module.exports` lanes.** Documented as
      compatibility-only (`docs/modules/frontend/core/README.md`), but they
      still drive per-handler work (items above). Audit consumers and remove
      per the no-backward-compat rule; if `module.exports` is still the
      mechanism behind component exports, rename/keep that half explicitly
      and document it as current, not compat.
- [ ] Healthy: `LuaVmPool` sandboxing with baseline-global capture, cached
      lifecycle self table, flag-gated side channels, storage read tracking,
      interface-proxy seen-field cache, backend snapshot only on emit paths.
      No further findings.

### S. Events, services & backends — 2026-07-04 deep dive

Focused trace of `broadcast_service_event` → dedup/validation → delivery,
the `InterfaceRegistry`, and the backend service loop. `file:line` as of this
scan.

Performance:

- [x] **`InterfaceRegistry::resolve` deep-clones the entire interface catalog
      on every call.** `resolve()` goes through `catalog()`, which clones the
      full contracts map **and** providers map (every contract's state fields,
      events, and commands for every interface)
      (`extension/service/src/interface.rs:54-56,86-91`), then clones the
      matched contract again (`interface.rs:126-133`). It is called per
      accepted service state update (`validate_service_state_shape`,
      `shell/runtime/service_state.rs:228`), per named interface event
      (`service_state.rs:243`), and per service command dispatch
      (`shell/runtime/request.rs:774,814`). Every audio update and every
      volume command deep-clones every registered contract. Fix: resolve
      directly under the read lock and return `Arc<InterfaceContract>`;
      keep `catalog()` for the debug/discovery paths that genuinely want a
      snapshot.
      Completed 2026-07-05: registry contracts are stored behind `Arc`, and
      `resolve()` now looks up contracts/providers directly under read locks
      instead of materializing a catalog snapshot. `has()` and
      `list_interfaces()` likewise query registry maps directly. Explicit
      `catalog()` calls retain owned snapshot semantics for diagnostics and
      script setup. A release benchmark with 64 contracts (32 methods each)
      over 10k resolutions measured 879.6ms for snapshot resolution versus
      1.20ms for direct registry resolution (~730x faster).
- [ ] **Contract validation re-derives typed information per event.**
      `json_value_matches_contract_type` allocates a lowercased String per
      field per update (`service_state.rs:401-415`), and named-event payloads
      re-parse the inline schema **string** on every event
      (`parse_inline_object_schema`, `service_state.rs:345,375-395` — also
      hand-rolled string parsing, which project policy treats as migration
      debt). Precompile contract field types and event schemas into typed
      enums at contract-registration time; validation becomes match arms with
      zero allocation.
      Progress 2026-07-05: primitive type matching no longer allocates a
      lowercased `String`, and inline event-schema parsing borrows field names
      and types instead of allocating two strings per field. A release
      benchmark over 1M type checks measured 12.56ms allocating versus 5.96ms
      allocation-free (~2.1x faster). Registration-time typed schema
      precompilation remains open. Progress 2026-07-10: shell-side contract
      validation now caches compiled inline event schemas and contract type
      classifications, so steady-state named interface events reuse typed
      field descriptors instead of reparsing the schema string. A release
      benchmark over 300k four-field event validations measured 45.182ms
      parse-per-event versus 15.958ms cached (~2.8x faster). Moving the
      compiled descriptors onto the registered contract remains open. Added
      2026-07-25: steady-state validation-cache hits now borrow the registered
      contract's interface name for the map probe and clone it only when
      replacing a cache entry. A release benchmark over 2M hits measured
      44.67ms with the per-hit clone versus 25.18ms with the borrowed lookup
      (1.77x faster); the relative speedup is checked by the canonical gate.
- [ ] Minor: `canonical_interface_name` / `service_name_from_interface`
      allocate fresh Strings 2–3× per event across normalize/record/profiling
      (`service_state.rs:44,92`, `interface.rs:95-118`); thread the canonical
      name through instead of re-deriving, or intern interface names (v1.23).
      Progress 2026-07-09: canonical names now use a `Cow<str>` helper that
      borrows already-qualified interfaces, normalized service events carry
      the canonical name forward into record/profiling, and allocation occurs
      only when a short alias needs the `mesh.` prefix or state is inserted.
      Over 2M canonical-name calls, release time fell from 20.827ms owned to
      6.446ms borrowed (~3.2x faster). Runtime event observation now also
      borrows the `mesh.`-stripped service segment rather than allocating once
      per component/runtime check; 2M projections measured 1.921ms owned versus
      1.267ms borrowed (~1.5x faster). Progress 2026-07-10: service-event
      normalization now reuses the owned incoming name when it is already
      canonical, avoiding one extra allocation before provider/status checks.
      A release benchmark over 2M canonical runtime names measured 32.137ms
      borrowed-to-owned versus 25.147ms owned-reuse (~1.3x faster). Some
      callers that retain names still use the generic owned API. Progress
      2026-07-10: per-interface service capability caches now include the
      control capability, and service-command dispatch borrows the cached
      capability while reusing one canonical interface name through support,
      optimistic-state, provider, and debug-call bookkeeping. A release
      benchmark over 1M control-capability lookups measured 27.754ms formatting
      versus 23.994ms cached clone (~1.2x faster). Remaining work is broader
      API cleanup for retained interface names outside the command path.
      Progress 2026-07-10: service update delivery now threads the cached short
      service name into state writes for live delivery, locale seeding, and
      cached replay into new runtimes, avoiding another canonical-interface to
      service-name projection per receiving runtime. A release benchmark over
      200k state writes measured 66.567ms projected-name versus 61.055ms
      borrowed-name (~1.1x faster). Progress 2026-07-12:
      `apply_service_update` now also uses the borrowed `Cow` projection for
      generic callers instead of allocating an owned projected name before
      immediately passing it by reference. A release benchmark over 200k state
      writes measured 64.151ms owned projection versus 61.845ms borrowed
      projection (~1.04x faster); explicit pre-borrowed-name state writes
      remain slightly faster at 61.472ms.

Structure:

- [ ] Concrete citation for the tracked "eliminate service-specific Rust
      branches" item: the hardcoded `mesh.audio` optimistic-mute merge lives
      in `normalize_service_event` (`service_state.rs:66-75`) and
      `apply_optimistic_audio_muted_state` (`service_state.rs:137-165`).
      The generic replacement is an optimistic-state declaration in the
      interface contract (field + command linkage) so core stays
      service-agnostic.
- [ ] Healthy/confirmed: shell-boundary payload dedup before delivery,
      wake-level coalescing with barriers, backend-side dedup
      (`publish_changed_update` + `last_payload`), stream line batching per
      program, `Arc<Event>` bus. The open C items (shell-side subscription
      index, push-based host API primitives) remain the section's structural
      backlog.

### T. Layout — 2026-07-04 deep dive

Focused trace of `compute_incremental` → retained style sync → Taffy compute →
text measurement. Confirms the F-item paint-only fast path is in place
(`layout.rs:347-355`). `file:line` as of this scan.

Performance:

- [x] **Unconditional `set_style` per node defeats Taffy's internal caching on
      every layout-dirty frame.** `update_retained_node_styles` converts all
      ~60 style fields (`taffy_style_for_node`) and calls
      `state.tree.set_style` for **every** node whenever layout is dirty
      (`ui/elements/src/layout.rs:811-855`), and `set_style` invalidates that
      node's Taffy layout cache — so one changed node forces Taffy to
      recompute as if everything changed. The retained tree already computes
      per-node STYLE/LAYOUT dirty flags (§N); feeding them here so only dirty
      nodes get converted + `set_style` is the mechanism that makes the
      existing "dirty-node-only sync" item (F) pay off twice: skips the
      conversion walk *and* preserves Taffy's caches for clean subtrees.
      Rejected experiment 2026-07-12: checking `tree.style(id) == new_style`
      before `set_style` without per-node dirty bits measured slower in an
      optimized direct Taffy microbenchmark (46.115ms unconditional
      `set_style` versus 51.642ms skip-equal over 2k×512 equal-style updates),
      so this needs the retained dirty-bit feed rather than an equality guard.
      Completed 2026-07-12: `RetainedWidgetTree` now derives a
      layout-relevant dirty `NodeId` set from the finalized tree before the
      retained-tree update, falling back to the full path on structural
      uncertainty. `LayoutEngine::compute_incremental_with_dirty_nodes` uses
      that set to call `set_style`/`mark_dirty` only for dirty nodes while
      still refreshing text measurement context and output layout maps for
      all retained nodes. Added retained-layout parity coverage for the
      narrow dirty-node path. Verification: `cargo test -p mesh-core-elements
      retained_layout` passes; shell crate verification is still blocked by
      the missing system `xkbcommon.pc` dependency from
      `smithay-client-toolkit`.
- [x] **Text measurement clones the content String twice per node per pass.**
      `update_text_context`/`build_taffy_tree` clone every text node's
      `content` into `TextMeasureData` per layout-dirty and structural pass
      (`layout.rs:857-884,580-596`), and `TextMeasureKey::new` clones it
      **again** per measure probe — including on cache hits, since the owned
      key is built just to probe the LRU (`layout.rs:119-130`). Fix: share
      content as `Arc<str>` (the §N `Arc<str>` payload item's layout face) and
      probe the intrinsic cache with a borrowed/hashed key instead of an owned
      one.
      Completed 2026-07-09: `TextMeasureData` and `TextMeasureKey` now share
      content through `Arc<str>`, leaving one node-attribute-to-measurement
      allocation per layout sync and making cache-key construction a pointer
      clone. A 376-byte string benchmark over 1M iterations measured 28.481ms
      for two `String` clones versus 9.820ms for `Arc` build+clone (~2.9x
      faster). Text content-change measurement regression coverage passes.
- [x] **Structural reconcile is string-keyed and clone-heavy.**
      `reconcile_retained_taffy_node` clones each node's `_mesh_key` String
      (`layout.rs:773-810`), `collect_mesh_keys` clones every key into a
      `HashSet<String>` per structural pass (`layout.rs:901-908`), and the
      stale sweep clones + length-sorts keys (`layout.rs:706-722`).
      Progress 2026-07-09: existing retained nodes now borrow `_mesh_key`
      during lookup and clone only on insertion; the live-key set borrows from
      the widget tree, so only genuinely stale keys are cloned for ordered
      removal. Add/remove/reorder parity tests pass. A 1,024-key release
      benchmark over 5k collections measured 219.204ms cloned versus 70.786ms
      borrowed (~3.1x faster). Completed 2026-07-09: retained Taffy identity is
      now keyed by stable `NodeId`; structural stale removal finds stale roots
      through Taffy's parent links and removes each subtree once. Five retained
      add/remove/reorder/style/layout parity cases pass. A 5M-lookup release
      benchmark measured 86.858ms for long String keys versus 47.315ms for
      `NodeId` (~1.8x faster).

Structure:

- [x] **The LAYOUT-03 string-keying rationale is obsolete.**
      `PerSurfaceLayoutState.node_map` is keyed by `_mesh_key` String with a
      comment "NOT ephemeral NodeId per LAYOUT-03" (`layout.rs:144-146`) — but
      runtime NodeIds are no longer ephemeral: they are stable hash-chained
      ids derived from the same key paths (§J progress, `runtime_tree.rs`).
      Re-keying `node_map` by `NodeId` removes every string clone above and
      the per-node hash of long key strings in `retained_taffy_id`
      (`layout.rs:910-915`). Do together with the §Q interaction-map NodeId
      migration so `_mesh_key` strings have no remaining hot consumers.
      Completed for retained layout on 2026-07-09; interaction/refs maps remain
      separate follow-up consumers of `_mesh_key`.
- [x] Healthy/confirmed: paint-only frames skip all layout sync; fresh
      `node_map`/`text_nodes` maps per pass were measured (scratch reuse
      rejected 2026-07-04); intrinsic text cache is LRU-bounded; Taffy
      diagnostics are report-gated. Confirmed 2026-07-12 while closing the
      retained dirty-node style-sync path.

### U. Presentation & memory — 2026-07-04 deep dive

Focused trace of `present_with_damage` → SHM pool copy → `attach_shm_buffer` →
commit, plus popup promotion, scale/blur/input-region protocol handling, and
input normalization in `crates/core/presentation`. Existing H items (direct
Skia-into-SHM paint, size-class pools, `copy_bgra_to_canvas` cites) still
stand; findings below are additional. `file:line` as of this scan.

Performance:

- [x] **Per-buffer pending damage is a single bounding rect, which forces the
      SHM copy to be a union.** `SurfaceShmBuffer.pending_damage` is
      `Option<DamageRect>` (`presentation/src/wayland_surface/backend.rs:73-76`),
      accumulated via `union_damage` (`backend.rs:270-283`), and
      `present_with_damage` folds the frame's multi-rect damage into one union
      before the copy (`backend.rs:1174-1183`, the "Pitfall 1" comment). Two
      small disjoint changes on one surface — clock text at the left of a bar
      plus a volume icon at the right — memcpy the entire span between them
      every frame, even though the `damage_buffer` protocol calls downstream
      are correctly per-rect. Making `pending_damage` a small bounded rect list
      (same 16-rect cap as `MAX_PROTOCOL_DAMAGE_RECTS`) lets the copy loop run
      per rect and shrinks steady-state SHM traffic to the actual changed
      pixels. Pairs with the H direct-paint item; whichever lands first should
      carry the rect-list change.
      Completed 2026-07-05: each SHM buffer now retains an inline bounded list
      of up to 16 physical damage rects and copies each region independently;
      overflow collapses safely to one union. Protocol damage includes every
      region actually refreshed on a reused buffer. A two-edge-rect benchmark
      over 1,000 1920x100 frames measured 16.25ms for bounding-union copies
      versus 3.47ms for rect-list copies (~4.7x faster in the debug profile).
- [x] **kde_blur region is re-created and re-committed on every present while
      blur is active.** `present_with_damage` unconditionally creates a fresh
      `Region`, calls `set_region` + `commit` per frame whenever
      `entry.blur_region` is `Some` (`backend.rs:1192-1215`) — the shell-side
      gate (`last_region_state` in `runtime/render.rs:900-930`) only gates
      `update_blur_region`, not the per-present protocol churn, because the
      backend re-commits from stored state each frame. A surface with an
      animated element and a static backdrop-blur pays wl_region create +
      2 protocol requests per frame for a region that never changes. Track the
      last-committed rect on `SurfaceEntry` and skip when unchanged (the
      `input_region_dirty` pattern two blocks below it is the right shape —
      blur is the one region type that didn't get it).
      Completed 2026-07-05: `SurfaceEntry` now tracks blur-region dirtiness;
      unchanged presents skip region creation and KDE blur protocol commits,
      while region changes and removal still commit exactly once. Regression
      coverage proves 1,000 unchanged presents require one blur update.
- [ ] **Input normalization allocates a String per event via a linear surface
      scan.** Every pointer/keyboard event calls `surface_id_for_wl_surface`,
      which iterates all surfaces comparing `wl_surface` handles and clones the
      id String (`wayland_surface/state.rs:314-322`, called from
      `handlers.rs:217` per pointer-frame event, `handlers.rs:438,461` for
      keyboard focus). Motion events then carry that String into
      `DevWindowEvent`, the shell re-allocates it again in `dispatch_wayland`
      (`event_surface_id(&event).to_string()`, `runtime/wayland.rs:24`), and
      key repeat clones surface-id + key name per synthesized event
      (`state.rs:19-29`). Coalescing caps what reaches the shell but every raw
      Wayland event still pays the scan + clone. Store the surface id as
      `Arc<str>` on `SurfaceEntry` (or a numeric id + side table) so the lookup
      is a pointer clone; `keysym_name`/`normalize_keysym_name` String
      allocation per key event (`handlers.rs:561-585`) and the lowercase alloc
      in `is_non_repeating_key` (`state.rs:336-348`) fold into the same pass.
      Progress 2026-07-08: Wayland state now maintains a `wl_surface`
      `ObjectId` → surface id index alongside the owned surface map, so raw
      pointer, keyboard-focus, layer-close, and popup-dismissal events avoid
      linearly scanning all surfaces before routing. Public `DevWindowEvent`
      payloads remain `String`-based, so the remaining clone/API cleanup and
      key-name allocation pass are still open. A release-only benchmark over
      500k lookups across 128 surfaces measured 13.141ms for the scan path
      versus 0.715ms indexed (18.4x faster).
      Progress 2026-07-08: key-repeat filtering no longer lowercases each
      key name to detect modifiers/non-repeat keys; it now uses borrowed ASCII
      case-insensitive equality/window checks. Release benchmark over 1M
      eight-key batches measured 323.835ms for lowercase allocation versus
      61.248ms borrowed (5.3x faster). `keysym_name` allocation and public
      event payload `String` clones remain open. Progress 2026-07-12:
      key press repeat setup now builds repeat state from borrowed
      `surface_id`/key-name inputs and clones only when a repeat state is
      actually retained; non-repeating key events move the owned event
      surface/key strings directly into `DevWindowEvent` instead of cloning
      them and dropping the originals. Added repeat-state coverage and a
      release-only benchmark; the local dev-shell run measured 16.578ms for
      the old clone-before-schedule shape versus 7.198ms borrowed over 500k
      non-repeating key presses (2.3x faster for that isolated repeat setup
      path). `keysym_name` allocation and public event payload `String` clones
      remain open. Progress 2026-07-13: Wayland `keysym_name` now returns a
      `Cow<'static, str>` and `normalize_keysym_name` borrows common xkb names
      through repeat filtering/release matching, only owning the key name at
      the public `DevWindowEvent` boundary or for raw numeric fallbacks.
      Focused presentation tests remain blocked in this environment by the
      missing `xkbcommon.pc` system dependency; source check confirms
      `xkeysym::Keysym::name()` returns `Option<&'static str>`. The remaining
      cleanup is the public `String` event payload shape.
- [ ] **Child popup targets force a full-buffer present every frame.**
      `paint_and_present_child_surface` sets `force_full_present = true`
      unconditionally after each child paint (`shell/runtime/render.rs:789-791`),
      so even if the popover subtree gained retained damage tracking (§P child
      item), presentation would still upload the full buffer. This is the
      presentation-side half of the §P "child popups bypass the retained
      pipeline" item — fix them together, otherwise the display-list work
      shows no SHM win.
      Progress 2026-07-08: child popup targets now force full present only for
      first/resized buffers or backend scale/full-redraw requests. Steady child
      frames carry child-local damage translated from the parent component's
      retained present damage, and the test backend records damage payloads for
      regression coverage. The focused child-popup test path reduced a steady
      child present from full `72x32` damage (2,304 logical px) to `12x7`
      damage (84 logical px), a 27.4x smaller present/upload region. The
      deeper child-subtree retained-paint path remains open: child buffers are
      still repainted eagerly before presenting sparse damage.
- [x] **`wait_for_surface_configure` runs up to 10 blocking roundtrips on the
      shell thread.** Called from `present_with_damage` (`backend.rs:1130`)
      and `surface_size` (`backend.rs:1324`) whenever the surface is not yet
      configured (`backend.rs:1405-1432`). Fine for first map, but a
      compositor that delays configure (or a dead popup) stalls the whole
      frame loop for 10 round trips; every other surface's present waits
      behind it. Bound it by deadline instead of roundtrip count, or return
      not-ready and let the render loop retry on the next Wayland wake (the
      loop already wakes on the connection fd).
      Progress 2026-07-08: configure waiting is now deadline-bounded at 2ms.
      The backend flushes and dispatches pending events, reads Wayland events
      only while the connection is ready within the remaining budget, and then
      returns to the render loop if the surface is still unconfigured. This
      removes the previous worst case of 10 blocking roundtrips on the shell
      thread; delayed/dead configure now costs at most the 2ms local budget per
      attempt before other surfaces and IPC work can continue. Completed
      2026-07-13 verification: the shipped backend is deadline-bounded by
      `SURFACE_CONFIGURE_WAIT_DEADLINE` and polls the Wayland fd until either
      the surface configures or the deadline expires. The deadline is now
      500ms, not 2ms, because the shorter budget regressed first-configure
      sizing on startup; the old fixed 10-roundtrip loop is gone.
- [x] Minor per-present allocations: `attach_shm_buffer` builds two
      `Vec<DamageRect>` per present (`backend.rs:334-343`) and
      `protocol_damage_rects` re-allocates via `to_vec` even in the ≤16
      passthrough case (`backend.rs:569-582`) — smallvec/iterate-in-place;
      `surface_config_fingerprint` is a fourth hand-rolled byte-at-a-time FNV
      hasher (`backend.rs:142-161`), the presentation face of the §N
      hasher-drift item.
      Progress 2026-07-05: physical scaling and clipping are fused into one
      vector build, and the common <=16-rect protocol path borrows the damage
      slice instead of allocating with `to_vec`. A debug microbenchmark over
      1M four-rect passthroughs measured 36.09ms cloned versus 8.97ms borrowed
      (~4.0x faster); the release run was blocked by a full build volume.
      Progress 2026-07-08: the fused clipped-damage scratch in
      `attach_shm_buffer` now uses inline `SmallVec` storage, keeping the
      common <=16 rect path allocation-free through scaling, clipping, copied
      damage extension, and protocol-damage borrowing. Release benchmark over
      1M six-rect clipped scratch builds measured 45.497ms for heap `Vec`
      versus 16.218ms for `SmallVec` (2.8x faster).
      Progress 2026-07-08: `surface_config_fingerprint` now overrides
      primitive `Hasher::write_*` methods instead of falling back to
      byte-at-a-time hashing for every numeric config field. Release benchmark
      over 2M config fingerprints measured 27.644ms for byte writes versus
      5.309ms for primitive writes (5.2x faster).

Structure:

- [x] Healthy/confirmed: SHM pool reuse with per-buffer pending-damage
      refresh and the busy-buffer overflow slot (`backend.rs:265-316`);
      surface config fingerprint gating with the keyboard-only reconfigure
      carve-out (`backend.rs:198-227,454-469`); popup reconcile gated on
      `PopupConfig` equality shell-side (`runtime/render.rs:629-649`); opaque/
      input/blur region *updates* gated by display-list generation shell-side
      (`runtime/render.rs:900-930`); input region applied lazily with a dirty
      flag so it survives configure/remap ordering (`backend.rs:1220-1239`);
      frame-callback wait treated as a hint with a 50 ms escape hatch
      (`backend.rs:63,401-406,1132-1146`); `wait_for_events` blocks on Wayland
      fd + shell eventfd together with no spin (`backend.rs:1510-1602`);
      pointer/scroll coalescing at the engine boundary
      (`presentation/src/lib.rs:427-481`). The dev-window backend is dev-only
      and was not audited for hot-path cost. Confirmed 2026-07-13 against the
      current presentation backend; existing regression coverage includes
      disjoint pending-damage preservation/collapse, keyboard-only configure
      retention, and unchanged blur-region dirtiness.

### V. Shell orchestrator, threading & startup — 2026-07-04 deep dive

Focused trace of `Shell::run` (event loop, wake scheduling, message
coalescing), `render_components` orchestration, `dispatch_wayland`, reload
gating, and the discovery → catalog → mount startup path. The K threading
items (parallel paint, pipelining, blocking IO off-thread) and H startup item
(parallel module compile) still stand; findings below are additional.
`file:line` as of this scan.

Performance:

- [x] **Every top-level surface gets a deep clone of the entire compiled
      frontend catalog at startup.** `FrontendCatalog` is a plain `Clone`
      struct holding every `CompiledFrontendModule` (parsed templates, styles,
      scripts for *all* frontend modules; `shell/component/catalog.rs:11-21`),
      and `load_frontend_components` passes `frontend_catalog.clone()` to
      each `FrontendSurfaceComponent` (`shell/discovery.rs:366-377`) after
      `top_level_surfaces()` has already cloned every matching entry once more
      (`catalog.rs:148-163`). With N surfaces the shell holds N+1 full copies
      of every compiled module for the life of the process — startup time and
      resident memory both scale as catalog × surfaces. Wrap the catalog in
      `Arc<FrontendCatalog>` (it is read-only after build; hot reload can
      rebuild-and-swap the Arc). Same call site also hands each component its
      own deep `interfaces.catalog()` clone (`discovery.rs:375`,
      `extension/service/src/interface.rs:86-91`) — the startup face of the
      §S resolve-clone item; fix both with the same `Arc` treatment.
      Progress 2026-07-05: the compiled `FrontendCatalog` is now shared by all
      top-level surfaces through `Arc`; source reload uses copy-on-write via
      `Arc::make_mut`. This removes the persistent N+1 deep copies of every
      compiled frontend module. Completed later 2026-07-05: the interface
      catalog is also constructed once and shared through every surface,
      embedded `ScriptContext`, and Lua lookup closure. A release benchmark
      with 64 contracts (32 methods each) over 10k clones measured 799.7ms for
      deep catalog clones versus 27.3us for `Arc` clones (~29,000x faster).
- [x] **`interfaces.resolve()` catalog deep-clones also fire from the command
      dispatch path.** `service_command_is_supported` and
      `service_command_is_coalescable` each call `self.interfaces.resolve()`
      (`runtime/request.rs:773-779,813-820`) — two full catalog clones per
      `ServiceCommand` request (every slider drag tick that passes the
      throttle, every button command), and `flush_throttled_commands` resolves
      again per flushed command. Already covered mechanically by the §S
      "resolve under the read lock" item; listed here so the orchestrator-path
      call sites get retired with it.
      Completed 2026-07-05 by the direct `InterfaceRegistry::resolve` lookup;
      command support/coalescing checks now clone only the selected contract
      `Arc` and provider record.
- [ ] **Startup is fully serial on the main thread.** Confirmed the H item:
      `discover_modules` scans + parses manifests dir-by-dir
      (`discovery.rs:124-136,209-304`), then `FrontendCatalog::from_modules`
      compiles every frontend module one at a time (`catalog.rs:45-69`),
      then backends spawn. Manifest load, `.mesh` parse, and compile are pure
      per-module — rayon over `module_ids` in `from_modules` is the smallest
      first cut. (Graph load is cached via
      `load_installed_module_graph_cached`; its `clone()` uses at
      `discovery.rs:365` and `backend/spawn.rs:18` are startup-only and fine.)
      Progress 2026-07-05: frontend compilation now runs concurrently through
      Rayon and measured ~1.9x faster on the shipped catalog. Discovery and
      manifest loading remain serial, so the broader startup item stays open.
- [x] **Per-event allocations in `dispatch_wayland`.** Each dispatched event
      allocates the physical surface id String (`runtime/wayland.rs:24`),
      clones the routed target id (`wayland.rs:53`), calls
      `surface_size_changed` per event even when the size cannot have changed
      (`wayland.rs:180`), and wraps every emitted request in its own
      single-element `VecDeque` (`wayland.rs:216-219`). Bounded by the
      32-events-per-frame cap and input coalescing, so this is allocation
      hygiene, not a hot bug — retire together with the §U `Arc<str>`
      surface-id change so ids stop being re-allocated at each layer. Progress
      2026-07-06: `dispatch_wayland` now borrows the physical event surface id
      instead of allocating it per event, clones keyboard focus only for
      keyboard routing, and drains emitted requests as one `VecDeque` instead
      of wrapping each request separately. Target-surface clone and redundant
      size-change calls remain open. Progress 2026-07-10: added a
      single-request drain helper and used it for Wayland global shortcuts and
      popup pointer-leave hides, removing the remaining single-element
      `VecDeque::from([request])` allocations in `dispatch_wayland`. Progress
      2026-07-10: stable-size input events now compare against the component's
      current content input size before calling `surface_size_changed`, so the
      redundant per-event size-change call is skipped unless the routed content
      size actually changed. Rejected experiment 2026-07-10: replacing the
      emitted-request `VecDeque::from(Vec<CoreRequest>)` adapter with direct
      `Vec::drain` looked plausible but measured slower: 74.102ms for the
      `VecDeque` adapter versus 82.505ms for direct vector drain over 1M
      four-request batches (0.9x), so the prototype was reverted. Follow-up
      2026-07-12: `dispatch_wayland` now splits incoming `WindowEvent`s by
      value into `(surface_id, payload)` and routes by borrowed `&str`, so
      the event-owned surface id is reused instead of cloning the routed
      target id for every event. Pointer-leave hide requests still allocate
      only when they need an owned `CoreRequest` surface id. Added payload
      preservation coverage and a release-only benchmark; the local dev-shell
      run measured 12.274ms for the old target-id clone path versus 7.151ms
      for split-by-value over 500k events (1.7x faster for the isolated
      dispatch id step). The listed `dispatch_wayland` allocation sources are
      now addressed; broader public `WindowEvent` surface-id API cleanup
      remains tracked under input normalization.
- [x] Minor idle-loop hygiene in `render_components`: the surface id String
      is cloned for every component before the `wants_render` gate
      (`runtime/render.rs:23`), and `component.id().to_string()` runs per
      rendering component per frame (`render.rs:66`); `reconcile_child_surface
      _requests` rebuilds `requested_keys`/`closing_keys` HashSets and
      re-clones entering-key sets per frame while any popover is open
      (`render.rs:432-499,524-527,669-673`). All small; fold into the v1.23
      interning pass. Progress 2026-07-06: `render_components` now checks
      `wants_render()` before cloning the parent surface id, so idle components
      do not allocate a surface-id `String` on each render pass. The remaining
      per-rendering-component id clone and popover reconciliation sets remain
      open. Progress 2026-07-10: rendering components now reuse the already
      cloned parent `surface_id` as the component/profiling id instead of
      calling `component.id().to_string()` again. A release-only benchmark over
      2M id reads measured 15.359ms for the extra clone versus 1.368ms for the
      borrowed surface id path (11.2x faster). Popover reconciliation set
      churn remains open. Progress 2026-07-10: child-surface reconciliation now
      stores requested child keys in a stack-backed `SmallVec` for the usual
      small-popover case instead of allocating a `HashSet<&str>` each frame.
      A release-only benchmark over 500k three-popover reconciliation
      membership checks measured 39.237ms for the `HashSet` path versus
      372.816us for `SmallVec` (105.2x faster). Closing-key owned set
      construction remains open. Follow-up 2026-07-12: `ShellComponent` now
      exposes a borrowed `set_closing_child_keys_from_slice` path, and child
      reconciliation compares stack-backed closing-key slices against the
      component's existing set before allocating owned strings. The local
      release benchmark measured 81.703ms for owned `HashSet<String>`
      construction versus 23.756ms for borrowed `SmallVec` comparison over
      500k steady-state checks (3.4x faster). The listed idle-loop allocation
      sources are now addressed.

Structure:

- [ ] **`legacy_backend_candidates_from_discovery` is a compat lane.** The
      graph-load failure fallback spawns backends from discovery-time module
      scanning (`backend/spawn.rs:48-59`, `backend/candidates.rs:300+`),
      duplicating the graph-driven candidate logic. Per the
      no-backward-compat rule: decide whether a missing/broken
      `config/module.json` should be a hard startup error (matching the
      manifest migration-diagnostics stance) and delete the legacy lane, or
      document why a degraded-mode boot is a product requirement. Currently it
      is a second candidate-selection implementation that can drift.
- [ ] Healthy/confirmed: the event loop is deadline-driven end to end —
      `next_runtime_sleep` computes exact deadlines from reload checks,
      command throttles, closing surfaces, popover hides, and component ticks
      (`runtime/mod.rs:76-151`) and blocks on Wayland fd + eventfd
      (`mod.rs:254-287`); all four reload checks park for 24 h when the
      inotify watcher is active and wake via `FilesystemChanged`
      (`reload.rs`, `theme.rs`, `mod.rs:391-397`); shell messages are drained
      with a 256 cap and coalesced with correct barrier semantics for
      lifecycle/interface-event ordering (`mod.rs:225-241,425-475`);
      `component_target_for_surface` rebuilds its index lazily on miss only
      (`mod.rs:46-66`); backend event bridges are per-provider Tokio tasks
      that wake the loop via eventfd writes (`backend/spawn.rs:100-241`);
      `flush_wayland` is TRACE-gated (`wayland.rs:225-244`). The remaining
      structural gap is the K phase-split (serial VM phase / parallel paint
      phase), unchanged by this pass.

### Suggested attack order

1. **Pointer-motion + scroll coalescing (J)** — one small diff in
   `dispatch_wayland_events`; divides all per-motion costs by the
   motion-to-frame ratio. Do this first.
2. Fractional-scale partial damage (D, first item) — biggest visible win on
   scaled outputs, bounded scope.
3. Per-node `StyleRuleIndex` rebuild on the build path (E) — turns every
   script-driven rebuild from O(nodes × rules) into O(nodes + rules); tiny
   diff.
4. Per-paint key/hit-test index (B + J) — kills the input-path tree clone,
   the 5-walk hover dispatch, and the per-paint `prune_stale` key sweep with
   one shared structure.
5. `sync_state_from_lua` write log (G) — removes per-handler full-globals
   conversion; helps every interaction.
6. Slider-drag reclassification + narrow-path gating (J) — makes drags cost
   a restyle instead of rebuild+diff overhead.
7. Element-metrics laziness (A) — removes per-paint JSON build/compare/convert.
8. Animation walk gating (F) — free win for the common no-animation surface.
9. Event routing index + payload `Arc` (C) — cheap, unblocks chatty backends.
10. Service-payload dedup + interaction rule-existence gate (K) — two small
    diffs that eliminate steady-state work at poll/hover frequency.
11. Per-surface parallel paint (K) — first threading step; needs the
    phase-split refactor of `render_components` but no new invalidation
    machinery.
12. Component-level render memoization (I) — largest structural win; plan it
    with the v1.18/v1.27 invalidation work since it shares the dependency
    bookkeeping.
13. State snapshot COW + typed expression/declaration values (A/E) — feeds
    the same invalidation work.
14. Paint/script pipelining + tile-parallel raster (K) — after the
    per-surface split proves the phase boundary; pairs naturally with the
    GPU work (v1.25).

---

## 2026-07-13 — Component-level render memoization shipped (section I)

`render_import` (`crates/core/shell/src/shell/component/composition.rs`) now
memoizes each imported/local component instance's built subtree
(`crates/core/shell/src/shell/component/memo.rs`). Entry key/validity:

- props fingerprint (props map + typed `EventHandlerCall`s, structural JSON
  hashing for args),
- the instance's own `ScriptState::mutation_generation` **and** every
  descendant instance's generation (descendants found by hierarchical
  instance-key prefix, so a nested child's state change invalidates every
  enclosing cached subtree),
- active theme `Arc` pointer identity (`refresh_active_theme` swaps the Arc
  only on real theme changes),
- active locale,
- container size the subtree was built against (first build runs pre-layout,
  so entries settle from the second build onward).

Build side effects are made replayable or vetoed via mark counters on
`FrontendSurfaceComponent`: promoted-popover wrappers and error placeholders
inside a cached subtree re-set their per-build presence flags on reuse;
surface-portal visibility writes (`pending_surface_states`) veto caching.
`bind:this` live bindings persist on the shared surface VM, so hits safely
skip re-installation. Tracked service reads accumulate on the `ScriptContext`
and cached nodes carry `service_field_reads`, so service observation and
`NodeServiceFieldDependencies` stay intact across hits. The cache clears in
`reset_render_caches` (theme change, locale change, source reload — the same
sites that clear runtimes).

Coverage: `component_memo.rs` integration tests for unchanged-sibling reuse
(identical pixels), prop-change invalidation with sibling reuse,
descendant-generation invalidation through an enclosing component, and
popover-promotion flag replay on cache hits. Release benchmark
(`memoized_rebuild_beats_full_child_reeval`): 200 full rebuild+paint cycles
of a 12-distinct-child surface measured 212.7ms forced-miss versus 134.5ms
memoized (1.6x end-to-end including the untouched restyle/layout/paint
stages; hits=2400/2400).

Known limits: repeated same-alias instances share one runtime (pre-existing
module-identity limitation), so their alternating prop application bumps the
shared generation and every lookup misses — correct but unaccelerated until
per-occurrence instance identity lands. `render_slot` instances are not yet
memoized. Only public script members are reactive (unchanged contract):
template expressions over private locals were never guaranteed to re-render.

---

## 2026-07-28 — child display-list cache evicts individually

area: retained rendering, promoted child surfaces

The promoted-child display-list cache keeps its 64-entry bound but now evicts
only the least-recently-used entry on a miss at capacity. The previous policy
cleared the entire map, so introducing one new popup could cold-start every
still-active popup even though only one slot was needed. Hits update a monotonic
recency stamp; misses scan at most 64 entries, remove one, and retain all other
lowered command streams.

**Measured.** Release under `nix develop`, five interleaved before/after runs
over 2,000 frames with 48 hot and 48 rotating 25-node popup trees:
flush-all took 118.756–127.112ms versus 30.694–31.272ms with individual LRU
eviction. Median time improved 3.86x, and cold display-list rebuilds fell from
8,000 to 2,048. The benchmark itself requires at least a 1.25x median
improvement and at least a 2x reduction in cold starts. The checked
`child_display_list_lru_speedup` CI baseline is 3.50x with 20% tolerance,
requiring at least 2.80x on other runners.

**Correctness.** The eviction regression fills the production-sized cache,
refreshes one entry, inserts at capacity, and verifies that only the oldest
entry leaves. The shell crate compiles in the Nix development environment; all
13 non-ignored `child_surface` tests pass. The broader shell suite reported 556
passing, 18 failing, and 123 ignored; none of those failures exercises the
64-entry cap or eviction path, so that result is recorded as the current shell
baseline rather than attributed to this checkpoint.

**Aggregate gate run.** `tools/check-performance` remeasured this checkpoint at
4.164x, passing its 2.80x threshold. The aggregate command exited nonzero only
because the existing `shared_theme_revision_cache_speedup` gate measured 3.175x
against its 4.40x threshold; that separate baseline was left unchanged.

---

## 2026-07-28 — shell restyles borrow stable CSS props

area: style resolution, interaction and animation restyles

`StyleResolver` now accepts either owned or borrowed component-prop maps. The
compiler paths that construct temporary prop maps retain owned semantics, while
shell restyles and animation passes borrow the `SurfaceCssProps` map that
already lives for the complete cascade. This removes a full `HashMap<String,
StyleValue>` clone, including every key and value allocation, from each
restyle-resolver setup without changing prop lookup, fingerprinting, or shared
theme-default cache identity.

**Measured.** Release under `nix develop` (rustc/cargo 1.94.0), five
interleaved before/after samples of 50,000 targeted-restyle resolver setups and
one live prop lookup with 32 component props: owned cloning took
120.079–121.414ms versus 42.551–43.847ms borrowed. Medians were 120.455ms and
42.979ms, a 2.80x improvement. The checked
`borrowed_style_props_speedup` baseline is 2.50x with 20% tolerance, requiring
at least 2.00x; the aggregate gate run measured 2.803x and passed.

**Correctness.** Owned and borrowed resolvers have direct fingerprint and
resolved-value parity coverage. `cargo check -p mesh-core-shell` passes; all 30
non-ignored animation-filtered shell tests pass. The restyle-filtered shell
slice reported 26 passing and the same two unrelated interaction failures
already present in the recorded 556/18 shell baseline. The elements suite
reported 197 passing and one unrelated order-sensitive assertion in
`indexed_theme_defaults_reuse_lowered_declarations_per_revision`.

**Aggregate gate run.** Every gate except the previously recorded
`shared_theme_revision_cache_speedup` passed. That unrelated gate measured
3.021x against its existing 4.40x threshold; its baseline was not weakened.

---

## 2026-07-28 — multi-rectangle damage shares one raster session

area: retained display-list replay, sparse damage raster

`FrontendSurfaceComponent::paint_pixel_regions` now clears every physical
damage rectangle and submits the complete region set through one renderer
entrypoint. For disjoint regions, the renderer traverses the selected display
commands once, shares one `PixelCanvasSession`, and replays each command only
for the clips it intersects. The existing single-region path is unchanged.
Physical clips that overlap after fractional scaling retain the old
region-major replay order, but still share the session and profiling reset, so
translucent compositing remains byte-for-byte compatible. Per-frame text,
raster-cache, and command-attribution metrics now come from one reset instead
of merging independently reset samples.

**Measured.** Release under `nix develop` (rustc/cargo 1.94.0) on an AMD Ryzen
7 7840HS. The workload is a retained 512×512 surface with 256 translucent
primitive nodes; each result totals six alternating samples of 250 frames for
the repeated-session control and batched path. Three complete runs measured:

| Disjoint regions | Repeated sessions | One session | Relative |
| --- | --- | --- | --- |
| 1 | 9.737–10.416ms | 9.601–10.172ms | 0.957–1.082x |
| 4 | 38.596–41.601ms | 11.668–11.948ms | 3.308–3.482x |
| 16 | 155.264–159.446ms | 27.524–27.844ms | 5.576–5.793x |

The checked `multi_damage_4_rect_speedup` baseline is 3.30x and
`multi_damage_16_rect_speedup` is 5.55x, both with 15% tolerance (minimums
2.805x and 4.7175x). The ignored release benchmark asserts those thresholds
directly and is also registered in `tools/check-performance`.

**Correctness.** Pixel-parity tests cover disjoint command-major replay,
attributed replay, and overlapping region-major fallback. The complete render
library suite reports 191 passing and 36 ignored; the shell damage-filtered
slice reports 18 passing and one ignored. `cargo check -p mesh-core-render
-p mesh-core-shell` passes under the Nix development environment.

**Aggregate gate run.** The aggregate command could not reach its final
baseline comparison because the unrelated pre-existing
`stable_child_id_reuse_beats_rewriting_slots` benchmark self-asserted at
1.051x against its 1.25x requirement and aborted the script before this new
entry in command order. The focused new benchmark passes both checked
thresholds; the existing baseline was not weakened. The previously recorded
`shared_theme_revision_cache_speedup` aggregate failure also remains open.

Focused gate confirmation after the final formatting pass measured 3.476x for
four regions and 5.662x for sixteen; both direct assertions passed.

---

## 2026-07-30 — compositor configure waiting leaves the shell thread

area: Wayland presentation, shell scheduler

`present_with_damage` and `surface_size` no longer enter the former
`wait_for_surface_configure` poll, whose deadline allowed one unconfigured
surface to occupy the shell thread for up to 500ms. Both paths dispatch only
events already available. Presentation returns the typed `PresentStatus::NotReady`
outcome when the compositor has not configured the surface yet.

The shell keeps damage for the already-painted buffer on its `SurfaceTarget`,
requests a retry, and excludes unconfigured surfaces from ready render work.
The main loop therefore waits on the Wayland connection fd instead of spinning;
after configure arrives, the retained damage is combined with any new damage
and committed. Missing surfaces remain renderable so the first pass can create
and configure them, and hidden surfaces preserve their existing detach/destroy
lifecycle.

**Structural gate.** `unconfigured_surface_returns_typed_not_ready_without_recording_present`
checks the presentation boundary, and
`unconfigured_surface_keeps_pending_frame_until_configure_retry` checks retained
damage, scheduler readiness, and the configured retry. The complete
presentation suite reports 58 passed / 12 ignored. The full shell suite reports
572 passed / 17 failed / 122 ignored; all 17 failures are in the pre-existing
style, shipped-surface, interaction, and debug-snapshot baseline areas, while
the new regression passes. `cargo check -p mesh-core-shell --tests` also passes
under `nix develop` with rustc/cargo 1.94.0.

---

## 2026-07-30 — retained text-measure contexts

area: layout, retained Taffy state

`PerSurfaceLayoutState` now retains text measurement inputs by `NodeId`, and
the retained Taffy measure callback borrows that map directly. Incremental
layout updates keep a clean entry's `Arc<str>` content and font inputs intact;
they replace it only when text content or a measurement-affecting style input
changes. Fresh and structural rebuilds populate the same state, while removing
a structural subtree removes its text entry as well.

This intentionally revisits the 2026-07-04 rejected scratch-map experiment:
that full-style-sync workload was 0.99x. Dirty-node style scoping since then
leaves clean text-context rebuilding as independently measurable work.

**Measured.** Release build (rustc 1.95.0, cargo 1.95.0) on the local
development host. The gate refreshes 512 stable text nodes containing 376-byte
strings for 2,000 passes after an initial retained layout. Rebuilding all text
inputs took 413.843–436.431ms; retaining and refreshing the contexts took
240.291–275.849ms, a 1.58–1.72x improvement. The checked ignored gate
`retained_text_context_reuse_beats_rebuilding_inputs` requires at least 1.25x.

**Verified.** `retained_text_context_keeps_clean_content_and_replaces_changed_content`
proves pointer reuse for clean content and replacement after a content change.
The focused retained-layout suite passes 6 tests (1 release-only benchmark
ignored); the new focused context slice passes 2 tests (1 release-only benchmark
ignored). All three release gate samples passed.

---

## 2026-07-30 — subscriber-proportional service delivery completed

area: service event routing

The shell-side delivery index now owns the complete event hot path. State
updates visit only fallback components, subscribers, and declared cache
consumers; named interface events visit only fallback components and the exact
service/event subscriber list. A per-delivery epoch prevents overlap from
dispatching or caching an update twice, without cloning or normalizing a target
vector per event.

Subscriber lists are normalized once when observation summaries change.
Component indices are appended in order, so rebuild-time `dedup` handles
duplicate update, cache, and named-event declarations without a sort. The
named-event case is correctness-significant: unlike state updates, it has no
epoch pass and previously could dispatch twice if a summary repeated the same
subscription.

**Measured.** Release build under `nix develop` (rustc/cargo 1.94.0), local
development host, three runs. The workload delivers 20,000 audio updates
through 256 components with one audio subscriber. A full component scan took
230.086–238.769ms; indexed delivery took 14.229–18.797ms, a 12.24–16.78x
improvement. The checked ignored gate
`service_delivery_index_beats_full_component_scan_benchmark` requires at least
5x.

**Verified.** The focused index slice passes four correctness tests (one
release-only benchmark ignored), including duplicate update/cache/event
normalization. The cache-warming regression passes separately. All three
release gate samples passed. The broader `service_` slice reports 63 passing,
14 ignored, and the existing
`raw_service_state_update_schedules_repaint_without_proxy_tracking` failure
from the retained-layout baseline; all delivery-index tests pass in that run.

---

## 2026-07-30 — structural-sharing component memo hits completed

area: component composition and memoization

`WidgetNode` now separates immutable authored payload (tag, attributes,
handlers, module identity, and service reads) from live runtime overlays. Both
payload and child topology use copy-on-write `Arc`s, so storing and reading a
component memo shares a clean subtree rather than deep-cloning it twice. Style,
layout, accessibility, interaction, scroll, and identity data remain local to
the live tree; a mutation copies only the branch it changes.

The tree regression proves a child mutation does not affect the cached source
and leaves untouched siblings shared. The component-memo integration regression
proves a real cache hit retains shared authored payload after runtime
finalization. Existing prop, descendant-generation, repeated-alias, loop, slot,
and popover memo regressions remain covered.

**Measured.** Release build under `nix develop` (rustc/cargo 1.94.0), local
development host, three runs of the checked
`cow_widget_tree_clone_beats_legacy_deep_clone` gate. Each run clones twice for
2,000 iterations. The 273-node wide shape took 674.353–686.963ms with the
legacy deep clone versus 245.772–271.893µs with COW (2,526.59–2,776.85x), and
its exclusive retained heap payload fell from 418,323 to 120 bytes. The
97-node deep shape took 211.857–214.085ms versus 256.457–263.581µs
(803.27–826.09x), falling from 147,826 to 120 exclusive bytes. The gate requires
at least 5x in both shapes and at least 10x less exclusive retained payload.

**Verified.** The focused debug component-memo suite passes 8 correctness
tests (3 release benchmarks ignored). Its three release end-to-end gates also
pass: distinct-child memoization 1.4x, repeated aliases 1.16x, and slots 1.2x
against forced misses in this run.

---

## 2026-08-25 — bound decoded asset and rendering cache residency

`working tree` · area: rendering/resource cache budgets

Decoded image, raster, glyph, font, text-layout, glyph-atlas, Skia image and
gradient-shader caches now track resident byte weights in addition to entry
counts. Icon resolution metadata, glyph-map parsing, named-font availability,
intrinsic text measurement, ellipsis shaping, and the last-good glyph fallback
map use the same bounded policy. Font and glyph inputs reject oversized files
or dimensions before rendering; PixelBuffer keeps a process-wide allocation
reservation and per-buffer byte/dimension limits; Wayland SHM retains its
per-buffer, per-surface, and SlotPool byte gates.

No speedup claim is made for this correctness/budget change.

**Verified.** `cargo fmt --all`, `cargo check --workspace`, the full
`mesh-core-render` library suite (250 passed, 36 ignored), and the full
`mesh-core-icon` library suite (31 passed, 3 ignored) passed. Focused
PixelBuffer, glyph, text, painter, layout-cache, icon, and SHM slices passed.
The broad `mesh-core-elements` and `mesh-core-presentation` suites report
unrelated failures in unchanged tests (10 and 2 tests respectively); those
failures are recorded in the implementation handoff.

## 2026-09-02 — serialize settings/profile CAS and move shell durable writes

Settings and profile revision-checked replacements now use a stable
`.mesh-control-plane.lock` advisory lock around the on-disk read/check/write
boundary, with checked revision increments that reject exhaustion rather than
wrapping. CLI profile and settings mutations route through those CAS methods.
Shell control-plane, profile node, provider, and module-enable writes run on a
worker and publish their live completion only after the durable write is
acknowledged; backend provider handoff persistence uses the same worker path.

No performance measurement was run and no speedup claim is made. Verification
included `cargo fmt --all`, `nix develop -c cargo check -p mesh-core-shell
-p mesh-tools-cli`, all 55 `mesh-core-config` library tests, focused profile
CAS tests, focused shell settings/theme/locale tests, and backend persistence
tests. The full 952-test shell library run completed with 774 passed, 47
failed, and 131 ignored; its failures were unrelated geometry/rendering,
fixture, and retained-layout tests. The two focused navigation geometry tests
also fail independently with existing anchor mismatches.

## 2026-09-03 — generation-cached debug snapshot publication

area: shell debug telemetry publication

The shell now keys the serialized debug service payload by shell-owned state,
module-graph, interface, frontend-catalog, resource, settings, theme, locale,
and diagnostics generations. Unchanged non-profiling frames reuse the payload
without rebuilding or publishing it. Profiling keeps a bounded 100ms
publication cadence, and state changes invalidate the cache at lifecycle,
surface, request, resource, component, and diagnostic boundaries. A release
benchmark covers both cache behavior and serialized payload size across the
requested graph shapes.

**Measured.** Machine: Linux 6.12.93 x86_64, Intel Core i5-6200U (2 cores / 4
threads); `nix develop`; rustc/cargo 1.94.0. Gate:
`debug_snapshot_generation_cache_release_benchmark`, with
`--release --features allocation-profiling`, `CARGO_PROFILE_RELEASE_LTO=off`,
and `CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16`. The gate runs 24 workload shapes
(16/128/512 modules × 4/32 surfaces × 0/100 diagnostics × profiling off/on),
with 1,000 repeated frames for forced-rebuild publication and 1,000 repeated
frames for steady-generation publication per shape. Ranges below are the
minimum and maximum across those shapes; p50/p95 are the per-shape sample
percentiles.

| Measurement | Forced rebuild/publication | Generation cache |
| --- | ---: | ---: |
| p50 frame time | 2,749,031–93,685,204 ns | 496–592 ns |
| p95 frame time | 3,017,630–112,437,903 ns | 504–824 ns |
| Speedup range | — | 4,998.2–167,894.6x |
| Serialized payload | — | 21,766–564,525 bytes |
| Snapshot rebuilds | 1,000/shape | 1/shape |
| Publications | 1,000/shape | 1/shape |
| Allocations | 4,721,568–159,251,553 | 0 |
| Allocated bytes | 361,442,058–12,922,522,912 | 0 |

At the largest shape (512 modules, 32 surfaces, 100 diagnostics, profiling
on), p50/p95 fell from 93,685,204/112,437,903 ns to 558/566 ns, allocations
from 159,251,553 to 0, and allocated bytes from 12,922,522,912 to 0; rebuilds
and publications fell from 1,000 to 1. The focused publisher tests also pass,
including the 110ms profiling-cadence case. The repository-default thin-LTO
release build was stopped after more than 15 minutes before benchmark
execution; the LTO-disabled release profile above is the completed, recorded
measurement fallback.

**Verified.** `cargo fmt --all -- --check`, `git diff --check`,
`nix develop -c cargo check --workspace`, the three focused publisher tests,
and the allocation-enabled release gate passed. The full shell library suite
reported 779 passed, 47 unrelated baseline failures, and 132 ignored; the
failures remain in existing rendering, interaction, fixture, lifecycle, and
profiling tests (including the legacy `mesh.surfaceLayout` fixture).

## 2026-09-03 — coherent diagnostics snapshot publication

area: diagnostics health/history/debug publication

`Diagnostics::snapshot` now takes one state lock, clones and sorts the issue
history once, and derives active issues and health from that immutable view.
The collector builds module aggregates from those per-instance snapshots, and
shell debug publication reuses one collector snapshot for health and issue
payloads instead of collecting diagnostics twice.

**Measured.** Machine: Linux 6.12.93 x86_64, Intel Core i5-6200U (2 cores / 4
threads); `nix develop`; rustc/cargo 1.94.0; repository-default `--release`
profile (thin LTO, one codegen unit). The benchmark ran 100 repeated snapshot
operations for each workload: 128 modules × 4 instances, with 0, 10, or 100
retained issues per instance. The legacy column models the former collector's
three per-instance lock acquisitions (health, history, and active history);
the snapshot column takes one lock and one issue-code sort per instance.

| Issues/instance | Legacy p50/p95 ns | Snapshot p50/p95 ns | Locks/100 operations | Allocations | Allocated bytes | Payload bytes |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 0 | 473,056 / 898,928 | 409,962 / 497,839 | 153,600 / 51,200 | 117,301 / 117,401 | 8,805,600 / 8,860,000 | 48,927 |
| 10 | 20,350,866 / 29,340,088 | 20,099,855 / 23,701,751 | 153,600 / 51,200 | 7,707,701 / 7,759,001 | 397,122,400 / 584,773,600 | 4,002,623 |
| 100 | 208,554,743 / 247,797,441 | 168,812,011 / 195,522,694 | 153,600 / 51,200 | 72,373,301 / 72,578,201 | 3,900,088,800 / 5,477,922,400 | 39,773,663 |

Values in paired columns are legacy / snapshot. Across the three workloads,
p50 improved 1.01–1.24x and p95 improved 1.02–1.81x; lock acquisitions fell
from 153,600 to 51,200 for every shape, and the serialized payloads were
byte-for-byte equal. The owned `active_issues` compatibility field means this
snapshot representation allocates slightly more in the measured shapes:
117,301–72,373,301 to 117,401–72,578,201 allocations and
8,805,600–3,900,088,800 to 8,860,000–5,477,922,400 allocated bytes. Those
allocation results are recorded rather than hidden behind a gate.

Gate: `diagnostics_snapshot_release_benchmark`, run with
`nix develop -c cargo test -p mesh-core-diagnostics --release --
diagnostics_snapshot_release_benchmark --ignored --nocapture`.

The focused coherent-snapshot test and shell debug publication tests passed.
The full shell library suite reported 779 passed, 47 unrelated baseline
failures, and 132 ignored; those failures remain in existing rendering,
interaction, fixture, lifecycle, and profiling tests.

## 2026-09-03 — benchmark settings schema validation

`working tree` · area: settings schema validation

Added the ignored release-only gate
`settings_schema_validation_release_benchmark` to the settings unit-test
module. It exercises the public transactional schema replacement path over the
required 1/32/256 namespace × 8/32/128 declared-field matrix. Each synthetic
namespace uses a mixed schema of booleans, bounded integers, enums, string
arrays, bounded numbers, strings, and nested objects. The settings document has
one valid value for every declared field. The gate performs three warmup
replacements and 30 measured replacements per shape, and reports fixture
schema/settings bytes, latency, allocation count, allocated bytes, and tracked
live retained bytes. A thread-local test allocator keeps counters isolated from
other unit tests.

**Measured.** Machine: Linux 6.12.93 x86_64, Intel Core i5-6200U (2 cores / 4
threads); `nix develop`; rustc/cargo 1.94.0; repository-default optimized
release profile (thin LTO, one codegen unit). Gate command:
`nix develop -c cargo test -p mesh-core-config --release --
settings_schema_validation_release_benchmark --ignored --nocapture`. Each row
below reports the minimum–maximum median and p95 from three complete gate runs;
allocation values are stable for all 30 samples and all three runs.

| Namespaces × fields | Schema bytes | Settings bytes | Median ns | p95 ns | Allocations | Allocated bytes | Retained bytes |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 1 × 8 | 574 | 237 | 111,726–124,982 | 140,564–176,760 | 426 | 49,973 | 25,788 |
| 1 × 32 | 2,115 | 839 | 375,690–446,080 | 416,921–582,650 | 1,314 | 159,677 | 93,182 |
| 1 × 128 | 8,303 | 3,271 | 1,440,461–1,696,985 | 1,485,872–3,334,967 | 4,866 | 597,386 | 362,020 |
| 32 × 8 | 18,337 | 6,995 | 2,925,749–3,254,377 | 3,172,816–3,918,615 | 10,149 | 1,202,382 | 750,508 |
| 32 × 32 | 67,649 | 26,259 | 12,289,193–14,022,618 | 13,026,731–20,390,894 | 38,565 | 4,712,910 | 2,907,116 |
| 32 × 128 | 265,665 | 104,083 | 56,103,052–60,919,406 | 62,593,335–65,524,311 | 152,229 | 18,719,598 | 11,509,932 |
| 256 × 8 | 146,689 | 55,827 | 27,233,236–27,363,239 | 29,876,185–36,242,652 | 80,446 | 9,562,534 | 6,006,180 |
| 256 × 32 | 541,185 | 209,939 | 118,446,208–124,054,311 | 132,070,773–149,376,242 | 307,774 | 37,646,758 | 23,259,044 |
| 256 × 128 | 2,125,313 | 832,531 | 499,894,319–519,039,295 | 550,854,113–597,393,335 | 1,217,086 | 149,700,262 | 92,081,572 |

No before/after comparison or optimization is claimed; this establishes the
current validation baseline before considering borrowed schema maps, reusable
paths, or output reservation. The first harness revision incorrectly reset
live-byte accounting for each replacement and reported zero retained bytes; it
was corrected before the three final runs above, and those preliminary numbers
are not used.

## 2026-09-03 — capability activation grant materialization

area: activation-time capability-set cloning

Added the ignored release-only gate
`capability_activation_release_benchmark` to the capability foundation test
module. It follows the current activation boundary: each restart resolves a
fresh effective-grant map for 500 modules, then materializes the runtime
`CapabilitySet` for four distinct active root instances. Every module declares
10 catalog capabilities (five required and five optional), and all ten are
approved. The fixture and the final activation snapshot remain live while the
allocator records per-restart activity, so retained bytes report the activation
snapshot above the fixture baseline.

**Measured.** Machine: Linux 6.12.93 x86_64, 4 logical CPUs; rustc/cargo
1.95.0; repository-default optimized release profile (thin LTO, one codegen
unit). Gate command:
`cargo test -p mesh-core-capability --release --
capability_activation_release_benchmark --ignored --nocapture`. Each gate run
uses three warmups and 50 measured restart cycles; the ranges below span three
complete gate runs.

| Measurement | Run 1 | Run 2 | Run 3 | Three-run range |
| --- | ---: | ---: | ---: | ---: |
| Latency min–max ns | 3,625,588–6,759,164 | 3,633,419–4,362,411 | 3,660,816–5,145,725 | 3,625,588–6,759,164 |
| Latency median ns | 3,684,253 | 3,681,029 | 4,791,732 | 3,681,029–4,791,732 |
| Latency p95 ns | 4,366,496 | 4,133,607 | 5,056,627 | 4,133,607–5,056,627 |
| Allocations/restart | 17,546 | 17,546 | 17,546 | 17,546 |
| Allocated bytes/restart | 820,456 | 820,456 | 820,456 | 820,456 |
| Retained activation bytes | 735,456 | 735,456 | 735,456 | 735,456 |

The fixture retained 517,580 bytes, and the final activation retained the same
735,456-byte activation delta in every run. No before/after comparison or
optimization is claimed; this records the current capability-resolution and
runtime grant-copying baseline before considering shared immutable resolved
grants.

## 2026-09-05 — retained-render allocation-profiler workload

area: retained rendering and allocation profiling

Added `allocation_profiler_retained_render_overhead` to the shell's
allocation-profiler performance gate and registered it in
`tools/check-performance`. The workload builds a 1,026-node retained display
tree (five groups with 204 items each), runs 12 warmup and 120 measured frames,
and changes one group transform every sixth frame. Each frame updates the
retained display list, selects minimal-damage paint commands, clears damage, and
replays the selected commands. Five samples interleave normal profiling with a
tracking-suspended control, keeping the real render and paint work identical.

**Measured.** Linux 6.12.93 x86_64, 4 logical CPUs; `nix develop`; rustc/cargo
1.94.0; optimized release with LTO disabled and 16 codegen units because the
repository-default thin-LTO shell test build did not finish within 17 minutes.
The gate command was
`nix develop -c env CARGO_PROFILE_RELEASE_LTO=off
CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16 cargo test -p mesh-core-shell
--features allocation-profiling --release --
allocation_profiler_retained_render_overhead --ignored --nocapture`. The
ranges below span three complete runs; each run contains five interleaved
tracked/control samples.

| Measurement | Run 1 | Run 2 | Run 3 | Three-run range |
| --- | ---: | ---: | ---: | ---: |
| Tracked frame-loop ns | 359,487,565–378,735,110 | 361,258,662–370,259,040 | 352,072,692–380,220,570 | 352,072,692–380,220,570 |
| Tracking-suspended frame-loop ns | 353,002,744–375,372,870 | 352,758,460–380,869,722 | 350,732,369–387,721,775 | 350,732,369–387,721,775 |
| Tracked median ns | 360,972,153 | 363,627,102 | 363,817,555 | 360,972,153–363,817,555 |
| Tracking-suspended median ns | 353,532,542 | 355,180,534 | 356,319,275 | 353,532,542–356,319,275 |
| Overhead ratio | 1.018x | 1.027x | 1.009x | 1.009–1.027x |
| Tracked allocations/120 frames | 87,460 | 87,460 | 87,460 | 87,460 |
| Tracked allocated bytes/120 frames | 69,864,360 | 69,864,360 | 69,864,360 | 69,864,360 |

The gate reports the median overhead ratio and requires it to remain below
4.0x; the configured `allocation_profiler_retained_render_overhead` baseline
is 1.10x with 25% tolerance. The retained workload rebuilt 5,120 entries over
20 dirty frames, retained 100 clean frames, and selected 4,120 paint commands.
The existing direct 64-byte allocator gate also passed at 1.307x. The aggregate
`tools/check-performance` run could not reach this new gate because the
pre-existing `stable_child_id_reuse_beats_rewriting_slots` gate stopped the
sequence at 1.023x against its 1.25x direct requirement; the focused new gate
passed independently.


## 2026-09-21 — semantic capture after retained diff and shared display fingerprints

`working tree` · area: frame finalization, semantic snapshots, retained rendering

Completes the three remaining render-pipeline backlog items. `finalize_tree`
prepares live annotations and layout; the paint boundary samples animation,
refreshes affected semantics/selection, runs the authoritative retained diff,
then publishes `FrameSnapshot::capture_dirty`. Insert/remove/reorder frames use
full capture, and the existing visibility/reference fallback remains in place.
Snapshots now contain sampled animation styles and geometry. This supersedes
the earlier entry's **Not done — `FrameSnapshot::capture_dirty`** status without
changing that historical record.

Retained updates distinguish subtree roots from individual annotation changes
and normalized ancestors. Focus/hover without selector dependencies therefore
still reaches semantic capture without promoting an ancestor's entire subtree.
Accessibility normalization uses the previous frame's indexed paths and cached
child text; inherited visibility/disabled changes include descendants, and
restyled subtrees are explicitly included. Full normalization remains the
structural/broad fallback. Runtime string projection skips unchanged ordinary
retained nodes; mutable controls and shortcut/promotion candidates remain
conservative. Selection projection is retained across unchanged frames, clears
the old target on move/removal, and refreshes its origin after animation.
Geometry and overflow traversal are still required; this is not a claim that
all frame work is proportional only to dirty nodes.

Display entry signatures consume the retained `RenderObjectFingerprint` paint
inputs through a lookup, including sparse batch-index collection. Missing
fingerprints or stale resource revisions reconstruct paint inputs. Callers
without retained fingerprints construct paint inputs once per selected node,
rather than once per primitive slot.

**Measured.** Local AMD Ryzen 7 7840HS, x86_64 Linux, release profile built in
`nix develop`; three paired samples per workload. These are isolated stage
comparisons, not end-to-end shell frame timings; concurrent build/desktop load
contributed to the ranges. No speed threshold gate was added.

- `dirty_semantic_finalization_benchmark`: 1,025 nodes (one column, 1,024 text
  leaves), 100 frames changing one leaf's focus. Full normalization plus full
  capture: **270.829–322.552 ms**; scoped normalization plus dirty capture:
  **12.746–15.680 ms**. Paired ratios: **18.7–24.5x**.
- `shared_retained_display_fingerprint_benchmark`: the same 1,025-node shape,
  100 display-entry collections with precomputed retained fingerprints.
  Fresh per-node paint inputs: **43.060–92.610 ms**; shared retained inputs:
  **19.058–39.040 ms**. Paired ratios: **2.26–2.37x**. Retained fingerprint
  construction is outside both collection timings because the shell already
  performs that authoritative pass.

**Validation.** Added differential coverage for inherited semantic policy,
child-derived names, restyled descendants, structural fallback, exact-node
retained updates with geometry propagation, reused/stale-resource display
fingerprints, and complete snapshots across focus/hover/checked/selection
changes without state selectors. The animation regression also checks captured
styles/layout against the sampled live tree. New and augmented tests pass.
`cargo check -p mesh-core-shell` and `git diff --check` pass.

Full debug suites: elements **268 passed / 10 failed**, renderer **257 passed /
0 failed**, shell **806 passed / 32 failed**. A detached checkout of unchanged
`1e734133` reproduced the identical ten elements fixture failures and 32 shell
failures (267 and 804 passing respectively); renderer passed 256 tests there.
An initial renderer run hit an icon-worker assertion followed by poisoned-lock
failures; the full rerun passed. During implementation, recording full-build
annotation candidates as exact dirty nodes regressed two narrow-service scope
checks; restricting that tracking to retained annotation restored the baseline.
The temporary baseline checkout was removed and test-written font settings
were restored to their initial contents.

## 2026-09-30 — checkpoint waves 1–3; live hover follow-up and wave 4 design

`1b7c3158` `e930668f` `9087b71b` `84128e0c` `1253ce1d` · area: profiling,
scroll/animation, compositor blur, retained scroll coordinates

Committed the original mixed tree on main: session profiling tools;
hover/focus fixes, navigation profiling fixes, and waves 1–2; compositor blur;
planning/measurement records; local settings and tool permissions. The shell
changes share files and retained-state dependencies, so the hover/focus and
scroll fixes remain together. No push was performed.

**Live follow-up.** Hyprland, the existing 2880×1800 output at scale 1, current
`profiling` build (release optimization plus symbols), stage profiler enabled,
`perf record -e cpu-clock -F 499 --call-graph dwarf,16384`. Capture
`profiles/session/20260930-181254`, report alongside it. An automated sweep
moved the pointer across the bar twice at y=78 for approximately 20 seconds
(120 positions, 166 ms between moves), restoring its prior position afterward.
The full capture includes startup/idle and concurrent desktop use; its activity
classifier assigns 233 bar input events to six hover windows totaling 4.66 s,
with 110 renders. Single diagnostic capture, not a repeated A/B benchmark:
hover CPU 14% of one core (shell thread 13.1%), mean render 5.190 ms, mean paint
0.565 ms. Across all 3,208 CPU samples, symbolized stacks contain no `blur`,
`SkImageFilter`, or `apply_backdrop` symbol. This supports disappearance of the
client blur hotspot; it does not establish a matched 20% before/after saving.
The earlier `20260930-180735` attempt captured mostly idle because the old
Hyprland pointer dispatcher syntax failed; excluded from the hover assessment.
Perf's final symbol collection took minutes; reports were regenerated after
it completed, avoiding a misleading intermediate zero-sample report.

Screenshots of the running bar were inspected: rounded corners have no obvious
rectangular halo. The dark wallpaper makes blur strength inconclusive by eye;
the live backend reports `ext_background_effect_manager_v1` blur capability.
The ordinary release shell was restored after capture. Stale, unlistened IPC
sockets left by termination were removed before each restart.

**Wave 4 started.** The pending scroll-coordinate design links from the existing
backlog item and covers offset-specific dirt, retained coordinate scopes,
signed content bounds, replay-time viewport culling, scope-balanced selection,
and shared damage/blur geometry. Raster shifting remains dependent work; item
14 is still open. Added
`retained_scroll_reveals_content_and_matches_fresh_full_paint`: newly exposed
children, nested overflow, fixed descendants, fractional offsets at scales
1 and 1.25, reverse scrolling, and a simultaneous material update compare
partial retained replay against freshly built full paint.

**Verified.** Renderer 261 passed / 0 failed / 36 ignored; presentation suite
passes; animation crate 40 passed. Focused shell scroll suite: 23 passed /
5 failed / 3 ignored. Shell animation group: 26 passed / 4 failed / 1 ignored;
the new scheduling, annotation, and diagnostic tests pass. Failures concern
older retained-scope, scroll-into-view, native fallback, and volume-command
fixtures; no production code was changed in this follow-up. Existing earlier
records document shell baseline failures; no fresh baseline comparison was
performed here. New test rustfmt and `git diff --check` pass. Workspace-wide
rustfmt check reports pre-existing formatting differences outside this work.

## 2026-09-30 — wave 4 item 14: retained scroll content coordinates

Offset dirt is distinct from layout/extents. Eligible translation-only scroll
content retains immutable descendant commands and content-space entry inputs;
one balanced canvas translation sits under the anchored affine/rounded viewport
clip. Owner background, border and scrollbars remain outside the scope. All
visible content is retained, including offscreen rows, and culled during replay.
Frame plans expose scope geometry; moving it advances generation and damages
the old/new anchored viewports. Material, layout, clip, resource and admission
changes keep conservative invalidation. Entry damage inside scopes is deliberately
viewport-wide, not a guessed unsigned content-space rectangle.

Admission rejects scale/rotation, fixed descendants, opacity/blend/filter and
backdrop dependencies. Nested scrollers retain existing baked inner geometry
inside the outer scope; changing an inner offset rebuilds its content. The
general Section 12 transform/clip item remains open. The single-command
AnyRender proof explicitly defers content without its enclosing scope instead
of claiming incorrect surface-space encoding. Scope admission transitions and
rounded viewport clips were corrected before final validation.

**Measured:** `cargo test -p mesh-core-shell --release settings_scroll_frame_gate
-- --ignored --nocapture`, desktop in use, stage profiling disabled. Baseline
detached worktree `69705c95` (production code identical to checkpoint `1253ce1d`),
with only the same diagnostic counter printing added. Three alternating runs
per side, no concurrent builds. Workload: 920×900 shipped Settings Appearance,
240 entries in each resource catalog (bounded rendered lists), 60 two-finger
4px frames plus 30 momentum frames at 16ms intervals. Both sides: 90 paints for
90 frames, no restyle after the first.

| Metric | Checkpoint | Content coordinates |
| --- | --- | --- |
| Frame p50 | 4.714–5.070 ms | 4.154–4.519 ms |
| Frame p90 | 6.918–7.113 ms | 6.423–6.502 ms |
| Entry rebuilds / 90 frames | 24,714–25,019 | 0 |
| Command rebuilds / 90 frames | 4,526 | 704 |
| Final retained commands / nodes | 112 / 97 | 182 / 165 |
| Peak process RSS (`RUSAGE_CHILDREN`) | 65,288–66,232 KiB | 65,204–66,116 KiB |

These are complete-frame timings, not isolated paint/update attribution. The
memory ranges overlap; retaining more offscreen commands is observable but
these process measurements do not isolate their byte cost. Earlier exploratory
runs while builds were active ranged from 3.1 to 4.2ms and are excluded. The
historical 8.2–8.4ms wave-2 measurement is not a matched baseline for this run.
Raster work still paints visible glyphs; no raster-cache speedup is claimed.

**Checked structural gate:** `scroll_scope_reuses_offscreen_content_commands_and_entries`
retains 40 rows and proves unchanged descendant command `Arc`s and entry maps
across forward, reverse and fractional offsets, zero entry reconstruction,
three rebuilt wrapper/owner commands, and immutable old frame-plan geometry.
Pixel regressions compare partial retained replay with freshly built full paint
at scales 1, 1.25 and 2, newly exposed text, rounded clipping, nested scrolling
and scrollbar painting, material edits, admission removal/restoration and
viewport/extent changes. Complex fixed/nested fallback regression also passes.

**Validation:** renderer serial suite 264 passed, 0 failed, 36 ignored. A prior
parallel attempt hit the existing icon-worker assertion and poisoned locks;
serial reruns pass. Focused shell scroll suite: 23 passed, 5 failed, 3 ignored;
same five fixture failures recorded in the checkpoint follow-up (scope
fingerprinting, scroll-into-view, native fallback, volume command expectations).
Rustfmt on touched Rust files and `git diff --check` pass. Item 14 leaves the
backlog; its completed design is replaced by the pending item-15 raster contract.
The pre-existing settings revision-only change (137 → 146) is preserved separately.

## 2026-09-30 — wave 4 item 15 started: pixel shifting contract

`9558a889` checkpoints item 14 before cache work. Added an overlap-safe integral
device-pixel shift for standalone content rasters: rows move in the safe vertical
order with `copy_within`, newly exposed pixels clear to transparent, and at most
two disjoint strips need repainting. Empty/zero shifts do nothing; viewport-sized
or extreme signed displacements clear and expose the entire raster. Backing
storage never moves. This helper is not wired into production painting yet.

Independent byte/coverage oracle exhausts 0–7px widths, 0–5px heights, signed
horizontal shifts −9…9 and vertical shifts −7…7 plus both i32 extremes. Every
destination pixel is checked against the original raster, with exactly one
strip covering each newly exposed pixel and none covering retained pixels.
Text regression shifts a 112×64 transparent viewport containing 20 rows through
forward/reverse/large integral shifts, repairs only exposed strips, and compares
against fresh full paint at scales 1, 1.25 and 2. Both tests pass.

No cache-hit or performance saving is claimed. The bounded cache, content/resource
identity, rounded-mask-at-composition rule, partial-damage validity, eviction,
profiling parity and representative A/B measurement remain in the existing
backlog item and its pending design. Wave 5 remains lower-priority dependent
workflow, beginning with its partial-repaint blur pixel test rather than a cache.

Final renderer serial suite: 266 passed, 0 failed, 36 ignored. Touched files
pass rustfmt and `git diff --check`. The existing local settings revision-only
change is checkpointed in a separate chore commit; no preference values change.

## 2026-10-10 — hover-intent workload: script tasks and typed shell calls

New gate: `navigation_hover_profile` (release, ignored, in
`real_surfaces/navigation_profile.rs`). It runs a 1920-wide navigation bar,
25 cycles over its 4 popover triggers, 400 steps in all. Each trigger step is:
pointer move onto the trigger (the popover opens), bar `PointerLeave` (the
close bridge is scheduled), pointer move into the promoted popup (cancel), and
popup `PointerLeave`. Each step is one input followed by the frame the shell
would paint. It reports step (input + frame) and input-dispatch percentiles,
plus `mesh.events.publish` calls drained (a test-only counter).

Baseline: detached worktree at `7614977e` with only the bench and the counter
added. Five runs per side, nothing else compiling. Both sides end with 4
popovers still open, from the known popup-leave defect (see the October log).

| Metric | `7614977e` | Tasks + typed shell calls |
| --- | --- | --- |
| Publishes per run (per popover open) | 454 (4.54) | 0 |
| Step p50 | 1.775–1.960 ms | 1.793–1.922 ms |
| Step p90 | 2.225–3.763 ms | 2.623–3.786 ms |
| Dispatch p50 | 0.160–0.190 ms | 0.171–0.179 ms |
| Dispatch p90 | 0.323–0.393 ms | 0.347–0.378 ms |

The removed publishes do not show in time on this workload; paint dominates
each step. An intermediate version, where the theme and language triggers
called `cancelPendingClose()` through `bind:this` on every cancel, measured
dispatch p50 at 0.188–0.205 ms over five runs. Each such call resyncs the
popover's state. It now crosses only while the popover has a close pending.

Microbenchmark `typed_shell_call_beats_publish_path` (release, ignored,
`shell/service.rs`) compares 20,000 handler calls that each queue one shell
request, followed by drain and lowering to a `CoreRequest`. The typed side
calls `mesh.shell.toggle_surface`. The publish side publishes
`shell.toggle-debug-overlay`, the cheapest remaining `shell.*` channel. Over
five runs, publish took 138.7–142.7 ms and typed 123.4–130.4 ms, a 1.08–1.14×
ratio. Most of each ~6.5 µs is the handler call itself.

## 2026-10-10 — `navigation_hover_profile` after the popup-leave fix

Release, `navigation_hover_profile`, the same 25 cycles × 4 triggers as the
previous entry. Five runs:

| Metric | Tasks + typed shell calls (prior entry) | Popup leave fixed |
| --- | --- | --- |
| Step p50 | 1.793–1.922 ms | 2.103–2.121 ms |
| Step p90 | 2.623–3.786 ms | 4.021–4.048 ms |
| Dispatch p50 | 0.171–0.179 ms | 0.211–0.217 ms |
| Popovers open at the end | 4 | 0 |

The workloads are not the same. Before the fix, the popup's `PointerLeave`
dispatched nothing, so no close bridge was scheduled and no popover ever
closed. Now every cycle runs the popup's leave handlers, a `task.delay`
bridge, and the close and its animation frames. The extra time is that
work. It is not a cost added to the old path: the eligibility walk change is two
flag reads per visited node. The bench now asserts that every popover closes.

## 2026-10-11 — `navigation_hover_profile` with one hover intent

Release, `navigation_hover_profile`, same workload (25 cycles × 4 triggers).
Five runs each.

| Metric | Popup leave fixed (2026-10-10) | One hover intent |
| --- | --- | --- |
| Step p50 | 2.103–2.121 ms | 1.927–2.042 ms |
| Step p90 | 4.021–4.048 ms | 3.196–3.668 ms |
| Dispatch p50 | 0.211–0.217 ms | 0.205–0.217 ms |
| Popovers open at the end | 0 | 0 |

Each popup leave used to schedule two timers (the popover's and, through
`PopoverLeave`, the trigger's) and resync across `bind:this` on every cancel.
Now it schedules one. Dispatch is within noise; the step gain comes from the
frames that no longer run the popover-side close.

## 2026-10-11 — D1 prototype: binding patches on the navigation bar

Release, `navigation_frame_cost_profile` (1920-wide bar, 60 frames per loop),
`MESH_TEMPLATE_PATCHING` off vs on, interleaved, five runs each:

| Loop | Off | On |
| --- | --- | --- |
| Audio poll (read) | 4.033–4.431 ms | 2.212–2.322 ms |
| Pointer move | 0.790–0.878 ms | 0.806–1.052 ms |
| Paint only | 0.315–0.331 ms | 0.320–0.368 ms |
| Render hooks alone | 0.509–0.541 ms | 0.559–0.618 ms |

Audio-poll stages with patching on, one run: no `tree_build` (1.99 ms per
frame off), `style_restyle` 0.200 ms (1.81 off), `layout` 0.245 ms (0.11 off;
the patched text relayouts). About 1.5 ms per frame remains outside the frame
stages: render hooks, the patch's expression evaluation, and service delivery.

`navigation_hover_profile`, three runs each: step p50 1.97–2.01 ms off and
1.88–1.98 ms on, dispatch p50 0.245–0.260 ms off and 0.195–0.258 ms on.
Within noise: hover frames are interaction restyles, not script writes.

Pointer move and render hooks overlap between modes; no regression is claimed
or ruled out at this sample size.


## 2026-10-13 — D1: binding patches enabled by default

`working tree` · area: shell template updates and profiling

Removed the `MESH_TEMPLATE_PATCHING=1` opt-in. Handler and render-hook writes
whose readers are covered by recorded element bindings now use the existing
in-place patch path. Profiling snapshots report `patched_node_count`, so a
patched update is distinguishable from a narrow rebuild. Direct service-field
reads, structural conditions, loops, component props, and component-function
calls retain the template-evaluation fallback. The component contract and
syntax guide describe the boundary; D1's remaining work stays in the backlog.

**Measured.** Same workstation, release, `navigation_frame_cost_profile`,
1920-wide navigation bar, 108 nodes, 60 events per loop, compositor-region
backdrop policy. Three alternating before/after runs, desktop in use, no
compilation running. Before: the previous release executable with
`MESH_TEMPLATE_PATCHING=0`; after: the rebuilt default-path executable with
no opt-in. Per-event ranges:

| Loop | Before | Default patches |
| --- | --- | --- |
| Audio poll (read) | 4.450–4.652 ms | 2.302–3.112 ms |
| Pointer move | 0.795–0.911 ms | 0.758–1.072 ms |
| Paint only | 0.307–0.358 ms | 0.308–0.542 ms |
| Render hooks alone | 0.458–0.508 ms | 0.497–0.585 ms |

The first after run was slower across every loop; these ranges retain it.
Only the audio-update improvement is supported by separated ranges. No
relative timing gate was added: the structural regression
`template_patching_applies_a_bound_write_without_a_rebuild` asserts patched
nodes, no narrow rebuild, sparse damage, and equality with rebuilt pixels.

**Validated.** Release binding-patch regressions pass (3/3), including class
changes that alter width/background, accessibility labels, and pixel equality
with a rebuild at a fixed surface extent. The first version of that test used
a content-sized root and compared paints across different measured extents;
fixing the root extent made the comparison valid. The unpatchable-function
fallback and backend churn profiling regressions pass (2/2); the latter reports
seven patched nodes and no rebuild. The serial shell suite before the final
test-fixture adjustment matched the previous executable exactly: 846 passed,
29 failed, with the new regression excluded. Rustfmt on all touched Rust files
and `git diff --check` pass. Pending design notes remain linked from the backlog.

**Final suite.** The final serial release shell run completed with 847 passed,
29 failed, and 132 ignored. Its failure names exactly match the previous
executable's serial baseline (846 passed, 29 failed); the added regression
accounts for the extra pass. No new failures.


## 2026-10-13 — D1: service fields, keyed rows, and template function patches

`working tree` · area: scripting dependencies, frontend bindings, shell updates

Service expressions record field reads per evaluation scope, including reads
inside lexical helpers and keyed iterations. A proxy's lifetime read cache hid
later expressions' dependencies; deduplication now belongs to the active read
sink. Unchanged compound globals no longer appear as public writes. The compiler
records structural, component-prop, unkeyed-body, and pre-bound-handler consumers
so a property binding cannot accidentally cover a structural use of the same
source. Keyed property bindings retain their iteration locals; collection/key
changes still reconcile. Calls conservatively invalidate on public writes.

Service updates and render-hook writes patch together. Dependency indexes rebuild
only when the read set changes. Expression failures roll back the entire patch;
failed-evaluation reads remain tracked, and recovery requires a complete template
evaluation before either property patches or selective builds resume. The
receiving-service example now derives presentation in expressions and lexical
helpers without a render hook. Shipped-module `render()` migration remains open.

**Measured.** Local AMD Ryzen 7 7840HS, x86_64 Linux, release in `nix develop`,
desktop in use, no concurrent builds during the final measurements. Saved
pre-session release executable versus the final rebuilt executable, three
alternating paired samples. `affected_template_eval_beats_full_rebuild` runs
100 one-field updates over a column with 1,024 static text rows, a service-bound
text, and a hook-label text. Both variants also time the same full-template
control and compare final pixels. The historical benchmark output labels this
shape as 1,026 nodes; the authored column plus its children has 1,027 nodes.

| Service workload, 100 updates | Before | Binding patches |
| --- | --- | --- |
| Update total | 6,874.552–6,962.127 ms | 619.242–662.531 ms |
| Per update | 68.746–69.621 ms | 6.192–6.625 ms |
| Full-template control total | 6,977.020–7,093.618 ms | 6,950.632–7,099.270 ms |

The within-executable full-template/patch ratio is 10.549–11.299x. CI gate
`affected_template_eval_speedup` now has baseline 6.0 and tolerance 25%, requiring
at least 4.5x. All three final samples pass. The runner also checks patch counts,
absence of narrow rebuilds, pixel equality, structural fallback, and atomic
failure/recovery. An exploratory run of the unchanged executable failed its old
1% assertion: full 7.008681 s, selective 6.963247 s, ratio 1.006525x; it was not
silently counted as a passing baseline gate.

**Navigation limitation.** Same machine/profile, final alternating samples of
`navigation_frame_cost_profile`: 1920-wide shipped bar, 108 nodes, 60 events per
loop, compositor-region backdrop policy. Its catalog still lacks a `mesh.wm`
provider. These are per-event values, including events that do not paint.

| Navigation loop | Before | Binding expansion |
| --- | --- | --- |
| Audio poll (read) | 2.503–2.801 ms | 3.037–3.227 ms |
| Media poll (unread) | 0.012–0.015 ms | 0.013–0.016 ms |
| Pointer move | 0.813–0.976 ms | 0.908–0.967 ms |
| Paint only | 0.510–0.711 ms | 0.536–0.843 ms |
| Render hooks alone | 0.884–1.068 ms | 1.011–1.219 ms |

The navigation audio path regresses; the synthetic direct-service improvement
is not a claim of an overall shell improvement. Tracking overhead in the
existing render-hook workload remains open under D1 in the backlog. Removing
redundant dependency queries and avoiding unchanged index reconstruction did not
eliminate that regression. Other loop ranges overlap.

**Validated.** Final serial release shell: 851 passed, 29 failed, 132 ignored;
failure names exactly match the saved pre-session executable (847 passed,
29 failed, 132 ignored). Final focused debug invalidation group: 64 passed,
2 failures reproduced by the saved executable, 14 ignored. Scripting: 298 passed,
0 failed, 26 ignored; expressions: 6 passed. Frontend: 84 passed, 2 failed,
21 ignored; both failures reproduce in a clean HEAD worktree, which was removed.
The earlier complete debug shell run had 852 passes and 27 baseline failures,
before the added rollback/recovery regression. Workspace formatting, shell syntax
for the CI runner, and `git diff --check` pass. All pending notes retain backlog
links. Existing settings changes are preserved.

## 2026-10-13 — D1 derived expressions and tracked effects replace frontend render hooks

Area: frontend scripting and binding updates · uncommitted workspace.

Automatic frontend `render()` scheduling is retired with a compiled-component
migration diagnostic. Shipped frontend hooks now use lexical template helpers
or `effect(fn)`. The clock writes public time at minute boundaries through its
owned task instead of formatting on every frame. Effects track public scalar
and service-field reads, conservatively observe compound public inputs, settle
in registration order under callback budgets, and run cleanup before replacement
and on cancellation, reload, or unmount. Independent failures remain local.
Identical live child bindings are reused. Effects waiting on bindings settle
before handlers so optimistic wheel/touchpad changes are not overwritten.
Input equality accounts for host integers round-tripping as Luau floats inside
props. Dependency selection uses the Rust read cache instead of Lua table probes.

**Measured navigation.** Release, 1920-wide shipped navigation bar, 108 nodes,
60 events per loop, compositor-region backdrop policy, three alternating paired
samples on the same machine. Saved executables read module files at their baked
workspace path: each baseline ran in a private bubblewrap mount with matching
HEAD frontend sources; current sources were snapshotted for the final paired
comparison. Workspace module files were never swapped. The fixture still has
no `mesh.wm` provider. These figures divide by all events, including no-paint events.

| Loop, ms/event | Before D1 | Current derived/effect implementation |
| --- | --- | --- |
| Audio update | 3.469–3.726 | 3.280–3.328 |
| Unread media update | 0.014–0.014 | 0.016–0.017 |
| Pointer move | 1.047–1.160 | 1.036–1.090 |
| Paint only | 0.588–0.766 | 0.488–0.724 |
| Hooks / unchanged effects | 0.789–1.081 | 0.015–0.025 |

An additional three paired samples against the binding-expansion executable
from the start of this session measured audio 3.943–4.042 ms before versus
3.304–3.445 ms after; hooks 1.196–1.213 ms versus effects 0.015–0.028 ms.
Earlier historical ranges were taken under different machine conditions and
are retained above in their original entry; they are not an interleaved
comparison against this migration. Unpaired exploratory current runs measured
audio 3.348–3.428 ms and effects 0.025–0.028 ms. Pointer and paint ranges overlap;
this entry does not claim a general rendering speedup or an idle-wakeup result.
A 1,200-event audio-only perf sample attributed changed-frame work largely to
style resolution and retained layout snapshots; it is diagnostic evidence,
not a before/after benchmark or a reason to fold D7 into this change.

**Binding workload.** Release, column plus 1,024 static texts, one service text,
and one inert label: 1,027 authored nodes, 100 alternating one-field updates.
Three current samples measured full-template control 6,971.667–7,027.274 ms and
binding updates 610.418–635.192 ms (6.104–6.352 ms/update), ratio
10.976–11.512x. Pixels match and exactly one node patches. Existing gate
`affected_template_eval_speedup` requires 4.5x and passes all samples.

**Effect gate.** Release, 32 service-reading label effects, 1,000 alternating
unchanged and forced flushes per sample. `unchanged_effect_inputs_speedup`
requires 2x. The performance runner also runs effect lifetime, failure, input
equality, and legacy-hook retirement correctness checks. Final repeated
measurements are recorded in the following entry if needed.

**Validated.** Serial release shell: 852 passed, 29 failed, 132 ignored, with
exactly the saved pre-migration failure names. Parallel release: 850 passed,
31 failures, 132 ignored; the three additional scheduler failures are the
known load-sensitive tests, while one baseline profiling failure passed.
Debug shell: 853 passed, 28 failed, 132 ignored; its differing debug-snapshot
failure passes alone. Release scripting: 306 passed, zero failed, 27 ignored.
Frontend: 85 passed, the same two independently reproduced baseline failures,
21 ignored. The lexical-helper test confirms private helpers are allowed in
expressions without becoming public component fields. Workspace formatting,
runner shell syntax, diff whitespace, and pending-note links pass. Existing
settings changes remain byte-for-byte preserved.

A standalone scripting release build initially found an old cached service
contract artifact missing `ContractStateField.default`. Cleaning only that
package's generated release artifacts and rebuilding resolved it; source
contracts were unchanged. D1 leaves the backlog; structural reconciliation,
compound dependency precision, backend async, and the other direction tracks
remain open targets.

### Final effect-gate measurements

Same release workload described above, three samples: unchanged flushes
4.779–7.755 ms total versus forced callbacks 245.114–354.096 ms total;
45.657–51.291x. Gate `unchanged_effect_inputs_speedup` requires 2x and all
samples pass. The full release scripting suite includes 306 passes, zero
failures, and 27 ignored benchmarks.

## 2026-10-13 — D5: core popover disclosure on `navigation_hover_profile`

Release, `navigation_hover_profile` (25 cycles × 4 triggers, 400 steps,
100 popover opens), five interleaved runs each, `f0ef694c` in a worktree
against the D5 tree. The profile now finds triggers by `aria-haspopup`
because they no longer carry `pointerenter` handlers; the input sequence is
unchanged.

| Metric | `f0ef694c` (script hover intent) | Core disclosure |
| --- | --- | --- |
| Step p50 | 2.131–2.283 ms | 1.855–1.889 ms |
| Step p90 | 4.341–4.620 ms | 2.803–2.936 ms |
| Dispatch p50 | 0.238–0.248 ms | 0.038–0.039 ms |
| Popovers open at the end | 0 | 0 |

Trigger and popup enter/leave no longer run Luau handlers; the controller
updates a deadline in Rust and calls into script only to open or close.

`navigation_frame_cost_profile`, same five interleaved runs, pointer move:
1.405–1.727 ms per frame before, 1.320–1.706 ms after. The machine slowed
partway through (paint only rose from about 0.27 to 0.6 ms in both trees), so
this only rules out a large regression.


### 2026-10-13 — D7 shared theme aliases and typed scalar projections

Reviewed the rejected full-restyle `StyleShareCache` experiment before changing
resolution. This change retains token values rather than computed node styles.
Theme snapshots lazily share reference-bearing token resolutions across clones;
mutable accessors detach the cache. Missing names do not enter that cache.
Style resolvers share typed color/number projections across frames in thread-local
LRUs bounded to 16 revisions and 512 values per revision. Per-node custom values
and component props retain precedence. Other property lowering remains open.

Release profile, three repeated paired microbenchmark runs:

| Workload | Control | Cached/typed | Ratio | Gate |
| --- | --- | --- | --- | --- |
| 32-token scalar alias chain, 20,000 reads | 120.271–126.262 ms recursive graph resolution | 1.057–1.095 ms snapshot reuse | 113.817–116.862x | `shared_theme_alias_resolution_speedup` >= 2.0 |
| One rgba color and one px number, 200,000 pairs, warmed token lookup in both paths | 38.043–77.055 ms repeated parsing | 13.658–30.539 ms typed reads | 2.523–3.048x | `typed_theme_scalar_reads_speedup` >= 1.2 |

These are token-resolution/parsing measurements, not end-to-end theme-switch
speedups. Validation: theme debug and release suites 33 passed / 1 ignored;
style resolver debug and release suites 50 passed / 38 ignored. Tests cover
snapshot sharing, clone detachment, serialization, module/set/remove invalidation,
root-specific cycle/missing/malformed diagnostics, fallback behavior, override
precedence, and cache bounds. Both gates are wired into `tools/check-performance`.

### 2026-10-13 — D7 indexed literal operations

Indexed literals now carry property-specific typed operations, so applying them
switches on the operation rather than matching a property name against a generic
value kind. Dynamic declarations keep their validated resolver path. This also
fixes numeric inherited text literals bypassing the `inherit` guard and marking
font-size/weight/line-height explicit.

Release profile, 8 numeric declarations x 200,000 applications; three paired
runs compare an eight-property string dispatch with pre-indexed operations:
3.372–4.841 ms control versus 2.221–2.492 ms operations, 1.518–1.942x.
Gate: `typed_literal_operation_dispatch_speedup` >= 1.05, wired into
`tools/check-performance`. This measures dispatch only, not whole restyles.
Style resolver debug/release suites: 52 passed / 39 ignored. A parity test
compares supported indexed operations with string resolution across zero,
numeric, edge, percent, size-keyword and color values; an inheritance regression
test checks explicit masks. The preceding full serial elements run had
276 passed / the same 10 known fixture failures / 59 ignored.

### 2026-10-13 — D7 separate selector dependencies from candidates

Compound selectors now register state dependencies separately from their forward
candidate bucket. A class/id/tag compound no longer enters the global state
bucket merely because it depends on hover/focus. Reverse dependencies are
collected recursively and deduplicated per rule, preserving invalidation for
multi-state and nested selectors.

Release profile, 1,000 distinct `.row-N:hover` rules, one matching hovered row,
10,000 indexed candidate/match passes, three paired runs: 78.860–80.115 ms with
the former global state spill versus 0.287–0.297 ms with structural buckets;
265.676–278.827x. Gate `structural_selector_bucket_speedup` >= 4.0 is wired into
`tools/check-performance`. This is a selector candidate workload, not a complete
hover-frame speedup. Scoped debug tests: 53 passed / 40 ignored. Full serial
release elements: 279 passed / the same 10 known fixture failures / 61 ignored.
The regression test compares indexed matching with a full selector scan for
state/class changes and verifies deduplicated reverse dependencies.

### 2026-10-13 — D7 expanded literal dispatch regression (rejected ordinary inlining)

Expanding the typed literal operation table to keyword and layout longhands
preserved semantic parity but failed `typed_literal_operation_dispatch_speedup`.
Release profile, 8 numeric declarations x 200,000 applications: the first
unannotated run measured 3.656 ms string control versus 4.236 ms typed dispatch
(0.863x). Three runs with an ordinary `#[inline]` hint measured
2.764–3.397 ms control versus 4.584–5.237 ms dispatch (0.528–0.741x).
The release binary still emitted an out-of-line `apply_typed_literal` function.
The ordinary-hint version is rejected; forced inlining is being checked before
accepting or reverting the expansion. The existing 1.05x gate remains unchanged.

### 2026-10-13 — D7 longhand expansion reverted after forced-inline gate failure

Follow-up to the preceding expanded-dispatch experiment: forced inlining removed
the emitted assignment function, but the same release eight-operation workload
measured 3.200 ms control versus 3.119 ms typed dispatch (1.026x), below the
unchanged 1.05x gate. The keyword/layout-longhand expansion and inline annotations
were reverted. The smaller property-specific operation table remains, with
expanded parity inputs and separate ownership in `resolve/literal.rs`. Broader
lowering needs a representation that preserves scalar dispatch costs; do not
retry one larger flat operation enum without checking this measurement.

## 2026-10-17 — CMP-13 child-settings cache key experiment

Debug test profile, `CARGO_INCREMENTAL=0`, local x86_64 development host under
`nix develop`; `child_settings_resolution_workload`, four runs of 2,000 child
settings projections with 32 declared props and no stored overrides. Direct
resolution took 2.460–3.990 ms; the first cache, keyed by module plus deep prop
schema equality on every lookup, took 3.195–5.422 ms. This key design is rejected:
comparing the complete schema costs more than resolving an empty namespace.
The replacement uses the canonical source path within the immutable catalog /
settings snapshot, with cache invalidation on source, catalog or settings
replacement. Populated-namespace and replacement-key measurements follow after
validation. No performance gate is claimed for this experiment.


## 2026-10-18 — rejected glass-interior and separate state-hash experiments

Area: liquid-glass raster and backend state publication. AMD Ryzen 7 7840HS,
release (thin LTO, one codegen unit), CPU Skia raster for glass. Reviewed the
rejected-experiments table before starting.

A separate keyed `Value::hash` pass removed retained JSON cloning but added a
second content traversal after validation. On 128-row, 12,626-byte snapshots,
2,000 unchanged publications per sample, two attempted interleaved samples took
177.458–314.765 ms before versus 226.492–274.504 ms after. The second sample
failed `validated_state_publication_profile`'s 1.05x gate; no changed-state win
is claimed for this design. The replacement fuses the fingerprint into the
existing bounded serialization writer.

An SkSL early return when the signed distance was outside the refracting rim
preserved reference pixels but regressed rounded pills. Three interleaved,
warmed samples of ten 180×40 draws (radius 20, blur 5, depth 12, edge 11,
dispersion 1.5; cached 1920×1080 backdrop) took 9.269–9.338 ms before versus
10.524–10.660 ms after (0.87–0.89x). A single ten-frame 360×240 card probe
(radius 25, blur 6, depth 40, edge 30, dispersion 4) took 107.834 ms before
versus 100.134 ms after (1.08x), failing `glass_interior_speedup`'s 1.2x gate;
the run stopped there. Reverted this shortcut. The separate direct image-shader
path for zero-edge, zero-dispersion wallpaper passed its 1.2x gate and remains.


## 2026-10-18 — fused state hash rejected; fast keyed value hash passes

Follow-up to the preceding state-hash experiment. AMD Ryzen 7 7840HS,
release (thin LTO, one codegen unit), 128 nested device records / 12,626 encoded
bytes, 2,000 publications per sample.

Fusing FNV-1a into the bounded serialization writer still spent too much time
hashing bytes, including JSON punctuation. Three controlled first-sample
attempts took 178.085–191.996 ms baseline versus 202.623–204.492 ms candidate
(0.87–0.95x); each aborted at the 1.05x unchanged-state gate. Reverted the writer
and shared-runtime API changes completely.

An isolated hash-only probe on the same value, 10,000 hashes × three runs,
measured std `RandomState` at 179.511–180.178 ms versus `ahash::RandomState` at
90.767–93.048 ms. This supplied new evidence for revisiting the separate hash
pass with a different implementation. The final publication harness guards the
input at each legacy validation boundary with `black_box`; its common snapshot
clone, callback budget reservation, and payload drop remain in both paths.

With keyed `ahash`, three warmed, alternating-order samples passed
`validated_state_publication_profile`'s 1.05x gate in both workloads:

| Workload (2,000 publications) | Legacy validation + retained clone | Validated snapshot + fingerprint | Paired speedup |
| --- | --- | --- | --- |
| Unchanged | 176.038–178.055 ms | 143.109–146.284 ms | 1.22–1.23x |
| Alternating generation field | 301.504–308.900 ms | 143.448–146.937 ms | 2.05–2.13x |

These are Rust publication-boundary measurements, including synthetic snapshot
construction by cloning the common input. They exclude Lua conversion, channel
transport, shell contract validation, and rendering; no whole-frame speedup is
claimed. Final implementation/validation is recorded below when the batch lands.


## 2026-10-18 — coalesced reload maintenance and blocking file-watch control

Area: idle scheduling. AMD Ryzen 7 7840HS, workspace release profile (thin LTO,
one codegen unit). Removed the completed idle-wakeups item from the backlog.

Healthy-watcher theme, settings, frontend, and watch-set checks now share the
same two-second deadline, instead of completing and rearming independently.
The scheduler includes the watch-set deadline explicitly. An unavailable
watcher uses the existing shorter scan intervals; filesystem events force
immediate checks. A dedicated control eventfd wakes the watcher for replacement
and shutdown, allowing it to block on inotify/control readiness instead of
sleeping for 20 ms between command checks.

**Measured.** `coalesced_reload_wakeup_profile`, three deterministic completion-
phase variations across 60 maintenance passes / 120 modeled seconds, produced
180 independent reload deadlines versus 60 shared deadlines in every run.
This measures deadline coalescing, not observed whole-session shell wakeups.
`watcher_idle_wait_profile`, three paired 160 ms idle waits, measured 7 polling
wakeups versus 0 blocking-wait wakeups in every run, excluding shutdown. Both
structural count gates are included in `tools/check-performance`.

**Verified.** Release: four watch-set/worker tests, eight scheduler tests, three
watcher lifecycle tests, retired-generation rejection, and render-stall backoff
passed. Worker tests cover idle rebinding, no initially bound directories,
file-change delivery, and explicit shutdown wakeup. The architecture overview
records the maintenance and fallback contract. Full live-desktop CPU/fork-rate
measurement remains outside these isolated wait/deadline gates.


## 2026-10-18 — validated backend publication and measured glass wallpaper fast path

Area: backend state publication and software backdrop raster. AMD Ryzen 7
7840HS, workspace release profile (thin LTO, one codegen unit). Removed the
completed backend-emit and glass profiling/wallpaper items from the backlog.
The other 23 performance items remain open. Earlier rejected experiments above
are retained, including the reverted glass-interior shortcut.

**Backend implementation.** Backend callbacks return immutable `BackendState`
values. Their private constructor validates session and transport byte/depth
limits once, reserves callback output, and computes a keyed `ahash` fingerprint.
`emit`, `emit_json`, `emit_unavailable`, and exported global state all use that
constructor. Snapshot extraction moves the already validated value; publication
moves its JSON into the channel and retains only the fingerprint. The last
fingerprint advances after a successful send. No JSON encoding, exact-equality
clone, or second validation is retained in the orchestrator's publication path.
`ahash` was already in the dependency graph and is now a direct scripting
dependency.

**Final backend measurements.** `validated_state_publication_profile`, 128
nested device rows / 12,626 encoded bytes, 2,000 publications per sample, three
warmed alternating-order samples per workload immediately after the final build:

| Workload | Legacy validation + retained clone | Validated snapshot + fingerprint | Paired speedup |
| --- | --- | --- | --- |
| Unchanged | 179.684–337.302 ms | 146.020–194.202 ms | 1.09–1.74x |
| Alternating generation field | 308.638–316.087 ms | 147.779–149.549 ms | 2.07–2.12x |

All samples passed the 1.05x gate. The immediate post-build unchanged samples
have wider clock/load variance than the earlier controlled 1.22–1.23x result;
both ranges are recorded rather than dropping the outlier. These remain Rust
boundary microbenchmarks, not end-to-end script or frame timings. For
reproducibility, the exploratory hash-only probe in the preceding entry used
standalone `rustc -O -C lto=thin` with default codegen units; the publication
gates use the workspace release settings above.

**Glass implementation.** Zero-edge, zero-dispersion backdrop draws reuse the
prepared image shader directly, retaining source alpha, output-coordinate
mapping, blur preparation, and the same rounded-rect clip. Refraction and
highlighted rims use the original lens shader. `glass_raster_profile` retains
an independent original SkSL reference for the wallpaper A/B comparison.

**Final glass measurements.** Cached cover-fit source and precompiled shaders,
three alternating-order samples of ten CPU raster draws per case. Allocation,
image decode, Lua, display-list construction, SHM copy, and compositor time are
outside this draw-only benchmark:

| Unrefracted output | Original lens, ten draws | Direct image shader, ten draws | Paired speedup |
| --- | --- | --- | --- |
| 1920×1080 | 1,918.276–2,049.525 ms | 51.430–52.950 ms | 37.19–39.01x |
| 3840×2160 | 7,736.758–7,956.938 ms | 208.691–209.469 ms | 37.03–38.13x |

`glass_raster_profile` passes `unrefracted_wallpaper_speedup`'s 1.2x gate at
both sizes and is included in `tools/check-performance`. Regular lens costs
are measured, not optimized by this batch: three ten-draw samples each give
0.968–0.981 ms/draw for a 180×40 pill (radius 20, blur 5, depth 12, edge 11,
dispersion 1.5), 10.767–10.921 ms/draw for a 360×240 card (radius 25, blur 6,
depth 40, edge 30, dispersion 4), and 261.300–262.314 ms/draw for a 1920×1080
stress lens with the card's filter. All glass samples use a cached 1920×1080
backdrop. This does not claim that a retained idle wallpaper redraws every frame.

**Verified.** Final release tests: render 285 passed / 37 ignored; scripting
backend 127 passed / 1 ignored; backend orchestration 46 passed; the state and
glass performance gates passed. State regressions cover session/transport and
callback limits, nested changes, duplicate suppression, and failed delivery.
Glass refraction matches reference pixels exactly; unrefracted draws differ
by at most one 8-bit channel level for translucent fractional samples because
the direct image and runtime-effect raster paths round differently. Cases cover
blur, fractional origins, and asymmetric corners. Final `cargo check --tests -p
mesh-core-shell` and script syntax/changed-file whitespace checks pass.

The wider shell backend-lifecycle run has 37 passed / 5 failed. All five failing
names also fail in cached pre-change binary `mesh_core_shell-7a1a32dc7d159cd0`
(built 2026-10-17; 36 passed / 5 failed):
`backend_lifecycle_uses_explicit_active_provider_from_package_graph`,
`changing_ui_font_repaints_the_mounted_settings_surface`,
`committed_activation_replaces_the_previous_capability_grants`,
`frontend_module_activation_mounts_shipped_surface_live`, and
`legacy_graph_delta_uses_the_activation_coordinator_commit_boundary`. These
fixture/configuration failures are not claimed fixed by this performance batch.

**Backdrop dimensions clarification.** The cached 1920×1080 source above
refers to the regular-lens probes. Each wallpaper probe uses a prepared image
matching its own output dimensions (1920×1080 or 3840×2160).


## 2026-10-18 — RND-03 / RND-04: frame-plan derivations and visual damage on demand

Area: retained display list and shell damage. AMD Ryzen 7 7840HS, workspace
release profile (thin LTO, one codegen unit). Removed the RND-03 / RND-04
backlog item.

**RND-03.** `refresh_frame_plan` built per-node payload, transform, and
scroll-scope arrays on every update; nothing on the paint, presentation, or
scene-thread paths read them. `FramePaintPlan::nodes()`, `transforms()`, and
`scroll_scopes()` now derive them from the plan's immutable command topology
when asked. The plan keeps topology, effects, replay spans, and damage.

**RND-04.** The shell built a whole-tree `NodeId -> DamageRect` map every
painted frame and kept it for the next frame, though it only looked up dirty
nodes, and only on frames with reorder/transform/opacity/material bits. It
now keeps the previous immutable command snapshot plus its content clip. On
those frames it scans the previous and current snapshots for the dirty ids;
other frames do no visual-damage work.

**Measured.** `frame_plan_derivation_and_visual_damage_profile`, a 1,261-node
display list (60 rows × 20 text cells), a one-node text change per frame,
2,000 iterations, three runs:

| Workload | Before | After | Speedup |
| --- | --- | --- | --- |
| Sparse text update (eager plan + whole-tree map vs update only) | 794.4–809.2 ms | 177.9–181.9 ms | 4.45–4.52x |
| Visual-dirty frame lookup (whole-tree map vs two filtered scans) | 269.1–270.4 ms | 130.1–134.0 ms | 2.02–2.07x |

The first row is the display-list update path alone, not a whole frame. On the
1920-wide navigation bar `retained_display_list_update` was 0.101 ms/frame
before this change (`navigation_frame_cost_profile`, paint-only loop), so the
bar-level win is a fraction of that. Gates `lazy_frame_plan_speedup` (2.0x)
and `filtered_visual_damage_speedup` (1.5x) are in `tools/check-performance`.

**Verified.** Release: render 285 passed / 38 ignored. Shell lib: same failure
set (50) before and after, diffed by name against a stashed baseline.


## 2026-10-18 — RND-02: layout changes damage only rebuilt scroll viewports

Area: retained display-list damage. AMD Ryzen 7 7840HS, workspace release
profile. Removed the RND-02 backlog item.

Content inside a retained scroll scope has no offsets in its signatures, so
any layout-class dirty bit used to damage **every** anchored viewport on the
surface. A viewport is now damaged only if its owner's retained subtree was
rebuilt (owner or a descendant dirty, an ancestor forcing a rebuild), appeared,
or disappeared. Reused owners replay their content unchanged at the same anchor.
A first version compared only the owner's scope geometry; it missed a nested
scroll inside retained content (`nested_scroll_inside_retained_content_matches_fresh_paint`
caught it) and was replaced by the rebuilt-subtree rule.

**Measured.** `label_resize_scroll_viewport_damage_profile`: 960×480 surface
with three 80-row scroll areas (240/480/240 wide) and a status label outside
them whose width changes every frame, 200 frames, three runs each. Damage is
deterministic:

| | Damaged px/frame | Share of surface | Update time, 200 frames |
| --- | --- | --- | --- |
| Before | 425,791 | 92.4% | 68.6–99.0 ms |
| After | 3,391 | 0.7% | 65.6–68.1 ms |

The win is in repainted pixels downstream of the display list, which this
profile does not time; update cost is unchanged. Gate
`label_resize_viewport_damage_percent` (upper 1.0%) is in
`tools/check-performance`. `settings_scroll_frame_gate` still passes (p50
5.647 ms, p90 6.669 ms, 90 frames).

**Verified.** Release render 286 passed / 38 ignored, including the new pixel
regression `layout_change_damages_only_scroll_viewports_whose_scope_moved`
(label resize damages neither viewport; moving one damages only it; partial
repaint equals fresh paint). Shell lib failure set unchanged apart from the
five theme tests fixed earlier today.


## 2026-10-18 — SH-05 / SH-06 fixed; SH-13 measured and left alone

AMD Ryzen 7 7840HS, workspace release profile. Removed the SH-05 / SH-06 /
SH-13 backlog item.

**SH-05 — font-alias cache thrash.** The shell sets each module's font aliases
before painting its surface. Any change cleared the text layout cache, glyph
atlas, and named-family availability cache, and bumped the measurer revision
(which also makes retained layout remeasure every text node). Two surfaces
with different alias maps did that on every switch. Alias maps are now
interned per font-system generation, and the measurer revision is
`base × 64 + alias-set id`. A surface switching back gets its old revision
and its cached layouts. The glyph atlas (keyed by resolved face) and
availability cache (raw family name) no longer clear on alias changes;
font-database and resource-revision changes still clear everything.

`alternating_font_alias_surfaces_profile`: two alias sets (none / `Inter` →
`DejaVu Sans`) alternating for 200 frames, 40 labels each, three runs:

| | Time | Layout hits / misses | Distinct measurer revisions |
| --- | --- | --- | --- |
| Before | 91.1–93.5 ms | 0 / 8,000 | 200 |
| After | 6.4–7.6 ms | 7,920 / 80 | 2 |

Gate `alias_alternation_layout_hit_ratio` (≥ 0.95). The retained-layout
remeasure this also avoids is not timed here.
`interned_alias_sets_never_share_layouts` checks that measurements under each
alias set equal a fresh renderer's.

**SH-06 — double `Debug` formatting per effect.** `effect_fingerprint` and
`effect_weight` each built `format!("{request:?}")`, at enqueue and again at
dispatch. One streaming pass into a hashing/counting `fmt::Write` now runs at
enqueue; the weight is stored on the effect. `effect_identity_single_pass_profile`,
1,000 `ServiceCall` slider commands × 3 rounds, three runs:

| Payload | Two `Debug` strings | One streaming pass | Speedup |
| --- | --- | --- | --- |
| 2 KB | 11.7–14.7 ms | 5.8–8.0 ms | 1.84–2.19x |
| 50 KB | 203.4–204.7 ms | 111.8–115.2 ms | 1.76–1.83x |

Gate `effect_identity_single_pass_speedup` (≥ 1.5x, 50 KB). The remaining
~37 µs per 50 KB effect is the `Debug` walk of the JSON payload itself.

**SH-13 — idle watch-set rebuild: measured, not changed.** With a healthy
watcher, `reconcile_file_watcher` rebuilds, sorts, and compares the watch-path
vector every two seconds. `idle_watch_set_reconcile_profile` on the repository
graph (52 modules, 121 paths, no mounted components), 2,000 passes: 223.4 ms,
**112 µs per pass**, about 0.006% of one core at the 2 s cadence. An input
fingerprint cache would add invalidation risk for no measurable win. The
periodic metadata stat of component sources is the deliberate lost-event
safety net from the reload-maintenance entry above.

**Verified.** Release render 287 passed / 40 ignored; shell lib failure set
unchanged (45, diffed by name).


## 2026-10-18 — UI-11: keyframe rules compile once per instance

Area: shell keyframe animation. AMD Ryzen 7 7840HS, workspace release profile.
Removed the UI-11 backlog item; this is also evidence for S09-PERF-002.

Every animation frame, for every animated node, the shell cloned (or, for
theme keyframes, re-resolved) the stops, hashed every declaration for the
instance fingerprint, cloned the full `ComputedStyle` per stop to re-apply
string declarations through the resolver, built a `KeyframeRegistry`, and
cloned the rule and the active timeline back into their maps. Now:

- `KeyframeSource` caches each animation name's stops (`Arc`), their
  fingerprint, and declared properties per theme revision; the cache clears on
  theme change and component recompile.
- `CompiledKeyframeRule` keeps the resolved rule with its inputs (theme
  revision, props fingerprint, tag, exact `ComputedStyle`) and is rebuilt only
  when one differs. The keyframe base is restored to authored values before
  sampling, so the input style is stable while an animation runs.
- `ActiveKeyframeAnimation::current_with_rule` samples without a registry;
  timelines update in place.

**Measured.** `keyframe_animation_frame_profile`: N `<box>` nodes running a
three-stop opacity + `translateX` keyframe animation, 512×8 surface, 300 frames
after 4 warm-up frames, whole `paint()` per frame, three runs each:

| Animated nodes | Before (µs/frame) | After (µs/frame) | Speedup |
| --- | --- | --- | --- |
| 1 | 46.4–53.2 | 43.4–52.6 | ~1.0x |
| 10 | 153.7–179.0 | 116.5–148.5 | 1.2–1.3x |
| 100 | 1,322.5–1,345.4 | 987.7–1,053.4 | 1.27–1.34x |

With temporary probes around the keyframe apply step only (100 nodes, 300
frames): 170.5–174.7 ms before vs 75.0–84.8 ms after. Each probe pair costs
about 1.7 µs (see the clock note below), so net of probe overhead the apply
step is ~2.5–2.9x faster. The rest of the frame is restyle, layout, and paint.
Gate `keyframe_compilations_per_instance` (upper 1.0): 100 compilations over
300 frames for 100 instances.

**Clock note for this workstation.** The kernel clock source is `hpet`
(`acpi_pm` the only alternative; no TSC), so `Instant::now()` costs about
0.85 µs here. Per-node timing probes therefore inflate results heavily, and
hot paths that read the clock per node or per text layout miss cost more on
this machine than on a TSC system.

**Verified.** Release: animation crate 44 passed; shell lib failure set
unchanged (45, diffed by name), including the keyframe easing/lowering tests.


## 2026-10-18 — CMP-04 / CMP-09 / CMP-10 / RES-15, and a coarse frame-budget clock

AMD Ryzen 7 7840HS (`hpet` clock source, `CONFIG_HZ=1000`), workspace release
profile. Removed the CMP-04 / CMP-09 / CMP-10 / RES-15 backlog item.

**CMP-04 / CMP-10 — expression cache and parser threads.**
`compile_expression` held the global cache lock while spawning and joining a
fresh 16 MB-stack thread for every new expression, and the cache never
evicted. Parsing now runs on one long-lived `mesh-expression-parser` thread
(same stack size; `catch_unwind` keeps the non-ASCII panic diagnostics), with
the lock released while parsing. The cache has two generations: hits in the
old generation are promoted, and the total is capped at 4,096 expressions.
`cold_expression_compile_profile`, 2,000 distinct expressions, three runs:
thread per parse 266.5–285.5 ms vs parser worker 72.1–74.7 ms (3.70–3.82x).
Gate `expression_parser_worker_speedup` (≥ 1.2x).
`cache_stays_bounded_and_keeps_recent_expressions` covers the bound.

**Frame-budget clock (found while measuring CMP-09).** The Luau interrupt
fires at every call/return boundary and loop back-edge. Each firing took a
mutex and read `Instant::now()` to enforce the 50 ms `frame_budget_us`. On
this HPET machine that read is ~0.85 µs, and one template expression with
`__index` metamethod lookups hits many checkpoints. The window start is now an
`AtomicU64` from `CLOCK_MONOTONIC_COARSE` (vDSO read of the cached tick, 1 ms
resolution here), which is ample for a 50 ms bound.
`frame_budget_aborts_a_long_running_callback_before_the_instruction_budget`
still passes. This applies to every Luau callback in the realm, not only
templates.

**CMP-09 — per-call template-expression work.** Each call created Lua strings
to look up the closure table and the full expression source, built a `locals`
table even when empty, walked the expression's member-read table, and
allocated several `String` cache keys. Bound closures and member-read tables
are now cached by source in Rust (cleared on reinstall and script reload);
empty locals pass `nil`; the Lua side bumps `member_reads[0]` when it records
a new name, so the host re-reads that table only when its version changes.

`template_expression_call_profile`: one row expression with `{item, index}`
locals × 200 rows, and 20 empty-locals member expressions, 10,000 calls each,
three runs:

| Step | Row call with locals | Empty-locals call |
| --- | --- | --- |
| Before | 21.94–31.93 µs | 11.68–14.08 µs |
| Coarse frame clock | 7.04–7.18 µs | 3.35–3.39 µs |
| + cached handles / `nil` locals | 4.68–4.88 µs | 1.15–1.29 µs |

A 200-row `{#for}` that evaluated one row expression in 4.4–6.4 ms per frame
now does so in about 0.95 ms on this machine. No relative gate: both arms
would need the old runtime in the same binary. The profile is kept for
re-measurement. TSC-clocked machines will see a smaller clock win.

**RES-15 — ICU formatters.** `plural_category` and `format_number_value`
parsed the locale and constructed `PluralRules` / `DecimalFormatter` per call.
Both are now cached per thread by locale string (8 locales, oldest evicted;
build errors are cached too). `cached_icu_formatter_profile`, 200 plural +
200 number calls × 50 frames, three runs: 3.97–4.80 ms per-call construction
vs 0.81–1.28 ms cached (3.11–5.32x). Gate `cached_icu_formatter_speedup`
(≥ 2x).

**Verified.** Release: expression 7 passed; scripting 339 passed / 29 ignored;
locale 28 passed; frontend the two known failures; shell lib failure set
unchanged (45). `mesh-tools-lsp` `shipped_manifests_validate_cleanly` fails on
an unrelated manifest-schema drift (`permissions`), now a backlog item.


## 2026-10-18 — in-surface blur engine phases 0–2: correctness, contract, routing

AMD Ryzen 7 7840HS, workspace release profile. Phases 3–5 (caches) remain in
the backlog, gated on the D6 renderer prototype.

**Phase 0 — the suspected defect was real.**
`hover_inside_in_surface_frosted_panel_matches_full_repaint`: a chip inside a
blur(4) frosted panel over red/blue content, under `InSurfaceFilter`. The
partial repaint differed from a full repaint in **58 pixels** on the first
hover. `expand_damage_for_blur_regions` grew damage by one kernel reach,
clipped to the read region. That kept the backdrop *source* fresh, but the
painter re-blurred the whole expanded rect, and inside the panel the buffer
holds last frame's blurred output rather than the raw backdrop. Damage
touching a read region now unions the whole region. The existing
`sparse_repaint_with_backdrop_damage_expansion_matches_full_repaint` had run
under the compositor policy, where nothing is blurred in-surface; it now runs
in-surface. `BackdropRegion::reach` and `backdrop_kernel_reach` were removed.

**Phase 2 — per-element routing.** Under `CompositorRegion`, an element with
in-surface content beneath it (it has a read region) now also blurs that
content in the surface; one with nothing beneath (the navigation bar root)
still does no pixel work. Damage expansion follows: every policy but
`Rejected`. `compositor_policy_blurs_only_in_surface_content` checks both
cases (a frosted root's pixels equal the `Rejected` reference exactly).

**Phase 1 — contract.** 04 §10 states per-element routing and whole-region
partial repaint as shipped; backdrop root, clamped edges, and the cache as
targets.

**Cost.** `in_surface_backdrop_workload_profile`, 480×320 surface with real
text, blur(8), `InSurfaceFilter`, 200 frames, three runs each (the "before"
column is the old bounded expansion restored temporarily):

| Workload | Before | After |
| --- | --- | --- |
| (a) chip inside a 300×200 frosted panel toggling | 117.5–133.9 µs/frame, 4,032 px/frame (wrong pixels) | 926.8–952.8 µs/frame, 104,016 px/frame |
| (b) text list scrolling beneath a 480×48 frosted header | 841.2–877.7 µs/frame, ~108.7k px/frame | 882.7–969.2 µs/frame, ~157–165k px/frame |

(a) is the case the phase-3 blurred-backdrop cache turns into a blit; (b)
misses any cache by nature, as the plan says. The navigation bar is unchanged
under both policies: `navigation_frame_cost_profile` damage per volume change
stays 1.7% of the surface (default and `MESH_BENCH_BACKDROP=in-surface`).

**Verified.** Release render 288 passed / 41 ignored. In one of three full
runs `scene_thread::tests::discarded_output_leases_retire_outside_the_queue_lock`
failed under load; it passed in six isolated runs and is unrelated (backlog).
Shell lib failure set unchanged (45).


## 2026-10-18 — scroll frames keep retained pixels; whole-pixel scrolls shift in place

AMD Ryzen 7 7840HS, workspace release profile. Narrowed the scroll-raster
backlog item to the remaining cases.

**Finding: every scroll frame repainted the whole surface.** Two-finger input
and momentum ticks invalidated `PAINT` through `invalidate`/
`invalidate_style_path`, and `PAINT` sets `surface_pixels_invalid`. Each scroll
frame therefore cleared and replayed all 920×900 pixels, although the retained
display list already damaged only the scrolled viewport.
`invalidate_scroll`/`invalidate_scroll_tick` now mark the frame dirty without
invalidating pixels. Damage comes from the display list: retained viewports,
moved entries for non-retained scrollers, and scrollbars.

**In-place shift.** A frame that is scroll-only for retained scopes, whose
previous state the shell painted (`RetainedDisplayList::mark_painted`), plans a
`ScrollShift` per scope when everything painted beneath the viewport is a
full-cover solid fill. The composed viewport pixels are then content over a
constant colour, and an integral device-pixel move is exact. The shell moves the
retained buffer pixels (`PixelBuffer::shift_region_pixels`) and replays only:

- exposed strips;
- owner edge bands across the direction of motion (vertical scrolls keep side
  borders and side edges, which are constant down a column);
- beneath nodes' border bands and corner squares;
- scrollbar strips;
- overlays painted above the scope.

Presentation damage stays the whole viewport. Shifts are skipped for
fractional displacements, layers or backdrops inside or above the scope, a
visible tooltip, shell debug overlays in the buffer (new
`ShellComponent::buffer_overlaid`), and full-surface frames.

This deviates from the design note's separate transparent raster. Shifting
composed pixels needs no second buffer and no compositing, but only applies
over a uniform backdrop. The note now tracks the remaining cases.

**Measured.** `settings_scroll_frame_gate`, 920×900 Settings Appearance, 60
two-finger 4px frames + 30 momentum frames, three runs per step:

| Step | Frame p50 | Frame p90 |
| --- | --- | --- |
| Before | 5.580–5.691 ms | 6.512–6.643 ms |
| Scroll keeps retained pixels | 3.938–3.998 ms | 4.736–5.019 ms |
| + in-place shift | 2.720–2.785 ms | 4.097–4.836 ms |

Stage attribution (`MESH_SCROLL_STAGES=1`, which perturbs timing) for paint
traversal per frame: 4.631 ms before, 3.021 ms partial, 3.476 ms with the
shift under profiling. Text draws went 2.901 → 1.650 → 0.983 ms/frame. With
the shift, 59/60 finger frames move; fractional momentum frames replay, so p90
moves less. Gate `settings_scroll_shifted_fraction` (≥ 0.9) is in
`tools/check-performance`. A first version that replayed both side bands and
the scrollbar command's whole box was slower than plain partial repaint
(p50 4.607–4.777 ms): every content row intersected those full-height rects.

**Correctness and a limit found.** `settings_scroll_shift_matches_full_paint`
compares the retained buffer with a fresh full paint after every frame. Exact
equality is impossible for any partial repaint whose clip crosses a rounded
arc: Skia's anti-aliasing of a rounded rect (fill or border) depends on
whether the clip cuts it. A minimal 920×700 rounded window repainted under the
viewport clip differs from a full paint at its corner (worst alpha 57 vs 42).
A clip holding the whole corner still differs by ±1. Hover repaints already
have this. Shifting would carry such pixels inward from the exposed strip, so
replayed strips and bands grow until their inner edges miss content arc rows
(`ScrollShift::arc_rows`/`arc_columns`). The test allows ≤ 96 differing pixels
with channel delta ≤ 64 (measured worst: 71 pixels, delta 55, at the window
corner and a scrollbar end). A misplaced shift fails it by thousands of
pixels. The clip-dependent anti-aliasing is a backlog item.

**Verified.** Release render 290 passed / 41 ignored (new: planner
uniform/gradient cases, region shift). Shell lib failure set unchanged (45);
the parity test passes with 3+ shifted frames.


## 2026-10-18 — D7: keyword and layout properties lower to typed operations

AMD Ryzen 7 7840HS, workspace release profile.

**Measured first.** Counting declarations by path over an Appearance service
update (tree rebuild) gave about 3,150 applied per frame: 71% typed, 29% on the
string path. The string path was dominated by `flex-shrink`, `align-items`,
`justify-content`, `text-align`, `border-color`, `padding-inline`,
`flex-grow`, `padding-block`, `flex-direction`, `overflow*`, `display`,
`align-self`, and `flex-wrap`.

**Change.** Those properties are now `TypedLiteralOperation`s. The lowering
macro takes a per-entry setter, so two-field properties (`padding-inline`,
`padding-block`, the `overflow` shorthand) fit, and keyword parsing reuses the
string path's exact matches. `indexed_literal_operations_preserve_string_resolution_semantics`
covers every new property against an extended value matrix, including
`inherit` and unknown keywords. The unread `CachedLiteral::Static` variant was
removed, which also clears the long-standing `mesh-core-elements` dead-code
warning.

**Result.** After the change, 97.5% of declarations apply typed (string path
5,582 vs 63,107 over 70 frames). Remaining: `font-family`, `color` and
`background` with fallbacks, `transition`, and `exclusive-zone`.
`typed_keyword_declaration_profile`, 1.1M keyword declarations, three runs:
string path 128.5–136.8 ms vs typed 9.29–9.33 ms (13.8–14.7x). Gate
`typed_keyword_declaration_speedup` (≥ 5x).

The frame-level effect is small. `appearance_frame_cost_profile` style_restyle,
three runs each: 2.743–2.766 ms before vs 2.672–2.705 ms after (~1.02–1.03x),
because keyword matching on the string path was already cheap (~119 ns per
declaration). A `perf` profile of the same workload puts the remaining
restyle cost in node resolution, `ComputedStyle` clone/drop/eq (2.6/1.7/1.6%
of samples), and the per-frame style-animation pass (3.2%). The frame itself
(8.4–8.6 ms) is mostly the full tree rebuild an unrelated service update
triggers (tree_build 5.8–6.0 ms), the existing memo-restyle backlog item.

**Verified.** Release elements 277 passed / 10 failed: the same 10 names as the
baseline, diffed.


## 2026-10-18 — finalized component memo entries and D7 incremental matching

Base tree `beccc092` · area: component memo, style matching and accessibility.
AMD Ryzen 7 7840HS, workspace release profile (thin LTO, one codegen unit).
Reviewed the rejected-experiments table before implementation.

Component memo entries retain pre-animation computed styles, semantic metadata
and child text. Only projections are copied back: authored attributes, runtime
identity and loop placement stay independent, so input overrides, promotion,
lifecycle classes and keyed reorders do not become authored memo inputs.
Annotation validates reusable subtrees bottom-up. Style reuse checks the theme,
unique rule-index revision, props, container dimensions and inherited values;
semantic reuse also checks inherited eligibility and changed child text.
Sampled animation styles cannot masquerade as authored styles in a selective
rebuild. Complete identity validation remains mandatory.

D7 now indexes every class atom and pseudo-state dependency separately from
forward candidate buckets, including nested compounds and active theme rules.
Targeted updates check selector entry and exit before reapplying declarations;
unchanged matches keep their computed style and input snapshot. Inheritance
still propagates. Retained frames do not repeat rebuild-only memo validation.
The remaining string declarations stay in the backlog.

Shell builds defer the compiler's preliminary layout and preserve finalized
memo semantics until host finalization. Overflow clamping follows the final
layout; previews retain their complete compiler layout path. This is necessary
for finalized memo styles: a preliminary layout against them can reintroduce
expensive intrinsic text measurement before the final cascade.

**Measured.** Three interleaved before/after samples, both binaries pinned to
CPU 4 with `taskset -c 4`, with compilation finished before measurement:

| Workload | Before | After |
| --- | --- | --- |
| `appearance_frame_cost_profile`: 550 nodes, 920×900, 60 unrelated audio updates per sample | 10.832–10.869 ms/frame | 5.621–5.777 ms/frame |
| Same workload, profiled `style_restyle` (rule attribution enabled) | 3.030–3.089 ms/frame | 0.669–0.680 ms/frame |
| `navigation_frame_cost_profile`: 112 nodes, settled 1920×40, 60 audio updates | 1.648–1.688 ms/frame | 1.646–1.649 ms/frame |
| Same navigation tree, 5,000 audio-only updates (`MESH_BENCH_FRAMES=5000 MESH_BENCH_ONLY=audio`) | 1.572–1.637 ms/frame | 1.553–1.592 ms/frame |

Appearance improves about 1.9×. Navigation ranges overlap; do not claim a clear
full-poll speedup from the smaller style and normalization changes. Exploratory
unpinned short navigation runs varied substantially (updated binary
2.241–2.943 ms/frame); the new animation counter reported zero active-animation
frames, rejecting startup motion as the explanation. Pinning both versions to
one core removed that apparent regression. Use the affinity-controlled ranges
above rather than comparing uncontrolled short samples.

**Structural gates.** `finalized_memo_and_selector_dependency_profile`, three
release runs pinned to CPU 4:

- 513-node tree, 256 class/state rules, 200 clone/finalize cycles: forced full
  117.128–122.110 ms versus finalized memo 57.474–61.597 ms,
  1.915–2.125×. `finalized_memo_speedup` >= 1.5.
- 256 rules, 100,000 unmatched class changes: full selector scan
  386.543–388.317 ms versus reverse dependencies 6.240–6.438 ms,
  60.321–62.077×. `selector_dependency_update_speedup` >= 4.0.

Both gates are registered in `tools/check-performance` and
`config/performance-baseline.tsv`; these are structural workloads, not complete
surface rendering measurements.

**Rejected prototype.** Retaining finalized styles while keeping compiler
preliminary layout made the Appearance frame slower: three interleaved,
unpinned release samples of the same 550-node/60-update workload measured
10.665–11.395 ms/frame baseline versus 25.345–26.575 ms/frame prototype.
`perf` identified intrinsic shaping/font fallback in compiler layout. The final
implementation defers that duplicate pass. Sampling evidence is under ignored
`profiles/finalized-memo-2026-10-18/`.

**Verified.** Release shell: 864 passed / 45 failed / 138 ignored, with exactly
the baseline's 45 failure names. Elements: 284 passed / 10 failed / 64 ignored,
with exactly the baseline's 10 failure names. Frontend: 81 passed / the same two
known failures / 19 ignored. New regressions cover full-resolution parity,
selector removal, compound/theme dependencies, container/prop/index invalidation,
child names, inherited eligibility, authored input preservation, sampled styles,
hide/show template patches, and cached-versus-fresh pixels and semantics.
The existing scroll pixel parity test passes in the shell suite;
`settings_scroll_frame_gate` also passes: 90 frames, 90 paint passes, zero
restyles after the first and shifted finger-frame fraction 0.983333 (gate >= 0.9).
`cargo check -p mesh-core-shell --tests`, shell syntax and diff whitespace checks
pass. Scope ends with these three backlog items; later performance items remain
open. All nine pending design notes remain linked.


## 2026-10-18 — D8: native backend events and coroutine stream lifecycle

AMD Ryzen 7 7840HS, HPET clock source. Nix development shell, Rust 1.98.1,
workspace release profile (thin LTO, one codegen unit). Removed D8, the remaining
D2 stream/stop item, and the D-Bus subscription item from the backlog.

**Implementation.** Capability-gated D-Bus signal filters, PipeWire Node/Metadata
observation, route netlink groups and wlr foreign-toplevel events reuse the
inotify/socket stream ownership, bounded queue and lifecycle. Scripts receive
asynchronous readiness/failure and can cancel an individual generation-owned
handle. PipeWire owns its native objects on a blocking executor thread, woken
on cancellation; other subscriptions await Tokio readiness. PipeWire and Wayland
object counts and record sizes are bounded. Inotify overflow now fails explicitly.
No domain normalization or control policy moved into Rust.

Stream hooks run through D2 coroutines. The service loop removes only the next
callback's records and leaves later input budgeted until it settles, preserving
line/lifecycle order while commands continue. Suspended callbacks retain private
state publications and provider events; failure/cancellation releases them.
Suspended snapshots share the queued-output byte budget. Consecutive suspended
stream failures retain the three-failure terminal policy. Stop cancels earlier
tasks and runs as a coroutine with a two-second suspended-hook deadline. Zero
poll interval explicitly disables the poll branch.

NetworkManager and UPower prefer D-Bus; audio prefers native PipeWire. Missing
grants or asynchronous source failure retain monitor/poll fallbacks. NetworkManager
filters protocol properties against the state it actually exposes; UPower retains
its battery safety poll while reacting promptly to power-source/device changes.
The author contract is in `docs/modules/backend-events.md`. The Nix shell supplies
PipeWire headers, Clang/libclang and the isolated-test D-Bus daemon. D-Bus versions
are locked to releases compatible with the declared Rust 1.85 minimum; validation
here used 1.98.1, not an MSRV toolchain.

**Live provider measurement.** `live_provider_event_profile`, three repetitions,
UPower + NetworkManager + PipeWire providers in a live Wayland session. Each arm
runs five seconds: two seconds of startup settling, then three seconds counted.
The same binary forces native acquisition off for the monitor baseline and enables
native grants for the other arm. Only provider startup/state reads run, without
control commands. The harness services coroutine deadlines, stream batches and
poll-interval changes; its own two-millisecond sampling loop is instrumentation,
not the production scheduler.

| Metric across three runs | Monitor fallback | Native sources |
| --- | --- | --- |
| Monitor process starts for the three providers | 3–3 | 0–0 |
| State-read command invocations in the settled three-second window | 0–0 | 0–0 |
| Sampled peak D2 host-worker occupancy during the five-second arm | 4–4 | 4–4 |

This establishes removal of the three monitor subprocesses. It does not establish
a CPU/latency speedup or lower recurring fork rate: both arms issued zero state
queries in the short settled window, which does not include long safety polls.
The sampled host-worker occupancy is below the process-wide limit of 64; native
PipeWire's loop thread is separate from that pool.

**Rejected broad signal handling.** Before property filtering, three repetitions
of the same release workload produced four settled-window state queries in the
native arm versus zero in the monitor arm. Signal attribution showed Wi-Fi
Strength and Bitrate notifications. Filtering unexposed properties removed the
unnecessary bitrate refresh while preserving Strength and invalidated-property
updates; the final ranges above replace those exploratory results. The native
provider can still read state for meaningful changes that `nmcli monitor` omits.

**Structural gate.** `native_subscription_idle_profile`, three release runs,
one route subscription to link/IPv4/IPv6 address groups, 250 ms with an otherwise
idle network namespace. An idle `sleep` helper represents the subprocess resource
baseline: 1–1 child versus 0–0 native children. Native queue records during the
idle window: 0–0. Gates `native_subscription_helper_processes` and
`native_subscription_idle_events` (both upper 0) are registered in
`tools/check-performance` and `config/performance-baseline.tsv`. These count child
processes and emitted records, not whole-shell kernel wakeups.

**Verified.** Serial release: scripting 350 passed / 33 ignored; backend 48 passed;
capability 9 passed / 1 ignored. All 407 active tests pass. Regressions cover an
isolated real D-Bus daemon (filters, scalar/structured/empty bodies and invalidated
properties), grant/startup denial, netlink admission/cancellation, generation-safe
close, typed and legacy coroutine ordering, commands during suspension, delayed
failures, publication isolation, bounded pending snapshots, cancellation cleanup,
stop deadline, provider fallback and NetworkManager property relevance. The prior
async file-read test now waits for actual completion rather than assuming one
notification means completion.

Live PipeWire startup/teardown and supported wlr foreign-toplevel startup/teardown
pass in all three release samples. Shell and LSP test targets type-check. Changed
Rust files pass scoped rustfmt checks; shell syntax, manifest JSON, diff whitespace
and all nine pending-note backlog links pass. No whole-workspace suite was rerun;
the known unrelated fixture failures remain outside this validation.


## 2026-10-18 — clip-invariant rounded fills and borders

AMD Ryzen 7 7840HS, workspace release profile · area: Skia painter

**Defect.** Skia's anti-aliased rounded-rect coverage depends on the clip. A
probe (200×160 surface, 180×140 rect, radius 14) measured up to 56/255 at arc
pixels when the clip cuts the arc. A clip that contains the whole corner still
differs by ±1, because cutting a straight edge elsewhere changes the result.
`draw_rrect`, `draw_path`, stroke and `draw_drrect` all behave this way, and so
do straight fractional inner border edges (±1–2).

**Change.** `surface/painter/rounded.rs` computes each rounded fill or border
ring once, without any clip, and caches the result per geometry. Rects are at
integer device bounds.
- Each corner is rasterized from a symmetric shape twice its region's size, so
  no cut touches the arc. Pills and other shapes whose corners overlap are
  rasterized whole, up to 256×256 pixels.
- Fully covered pixels (body, integer border bands, corner interiors) form one
  `Region` drawn as a solid fill.
- Partially covered pixels go into one A8 atlas drawn with a single
  `draw_atlas` at integer offsets.
- A fractional border edge is one row or column of solid fill at scaled alpha.

Full coverage stays out of the atlas: a tinted A8 blit rounds differently from
a solid fill, and a first version that masked whole corner squares left a
1/255 seam. That version failed
`real_navigation_bar_repaints_existing_transition_state_when_theme_changes_back_to_dark`
(`[6,4,4,92]` vs `[5,4,4,92]`).

Canvas matrices other than an integer translation, and larger overlapping
shapes, use the old direct draw. Strokes go through the same ring path. The
cache is a thread-local `ByteLruCache` (512 entries, 4 MiB).

**Correctness.** `rounded_shapes_repaint_identically_under_any_clip` (fill
with mixed radii, fractional-width border, pill fill and stroke, translucent
colours, about 500 clips) requires exact equality; it fails on the old path at
the first clip. `settings_scroll_shift_matches_full_paint` is now exact
(0 differing pixels, before up to 71 with delta 55), and its tolerance is
removed. `ScrollShift::arc_rows`/`arc_columns` stay: rounded gradients,
images, clips and transformed shapes still rasterize through Skia under the
damage clip (backlog).

**Cost.** Microbenchmark: 300 translucent 80×26 radius-8 fills on a 920×900
raster, mean of 50 passes. Direct `draw_rrect` takes 0.79–0.84 ms; the final
version takes 1.23–1.24 ms. Rejected variants, measured the same way:
- one tinted A8 image per corner: 2.5 ms;
- full runs as rects in the body path: about 10% slower on appearance paint
  end to end;
- region drawn under a canvas translate: 1.23 ms;
- regions cached in device coordinates: 1.37 ms;
- region boundary path: 1.39 ms.

End to end, three alternating rounds against `d2c8e294`:

| Workload | Before | After |
| --- | --- | --- |
| `appearance_frame_cost_profile` paint only | 1.429–1.534 ms/frame | 1.419–1.459 ms/frame |
| `appearance_frame_cost_profile` resource-list scroll | 2.022–2.060 ms/frame | 2.010–2.201 ms/frame |
| `settings_scroll_frame_gate` p50 / p90 | 2.650–3.091 / 4.622–4.896 ms | 2.762–2.924 / 4.612–4.693 ms |
| `navigation_frame_cost_profile` paint only | 0.334–0.514 ms/frame | 0.496–0.505 ms/frame |

All four are within noise. This is a correctness change, not a speedup.

**Verified.** Release `mesh-core-shell`, `mesh-core-render` and
`mesh-core-elements` (parallel, 4 threads): 1447 passed, 55 failed. The failure
set matches `d2c8e294` exactly. A first full workspace run of the earlier
version differed from the baseline in the seam test above, plus LSP
`shipped_manifests_validate_cleanly`, which now passes.


## 2026-10-19 — unread service updates schedule no frame

Base tree `17224ae1` · area: shell service delivery and effect settling.
AMD Ryzen 7 7840HS, workspace release profile, `nix develop`.

**Cause.** With D1 patching and finalized memo entries, an audio update the
Appearance page does not use still cost a frame of `tree_build`. No template
expression read the changed fields, so no patch target and no service node set
existed; `handle_service_event` still requested `SCRIPT_NARROW` because effects
might derive template state from the payload. With no pending node set,
`narrow_script_update` evaluated the whole template. The reader is the hidden
`AudioPage`: its `require("mesh.audio")` proxy observes every audio payload.

**Change.** When every runtime the update dirties has complete template read
records (`template_expressions_reading_service_fields` returns `Some`) and none
reads a changed field, delivery settles effects immediately. If they write
nothing a template reads, the narrow pass is dropped and the surface stays clean:
`wants_render()` is false and the render loop schedules no frame. Effect writes
still take the binding-patch path, or the narrow path when a patch is not
possible. First payloads and runtimes without evaluated templates keep the old
conservative path. `effects_changed_templates` now persists from delivery until
the frame's tree is built, so an effect that settled early still blocks the
selective build.

**Benchmark.** `appearance_frame_cost_profile` now paints only when
`wants_render()` is true, as `Shell::render_components` does, and reports how
many updates scheduled a frame. Warmup settles the page-enter transitions. A
second loop publishes contract-valid `percent` changes that the hidden Audio
page's effect turns into bound writes. The existing loop's payload
(`volume`/`sink`) is not part of the `mesh.audio` contract, so nothing reads
it. The baseline ran the same benchmark file in a HEAD worktree.

**Measured.** Both binaries pinned to CPU 4 with `taskset -c 4`, three
interleaved runs. The desktop was in use, so absolute values sit about 2× above
the 2026-10-18 entry; compare within this table only.

| Workload | Before | After |
| --- | --- | --- |
| Appearance, 550 nodes, 920×900: 60 unrelated audio updates | 10.392–11.081 ms/update, 60/60 frames | 0.021 ms/update, 0/60 frames |
| Same: 60 hidden-page volume updates (bound effect writes, patched) | 6.673–7.551 ms/update | 6.429–7.376 ms/update |
| Same: paint only | 2.551–2.587 ms/frame | 2.139–2.921 ms/frame |
| Same: resource-list scroll | 4.041–4.414 ms/frame | 3.416–4.362 ms/frame |
| `navigation_frame_cost_profile`: audio poll (read) | 2.484–2.942 ms/frame | 2.348–3.003 ms/frame |

Only the unrelated update moves; its frame is gone, not just cheaper. The other
rows overlap. Regression `unread_audio_updates_schedule_no_frame` asserts that
no frame is scheduled; it fails at `17224ae1`. `hidden_audio_page_volume_patches_without_rebuild`
asserts a patched, non-rebuild frame whose text matches a full rebuild.

**Remaining cost of patched frames.** perf, 1,500 hidden-page volume updates
(~4.1 ms each, unpinned): `layout_dirty_node_snapshots` fingerprints every node
(~21% inclusive; `style_fingerprint` + `attributes_fingerprint` ~19% self),
Taffy incremental layout ~13% for text inside a `display: none` page, the style
animation pass ~12%, `RetainedDisplayList::update_inner` ~10%,
`validate_widget_tree` ~5%; delivery plus effect and patch evaluation ~26%.
Backlog item added.

**Verified.** Serial release `mesh-core-shell`, after versus a HEAD worktree
with its own target dir: the failure sets differ only by the new regression
(fails at HEAD) and two tests that asserted `SCRIPT_NARROW` stayed pending after
a service event. Those now assert that a frame is pending without a template
rebuild and keep their rendered-text and scoped-retained checks. HEAD has 45
serial failures, not the 27+3 recorded in STATUS.

## 2026-10-19 — CMP-13 canonical source-path cache verification

Follow-up to the 2026-10-17 rejected deep-schema-equality key experiment.
Measured the existing replacement implementation; no new cache algorithm was
introduced in this verification. Local x86_64 development host under
`nix develop`, release profile (optimization 3, thin LTO, one codegen unit),
unpinned with other desktop activity. Existing
`child_settings_resolution_workload`: one child module, one canonical source
file, 32 numeric props, 2,000 resolutions per sample, four samples per case.
Each sample compares direct uncached projection against the production
source-path cache, cleared before the sample and populated on its first lookup.
These are same-revision path comparisons, not different-build timings.

| Namespace | Direct resolution | Canonical source-path cache |
| --- | --- | --- |
| No stored overrides | 0.931–1.007 ms | 0.201–0.213 ms |
| 32 stored prop overrides | 33.104–41.630 ms | 2.296–3.784 ms |

The workload passed; no named performance gate exists for it. The focused
`child_settings_cache_refreshes_for_props_and_effective_settings` regression
also passed, covering settings replacement and source-reload invalidation.
Catalog adoption clears the cache at the shared adoption boundary. This closes
the pending replacement-key/populated-namespace measurement from October 17;
it makes no claim about complete frame latency.

## 2026-10-19 — rounded clips, gradients, images and shadows under damage

Area: CPU Skia painter. AMD Ryzen 7 7840HS, x86_64, `nix develop`, workspace
release profile (optimization 3, thin LTO, one codegen unit). Reviewed the prior
rounded-fill/border entry and rejected experiments before implementation.

**Correctness.** Rounded `PainterClip` radii were ignored, background images
did not carry their rounded boundary into painting, and retained overflow
clips contained only rectangles. Clips now carry normalized corner geometry
through affine and retained scroll scopes; point membership uses that geometry.
Overflow radii participate in the render-object clip fingerprint, so radius
changes invalidate clean descendant clip stacks. Background images install
their rounded clip explicitly.

Rounded coverage is recorded at complete intrinsic bounds in a picture shader.
A runtime shader keeps mask composition on the high-precision pipeline. The
plain bitmap shader prototype still differed by one channel value under a
one-pixel-wide clip for translucent gradients and nested image masks; it is
rejected. High-precision masking passes without a gradient raster cache, but
its repeated evaluation costs more than retaining the completed color raster.
Rounded gradients and shadows therefore retain premultiplied RGBA rasters.
Clip shaders and each effect-raster cache are bounded to 128 entries / 32 MiB
(the clip budget estimates intrinsic raster plus recorded-picture bytes).
Geometry keys omit position; color rasters include their colors. Raster
admission retains the existing 8192-pixel dimension limit and direct-draw
fallback. Existing transformed solid-fill/border fallbacks are unchanged.

**Rejected cost prototype.** Runtime-masked gradients plus tinted A8 shadow
rasters passed exact repaint tests, but four paired release samples of the
workload below took 123.347–125.813 ms for gradients against a direct reference
of 57.178–63.582 ms; A8 shadows took 98.694–101.740 ms against 16.124–29.849 ms.
That is 1.94–2.20× / 3.39–6.12× the paired reference time. Retaining color
rasters replaces this prototype.

**Final cost.** `rounded_effect_raster_cost_workload`: 920×900 CPU raster,
140 rectangles in a 10×14 grid, each 80×54 with radius 12; four paired samples,
20 full replays per sample. Gradient colors are RGBA (240,100,40,170) to
(30,100,240,220); shadows use the first color and blur radius 5. Production
caches are warmed before sampling. The reference directly draws Skia rounded
rectangles with a prepared gradient shader or the original mask-filter blur.
It is a same-process native primitive reference, not a checkout of the old
shell. Timings below are totals for 20 replays (2,800 shapes).

| Workload | Direct Skia reference | Completed color rasters |
| --- | --- | --- |
| Rounded gradients | 85.843–88.293 ms | 31.057–36.257 ms |
| Blurred shadows | 61.670–75.388 ms | 55.159–64.211 ms |

The desktop and another release build were active; compare paired paths within
each probe, not absolute timings across the prototype and final probes. No
named performance gate exists for this correctness workload. No complete-frame
speedup or cold-cache improvement is claimed.

**Verified.** Final release renderer: 297 passed / 0 failed / 42 ignored; final
debug renderer: 298 passed / 0 failed / 42 ignored. All six new exact-pixel
regressions pass, including outside-damage preservation, translucent gradients,
nested image masks, sharp/blurred shadows, retained radius changes and overflow
scrolling at scales 1, 1.25 and 2. Release interaction geometry: 10/10. Final
debug shell `settings_scroll_shift_matches_full_paint` and
`real_navigation_bar_repaints_existing_transition_state_when_theme_changes_back_to_dark`: 2/2.
Scoped rustfmt and diff whitespace checks pass. The unrelated shell and GPU
work in the shared workspace was preserved.

## 2026-10-19 — GPU presentation: Skia-GL paint and fenced dmabuf buffers

Area: renderer, Wayland presentation, shell surface targets. AMD Ryzen 7
7840HS with Radeon 780M (Hyprland's main device) and RTX 4060 Laptop,
`nix develop`, workspace release profile. Design and remaining work:
[GPU backend note](../todos/pending/2026-07-15-gpu-rendering-backend.md).

Each surface keeps a retained `SurfaceCanvas`: a CPU `PixelBuffer` or a Skia
Ganesh render target on an EGL device context matching the dmabuf feedback's
main device. Presentation copies the frame's damage into one of a 2–3 deep
ring of GL textures exported with `EGL_MESA_image_dma_buf_export`, then
imports an `EGL_ANDROID_native_fence_sync` sync file into the dmabuf as an
implicit write fence. GPU paint stays on the UI thread (painter state is
thread-local there; the scene-thread move waits for SH-11). On by default in
the CLI; `MESH_RENDER_GPU=0` or any unsupported step falls back to SHM.

**Workload.** `gpu::tests::frame_cost::gpu_frame_cost_workload` (ignored
benchmark, `--features renderer-gpu`): a 920×900 logical surface at scale 2
(1840×1800 px) with a gradient root, a 6×10 grid of rounded, bordered,
shadowed cards with text, and an 840×260 frosted panel (backdrop blur 12).
Per frame: clear and repaint one damage rect, then copy it to the buffer the
compositor reads (SHM row copy on CPU; GPU copy plus fence on GPU). 40 frames
per sample, 4 samples after a warm-up, ranges per frame. GPU samples end with
one `glFinish`, so they include GPU completion.

| Damage | CPU raster + SHM copy | AMD 780M | RTX 4060 |
| --- | --- | --- | --- |
| Full surface | 35.447–36.183 ms | 1.847–1.971 ms | 2.474–2.588 ms |
| Frosted panel (840×260) | 16.627–16.825 ms | 0.965–1.243 ms | 1.206–1.280 ms |
| Small widget (138×76) | 0.196–0.237 ms | 0.215–0.246 ms | 0.231–0.246 ms |

**Rejected: CPU wait per present.** The first version waited for the GPU
(`flush_submit_and_sync_cpu`) before attaching each buffer. Same workload,
earlier run: AMD full 4.144–4.499 ms, frosted 2.818–3.725 ms, small widget
0.473–0.786 ms (2–3× the CPU path for small damage). The imported fence
replaces it; the wait remains only as the fallback when the fence cannot be
attached (kernels before 6.0).

**Live check.** Release shell under Hyprland: `gpu present: enabled` on the
780M. Top-bar captures against `MESH_RENDER_GPU=0`: 118 of 316,800 pixels
differ by more than 4% (antialiasing); clock and counter updates presented
correctly across ring buffers.

**Verified.** `mesh-core-render` release: 297 passed (default), 306 passed with
`renderer-gpu` (`MESH_REQUIRE_GPU=1` for `gpu::`). `mesh-core-presentation`:
111 passed / 3 failed with and without `gpu`; the same 3 config tests fail at
`ef6f3c8b`. `mesh-core-shell` serial release: 866 passed / 45 failed with and
without `gpu`, failure names identical to an `ef6f3c8b` worktree. No named
gate exists yet.

## 2026-10-19 — Shared glyph pages: text on the GPU path

Area: text painter (`surface/text.rs`, `surface/glyph_pages.rs`). Same machine
and profile as the previous entry. Follows it: the GPU path shipped there made
text-dense repaints far slower than the CPU.

**Cause.** Each glyph drew from its own Skia image. On Ganesh every glyph
became a texture op with a distinct texture, and op combining is quadratic in
them: a perf profile of the text workload put 77% of samples in
`TextureOpImpl::onCombineIfPossible`. The cost was CPU-side and identical on
both GPUs.

**Change.** Rasterized glyphs up to 128 px are also copied into shared 1024²
pages (A8 masks, RGBA color glyphs; at most 4 + 2 pages, cleared together with
the glyph cache when full). Text draws use a page snapshot with a strict
source rect; a glyph placed after the latest snapshot draws from its own image
until the next text node refreshes the page, so a first frame snapshots each
dirty page at most once per text node. CPU output is unchanged: all
exact-pixel renderer and shell text tests pass.

**Workload.** `gpu_frame_cost_workload` gained a text-dense surface: 40 lines
of 81-character text at 14 px, 920×900 logical at scale 2. Per frame ranges,
4 samples × 40 frames:

| Damage | CPU raster + SHM | AMD 780M before | AMD 780M after | RTX 4060 before | RTX 4060 after |
| --- | --- | --- | --- | --- | --- |
| 40 text lines | 6.999–7.579 ms | 120.024–122.587 ms | 2.589–2.640 ms | 119.949–121.973 ms | 2.350–2.462 ms |
| One text line | 0.137–0.142 ms | 0.452–0.502 ms | 0.169–0.278 ms | 0.358–0.399 ms | 0.198–0.205 ms |
| Settings full surface | 33.960–34.676 ms | 2.639–2.918 ms | 2.282–2.573 ms | 2.251–2.300 ms | 1.951–2.026 ms |

**Verified.** Renderer release 299 passed (default) and 308 passed
(`renderer-gpu`, `MESH_REQUIRE_GPU=1`). Shell serial release with `gpu`:
866 passed / 45 failed, failure names identical to the `ef6f3c8b` baseline.
Live top-bar capture against `MESH_RENDER_GPU=0`: 147 differing pixels, all in
the clock and CPU counter. Icon images and icon-font glyphs still draw one
texture each; with tens of icons per surface the combining cost is not
measurable yet.


## 2026-10-19 — Retained interaction damage and resource completion consumers

Area: shell invalidation, render resources, hover layout and wake deadlines.
AMD Ryzen 7 7840HS, workspace release profile (thin LTO, one codegen unit),
CPU raster harnesses, desktop in use.

**Baseline first.** Two-way bindings were committed separately as `3b68fb5c`;
the pre-existing sweep inventory is `27ac2c9b`. Release profiles initially
failed to compile because a shell render test omitted the new shadow-list
field. Test-only repair `0b47dd1c` establishes the clean baseline. Before this session's
performance edits, three runs each of `navigation_hover_profile` and
`appearance_frame_cost_profile` passed. Navigation step p50 was
1.396–2.071 ms; Appearance unchanged paint requests were 1.782–1.991 ms.
The baseline test executable was retained for interleaved comparisons.

**Changes.** Interaction restyles and animation ticks preserve retained surface
pixels, allowing display-list damage to select the repaint. Ordinary unchanged
paint invalidations can also produce no damage. Explicit host `request_paint`
still invalidates pixels and clears child display lists: resource completions
can change pixels without changing a node fingerprint, including alongside
unrelated partial style damage in a promoted popup.

Icon resolution, file-icon raster, font-glyph raster and image-decode queues
retain weak subscriptions per resource key. Shared pending jobs collect every
requester; only current published results notify them. Component runtime paint
scopes include parent and child targets. Completion polling schedules only those
components, coalesces their loads, and clears their child paint-generation cache;
dropped runtimes do not remain alive through subscriptions. A completion drained
during paint remains visible to the next loop's ready-work check.

Hover rules cache layout dependencies per pseudo-state across component imports,
contributions and theme rules. Paint-only hover changes omit LAYOUT; dimensions,
fonts, custom variables and unknown declarations remain conservative. Generic
interaction updates retain their existing layout obligation. The broader split
of layout/paint style fingerprints in mixed frames remains in the backlog.

Quiet surfaces borrow their IDs while receiving window-state/resize changes;
cloning happens after the wants-render gate. Focus-grab expiry and dirty visible
targets' 50 ms callback-escape deadlines now participate in loop sleep. The
parallel session's repeat deadline and shared-Arc text-measure key edits were
left intact; its font-pack parsing change is also present in the candidate.
These batch timings do not isolate those changes from this session's work.

**Final measurements.** Three interleaved baseline/final executable runs each,
after builds and other test runs finished. Navigation: 1920 px wide bar,
25 cycles × four triggers, 100 popover opens, 400 input/frame steps. Appearance:
920×900, 550 nodes, 60 updates per loop, including hidden Audio-page binding
updates and a 240-entry resource catalog. Times below are ms per step/frame.

| Workload | Baseline | Final |
| --- | --- | --- |
| Navigation hover step p50 | 1.434–1.524 | 1.183–1.237 |
| Navigation hover step p90 | 1.621–2.231 | 1.452–1.703 |
| Appearance unchanged paint invalidation | 1.782–1.845 | 0.099–0.106 |
| Appearance hidden-page volume update | 5.022–5.331 | 4.151–4.277 |
| Appearance resource-list scroll | 2.480–2.604 | 2.631–2.770 |

The Appearance paint-only loop changes no display-list pixels: its saving is
skipping raster work on unchanged invalidations, not faster full-surface raster.
Resource-list scrolling is not improved by this batch. Unrelated audio updates
still schedule zero of 60 frames on both sides.

`resource_completion_fanout_profile`: 32 components, eight shared resource keys
requested by two components, 1,000 batches, three release runs. Coalesced repaint
requests are 2,000 versus the old broadcast's 32,000 in every run (fraction
0.0625). Gate `resource_completion_repaint_fraction` ≤ 0.0625 is registered in
`tools/check-performance` and `config/performance-baseline.tsv`. The profile
measures routing/counts, not renderer throughput; all three gate samples pass.

**Correctness.** `hover_and_transition_damage_matches_full_repaint` requires
exact equality against a fresh full-surface paint after each of 324 frames:
three hover styles (colour/radii/shadows, opacity/transform, layout), entry/exit,
transition samples and scales 1, 1.25 and 2. Paint-only hover layout and layout
fallback tests pass. Explicit parent and child repaint regressions cover pixels
outside fingerprint-derived damage. Shared image completion, nested consumer
scopes, deduplication and dropped-consumer tests pass.

Final serial release shell: 872 passed, 45 failed, 138 ignored; failure names
are exactly the clean baseline's 45, with no additions or removals. Release
renderer: 306 passed, zero failed, 43 ignored. Scheduler-focused debug run:
14 passed. Final GPU shell check, scoped rustfmt, shell syntax and diff checks
pass. Presentation debug: 111 passed, three config-fixture failures, 11 ignored;
the existing pre-change release executable reproduces those same three failures.
Full debug shell before the final parent-pixel regression: 873 passed, 43 failed,
138 ignored; both explicit-repaint regressions subsequently pass together.

Completed sweep work was removed from the backlog; mixed-frame style hashes and
render-queued effects remain. No design notes were associated with these items,
and pending-note links have no orphans.


## 2026-10-19 — Dedicated scroll follow-up after retained interaction damage

Follow-up to the retained interaction/resource consumer entry above. The
sequential Appearance profile's scroll loop was higher in the final batch, so
`settings_scroll_frame_gate` was also measured: same AMD 7840HS, release,
920×900 Appearance with 240 resource entries, 60 two-finger input frames plus
30 momentum frames at 16 ms, three interleaved runs per executable. A second
three-run comparison reverses the order and pins both executables to CPU 6.
All runs report 90 frames, 90 passes, zero restyles after the first, and pass
`settings_scroll_shifted_fraction` (0.983333–1.0, threshold ≥ 0.9).

Times are frame p50 / p90, ms:

| Sampling | Baseline | Final |
| --- | --- | --- |
| Unpinned, baseline then final | 2.198–2.498 / 4.325–4.641 | 2.461–2.626 / 4.085–4.445 |
| CPU 6 pinned, final then baseline | 2.509–2.586 / 4.609–4.913 | 2.428–2.933 / 4.606–7.066 |

Pinning and reversing the order produces both positive and negative paired
p50 differences; the final p90 is also noisy. These samples support the
structural scroll gates, not a scroll speedup claim. The higher sequential
profile scroll range remains recorded in the preceding table.

## 2026-10-19 — hover repaint and hover layout skip: measured, no step win

Change under test: commit `228ef3f5`, which stops `:hover` and transition
invalidations from setting `surface_pixels_invalid`, and drops LAYOUT from
hover-only restyles when no hovered state rule touches a layout property.

**Baseline** is HEAD with only the hover-specific lines restored
(`invalidation_requires_pixel_repaint` blocks and the old
`invalidate_interaction_restyle()` call). `cc37c6f2`, the commit before
`228ef3f5`, does not compile on its own, so it was not usable as a baseline.

Release (`cargo test -p mesh-core-shell --release -- <name> --ignored
--nocapture`), three runs each, AMD Ryzen 7 7840HS, same machine session:

| Workload | Baseline (three runs) | Candidate (three runs) |
| --- | --- | --- |
| `navigation_hover_profile` step p50 | 1.464, 1.577, 1.500 ms | 1.611, 1.450, 1.472 ms |
| `navigation_hover_profile` step p90 | 2.016, 2.888, 2.074 ms | 2.643, 1.883, 2.131 ms |
| `appearance_frame_cost_profile` paint only | 1.786, 1.702, 1.679 ms/frame | 1.675, 1.657, 1.719 ms/frame |
| `appearance_frame_cost_profile` resource-list scroll | 2.305, 2.313, 2.403 ms/frame | 2.309, 2.227, 2.319 ms/frame |
| `appearance_frame_cost_profile` hidden-page volume | 4.736, 4.030, 4.049 ms/frame | 4.137, 4.420, 3.974 ms/frame |

**Result: no measurable change.** The ranges overlap on every row. The
navigation bar is about 112 nodes, so its full-surface repaint was already
cheap, and the step time is dominated by input handling and other per-frame
work the change does not touch. The appearance profile does not send any hover,
so it could not show a hover effect in the first place.

**Parity.** These pass on the candidate: `hover_and_transition_damage_matches_full_repaint`
(1.0, 1.25 and 2.0 scale, partial damage on hover frames),
`settings_scroll_shift_matches_full_paint`,
`structural_handler_write_on_narrow_path_matches_full_rebuild_pixels`, and the
render-crate tests `sparse_hover_repaint_under_frosted_root_matches_full_repaint`,
`hover_inside_in_surface_frosted_panel_matches_full_repaint`,
`sparse_repaint_with_backdrop_damage_expansion_matches_full_repaint` and
`retained_content_scope_scroll_matches_full_paint_with_text_and_damage`.

**Failing, not caused by this change:** `animation_scoped_retained_diff_matches_full_pipeline`
(`animation.rs:1154`, `last_update_was_scoped()`). It fails on the candidate, on
the hover-reverted baseline, and at `0b47dd1c`, which predates this session. It
was not in the earlier failure list, which I could not reconcile.

**Open:** the sweep's estimate of the hover cost was too high for this
workload. A bigger hover-frame win is more likely on a larger surface such as
Settings; `appearance_frame_cost_profile` does not measure hover there.

## 2026-10-20 — SH-11 / D6 production scene-worker integration

GPU execution now uses the bounded scene worker. The UI lowers a complete Skia
picture using an explicit frame `PaintContext`; immutable raster images, icon
variants, glyph pages, backdrop shaders and tooltip settings stay pinned by the
picture. Layout and glyph preparation share the UI font database, so production
does not duplicate the entire installed font catalog into a frozen worker font
database. `FrozenFontDatabase` remains available for other consumers.

The worker owns EGL/Ganesh, retained pixels, damage copies and fenced exported
textures. Each surface has at most three exports; targets share a 512 MiB byte
budget. Only duplicated dmabuf descriptors, damage and independent release
leases cross to Wayland. Successful attachment acknowledges the scene. Removed
protocol buffers retire their textures rather than falsely releasing storage
still held by the compositor. Old releases cannot unlock newly leased textures.
Errors retain the existing full-repaint SHM fallback. The production queue
admits 64 surface incarnations; idle workers block on a condition variable.

Presentation currently waits for worker completion. This moves GPU ownership,
but does not pipeline frame N raster with frame N+1 scripts. Full immutable
recording also removes the GPU path's in-place scroll shift; damage replay is
still partial. Pipelining and narrower recording remain backlog work. The
rejected-experiments table was checked before implementation.

**Measurement:** `gpu_scene_handoff_profile`, workspace release (thin LTO,
one codegen unit), Ryzen 7 7840HS, desktop in use. A synthetic Settings-like
920×900 logical surface at scale 2 (1840×1800 pixels), with frosted/gradient
cards; full damage, one 840×260 frosted panel, or one 138×76 widget. Three
alternating samples per side after one warm-up, 20 frames/sample. Both paths
include GPU completion: direct production painter + retained-to-dmabuf copy,
versus complete UI preparation + queue/wait + worker damage replay/copy.
These are per-frame ranges, not an idle or whole-shell responsiveness claim.

| GPU / damage | Direct | Scene worker including preparation | UI preparation alone |
| --- | --- | --- | --- |
| RTX 4060 Laptop / full | 2.493–2.892 ms | 2.803–2.998 ms | 0.415–0.446 ms |
| RTX 4060 Laptop / frosted | 1.181–1.234 ms | 1.707–1.869 ms | 0.348–0.405 ms |
| RTX 4060 Laptop / widget | 0.238–0.311 ms | 1.064–1.449 ms | 0.390–0.553 ms |
| AMD 780M / full | 4.162–4.570 ms | 4.157–4.377 ms | 0.504–0.622 ms |
| AMD 780M / frosted | 2.299–3.038 ms | 2.813–3.065 ms | 0.455–0.523 ms |
| AMD 780M / widget | 0.450–0.546 ms | 1.315–1.462 ms | 0.363–0.404 ms |

Small partial frames regress under the synchronous complete-recording path;
this is accepted ownership infrastructure, not a universal speedup. There is
no relative speed gate for this migration; D9 workload gates remain separate.

**Verified:** debug renderer suite 318 passed / 13 ignored, presentation suite
112 passed. Recorded GPU scenes are compared against direct GPU and CPU panel,
icon-grid and effect scenes. Tests cover immutable source lifetime, disjoint
damage, queue bounds/coalescing, failure recovery, leases and shutdown. A new
production component scene/raster byte comparison passes at scales 1, 1.25 and
2 across repeated frames. Required-GPU live compositor presentation passed six
frames on the current Hyprland session, including release and teardown; NVIDIA
as compositor main device remains untested. Full shell comparison is recorded
in the monthly log when complete.

## 2026-10-20 — D9 CPU workload budgets

Six absolute workload ceilings now run in a dedicated CI job and the full
performance runner. `tools/check-performance --workloads-only` builds one
release shell test binary using Cargo artifact metadata, then launches each
workload in a fresh process. Missing, duplicate, invalid and over-budget
measurements fail, as do missing budgets, failed builds, missing test binaries
and failed workload assertions. Python is declared in the Nix environment.

The service workload checks changing volume tooltips, hover requires rendered
state changes, popovers must produce child pixels, theme modes must change
pixels, and scroll retains its one-pass/no repeated restyle/shift assertions.
Idle models three 120-second reload-maintenance traces and checks the actual
scheduler sleep deadline. It bounds maintenance wakeups, not all OS wakeups.
CPU raster and GPU presentation budgets remain distinct; GPU gates stay in the
renderer backlog. The shared direction note remains linked for unfinished D4.

**Measured:** workspace release, thin LTO, one codegen unit, Ryzen 7 7840HS,
CPU raster at scale 1. Navigation is 1920×40; Settings Appearance is 920×900,
with 240 icon-theme and 240 font descriptors, bounded to 166 retained nodes.
Per sample: 64 audio deliveries, 64 crossings between two navigation controls,
12 theme-popover first paints, 24 dark/light switches, and 60 finger frames plus
30 momentum ticks. Three samples use the same compiled binary, each workload
in a fresh process: the first through the complete gate command, two repeated
direct test-binary runs. The table reports ranges of p90 milliseconds, except
idle, which is wakeups/minute. Every sample is below its ceiling.

| Metric | Three-sample range | Ceiling |
| --- | --- | --- |
| `d9_hover_p90_ms` | 1.113–1.323 | 16.667 |
| `d9_idle_wakeups_per_minute` | 30.000–30.000 | 30.000 |
| `d9_popover_open_p90_ms` | 3.435–6.427 | 100.000 |
| `d9_scroll_p90_ms` | 2.973–4.603 | 16.667 |
| `d9_service_update_p90_ms` | 1.986–2.182 | 16.667 |
| `d9_theme_switch_p90_ms` | 12.184–13.592 | 100.000 |

These establish initial regression budgets; production execution paths are
unchanged. Theme timing excludes file loading/settings persistence; popover
timing includes child allocation/raster but excludes compositor presentation.

**Validation:** all five workload tests (six metrics) pass; four runner tests
cover valid output, harness-prefixed output and rejection paths. The runner
tests also pass with Python from the updated Nix environment. Shell syntax and
`git diff --check` pass. A shared-process prototype failed the idle deadline
assertion after raster tests left resource work pending; separate workload
processes resolve that contamination while retaining one compiled binary.


## 2026-10-20 — K: parallel CPU raster across surfaces

Area: shell frame loop, CPU raster canvas. AMD Ryzen 7 7840HS (16 threads),
`nix develop`, workspace release profile. Removed the "parallelize paint across
surfaces" backlog item.

`render_components` now runs in three phases. Phase 1 (serial, VM-bound) is
unchanged up to paint, except that when two or more components want to render,
each parent `RasterCanvas` records its damage-clipped paint into a Skia picture
instead of rasterizing. Phase 2 replays every surface's pictures into its own
`PixelBuffer` on the rayon pool. Phase 3 presents each parent and reconciles its
children in the original order. Pictures pin their images and glyph pages, so
replay needs none of the painter's thread-local caches — this replaces the
original plan of running `paint_pixel_regions` itself on workers. Any pixel
read (scroll shift, debug overlay, SHM copy, dev window) replays pending
pictures first. Child popups and the SHM copy remain serial (new backlog line).

**Workload.** `gpu::tests::frame_cost::parallel_cpu_raster_profile` (ignored,
`--features renderer-gpu`; CPU only): three 920×900 logical surfaces at scale 2
(1840×1800 px) from the frame-cost scenes, 40 frames × 4 samples after warm-up,
two runs on the rayon pool, per-frame ranges. "Full" repaints all three surfaces (cards scene,
40 text lines, cards scene). "Partial" damages the 840×260 frosted panel, one
text line and a 138×76 widget, so one surface dominates.

| Frame | Serial raster | Record (UI thread) | Record + parallel replay |
| --- | --- | --- | --- |
| Full, 3 surfaces | 74.783–76.111 ms | 1.312–1.655 ms | 39.216–40.579 ms (1.84–1.94x) |
| Partial, 3 surfaces | 16.789–17.287 ms | 0.276–0.426 ms | 16.458–16.871 ms |
| One surface, frosted panel | 16.204–16.648 ms direct | — | 16.242–16.508 ms record + inline replay |

Gains are bounded by the largest surface in the frame. A lone painting
surface keeps the direct path (nothing to overlap), although recording
measured cost-neutral there.

**Rejected:** replaying on `std::thread::scope` threads spawned per frame (three
runs) cost 1.3–1.6 ms on the partial frame (18.075–19.090 ms vs 16.687–18.625 ms serial),
consistent with Skia's blur scratch allocations faulting in fresh threads'
malloc arenas. The persistent rayon pool removes that penalty.

**Verified.** Release: render lib 310 passed, presentation 112 passed, three
new canvas tests (replay matches direct raster pixel for pixel, ordered
replays over retained pixels, shift after recorded paint) and two shell tests
(multi-surface frames defer and present replayed pixels; a lone surface
paints in place). Shell lib: 42 failures before and after, identical names,
diffed against a path-limited stash of this change.


## 2026-10-20 — GPU bitmap residency and hardware workload gates

Area: Skia scene preparation, bitmap pages, GPU worker and performance CI.

Small scene images, raster icons and icon-font masks now share immutable
1024-wide RGBA/A8 pages through the existing glyph-page allocator. Cells up to
128×128 pixels use strict source rectangles; new cells use their original image
until the next scene-boundary snapshot. Full atlases preserve existing cells
and cache overflow as standalone images. Resource revisions reset the atlas;
submitted pictures keep their snapshots. CPU raster keeps its original images,
and text keeps its existing shared glyph pages.

Prepared scenes carry deduplicated image identities. The GPU worker retains up
to 2,048 / 64 MiB, conservatively charged at four bytes/pixel, and sets Ganesh's
cache allowance to admitted target bytes plus 64 MiB. Ganesh owns the actual
cached uploads and may evict under pressure. No context-bound image crosses the
UI/worker boundary. The target admission budget remains 512 MiB; the small-asset
atlas has at most two color/four mask pages and 16 MiB of source identities.

**Gates.** `tools/check-gpu-performance` runs exact release tests
`gpu_workload_visual_match_gate` and `gpu_workload_frame_cost_gate` with
`MESH_REQUIRE_GPU=1`. Missing GPU/export support and zero selected tests fail.
Visual comparisons read the actual exported slot against CPU raster and direct
GPU paint, including full, large partial and small partial damage. Recorded vs
direct GPU permits 0.1% of pixels above delta 2 / worst delta 3; CPU/GPU Settings
permits 10% / 40 for blur/coverage, while text and assets permit 1% / 4. Measured
Settings CPU/GPU differences: 241,964–250,576 / 3,312,000 pixels above delta 2,
worst 33; text and assets have zero pixels above delta 2, worst 2. Both GPUs pass.

Frame gates include complete UI preparation, worker handoff, retained paint,
dmabuf copy/fence submission and GPU completion, excluding compositor import and
scanout. Three samples of 20 frames follow four warm-ups. Both worst sample mean
and individual-frame p90 must stay below 16.67 ms for full Settings/text/assets,
12 ms for frosted-panel/widget/text-line damage. Test-only GLES hooks must first
observe bitmap transfers, then observe **zero additional transfers on warm
frames**; resource identities must also remain stable. Hooks are absent from
production. GPU CI uses `[self-hosted, linux, mesh-gpu]` when repository variable
`MESH_GPU_RUNNER=enabled`; regular hosted CPU jobs do not simulate GPU budgets.

**Measured.** Workspace release, AMD Radeon 780M (Mesa radeonsi, LLVM 21.1.7,
Linux 6.12.93) and NVIDIA RTX 4060 Laptop. 920×900 logical at scale 2,
1840×1800 pixels. Settings is 60 rounded, shadowed cards plus a frosted panel;
text is 40 lines; assets are 256 visible image/icon instances from 128 distinct
translucent PNGs plus eight text lines. Three interleaved baseline/candidate
runs per GPU, each with three 20-frame samples. Baseline uses the same recording,
resource collector, transfer probes and completion wait, disabling only bitmap
paging and explicit identity/cache-headroom policy. It is a controlled policy
comparison, not a checkout of the pre-change shell. Ranges below span all nine
sample means, milliseconds per completed frame:

| Workload | NVIDIA automatic | NVIDIA resident | AMD automatic | AMD resident |
| --- | --- | --- | --- | --- |
| Settings full | 2.416–4.570 | 3.171–4.323 | 4.378–5.164 | 4.500–5.108 |
| Frosted panel, 840×260 logical | 1.462–2.343 | 1.685–2.557 | 2.854–3.436 | 2.887–3.315 |
| Widget, 138×76 logical | 0.897–1.551 | 0.968–1.687 | 1.426–1.914 | 1.441–1.734 |
| Text full | 2.559–4.304 | 2.525–3.637 | 4.178–5.727 | 4.429–5.738 |
| Text line, 880×20 logical | 0.819–1.315 | 0.916–1.275 | 1.263–1.576 | 1.244–1.880 |
| 256 image/icon instances | 3.830–6.480 | 1.642–2.522 | 6.258–7.092 | 2.755–3.365 |

The texture-dense workload improves; the other ranges overlap and establish no
general speedup. Maximum candidate frame p90 across these runs is 7.633 ms.
All warm samples make zero additional observed GLES bitmap transfers.
The earlier 80-instance / 16-source probe also had overlapping ranges (AMD
1.992–2.370 → 1.955–2.455 ms; NVIDIA 1.026–1.695 → 1.387–1.750 ms), motivating
the denser canonical workload rather than a claim about small icon grids.

**Rejected correctness prototype.** Retaining aliases returned by
`SkImages::TextureFromImage` does not pin the raster playback upload: repeated
conversions returned different backend textures in the explicit identity test.
The implementation was discarded before timing. Residency instead preserves
raster/page identity and uses Ganesh's existing cached replay uploads. This API
explicitly uses an uncached texture policy in
[Skia's implementation](https://raw.githubusercontent.com/google/skia/main/src/gpu/ganesh/image/SkImage_GaneshFactories.cpp).

**Verified.** Required-GPU renderer release suite: 324 passed, 17 ignored,
zero failures. Both hardware gates pass on both devices, including exported-slot
visual parity. All six runner failure-path tests pass; shell `cargo check` with
`gpu`, scoped rustfmt, shell syntax and diff whitespace checks pass. Atlas
coverage includes exact RGBA/mask pixels, immutable retired snapshots, shared
identity, bounded overflow and CPU fallback. The completed GPU backend design
note is removed; current ownership and gate operation live in
`docs/renderer-ownership.md` and `docs/performance-profiling.md`.

**Next.** GPU presentation still waits for render-worker completion. Frame-N /
frame-N+1 pipelining and Wave 5 in-surface blur remain separate backlog work.


## 2026-10-20 — live glass session: backdrop preparation, shared custom properties, animation-scoped retained updates

`working tree` · area: liquid-glass backdrop source, style resolution, retained-tree invalidation

**Method.** Three live `tools/profile-shell session` captures of the `glass`
profile (Hyprland, AMD 780M, eDP 2880×1800 at scale 1, GPU scene worker
enabled, `profiling` build), each driven by the same IPC script: 8 s idle,
six `@mesh/glass-dashboard` toggles, four `@mesh/glass-wallpapers` toggles and
four `@mesh/settings` toggles, 1.5 s apart. Captures are
`profiles/session/20261020-172800` (before), `-182153` (fixes 1 and 2) and
`-191017` (all three). Each is one run, so live numbers are indicative; the
offline profiles below carry the repeated measurements.

**Finding.** Before: opening the dashboard took 150–520 ms per frame. One
448 ms paint traversal was CPU Skia box blur and bilinear sampling inside
`BackdropSource::prepared` (11.5% of all shell samples in the capture). The
glass theme uses nine `backdrop-filter` blur radii (0, 5, 8, 12, 14, 16, 20,
22, 24 px) against a four-entry cache of full-output copies, so painting the
dashboard re-blurred the 2880×1800 wallpaper several times per frame. The
next hotspot was the custom-property map. Each node cloned its parent's
`HashMap<String, StyleValue>` twice, into a scratch map and back, and then
sorted and byte-hashed it twice for paint and style fingerprints:
`paint_variables_fingerprint` 6.2%, `style_fingerprint` 7.4% and map clone
5.9%. The glass bar spent 4.0 ms restyle plus 6.1 ms layout (means) on each
one-second clock tick.

**1. Backdrop preparation.** Blurred copies are prepared at a power-of-two
downsample `f <= sigma / 2` (cap 8) and scaled back up in the refraction
shader's local matrix. The cover-fit is computed in output coordinates under
an exact copy/output canvas scale, so rounding the copy size up cannot shift
the image. The cache is bounded at 96 MiB instead of four entries. The source
builds its mipmaps once at decode. `backdrop_prepare_profile` (release,
synthetic 3840×2160 source, 2880×1800 output, all nine radii cold, three
runs): 1.207–1.261 s before, 0.449–0.504 s after. Warm frames no longer
re-blur at all: the nine radii stay resident
(`prepared_cache_keeps_every_glass_theme_radius`). Fidelity is checked
against a full-resolution blur on hard 8 px bands (the worst case, at the
blur cutoff): max channel error 9/13/16 and mean 2.7/4.3/2.6 at blur
8/20/48 px. Ganesh's own blur downsamples further.

*Rejected:* subtracting the resampling variance (`f^2 / 4`) from the copy's
sigma made blur 8 worse (max 9 → 14, mean 2.7 → 3.6). The residual error
follows the image derivative, which is Skia's box-blur approximation at a
different sigma, not over-blur.

**2. Shared custom properties.** `ComputedStyle::custom_properties` is now
`CustomProperties`: an `Arc`-shared immutable map with its order-independent
fingerprint computed once. Resolution uses a copy-on-write `VariableScope`
that keeps the parent's map until a declaration changes an entry (an equal
re-declaration does not copy). `paint_variables_fingerprint` returns the
stored hash. D9 theme switch is neutral (twelve interleaved runs, desk
theme): p50 7.86–10.01 ms before, 8.89–10.50 ms after.

**3. Animation-scoped retained updates.** Animation-sampled nodes were
recorded both as dirty nodes and as dirty roots. The roots re-fingerprinted
each animated node's whole subtree, and an animated tree root forced a full
`update()`. They are now dirty nodes only. The sampling pass reports every
node whose own sample changed, and the scoped update re-checks every
descendant's layout fingerprint.
`animated_ancestor_scoped_update_leaves_nothing_for_a_full_update` runs a full
update after each scoped frame for opacity, transform, colour, padding and
width transitions and requires zero leftover dirt. With animated nodes
removed from the dirty-node set too, it reports seven missed style changes.
Layout-affecting transitions still take the full update path.
`glass_dock_hover_animation_profile` (release, 2880×180, 10 dock tiles and
150 launcher apps, 1,879 nodes, paints gated on `wants_render`, first run =
the 560 ms `dock-in` entrance, three runs): `render_object_sync` per painted
frame 2.1–2.9 ms before, 0.25–0.35 ms after. The entrance painted 27–35 frames
before and 42–44 after. With 0 launcher apps (199 nodes) it was 0.23–0.56 ms
before, so the closed launcher's 1,680 nodes were the cost.

**Live, per surface, `total_surface_render`** (before → fixes 1+2 → all
three, single runs):

| Surface | Mean | p90 |
| --- | --- | --- |
| Dashboard | 35.7 → 24.0 → 11.0 ms | 59.3 → 76.0 → 28.7 ms |
| Glass bar | 35.7 → 76.2 → 63.8 ms (startup frame dominated) | 41.2 → 71.8 → 35.1 ms |
| Wallpaper picker | 74.2 → 47.7 → 14.2 ms | 198.8 → 148.6 → 74.5 ms |
| Settings | 73.8 → 49.4 → 29.6 ms | 313.1 → 87.7 → 49.3 ms |

Glass-bar stage means: restyle 4.04 → 0.31 → 0.28 ms, layout 6.12 → 2.41 →
1.28 ms. Dashboard paint traversal max 448 → 73 ms after fix 1.

**Gaps found.** The D9 gates run the desk navigation bar and pass by a wide
margin on both sides (hover p90 1.00–1.05 ms), so they do not exercise glass
surfaces. The style-animation pass still walks the whole tree and samples
`AnimatableStyle` up to four times per node; it was 13% of the dashboard phase
after fixes 1 and 2. A closed `<popover>`'s content stays in the parent tree
(1,680 launcher nodes). Runtime style diagnostics re-resolve every style-dirty
node with diagnostics after each retained update (about 6%). First opens still
cost 100–160 ms (dashboard `tree_build` plus first GPU frame). New backlog
lines cover these.

**Verified.** Release: elements lib 296 passed on both sides. Render lib 313 →
316 passed (three new backdrop tests). Shell lib 899 passed / 42 failed on
both sides with identical failure names, diffed against a `HEAD` worktree.
D9 workloads pass on both sides. Profiles: `backdrop_prepare_profile`,
`glass_bar_clock_tick_profile`, `glass_dock_hover_animation_profile`
(ignored, release-only).

## 2026-10-20 — glass D9 workload gates

`tools/check-performance` now runs three glass workloads beside the desk-bar
D9 gates (`real_surfaces/glass.rs`, violet-glass theme with glass-material,
CPU raster at scale 1). Three release runs on the development machine:

| Metric | Workload | p90 | Ceiling |
| --- | --- | --- | --- |
| `d9_glass_dashboard_open_p90_ms` | 12 dashboard mounts + system/media/weather payloads + first 1920×1080 raster | 16.9–27.5 ms (p50 13.2–22.4) | 100 ms |
| `d9_glass_dock_frame_p90_ms` | dock, 10 pinned / 150 installed apps, 2880×180, 180 steps, 55–66 requested frames | 6.5–8.4 ms (p50 2.6–2.9) | 16.667 ms |
| `d9_glass_clock_tick_p90_ms` | 60 glass-bar clock ticks, 2880×100 | 1.25–1.79 ms (p50 1.0–1.6) | 16.667 ms |

The test dashboard open is far below the 100–160 ms live first open, which
adds the GPU first frame and a real backdrop. It gates the tree-build part.

**Workload bug fixed.** `glass_bar_clock_tick_profile` drove ticks at
`start + 1001 ms × n`, which ran ahead of real time. `task.sleep` re-queues
from `Instant::now()`, so `run_due_tasks` kept re-running the clock task
until real time caught up. The profile's per-tick wall times were ~1,001 ms,
and its stage records summed many task re-runs per tick; the live glass-bar
stage means in the entry above came from the running shell and are unaffected.
Both the profile and the gate now tick at the component's next task
deadline. The module compile step moved into a shared `GlassCatalog`.

## 2026-10-21 — skip closed popover subtrees in per-frame passes

A promoted popover wrapper whose `<popover>` is closed, and is neither in
`entering_child_keys` nor `closing_child_keys`, is now *dormant*
(`is_dormant_popover_wrapper`). The style-animation pass and the
previous-visual-style snapshot skip dormant subtrees, and child-surface
request collection stops at any closed `<popover>`. The subtree is still
built, restyled and laid out, so opening needs no rebuild. A popover becomes
non-dormant as soon as the shell marks it closing, so its exit transition
starts from the resting style kept in the last tree
(`closed_popover_subtree_is_skipped_until_it_opens_or_exits`).

Before, perf on `glass_dock_hover_animation_profile` (150 launcher apps,
1,879 nodes) put `apply_style_animations_to_node` at 23% inclusive and
`collect_child_surface_requests_with_diagnostics` at 2.7%. The hover-step
time was outside the profiled stages.

Release, dock 2880×180, 10 pinned / 150 installed apps:

| Measure | Before | After |
| --- | --- | --- |
| `glass_dock_hover_animation_profile` warm-run p90 (runs 1–2) | 2.41–2.57 ms (one run) | 0.81–0.88 ms (three runs) |
| `d9_glass_dock_frame_p90_ms` | 6.5–8.4 ms (three runs) | 3.4–4.7 ms (three runs) |

The D9 dock workload paints 82–107 frames instead of 55–66: frames finish
sooner, so the wall-clock transitions are sampled more often. Dashboard open
and clock tick are unaffected (17.1–25.3 ms and 0.95–2.5 ms p90). Retained-tree
annotation, fingerprinting and validation still walk dormant subtrees. They
were each at most 2.4% after this change.

Verified: release shell lib 900 passed / 42 failed on both sides with
identical failure names (stashed baseline).

## 2026-10-21 — held animations sleep until their next visible change

`wants_render` used to stay true for as long as any transition or keyframe
animation was live. A `steps()` plateau, a transition delay or an infinite
stepped loop (a blinking caret) therefore repainted identical pixels every
vsync. `ActiveTransition::next_change` and `ActiveKeyframeAnimation::next_change`
now report `NextChange::{EveryFrame, At(t), Never}`: a delay holds until it
ends, and a `steps()` easing holds until its next interval boundary. A keyframe
rule holds only if every segment easing is `steps()` with at most 120
intervals; the boundary set covers every stop pair, so the reported time may
be early but never late. When every live animation holds, the component stores
`animation_hold` instead of raising dirt. `wants_render` is false until the
hold ends, `next_tick_deadline` wakes the loop at that instant, and the tick
(or any paint) raises the stored dirt.

Measured with a 60 Hz test loop over 3 s (release) on a 1000 ms `steps(1)
infinite` blink: 7 paints, two of them the initial paint and its follow-up.
Before, `wants_render` was true on every iteration of the same loop (~180
paints), by construction.

Payoff for scoping the style-animation pass to restyled and animating nodes,
measured before deciding: after the dormant-popover change it is 3.1% of
`d9_glass_bar_clock_tick` and 3.8% of the new `glass_dashboard_spectrum_profile`
(151 nodes, 32-bar visualizer update per frame; 3.2–3.5 ms mean, with the
Player component's per-frame rebuild at 1.1 ms). The backlog line is kept as
low payoff.

Verified: animation crate 46 passed; release shell lib 903 passed / 42 failed,
with the same failure names as the stashed baseline.

## 2026-10-21 — P1 typed node state and property vocabulary

Release, `cargo test -p mesh-core-shell --release --lib -- --ignored --exact
…glass_dashboard_spectrum_profile …glass_dock_hover_animation_profile`, three
invocations per tree (each prints three runs), same machine, before = `3e441284`,
after = `8b7053c6`.

| Workload | Before | After |
| --- | --- | --- |
| `glass_dashboard_spectrum_profile` (151 nodes, 1920×1080, 120 frames) mean | 3.17–3.29 ms | 2.94–3.04 ms |
| same, p90 | 3.29–3.62 ms | 3.03–3.40 ms |
| `glass_dock_hover_animation_profile` (2880×180, 60 frames) run 1 p50 / p90 | 0.645–0.649 / 0.877–0.882 ms | 0.501–0.506 / 0.715–0.720 ms |
| same, run 0 (first hover) mean | 3.56–3.60 ms | 3.43–3.52 ms |

About 7% on the dashboard and 18% on steady dock hover, in line with the
design note's ~1.2× estimate for removing string attribute handling. Frame cost
still scales with tree size × passes; the payoff the note expects comes from P2
(one dirty set fed by these change classes), not from this step.

## 2026-10-21 — P2 animation dirty-set prototype; large-factor exit not met

`working tree` (baseline `ef0f45d8`) · area: frame invalidation and animation sampling

`FrameDirty` coalesces typed binding writes, interaction annotation changes and
animation property buckets by NodeId. Geometry-preserving animation-only frames
sample registered transition/keyframe nodes via the immutable frame snapshot's
child paths, capture previous styles only for those nodes, and update retained
snapshots through the same indexed paths. Layout, structural, entrance/exit and
interaction-restyle frames retain their conservative paths. Per-node render
fingerprints still select damage; full-tree discovery has **not** been retired
from the rest of the pipeline. Existing semantic capture and display-list
consumers receive the retained dirty IDs as before.

**Measured.** AMD Ryzen 7 7840HS, release with thin LTO and one codegen unit,
CPU Skia raster, pinned to CPU 2. Three alternating before/after invocations of
each exact ignored test, each printing three runs. Commands: build with
`cargo test -p mesh-core-shell --release --lib`; run the resulting test binary
under `taskset -c 2` with `--ignored --exact --nocapture --test-threads=1` and
`shell::component::tests::integration::real_surfaces::glass::` followed by
`glass_dock_hover_animation_profile` or `glass_dashboard_spectrum_profile`.

| Workload / statistic | Before | Prototype |
| --- | --- | --- |
| Dock: 10 pinned / 150 installed apps, 1,879 nodes, 2880×180, 60 steps; first-hover run 0 mean | 3.674–3.974 ms | 3.695–3.854 ms |
| Same, steady-hover run 1 p50 | 0.502–0.557 ms | 0.380–0.404 ms |
| Same, steady-hover run 1 p90 | 0.727–1.891 ms | 0.753–0.883 ms |
| Dashboard: 151 nodes, 32 spectrum values per update, 1920×1080, 120 frames/run; mean across all nine runs | 3.751–4.283 ms | 3.762–4.479 ms |
| Same, p90 | 5.282–6.749 ms | 5.563–7.034 ms |

Dock run 2 contains mostly steps that do not request a paint, so it is not used
as the steady-frame result. No large-factor timing improvement is established:
dock p90 and dashboard ranges overlap, and dashboard's upper range increases.
These are CPU fixtures, not a substitute for P0's live compositor gate.

**Why the exit failed / expansion withheld.** Spectrum updates still rebuild
Player and therefore never use the animation-only scope. Dashboard tree-build
cost is 1.228–1.416 ms/frame before versus 1.229–1.457 ms after; display-list
update is 0.313–0.392 versus 0.307–0.421 ms. Explicit layout propagation,
selector-dependent restyle scopes, remaining annotation/semantic work and
child-surface requests are still missing from the shared invalidation model.
Even sparse semantic ancestors aggregate clean child text. Indexed sampling
and retained lookup alone cannot remove those costs. Retain the tested
prototype as groundwork; P2 remains open and no broader pass conversion is
claimed. Reassess the dashboard's structural list path alongside P3 before
expecting the full exit result.

**Verified.** `animation_frame_visits_registered_nodes_and_full_diff_finds_no_omissions`
uses one animated leaf beside 1,024 clean siblings: each sampled frame visits
one animation node and selects at most three retained nodes (including semantic
ancestors). A subsequent full fingerprint/diff finds no missed changes. This
is a structural count for these two stages, not an all-pass proportionality
claim. Typed-write coalescing/no-op coverage passes (elements: 1 passed).
All six ignored glass workloads pass. Release serial shell filter `animation`:
baseline 50 passed / 3 failed / 5 ignored; prototype 51 passed / the same 3
failed / 5 ignored. Filter `invalidation`: both 70 passed / the same 3 failed /
1 ignored, including the animated-ancestor full-diff check and binding rollback
coverage. Existing failure names across those filters are
`animation_only_tick_uses_scoped_retained_fingerprinting`,
`external_invalidation_cancels_animation_only_retained_scope`,
`smooth_scroll_animation_uses_scoped_retained_fingerprinting`,
`paint_only_scroll_uses_scoped_retained_fingerprinting`, and
`phase26_real_surface_baseline_emits_canonical_proof_measurements`.

## 2026-10-21 — P0 per-pass node-visit counters; first glass visit profile

`working tree` (baseline `ef0f45d8`) · area: profiling instrumentation

Opt-in thread-local frame counters (`mesh_core_elements::frame_work`) count
visits in the named traversal functions of style, layout (including fit sizing
and descendant extents), animation, semantics, validation, annotation, retained
updates and display lists; child-surface requests report as separate samples.
Counts reach `ProfilingInvalidationSnapshot::node_visits`, session logs and
`profile-report`. Definitions and limits:
[performance profiling](../../docs/performance-profiling.md#node-visit-accounting).
Counter isolation tests (unwinding, worker threads) pass: 3 passed.
`tools/check-performance` runs the animation structural gate on the exported
counts (release: 1 passed). The live replay gate is still open (P0 backlog line).

**First visit profile** (release, glass fixtures, profiling enabled; counts are
deterministic, so one run each). Means per measured paint:

| Pass | Dock first hover (1,879 nodes, 60 paints) | Dock steady hover (50 paints) | Dashboard spectrum (151 nodes, 120 paints) |
| --- | --- | --- | --- |
| `annotation` | 1,879 | 1,879 | 151 |
| `validation` | 374 | 0 | 1,657 |
| `restyle_scan` / `style_resolve` | 313 / 64 | 376 / 0 | 59 / 137 |
| `retained_discovery` | 282 | 376 | 151 |
| `display_ancestors` | 1,437 | 0.1 | 57 |
| `layout_extents` / `layout_fit` | 131 / 31 | 0 / 0 | 763 / 151 |
| `accessibility_normalize` | 68 | 0 | 567 |
| `animation_sample` | 13.5 | 14.2 | 151 |

Findings: steady dock hover still annotates the whole tree every paint and
scans ~20% of it for restyle and retained discovery while sampling ~14 nodes.
Dashboard spectrum frames touch every node in every pass because Player
rebuilds; validation visits ~11× and descendant extents ~5× the node count per
paint, and semantic normalization ~3.8×. These are the next P2 scope targets
(annotation, validation, extents) and confirm the P3 keyed-list need.

Release serial filter `profiling`: 44 passed / 2 failed. Both failures exist at
`ef0f45d8`: `phase26_real_surface_baseline_emits_canonical_proof_measurements`
(known) and `profiling_snapshot_groups_backend_stage_proof_under_expected_provider_identity`
(missing `PollUpdate` stage; fails deterministically on baseline and working tree).

## 2026-10-21 — P2 step 1: interaction frames scoped by snapshot paths

`working tree` (baseline `df98fcc9`) · area: annotation, interaction restyle, retained update

**Measured shape first.** The P0 dock means hid the frame classes. Per painted
frame, a hover change (one step in six) visited annotation 1,879, restyle scan
1,878 and retained discovery 1,879. Animation-only frames visited annotation
1,879 and otherwise only sampled nodes. The "376 per paint" means were
1,879 / 5.

**Change.** No selector-dependents index was built. The selector subset rejects
combinators and `:has()`, so only inherited values propagate a state change
(design: P2 step 1 in the
[pipeline note](../todos/pending/2026-10-21-change-proportional-pipeline.md)).
Retained frames whose overflow extents are current diff the stored
`RuntimeAnnotationInputs`. They project only changed nodes and scroll
containers through the last frame snapshot's paths; this replaces the
scroll-only walk. Restyle reaches its targets by path and descends only while
inherited values change. Semantic normalization and the retained update
consume the exact restyled set; a frame that ran layout keeps the conservative
discovery.

**Latent bug fixed.** The targeted restyle resolved a synthetic surface root's
children with no parent context, while the full restyle passes the root's
style. After any hover, descendants lost the root's inherited color and fonts
(the hover gate saw color 255 → 192) until the next full restyle. Both
targeted paths now pass the root context. The gate checks rebuild parity.

**Visits** (release glass dock, 1,879 nodes, steady hover, per painted frame):

| Pass | Hover change before → after | Animation-only before → after |
| --- | --- | --- |
| `annotation` | 1,879 → 14 | 1,879 → 8 |
| `restyle_scan` | 1,878 → 12 | 0 → 0 |
| `retained_discovery` | 1,879 → 0 | 0 → 0 |
| `retained_lookup` / `retained_sync` | 0 / 19 → 9 / 5 | 36 / 9 → 3 / 3 |

**Timing.** AMD Ryzen 7 7840HS, release (thin LTO, one codegen unit), CPU Skia
raster, `taskset -c 2`; three alternating before/after invocations of
`glass_dock_hover_animation_profile` (10 pinned / 150 installed apps,
2880×180, 60 steps per run). Whole-run percentiles are not comparable: the
shelf's 560 ms wall-clock `dock-in` entrance spans more steps when frames are
faster. After-build run 1 has 1–9 entrance frames at 2.1–2.6 ms
(`display_ancestors` 1,834, unscoped), and the baseline's slower frames finish
the entrance within run 0. The whole-run run-1 p90 therefore rose from
0.725–0.769 to 2.105–2.269 ms. Painted frames in runs 1–2, classified by
visit signature with the same test-side instrumentation in both binaries:

| Frame class | Before p50 / p90 | After p50 / p90 |
| --- | --- | --- |
| Hover change | 0.724–0.740 / 0.762–0.817 ms | 0.256–0.297 / 0.285–0.524 ms |
| Animation only | 0.385–0.388 / 0.404–0.427 ms | 0.159–0.164 / 0.176–0.205 ms |

About 2.5× on both steady classes. The dashboard does not take this path
(Player rebuilds every update). Its mean across the same alternation was
2.973–3.090 ms before and 3.005–3.036 ms after, which overlap.

**Verified.** New gate `hover_frame_visits_scale_with_the_hovered_item_not_the_tree`
(added to `tools/check-performance`) uses 16 and 1,024 clean siblings. Equal
counts at both sizes, zero discovery, full-diff, twin pixel/style parity and
rebuild parity all pass. The animation gate now also asserts zero annotation
and restyle-scan visits. Elements: 300 passed / 7 ignored. Debug serial
`mesh-core-shell --lib`: baseline 905 passed / 40 failed / 30 ignored; working
tree 906 / 40 / 30, with identical failure names. The scroll-annotation tests
now use a forced-full twin.

**Remaining for the P2 exit.** Entrance and lifecycle frames still walk display
ancestors. Hover-change frames still capture previous styles for all 63
registered transition nodes. Every frame clones the annotation inputs (small
maps; not measured separately). Layout propagation, validation, child-surface
requests and the dashboard's P3 list path are unchanged.

## 2026-10-21 — Dashboard pass audit and P3 keyed native lists

`working tree` (baseline `1d50170a`) · area: compiler tree build, fit sizing, template patching

**Pass audit.** The dashboard's repeated visits came from nested building.
`build_widget_node`, its selective variant and `build_component_ref`
validated and normalized accessibility for each subtree they returned, so
the work was O(nodes × depth). The completed tree is now normalized once.
`resolve_fit_sizing` walked the descendants again for every fit-sized
ancestor. It now returns extents bottom-up in one visit per node; promoted
popovers and `position: fixed` subtrees are still excluded. Gates:
`compiler_normalizes_completed_tree_once` and
`nested_fit_extents_visit_once_and_respect_surface_boundaries`.

**P3 keyed native lists.** A `{#for … key={…}}` whose body contains only
native elements, text and expressions, with no handlers, two-way or instance
bindings and no nested control flow or components, records a
`KeyedTemplateList` with its source, item name, key expression and ordered
keys. When the source changes, the patcher re-evaluates it once per list. If
the keys are unique strings or numbers in the same order and the length is
unchanged, it refreshes the captured item local only on changed items' nodes,
and those nodes go through the normal property patch and frame dirty set.
Any key, order or length change, a duplicate key or an expression failure
falls back to the rebuild path.
Gates: `keyed_list_item_patches_preserve_pixels_and_structural_fallbacks`
(pixel parity against a forced rebuild across mutate/replace/reorder/append/
remove/duplicate/empty), `keyed_list_one_item_write_does_not_patch_clean_siblings`
(one patched node, no tree build, at 8 and 256 items),
`keyed_list_structural_body_and_duplicate_keys_keep_rebuild`.

**Visits** (release `glass_dashboard_spectrum_profile`, 151 nodes, per frame,
run 2 of the third invocation):

| Pass | Before → after |
| --- | --- |
| `validation` | 1,657 → 151 |
| `layout_extents` | 763 → 151 |
| `accessibility_normalize` | 567 → 0 |
| `accessibility_project` | 152 → 100 |
| `layout_build` / `layout_sync` / `layout_discovery` | 151 / 0 / 0 → 0 / 25 / 151 |
| `style_resolve` | 137 → 47 |
| `restyle_scan` | 59 → 336.5 |
| `retained_sync` | 63 → 35 |

The `tree_build` stage (1.009 ms/frame, 0.600 ms of it the Player
component) no longer appears. Layout rose from 0.165 to 0.274 ms/frame on the
sync-and-discover path, and the patched-node restyle scans about 2.2× the
tree. Both are P2 targets.

**Timing.** AMD Ryzen 7 7840HS, release (thin LTO, one codegen unit), CPU Skia
raster 1920×1080, `taskset -c 2`, 32 spectrum bars, 120 measured frames per
run. Three alternating invocations each of the saved baseline test binary and
the candidate (3 runs each; the baseline's first warm-up run of invocation 1,
8.3/3.8 ms mean, excluded):

| | mean | p50 | p90 |
| --- | --- | --- | --- |
| Before | 2.955–3.084 ms | 2.921–3.003 ms | 3.054–3.428 ms |
| After | 2.119–2.292 ms | 2.105–2.172 ms | 2.224–2.593 ms |

About 28% per frame. Both changes are in the candidate binary, so the audit's
and the keyed list's shares are not separated.

**Verified.** Release `mesh-core-shell --lib` (all tests, 8 threads): the
baseline binary and the candidate both fail the same 42 tests. Elements:
301 passed / 7 ignored. Frontend: 89 passed / 2 failed. Both failures are the
known `phase87_layout_display_source_semantics_survive_lowering` and
`shipped_settings_surface_compiles_with_local_pages`. While writing the
fallback test, two sibling keyed loops with overlapping keys under one parent
panicked on a NodeId collision. The collision predates this work, and the
loop identity has no loop discriminator. Filed in the backlog; the test gives
each loop its own parent.

## 2026-10-21 — P2 W1: patched-frame restyle was already scoped; paint-only patches keep geometry

`06d5750f` (baseline `55800ed5`) · area: patched-binding restyle and layout decision

**The workstream's premise was wrong.** At the baseline, dashboard spectrum
frames already took the path-directed `restyle_targets_by_path`. A trace of
`finalize_tree` showed scoped restyle with 23–25 patched nodes and no SCRIPT or
TEXT dirty. The 336 `restyle_scan` visits per frame are about 24 targets × a
path depth of about 14: each target re-walks its ancestors. They are not a tree
scan. The full-tree cost on these frames is elsewhere. Every patch frame
carries LAYOUT, and the dashboard's patched property (`style:height`) really
is layout. So annotation, layout sync and discovery, retained discovery,
validation and animation sampling each still visit all 151 nodes. That is W2's
work.

**Change.** `ComputedStyle::same_layout_inputs` compares two styles with
paint-only, animation-setup and inheritance-bookkeeping fields ignored. Any
field not listed counts as layout. The scoped restyle raises `layout_changed`
when a re-resolved style differs in a layout input. A patched frame drops
LAYOUT and reuses retained geometry when no patched prop was LAYOUT-class and
no layout input changed. Typed props such as `text`, content, `name` and `src`
are LAYOUT-class, so they keep the full path. Free-form `style:*` bindings are
caught by the style comparison.

**Visits.** Paint-only patch (gate fixture, 16 siblings): retained discovery
22 → 0, `restyle_scan` 4 and `style_resolve` 2, equal at 16 and 1,024
siblings. Dashboard and dock per-pass visits are unchanged (`restyle_scan`
336.4, `retained_discovery` 151, `layout_discovery` 151 on the dashboard).

**Timing.** Release (thin LTO, one codegen unit), CPU raster, `taskset -c 2`,
three alternating base/candidate invocations of three 120-frame runs each.
Agent-measured; another agent was building on other cores at the time.

| Fixture | Base mean / p50 / p90 | Candidate mean / p50 / p90 |
| --- | --- | --- |
| `glass_dashboard_spectrum_profile` | 2.31–2.60 / 2.31–2.57 / 2.47–2.83 ms | 2.36–2.57 / 2.35–2.56 / 2.51–2.80 ms |
| `glass_dock_hover_animation_profile` run 0 | 3.72–3.85 / 2.89–3.09 / 3.21–3.29 ms | 3.67–3.78 / 2.92–2.96 / 3.21–3.40 ms |

No change within noise. None was expected for these fixtures; the win is for
bound paint-only properties (colour, background, opacity).

**Verified.** New gate `patched_frame_restyle_visits_scale_with_the_patch_not_the_tree`
(in `tools/check-performance`). It patches a keyed item's `style:background`
and an inherited `style:color` on a container, checking that descendant text
changes. Visits are equal at 16 and 1,024 siblings, retained discovery is 0, a
full retained diff finds nothing, and a forced-full twin has identical pixels
and computed styles. Release `mesh-core-shell --lib`: 42 failures before and
after, with identical names. Elements: `style_profile_matrix_matches_supported_css_properties`
fails both with and without the change. Not attempted: walking sorted targets
with a shared prefix, which would cut `restyle_scan` 336 → about the union of
the paths. It is negligible in time.

## 2026-10-21 — P4 W6: pipelined GPU presentation and render-path diagnostic

On main after `f121a52e` (agent baseline `55800ed5`) · area: presentation, scene worker, shell present seam

**Change.** `present_with_damage` no longer waits for the scene worker. A scene
canvas is submitted and returns `PresentStatus::Submitted`. The worker signals
an eventfd after publishing, `wait_for_events` polls it (`WaitReason::SceneComplete`),
and the backend imports, attaches and commits on that wake. Frames commit in
submission order, and a popup never commits before a still-pending parent. A
frame commits only if the worker still holds it as current and the compositor
object is the one it was painted for. A frame older than 2 s disables GPU
presentation. While a frame is in flight the surface is gated by the existing
buffer-release wait, so other surfaces and the next frame's script and layout
run. Hiding a surface cancels its frame. The shell treats `Submitted` as
presented. A completion that did not present (superseded, resized away,
buffers busy, GPU error) forces a full repaint of its target. The fence,
release-lease and SHM fallback rules are unchanged. `GpuPresenter` logs one
`render path: GPU (…)` / `CPU/SHM (reason)` line and exposes
`PresentationEngine::render_path()`. GPU was already the CLI default.

**Timing.** Agent-measured. Live Hyprland at 120 Hz, AMD 780M as the
compositor main device, release (thin LTO, one codegen unit), `taskset -c 10`.
The new ignored `scene_pipelining_profile` alternates serialized (old wait) and
overlapped modes in one binary. Each frame paints 160 blurred rounded rects at
full surface plus synthetic next-frame CPU work. Three invocations (two with
4 ms of work, one with 1 ms), 6 rounds × 120 frames each.

| Metric | Serialized | Overlapped |
| --- | --- | --- |
| Frame N+1 work start after frame N submit, p50 (4 ms work) | 1.02–1.64 ms | 0.00 ms |
| Same, p90 (4 ms work) | 1.45–2.12 ms | 0.00 ms |
| Same, p50 (1 ms work) | 1.67–2.32 ms | 0.00 ms |
| Frame interval p50 | 8.33–8.43 ms | 8.32–8.37 ms |
| Frame interval p90 | 8.57–8.95 ms | 8.56–8.87 ms |

The UI thread gets back the whole GPU wait (1–2 ms per GPU frame). The frame
interval does not change because this load is bound by 120 Hz frame callbacks.
An unpinned run on a busier system (not vsync-bound, about 13 ms intervals)
showed 10.8–13.1 vs 12.7–13.8 ms p50; it was not repeated under controlled
conditions. No shell-level end-to-end measurement exists: shell tests build
without the `gpu` feature. `gpu_frame_cost_workload` is unchanged (render
crate untouched). For reference, the 780M full surface is 1.97–2.04 ms against
35.0–36.4 ms for CPU raster plus SHM copy.

**Verified.** New live tests `scene_frame_in_flight_survives_hide_resize_and_destroy`,
plus `scene_worker_exports_and_presents_frames_on_a_live_compositor` updated for
`Submitted` and retrying on busy buffers. Both pass on the live session; re-run on main: presentation release with `gpu` 112 passed, live matrix 5 passed / 1 ignored.
Agent-run release suites through `nix develop -c`, against a clean
`git archive 55800ed5` baseline: render 332 passed / 18 ignored and presentation
lib 112 passed on both sides, with identical names. Shell lib baseline has 43
failures and the candidate 42; the difference is the flaky
`resource_preparation_job_cancels_and_polls_without_blocking_the_caller`, and the
candidate has no new name. Scene preparation narrowed by retained revision
(W6 part a) is deferred until after W3.

## 2026-10-21 — P0 W4: live glass replay gate; the live dock never idles

On main after `d50b6c41` (agent baseline `55800ed5`) · area: profiling, live replay

**Change.** `live_glass_replay` (ignored, release only) runs the real `Shell`
event loop against the compositor in `WAYLAND_DISPLAY` under the `glass`
profile. It copies `config/` into a temp sandbox with private settings, module
graph, IPC socket, `MESH_HOME` and `XDG_STATE_HOME`, injects fixed service
payloads, and starts no host backends. It drives synthetic input through
`pending_wayland_events` and replays:
1. bar clock ticks for 8 s;
2. a dock hover sweep of 120 moves across the 10 pinned tiles;
3. dashboard open plus 120 spectrum updates at 33 ms;
4. Settings open and an Appearance two-finger scroll.

A test-only capture buffer records every `TotalSurfaceRender`/`PresentCommit`
sample and each frame's `node_visits`. The production seams (the
`replay_hooks` driver call, sleep clamp and backend suppression) exist only under
`#[cfg(test)]`. `tools/check-performance --live` runs it `MESH_LIVE_RUNS`
times (default 3), pinned to `MESH_LIVE_CPU`. With no compositor the test
skips and passes. Its structural assertion requires every live dock hover frame
to stay within the per-pass maxima of its frame class in the CPU fixture
`glass_dock_hover_animation_profile`. That assertion **fails today**, which is
the finding below. Layer namespaces are not unique, so stop a desktop shell
before running it.

**Live results.** Agent-measured, three runs, CPU raster (the GPU build also
reported CPU; not investigated, and the agent's tree predates W6's render-path
diagnostic), `taskset -c 6`, live Hyprland 120 Hz.

| Workload / surface | Frames | Mean | p50 | p90 |
| --- | --- | --- | --- | --- |
| Clock tick, `glass-bar` | 8 | 5.6–7.7 ms | 6.0–6.1 ms | 7.8–8.0 ms |
| Clock window, dock (no input) | 430–439 | 16.7–17.0 ms | 16.5–16.8 ms | 17.8–18.2 ms |
| Dock hover sweep, dock | 120 | 39.7–40.2 ms | 38.9–39.4 ms | 45.6–46.9 ms |
| Dashboard spectrum, dashboard | 119 | 100.9–103.1 ms | 99.4–101.6 ms | 103.3–107.2 ms |
| Settings Appearance scroll | 53–64 | 22.9–30.5 ms | 19.9–31.6 ms | 33.0–34.4 ms |

First present after an open request: dashboard 63.6–72.1 ms, Settings 85–92 ms.
The dashboard figure is suspect. During the spectrum window every surface
reports about 100 ms (dock 102, widgets 101, bar 99), so `total_surface_render`
probably includes cross-surface serial work. Check that attribution before
optimizing from it.

Orchestrator spot-check on main (W1 and W6 included), one run, `taskset -c 2`,
CPU raster. The clock window's dock painted 296 frames in 8 s with no input:
21.2 ms mean, 37.0 ms p90, `retained_discovery` 1,623, annotation 229 and
`restyle_scan` 117 visits per frame. Dashboard open first present 75.8 ms,
Settings 101.8 ms.

**Finding.**
- **Idle dock.** The live glass dock repaints continuously at about 37–54 fps
  with no input, rediscovering most of its 1,877-node tree each frame. The CPU
  fixture's `wants_render` gate never shows this.
- **Live hover.** 80–100 of 119 live hover frames walk the whole tree, outside
  the fixture's HoverChange and AnimationOnly classes: annotation 1,877, restyle
  about 1,876, validation 3,754–11,262, `accessibility_normalize` up to 3,754,
  `display_ancestors` about 1,840. The scoped interaction path of P2 step 1 is not
  reached on a live surface.

**Verified.** Agent release `mesh-core-shell --lib`: 43 failures at baseline and
42 after, with no new name. The difference is the flaky
`signal_requests_ordinary_shutdown_and_wakes_the_loop`, which passed. Tried and
reverted: a symlinked modules dir (fails profile resolution), an IPC socket at
the sandbox root (refused as unsafe), a single apps injection before the dock's
first paint (the list stayed empty; a later `available:false` then the full
payload populated it, cause unknown).

## 2026-10-21 — P0 live: the idle dock repaint loop and live hover rebuilds

On main after `634af104` · area: resources, template patching, live replay

**Idle dock.** `resolve_icon_for_render` (production path, not `cfg(test)`)
cached no negative result. A missing icon queued a resolution job on every
paint. The job also found nothing, notified the component's
`ResourcePaintConsumer`, and the shell's `request_paint` cleared the child
display lists and invalidated the pixels: a full repaint that queued the job
again. The glass dock's fallback `application-x-executable` (size 64) is
missing, so one 8 s idle window queued 5,939 jobs. The CPU fixture never shows
this because its paints run outside a consumer scope. Fix: a job that settles
`Missing` is remembered per resource revision and wakes no consumer. Test:
`production_icon::missing_icon_resolution_does_not_request_repaints` fails
before the fix.

**Live hover.** Every tile crossing fully rebuilt the 1,877-node template, for
three reasons:
1. `onpointerenter` runs inside `handle_component_input`, which holds
   `last_tree` outside the component. The handler's effect rewrote `apps`, and
   `try_patch_template_writes` refused with no tree, so the frame took
   `SCRIPT_NARROW`. Writes made while a dispatch holds the tree (input, child
   surface input, due disclosures, long presses, element actions) now patch
   when the tree returns. Nested dispatches defer to the outer one.
2. `patchable_keyed_loop` rejected any loop body with an `on*` attribute, so
   `apps` stayed unpatchable. Handler expressions that capture loop locals are
   now recorded in `TemplateBindings` and re-bound to the patched item's
   locals. A stale binding would act on the old item.
3. The CPU fixture never measured this. Its tile bounds came from the first
   paint, mid `dock-in` entrance, so every move after the first hit the slot
   below the button and `onHover` ran once per process. The fixture now takes
   the bounds after the entrance settles, starts each run from outside the dock
   and paints at 16 ms pacing, in both `glass_dock_hover_animation_profile` and
   the replay's envelope copy.

Test: `keyed_list_handler_write_during_input_patches_and_rebinds_item_handlers`.
Without the deferral it fails on the patch assertion; without the re-bind it
clicks with the stale item (`"b"`, not `"b*"`).

**Live results.** `MESH_LIVE_CPU=2 nix develop -c ./tools/check-performance
--live`, release, CPU raster, live Hyprland 120 Hz, 3 runs. Before figures are
the W4 entry's agent runs.

| Measure | Before | After |
| --- | --- | --- |
| Dock frames in the 8 s idle window | 430–439 (spot check 296; 406 this session) | 0, 0, 0 |
| Dock hover mean | 39.7–40.2 ms | 24.8–25.9 ms |
| Dock hover p50 | 38.9–39.4 ms | 21.9–22.8 ms |
| Dock hover p90 | 45.6–46.9 ms | 33.8–37.4 ms |
| Hover frames on the full path | 80–100 of 119 | 0 (20 hover change, 99 animation only) |
| `live_dock_visit_outliers` | fails | 0, 0, 0 (gate passes) |

The dashboard spectrum mean also read 85.8–92.9 ms (was 100.9–103.1). That is
consistent with the dock no longer burning CPU beside it, but cross-surface
attribution is still unchecked (backlog), so this is not claimed as a win. Of
nine runs with all the fixes (six during the investigation, three in the gate),
one showed 19 dock frames in the idle window (restyle-scan frames, about 30 ms
each) and eight showed none.
The likely cause is boot settling spilling into the window; this is
unconfirmed.

**CPU fixture.** `glass_dock_hover_animation_profile`, release, settled tiles,
unpaced, steady runs 1–2. The fixture's shape changed in this entry, so these
figures are not comparable with earlier entries. Before the handler fixes: mean
7.1–7.4 ms, p90 26.0–28.1 ms; tile-change frames annotated, laid out and
validated the whole tree (1,877 / 1,877 / 11,262). After: mean 4.7–5.1 ms, p90
15.3–15.5 ms, annotation 13 per frame, no layout build or validation. Patched
hover frames still resolve style for about 1,880 nodes (backlog).

**Tried and reverted.** Re-evaluating `EventHandlerCall` arguments: the parser
never produces that variant (handlers are `EventHandlerExpression`). Pacing the
fixture alone: live frames still exceeded the fixture's `display_build` (61 vs
37). Those frames are the dock row's own `:hover` transition at sweep entry,
which the fixture reaches only by entering from outside.

**Verified.** Release suites against a `git worktree` of `634af104`: shell lib
917/42 vs 916/42 baseline, with the same 42 failure names (the extra pass is
the new test). Frontend 89/2 and elements 301/1 have identical failure names
on both sides. Render 334 passed, 0 failed.


## 2026-10-21 — surface render totals exclude work on other surfaces

After `8fe3781a` · area: profiling attribution

**Finding.** Parallel CPU raster split parent rendering into preparation,
shared raster replay and presentation, but `total_surface_render` kept an
`Instant` from the start of preparation until presentation finished. Every
surface therefore included later surfaces' preparation, the entire raster
batch wait and earlier surfaces' presentations. Child popups inherited the
parent's start time, including work for unrelated parents and sibling popups.
This confirms the attribution defect suspected in the W4 dashboard results;
those live totals cannot be treated as per-surface work costs.

**Change.** Parent totals sum their own preparation duration, their individual
CPU replay duration (already measured by the raster worker), and their own
presentation tail. Child timers start at child subtree painting. Shared raster
queueing/wait time is excluded, as are asynchronous GPU execution and compositor
display latency. The profiling guide defines these boundaries. Allocation
counters retain their existing scope; this change only corrects timing.

**Validation.** The two-surface regression uses CPU canvases at 64×24 and
injects 100 ms into each paint of the second surface. It requires that surface
to retain its own delay while the first surface reports less than half its
cost, and checks that total timing includes CPU paint. Corrected live glass
numbers still require a replay; no runtime speedup is claimed.

Debug serial validation: render tests 9 passed; child-surface tests 25 passed;
profiling tests 14 passed / 1 failed. The failure is the previously recorded
`profiling_snapshot_groups_backend_stage_proof_under_expected_provider_identity`
(missing `PollUpdate`; see the 2026-10-21 dashboard pass audit entry). No live
replay was run for this change.


## 2026-10-21 — live dock gap is CPU raster; explicit GPU replay works

After `e04e6cb9` · area: live measurement, render-path diagnostics

**Reporting.** The live replay now prints non-attribution stage means per
rendered frame, separate deferred CPU replay and recording/immediate paint
means, and `PresentationEngine::render_path()` including fallback reasons.
`tools/check-performance --live` accepts `MESH_LIVE_FEATURES`; its default
remains CPU-only, while `MESH_LIVE_FEATURES=gpu` matches the CLI's default
feature. No renderer optimization or routing change was made.

**Measured.** Release (`opt-level=3`, thin LTO), `--features gpu`, same test
binary, Hyprland 120 Hz on 2880×1800 at scale 1, process pinned to CPU 2.
Three fresh processes per mode, interleaved CPU/GPU after compilation ended;
CPU uses `MESH_RENDER_GPU=0`, GPU uses `MESH_RENDER_GPU=1`. Each process runs
the full replay: 8 s clock window, 20 tile crossings with 6 synthetic moves per
tile, dashboard open/120 spectrum updates, and Settings Appearance scrolling.
The dock has ten pinned apps plus 150 installed entries. Compositor layer
snapshots in all six runs show a **2880×114 live dock**, whereas the CPU visit
fixture paints **2880×180**. The dock produces 119–120 CPU frames and 203–214
GPU frames over the hover window; these are per-rendered-frame work comparisons,
not identical frame sequences or end-to-end latency comparisons.

Ranges of per-run dock means, milliseconds:

| Work | Forced CPU | GPU enabled |
| --- | --- | --- |
| Preparation excluding paint and presentation (total − paint − present) | 5.152–5.673 | 5.000–5.444 |
| Recording/immediate paint | 0.428–0.472 | 0.525–0.636 |
| Deferred CPU raster replay | 19.330–20.643 | none; raster runs asynchronously on GPU |
| Presentation call/submission | 0.108–0.135 | 0.487–0.650 |
| Total attributed synchronous work | 25.045–26.924 | 6.012–6.668 |
| Per-run p90 of total work | 34.009–36.664 | 7.437–10.297 |

The raster replay accounts for about 77% of the CPU total. Presentation does not
explain the gap. GPU diagnostic: `GPU (device 0xe281, format 0x34324241,
modifier 0xffffffffffffff)`; sysfs identifies the compositor device as AMD
`1002:15bf` at `/dev/dri/renderD129`. Forced-CPU diagnostic explicitly says
`CPU/SHM (MESH_RENDER_GPU=0)`. Current GPU replay initializes successfully;
the default replay command previously omitted the GPU feature. This does not
reconstruct every historical invocation described as a GPU build.

**Fixture limits.** The visit fixture executes before `Shell::new()` initializes
the wallpaper backdrop and uses a default paint context with zero output size.
`draw_refraction` skips missing backdrop sources or zero output dimensions.
The fixture also rasterizes directly while the measured live CPU frames replay
recorded paint. Similar node counts therefore do not establish equal raster
workloads. The relative contributions of backdrop content versus replay mode
were not isolated here; a timing fixture with live raster parity is in the
backlog. Do not compare the older ~5 ms fixture number as though it paints the
same pixels as the live shell.

**Corrected dashboard attribution.** Its spectrum window now charges the
dashboard 86.084–87.445 ms in CPU mode versus 4.592–5.305 ms synchronous work
in GPU mode. In CPU run 1 the bar reads 4.499 ms and widgets 11.235 ms, rather
than all inheriting ~100 ms. GPU worker execution, queued waits and compositor
display latency are excluded. Existing GPU `first_present` also marks accepted
submission, not actual compositor commit; that measurement issue remains open.

**Gates.** Final CPU replays pass 3/3 with zero visit outliers. Final GPU replays
fail 3/3 with 15, 7 and 16 outlier frames. The sampled differences include seven
animated nodes against the CPU envelope's six (`animation_lookup` 50 versus
42), accessibility projection 30 versus 26, and retained lookup/sync 45/15
versus 38/13; hover-change projection also differs. No full-tree rebuilds are
reported. Faster GPU pacing and overlapping animations are a likely reason,
but that is not yet a proven complete explanation. The envelope was not
relaxed or bypassed in the checked-in tool. GPU timings were emitted before
the assertion failed and are retained as measurements, not passing gate proof.

Raw final logs, compositor geometry snapshots and `summary.json` are under
ignored `profiles/live-attribution/` (`final-cpu-{1,2,3}.log` and
`final-gpu-{1,2,3}.log`). Earlier exploratory captures are separate. Reproduce a
single fresh process with `MESH_LIVE_FEATURES=gpu MESH_LIVE_CPU=2
MESH_LIVE_RUNS=1 nix develop -c ./tools/check-performance --live`, setting
`MESH_RENDER_GPU=0` or `1`; repeat three times per mode. The GPU assertion
currently returns a failure, as recorded above. `bash -n tools/check-performance`
and `git diff --check` pass. All replay processes exited and removed their
private state; no desktop shell was stopped.

**Decision.** There is no demonstrated GPU routing defect in the current
production path. CPU raster explains most of the live/fixture gap; further
pipeline work addresses the remaining ~5 ms of preparation rather than that
~20 ms raster cost. Restore measurement parity for the GPU structural gate and
CPU raster fixture; W2/W5 remain the next planned pipeline work.

## 2026-10-22 — P2 W2: layout-class patch frames lay out the patched branch

On main after `f49ef59b` · area: layout, template patching

**Change.** `LayoutEngine::compute_incremental_for_stable_tree` lays out a
structurally unchanged tree from an explicit dirty set. Every full pass seeds a
per-node projection (`layout/sparse.rs`): the Taffy rect, fit extents and
visibility. A stable-tree pass syncs only the dirty nodes into Taffy, rewrites
their ancestor branches, and skips any subtree whose projected rect and
visibility are unchanged. Siblings the patch moved are still checked. It
reports the nodes whose final geometry changed in
`PerSurfaceLayoutState::changed_nodes`. In the shell, `finalize_tree` takes this
path on patch frames whose restyle was path-scoped, with the dirty set made of
the restyled nodes, the patched nodes and the surface root. Whole-tree
`layout_dirty_node_snapshots` discovery and `validate_widget_tree` are skipped
there. Other frames keep the old path. Gate:
`patched_frame_layout_visits_scale_with_the_patch` (visits equal at 16 and
1,024 clean siblings; rects and pixels equal a forced-full twin). The elements
test `stable_tree_layout_matches_full_layout_and_reports_moved_siblings`
compares against a full layout in six modes.

**Bugs found by existing tests during installation.**
- The subtree skip ignored `hidden_ancestor`. A closed sheet inside a dialog
  that had just opened kept a stale "hidden ancestor" entry, so opening the
  sheet hit the stays-hidden early return and laid it out 0×0
  (`nested_modal_escape_closes_only_sheet_and_returns_to_its_opener`).
- The shell writes the surface root's width and height just before layout,
  outside the restyle, so Taffy kept the stale root style. A navigation-bar
  audio patch then laid the bar out against 80 px instead of its 40 px content
  (`phase98_pixel_equivalence_backend_update`). The root now always joins the
  dirty set.

**Measurements.** `glass_dashboard_spectrum_profile`, release, 120 spectrum
updates, three runs each.

| Per frame | `f49ef59b` | After |
| --- | --- | --- |
| `layout_discovery` | 151 | 0 |
| `validation` | 151 | 0 |
| `layout_write` | 154 | 43 |
| `layout_extents` / `layout_fit` | 151 / 151 | 32 / 32 |
| `layout_sync` | 25 | 193 |
| Mean | 2.21–2.41 ms | 2.02–2.22 ms |
| p50 | 2.16–2.27 ms | 1.98–2.18 ms |

The timing gain is small and the ranges nearly overlap; the structural change
is the result. `layout_sync` grows because each dirty node's path is resolved
from the root (backlog). Still unchanged per frame: annotation 151, retained
discovery 151, animation previous-style and sample 151, restyle scan 336. The
dashboard tree is 151 nodes. `tools/check-performance --workloads-only` passes
every gate.

**Verified.** Release suites against a worktree of `f49ef59b`: shell lib
919/42 vs 918/42 with identical failure names (the new gate is the extra
pass). Elements 302/1 and frontend 89/2 have identical failure names on both
sides. Mutation check: with the stable-tree call disabled, the gate fails on
`layout_discovery` 25.

## 2026-10-22 — P3 W5: typed binding targets and retained binding addresses

`working tree` (baseline `4d6b37bb` plus the pre-existing working-tree changes,
compiled and saved before editing) · area: compiler bindings, template patches,
key dependency metadata

Element bindings now retain their resolved `Prop` or explicit free-form
`AttrKey` at build time. Patches compare resolved values without allocating a
scratch attribute map or resolving names again, then use the typed setter's
`ChangeClass`. Scalar and structured values keep their original runtime types;
equal legacy text representations retain the existing no-op semantics.
`class:*` and `style:*` continue to use the parser's ordinary combined class
and style bindings; computed style differences determine layout effects.

A rebuild-time index retains expression-to-target paths and per-list item
paths/snapshots. Write-time patches select these addresses directly instead of
walking the widget tree. Item snapshots publish only after all expressions
succeed, alongside the patched tree. Plain name/dotted key dependencies come
from the existing full_moon expression AST: unchanged item-field inputs reuse
the previous validated key. Changed inputs and complex/dynamic keys still
execute through Luau; key/order/length changes and duplicates retain the rebuild
fallback. This is dependency analysis, not another expression evaluator.
Collection snapshots still require item comparisons; write-time item deltas
remain open under P3/D4.

**Structural gates.** `binding_patch_visits_no_target_discovery` counts the
handler/patch dispatch itself, before paint: one patched node, zero target
discovery visits and zero runtime key validations at both 16 and 1,024 keyed
siblings, across repeated item and inherited-color writes. Every frame matches
a forced-rebuild twin's pixels/computed styles, and the full retained diff finds
no omitted changes. `typed_binding_setters_preserve_values_and_change_classes`
covers numeric, boolean, semantic and free-form structured bindings.
`keyed_list_dynamic_keys_and_nested_key_changes_keep_rebuild_parity` covers
nested, dynamic-index and function keys across data/key/duplicate changes.
The existing keyed-list, service-binding and scoped-restyle/layout gates pass.
The source-offset sibling-loop discriminator and its regression already existed
in the starting working tree; retained and verified them, and added the
collision regression plus the new binding gates to `tools/check-performance`.

**Measured.** AMD Ryzen 7 7840HS, `nix develop`, release (thin LTO, one codegen
unit), CPU Skia, `taskset -c 2`, `glass_dashboard_spectrum_profile`: 151 nodes,
32 spectrum bars, 1920×1080, 30 warm-up frames then three 120-frame samples per
invocation. Three alternating baseline/candidate invocations (nine samples per
side), after all compilation and other tests finished:

| | mean | p50 | p90 |
| --- | --- | --- | --- |
| Before | 2.035–2.286 ms | 2.008–2.236 ms | 2.164–2.694 ms |
| After | 1.958–2.136 ms | 1.916–1.998 ms | 2.167–2.700 ms |

Median sample p50 is 2.029 → 1.960 ms (about 3.4%). Mean and p90 ranges overlap;
this is a modest p50 improvement, not evidence of a large frame-time win. The
structural exit gate passes. Stop W5 here; downstream full-tree passes belong
to W3/W7, rather than extending this change with further local scoping.

**Verified.** Full release shell suite, eight threads: 922 → 925 passed,
44 failed and 31 ignored on both saved binaries; the 44 failure names are
identical. Elements: 306 passed / 7 ignored. Expressions: 8 passed / 1 ignored.
Frontend: 89 passed and the two already recorded failures,
`phase87_layout_display_source_semantics_survive_lowering` and
`shipped_settings_surface_compiles_with_local_pages`. Shell syntax check passes
for `tools/check-performance`. The shared pipeline design note stays pending
for W3/W6/W7; all pending notes remain linked from the backlog.


## 2026-10-22 — W7: retained discovery retired from stable frames

`working tree` (baseline: saved release binary at task entry, `4d6b37bb` plus
pre-existing changes including W5) · area: retained synchronization, layout
change reporting, annotation and animation writes

**Change.** Stable frames synchronize producer-reported nodes and subtrees
through immutable snapshot paths. The former full-tree retained discovery and
layout-candidate discovery helpers are compiled only in tests. Full layout
passes report changed geometry while updating their existing projection;
paint-only layout clears the report. Scoped layout, restyle, semantic
normalization, scroll annotation, and animation reports select retained updates.
Selected topology is checked before any retained snapshot is mutated; stale
paths, changed child lists, and rebuilt trees use full reconciliation.
Overlapping subtree/node selections synchronize each node once.

**Completeness.** `MESH_VERIFY_RETAINED=1` runs a test-only full verifier after
every paint, outside visit counters. It checks retained categories, node-owned
render categories, and stale snapshots. It uses the renderer's authoritative
render diff: the renderer-global resource generation can advance during
parallel tests and is invalidated separately. A deliberate omitted-layout
mutation proves the verifier fails. The verifier exposed scroll-metadata writes
missing from annotation reports, semantic/scroll changes missing from narrow
rebuild comparison, and raw border-radius clamping missing from animation
reports because effective samples were equal; those producers now report them.

**Structural gates.** `patched_frame_layout_visits_scale_with_the_patch` now
requires zero retained discovery and constant retained synchronization at 16 and
1,024 clean siblings, plus full retained completeness and forced-full pixel and
layout parity. `retained_change_paths_cover_geometry_and_overlapping_subtrees`
covers moved siblings, overlapping scopes and topology recovery. Added a raw
radius-clamping regression and updated the full-frame animation twin to use its
explicit forced-full switch. Lifecycle coverage checks the reported subtree
before the normal post-entrance snapshot retirement. The relevant structural
gates in `tools/check-performance` enable the full verifier.

**Measured.** AMD Ryzen 7 7840HS, `nix develop`, release (thin LTO, one codegen
unit), CPU Skia, `taskset -c 2`, `glass_dashboard_spectrum_profile`: 151 nodes,
32 spectrum bars, 1920×1080, 30 warm-up frames, three 120-frame samples per
invocation. Three alternating baseline/candidate invocations give nine samples
per side. Candidate also includes concurrent W3 changes. A concurrent W3 build
was active; these establish structural counts, not an isolated latency win.
Raw logs and saved binaries are under `/tmp/mesh-w7/`.

| Per frame, range across nine samples | Before | After |
| --- | --- | --- |
| `retained_discovery` | 151–151 | 0–0 |
| `retained_sync` | 35–35 | 35–35 |
| `retained_lookup` | 0–0 | 209–209 |
| `layout_discovery` | 0–0 | 0–0 |
| Render-object synchronization | 0.066–0.078 ms | 0.062–0.075 ms |
| Total mean | 2.360–2.949 ms | 2.209–2.921 ms |
| Total p50 | 2.312–2.911 ms | 2.132–2.768 ms |
| Total p90 | 2.821–3.476 ms | 2.648–4.080 ms |

Timing ranges overlap; no latency speedup is claimed. Stop W7 at its structural
exit gate. Repeated root-to-node lookup steps remain explicit work; sharing
path prefixes is recorded in the existing backlog item.

**Verified.** Saved release baseline: 925 passed / 44 failed / 31 ignored.
Release production candidate: 933 passed / 42 failed / 31 ignored, with no
added failure names; the two scoped-scroll tests now pass. Full debug suite
with the corrected verifier: 935 passed / 40 failed / 31 ignored; no added
failure names, omitted changes or stale snapshots. The early strict verifier
reported a parallel global-resource-generation change; switching to the
existing render-category diff corrects that test-only mismatch. Release
elements: 309 passed / 7 ignored. Production `cargo check -p mesh-core-shell`
passes. Changed Rust files were formatted; shell syntax and whitespace checks
pass. The shared pipeline note remains pending for the remaining W3/W6 work;
all pending design notes remain linked from the backlog.

**Additional workload checks.** The saved release candidate also passes
`navigation_hover_profile` and `glass_dock_hover_animation_profile`; these
are workload validation, not isolated latency claims.

### 2026-10-22 — W3: scope remaining frame consumers

**Implemented.** Display ancestor discovery follows checked retained paths, with
full discovery only for invalid paths. Transition previous values come directly
from immutable snapshot records; sampling selects restyled and registered
animation nodes. Lifecycle classes retain their base values, restore without
overwriting intervening binding writes, and invalidate scoped interaction
restyles instead of rebuilding the tree. Stable layout and semantic reindexing
reuse structural validation; public/full entry points still validate, and
structural changes retain the validated fallback.

Child-surface requests retain candidate paths, reference indexes and reverse
dependencies. Dirty paths refresh affected requests, including trigger geometry,
overflow, content extents and lifecycle changes. Closed popovers retain only
self/ancestor dependencies until opened. Cached requests and diagnostics match
the full collector, including request ordering. Request work is now counted in
the paint counters; the old paint counter's zero excluded collection and is not
a valid before/after comparison.

**Structural gates.** `display_ancestors_visit_only_retained_paths`,
`transition_and_lifecycle_frames_visit_only_changed_nodes`,
`retained_layout_validates_only_structure_changes`,
`semantic_reindex_reuses_validated_structure`,
`child_surface_requests_visit_only_dirty_dependencies`, and
`closed_child_surface_requests_skip_clean_descendants` compare 16 and 1,024
clean siblings/descendants. They cover constant scoped visits, zero retained
validation, semantic and request parity, invalid-structure/path fallbacks,
actual child lifecycle setters and marker removal. Transition/lifecycle twins
also compare pixels/styles with full frames. Shell gates run with
`MESH_VERIFY_RETAINED=1`; lifecycle restoration has a separate binding-write
regression. Gates are wired into `tools/check-performance`.

**Measured.** AMD Ryzen 7 7840HS, `nix develop`, release (thin LTO, one codegen
unit), CPU Skia (`MESH_RENDER_GPU=0`), `taskset -c 2`, verifier disabled during
timing. Saved baseline is `4d6b37bb` plus the starting workspace's W5 changes;
the candidate includes concurrent W7 work. These are combined workspace
measurements, not an isolated W3 latency attribution. Three alternating
baseline/candidate invocations per workload; no compile ran during final timing.
Raw logs, script and saved binaries are under `/tmp/mesh-w3/`.

Dock gate `glass_dock_hover_animation_profile`: 150 installed apps, 1,877 nodes,
2,880×180; each invocation has three 60-frame samples at 16 ms spacing (nine
samples per side). Navigation gate `navigation_hover_profile`: 1,920px width,
CSS-measured height, 25 cycles × four triggers, 100 popover opens, 400 steps and
400 painted frames per invocation (three samples per side).

| Metric, range across samples | Before | After |
| --- | --- | --- |
| Dock previous-style and animation-sample visits/frame | 14.7–14.8 | 6.2–6.3 |
| Dock display-ancestor visits/frame | 1,823.8–1,823.8 | 21.5–22.1 |
| Dock structural-validation visits/frame | 0–0 | 0–0 |
| Dock child-request visits/frame | Not counted in paint | 44.1–45.4 |
| Dock total mean | 6.049–7.968 ms | 5.650–8.184 ms |
| Dock total p50 | 4.052–6.087 ms | 3.859–6.048 ms |
| Dock total p90 | 15.131–18.148 ms | 14.714–18.010 ms |
| Navigation step mean | 0.9544–1.0008 ms | 0.6469–0.7228 ms |
| Navigation step p50 | 0.9427–0.9831 ms | 0.6328–0.6851 ms |
| Navigation step p90 | 1.1327–1.2906 ms | 0.7338–0.8869 ms |

Dock timing ranges overlap; no dock latency win is claimed. Navigation improves
in the combined candidate. Stop this wave at its structural exit gates; remaining
layout annotation, shared path prefixes and scene preparation stay in the backlog.

**Measured correction.** The first child-request cache registered descendant
extent dependencies even for closed popovers. It measured 317.7–348.1 dock
child-request visits/frame in the same release fixture. The closed-descendant
gate exposed that residual walk; pruning inactive dependencies gives
44.1–45.4 visits/frame above. Earlier measurements are retained under
`/tmp/mesh-w3/pre-pruning/`; they are not the final timing baseline.

**Verified.** Final release shell: 934 passed / 42 failed / 31 ignored versus
saved starting workspace 925 passed / 44 failed / 31 ignored; no new failure
names. The two scoped-scroll tests now pass in the combined W3/W7 candidate.
All new shell structural gates and existing visit gates pass with the full
retained verifier. Full release elements: 309 passed / 7 ignored; renderer:
323 passed / 12 ignored; interaction: 50 passed; animation: 46 passed. Rust
formatting, shell syntax and whitespace checks pass. Architecture/profiling
references and status are updated; the shared design remains linked for W6.

### 2026-10-22 — W6: retain node recordings during complete GPU scene preparation

**Implemented.** Each `SceneCanvas` retains prepared node drawings by immutable
`Arc<DisplayPaintNode>` identity, scale, effective clip and fallback module.
Clean ancestors retain their payload when only descendant topology changes and
inherited transforms/clips still agree. The cache admits at most 4,096 nodes /
16 MiB of picture storage and uniquely counted raster images (four bytes/pixel),
evicting oldest entries. Scene snapshots remain complete, immutable and usable
after skipped submissions, cache eviction, resource replacement or canvas drop.

Layer scopes, backdrop blur/refraction, scroll commands and scroll-content nodes
stay live; tooltips remain outside the node cache. No damage-clipped node picture
is reused for a different clip. Resource revision, async result publication,
font database/alias changes and newly snapshotted asset pages invalidate cached
recordings. Pending jobs prevent admission so placeholders cannot mask final
assets. The existing worker/presentation ownership and full-replay fallback
are unchanged.

**Recording seam.** Skia can inline small pictures during `drawPicture`, so
pictures alone do not establish retained command reuse. Node recordings use
recorded drawables; scene finalization freezes their immutable records. The
structural gate checks one actual scene operation per node reference as well
as preparation counters. Source checks against Skia m148:
[`SkCanvas::drawPicture`](https://github.com/google/skia/blob/chrome/m148/src/core/SkCanvas.cpp#L2634),
[`SkRecordedDrawable::onMakePictureSnapshot`](https://github.com/google/skia/blob/chrome/m148/src/core/SkRecordedDrawable.cpp#L41).
Complete-scene composition still visits references; GPU replay and live effects
are not made change-proportional by this cache.

**Gates.** `scene_preparation_records_only_changed_nodes` compares 16 and 1,024
clean siblings at scale 1.25: a clean frame records zero node operations; each
single material change records one node with at most eight operations, reusing
all other nodes. It compares bytes with uncached complete scenes and direct CPU
paint, and keeps the first snapshot unchanged across subsequent edits.
`scene_cache_preserves_effects_scroll_and_tooltip_inputs` covers affine/overflow
clips, opacity/blending, blur/backdrop, scroll offsets and tooltip opacity.
External-input and cache-budget gates cover scale/module/clip changes, result
publication, image deduplication, eviction and in-flight result rejection.
`publishing_asset_pages_invalidates_standalone_scene_recordings` checks the
standalone-to-page handoff. CPU structural gates are in `tools/check-performance`.
Hardware gates remain `gpu_workload_visual_match_gate` and
`gpu_workload_frame_cost_gate` through `tools/check-gpu-performance`.

**Measured.** Baseline `fa4e1bbb`, clean workspace before W6. AMD Ryzen 7 7840HS,
`nix develop`, release (thin LTO, one codegen unit), `taskset -c 2`. Three
alternating baseline/candidate GPU invocations; each workload has four warmups,
three samples × 20 completed frames (nine means per side). Hardware: NVIDIA
RTX 4060 Laptop and AMD Radeon 780M. Workload gate is 920×900 logical at scale 2
(1,840×1,800 device): full Settings, frosted panel 840×260, widget 138×76,
40 text lines/full, one text line 880×20, and 256 image/icon instances using
128 generated 32×32 PNG sources plus eight text rows. Measurements include UI
preparation, handoff, worker replay, dmabuf copy/fence and GPU completion, not
compositor import/scanout. No build ran during these final timings. Raw scripts,
binaries and logs are under `/tmp/mesh-w6/`.

| NVIDIA completed-frame mean, range across nine samples | Before | After |
| --- | --- | --- |
| Settings full | 2.325–3.693 ms | 2.247–2.513 ms |
| Frosted panel | 1.485–1.816 ms | 1.377–1.470 ms |
| Small widget | 0.879–1.183 ms | 0.617–0.722 ms |
| Text full | 2.318–2.936 ms | 2.394–2.639 ms |
| Text line | 0.795–0.899 ms | 0.332–0.386 ms |
| Assets full | 1.483–1.730 ms | 0.798–0.900 ms |

| AMD completed-frame mean, range across nine samples | Before | After |
| --- | --- | --- |
| Settings full | 4.872–6.222 ms | 4.144–6.061 ms |
| Frosted panel | 2.535–4.105 ms | 2.029–3.880 ms |
| Small widget | 0.959–1.988 ms | 0.746–1.770 ms |
| Text full | 3.355–4.999 ms | 2.498–5.129 ms |
| Text line | 0.887–1.625 ms | 0.480–1.554 ms |
| Assets full | 2.144–3.705 ms | 1.605–3.752 ms |

AMD ranges overlap; no AMD latency improvement is claimed. All workloads pass
the existing worst-mean and frame-p90 budgets (16.67 ms full, 12 ms partial)
on both GPUs with zero warm GLES bitmap transfers. NVIDIA small-widget, line
and asset means improve; full Settings/text ranges overlap.

The in-binary `scene_preparation_profile` compares the original uncached path
with the production cache: translucent 6×6 boxes on an 8px grid, a 256×256
logical scene at scale 1.25 (320×320 device). Four warmups; three samples ×
120 recordings per invocation, repeated three times on core 2. Mean preparation
ranges: 16 siblings 0.0042–0.0061 → 0.0026–0.0031 ms; 1,024 siblings
0.3341–0.3891 → 0.1675–0.2009 ms. Warm frames record zero node operations and
reuse 17 / 1,025 references. This fixture measures preparation, not GPU replay.

**Measured corrections.** The first prototype rebuilt a clean ancestor's paint
payload when a descendant changed: the gate found two recorded nodes instead
of one. Preserving valid ancestor payload identity fixes it. Initial picture
composition also inlined small records; the actual scene-op gate required frozen
drawable references. Before the atlas-publication invalidation, NVIDIA assets
regressed from 1.375–1.500 to 2.914–3.383 ms/frame across nine samples of the
same hardware fixture: cached standalone images prevented warm atlas use.
Publishing pages now invalidates those recordings; final results are above.
The rejected intermediate binary/logs are in `/tmp/mesh-w6/pre-atlas-fix/`.

**Verified.** Final GPU-feature renderer release suite: 339 passed / 19 ignored,
versus baseline 334 passed / 18 ignored. Exported-image visual gate passes on
both GPUs under the existing tolerances, including complete and partial damage.
Production shell `cargo check` passes. The shell candidate before the final
atlas/drawable correction has 934 passed / 42 failed / 31 ignored, identical to
its starting failure set; final shell regression validation follows below.
Rust formatting, shell syntax and whitespace checks pass. Ownership,
architecture/profiling guides, backlog and status are updated. The shared
pipeline design remains linked for its remaining annotation, binding-delta,
path-sharing and live-workload work.

**Final shell validation, same W6 candidate.** Rebuilt release shell with the
atlas-publication and frozen-drawable corrections: 934 passed / 42 failed /
31 ignored, with exactly the starting failure names (none added or removed).
`complete_scene_frames_match_raster_at_fractional_scale`,
`transition_and_lifecycle_frames_visit_only_changed_nodes` and
`retained_change_paths_cover_geometry_and_overlapping_subtrees` each pass with
`MESH_VERIFY_RETAINED=1`. Final logs are `/tmp/mesh-w6/final-shell-tests.log`
and `/tmp/mesh-w6/final-shell-gates.log`.

## 2026-10-22 — P2 keyed-list hover diagnostics and CPU glass raster context

`working tree after 470fb285` · area: keyed binding restyle scope, runtime style diagnostics, glass CPU timing

Patched binding IDs explicitly join the restyle targets, including patches
without a STATE invalidation. The existing interaction collector already carried
those IDs on ordinary hover frames. The remaining live dock full resolution was
in runtime style diagnostics: intervening animation generations broke its
consecutive-generation condition and made each hover patch diagnose the whole
1,877-node tree. Diagnostics now reuse the exact path-validated restyled set when
rules, props, container and structure are unchanged. Global changes keep the full
path. The test-only full-frame switch now forces actual full style resolution,
not just a full-tree ID-filtered traversal.

**Gate.** `keyed_list_hover_restyle_visits_scale_with_affected_items`, registered
in `tools/check-performance`, moves over a keyed list at 16 and 1,024 items.
Hovered and neighboring classes change inherited text color. An unpatched
pointer-leave frame changes opacity and advances the retained generation between
diagnostic updates, reproducing the animation-generation gap. Four hover steps
have exactly the same `(restyle_scan, style_resolve)` counts at both sizes:
`(23,25), (30,33), (30,33), (44,49)`. Every frame matches a full-restyle twin's
computed styles and pixels; `MESH_VERIFY_RETAINED=1` finds no missing updates.

**Timing.** AMD Ryzen 7 7840HS, release (thin LTO, one codegen unit), CPU Skia,
`taskset -c 2`, desktop in use. `glass_dock_hover_animation_profile`: 10 pinned /
150 installed apps, 1,877 nodes, 2880×180 scale 1, three fresh processes per
side interleaved, three 60-step sweeps per process, 16 ms sleeps outside timing.
Before is the pre-edit executable saved from the workspace's release artifacts,
not a fresh rebuild of HEAD; possible pre-existing artifact/source drift means
these timings are comparative evidence, not a commit-isolated speedup claim.

| All nine sweeps | Saved executable | Final candidate |
| --- | --- | --- |
| mean ms/frame | 10.747–11.895 | 5.579–8.176 |
| p50 ms | 6.744–8.289 | 4.944–7.790 |
| p90 ms | 26.750–34.453 | 8.436–12.185 |

Steady sweeps (runs 1–2 in each process) resolve about 313.4 styles/frame before
and 1.9 after. The candidate's first sweep includes one full diagnostic pass
while its initial global inputs settle (33.4 resolutions/frame averaged over
that sweep). An IDs-only intermediate did not fix diagnostics: nine sweeps still
resolved 313.4–313.8 styles/frame, with mean 10.371–12.015 ms versus the saved
executable's 10.078–11.770 ms in that earlier interleaved batch. The complete
fix, rather than further cascade scoping, removes the repeated whole-tree work.

**CPU glass fixture.** `glass_dock_cpu_raster_timing_profile`, also registered in
`tools/check-performance`, uses the shipped dock at the live 2880×114 size,
scale 1, bottom-aligned at `(0,1686)` on a 2880×1800 output. It loads an
output-sized nonuniform BMP (or `MESH_BENCH_WALLPAPER`) and supplies the real
`PaintContext`/`BackdropFrame`, compositor blur policy, and deferred
`SurfaceCanvas`/`RasterReplay::run` boundary. Service state is delivered after
mount. Direct/deferred snapped-animation pixels must match exactly; removing
the output backdrop must change them. Timed transitions retain wall-clock pacing,
and only requested frames are sampled. There is no hardware-independent latency
ceiling.

Same release build and CPU pin, three 60-step sweeps per mode, modes run serially
in fresh processes (not interleaved). Means across sweeps, milliseconds/frame:

| Fixture mode | Total | Preparation/input | Deferred replay |
| --- | --- | --- | --- |
| output backdrop, deferred | 35.445–39.760 | 4.059–4.534 | 31.355–35.448 |
| zero-output context, deferred | 6.624–7.319 | 4.443–4.699 | 2.169–2.630 |
| output backdrop, direct | 30.650–39.725 | 30.647–39.722 | 0 |

The backdrop control exposes the expensive raster branch absent from the visit
fixture. This synthetic wallpaper and compositor-free replay do not establish
pixel identity with a user's live desktop or reproduce the earlier live 19–21 ms
measurement exactly. No Wayland presentation is included. Preparation includes
input/scripts and recording, so the direct mode includes its raster there.

**Validation.** Release shell suite: 976 passed / 32 ignored, zero failures.
The new gate passes in release and debug with the retained completeness verifier;
existing patch restyle/layout gates pass with it too. All three CPU fixture modes
pass their pixel/effect checks. Performance-tool tests: 6 passed; shell script
syntax and targeted Rust formatting pass. Debug full suite: 975 passed / 32
ignored / one failure, `load_frontend_components_respects_disabled_debug_inspector`;
it also fails alone in the pre-change debug executable. No pending planning note
became orphaned. Both completed backlog items are removed; the broader pipeline
note remains linked because its other work is open.

## 2026-10-23 — Animation sampling moved before layout

`working tree after ca8b18d1` · area: style animation pass ordering

**Defect.** `finalize_tree` laid out the tree before paint ran the style
animation pass, so every layout-affecting sample (width, padding, …) was laid
out one frame late, and a transition starting on a keyed-list patch frame
painted its end state for one frame before snapping back. Glass-bar workspace
switch, release, scale 1, 16 ms paint pacing, dot widths per frame:
before `[13,42] → [42,13] → [21,34] → [17,38] → … → [13,42]`; after
`[42,13] → [22,33] → [18,37] → [16,39] → [15,40] → [14,41] → [13,42]`.
Single-run frame times from that probe: patch frame 8.7 → 7.8 ms, animation
frames 4.6–5.6 → 3.3–3.8 ms (one run; not a gate).

**Change.** Paint hands the pass inputs to `finalize_tree`, which samples after
restyle and before layout, and forces layout when a sample changed geometry.
Layout positions still round to whole logical pixels.

**Gates.** `tools/check-performance`: all pass except four that fail identically
without the change. Three runs each, release, same machine, desktop in use,
stashed baseline vs change: `d9_scroll_p90_ms` 36.0–37.5 vs 34.8–42.2;
`d9_glass_dashboard_open_p90_ms` 105.3–130.6 vs 90.1–129.9;
`affected_template_eval_speedup` 2.19–2.33 vs 2.12–2.20 (slightly lower,
overlapping); `settings_scroll_shifted_fraction` 0 vs 0. Shell suite 986 passed
/ 32 ignored / 1 existing failure; animation, glass, keyed and hover tests pass
with `MESH_VERIFY_RETAINED=1`.

## 2026-10-23 — Settings scroll shifts again; momentum snaps to device pixels

`working tree after 82bc82ab` · area: scroll-shift planning, momentum scrolling

**Defects.** `settings_scroll_shift_matches_full_paint` and the
`settings_scroll_shifted_fraction` gate had failed since Settings adopted the
glass material: (1) a compositor `backdrop-filter` beneath the page viewport
vetoed every shift although it paints nothing without an in-surface read
region; (2) the glass rim (`ApplyBackdropRim`) beneath it also vetoed; (3) once
shifting, `avoid_arc_edges` grew every replay band to the whole viewport,
because arc rows/columns used unclamped `9999px` pill radii (arcs like
240–10,239); (4) momentum offsets were fractional, so momentum frames replayed
the full viewport.

**Changes.** Backdrop regions are stored before shift planning; a compositor
backdrop with no read region is skipped; a rim anchors bands of
`max(radius, 3) + 2` px on each side; arc extents use CSS-scaled used radii;
momentum keeps an unsnapped position and publishes offsets rounded to device
pixels (clamped edges stay exact).

**Measured.** `d9_settings_scroll_frame_gate`, release, 920×900, scale 1, 60
finger + 30 momentum frames at 16 ms pacing, desktop in use:

| | shifted frames | p50 | p90 |
| --- | --- | --- | --- |
| before (3 runs) | 0 | 31.3–43.2 ms | 34.8–54.3 ms |
| shift planning fixed, arcs unclamped (1 run) | 60 finger | 31.7 ms | 44.0 ms |
| arcs clamped (1 run) | 60 finger | 7.1 ms | 48.9 ms |
| momentum snapped (1 run) | 60 finger + 21 momentum | 6.8 ms | 7.9 ms |

Gates now pass: `settings_scroll_shifted_fraction` 1.0, `d9_scroll_p90_ms` 7.9
(budget 16.7). The parity test compares every shifted frame with a full paint.
Shell suite 987 passed / 0 failed / 32 ignored (first fully green run); renderer
335 passed serially (the parallel `prepared_scene` flake is in the backlog).

**Remaining failing gates, diagnosed.** `d9_glass_dashboard_open_p90_ms`: warm
open is ~57 ms, of which CPU raster of glass primitives is 42–44 ms; cold
samples push p90 to 90–131 ms. `affected_template_eval_speedup` (needs 4.5×,
measures 2.2–2.3×): full-template control is now 1.13–1.16 s per 100 updates
(was ~7.0 s on 2026-09), selective 0.51 s (was ~0.62 s). The ratio fell because
full rebuilds got faster. Per patch frame, annotation, layout write and display
build each still visit all 1,028 nodes, which is the open P2 item.

## 2026-10-23 — P2: scoped annotation survives layout-class patch frames

`working tree after 16d95ed2` · area: runtime annotation, overflow measurement

**Change.** Runtime annotation measures overflow before layout, so every frame
that ran layout dropped its stored `RuntimeAnnotationInputs` and the next frame
walked the whole tree. After layout on a restyle frame whose layout reports its
changed nodes, `refresh_overflow_paths` now re-measures those nodes and their
ancestors (deepest first, reading other children's stored extents) and the
inputs are kept. This also fixes a one-frame-stale scroll extent: a frame that
grew scrolled content reported the old `max_y` until the next frame.

**Measured.** `affected_template_eval_beats_full_rebuild`, release, 100 one-field
updates over a 1,027-node column, desktop in use, three runs each:

| | annotation visits / patch frame | selective total | full-template control |
| --- | --- | --- | --- |
| before | 1,028 | 505–512 ms | 1,128–1,158 ms |
| after | 2 | 472–494 ms | 1,122–1,148 ms |

`affected_template_eval_speedup` 2.22–2.29 → 2.27–2.44 (gate needs 4.5, still
fails). Layout write and display build still visit 1,028 nodes per patch frame:
each dirty ancestor iterates all its children, and Taffy relays out the column
although the patched text keeps its size (backlog P2).

**Validation.** New test `layout_patch_remeasures_scroll_extents_without_a_full_annotation`
fails before the change. `container_size_restyle_preserves_runtime_and_local_state`
asserted a scroll offset that only survived because of the stale extent (its
text shrank to fit, so nothing overflowed); its text now keeps `flex-shrink: 0`
and the test passes before and after. Shell 988 passed / 0 failed, also with
`MESH_VERIFY_RETAINED=1`; interaction 51 passed.

## 2026-10-23 — Gate run after the scroll and annotation changes

`tools/check-performance` at 639d4dd7: 26 pass, 3 fail. `d9_scroll_p90_ms` 9.7
(was failing at 35–42). Failing: `d9_glass_dashboard_open_p90_ms` 100.2,
`affected_template_eval_speedup` 2.19 (both diagnosed above) and
`d9_glass_dock_frame_p90_ms` 18.7. The dock gate is noise at its edge, not a
regression: `d9_glass_dock_animation`, release, three runs each, 639d4dd7
16.0–20.1 ms versus ca8b18d1 (before today's animation and annotation changes)
17.4–19.7 ms; the morning run passed at 15.8.



## 2026-10-23 — P2: preserve layout for equal-size text patches

`working tree after 8e8b85f5` · area: retained text measurement, sparse layout

**Change.** Each retained text context records at most 16 measurement probes,
including width constraints, known dimensions and the exact size returned to
Taffy. Stable-tree text-only leaf patches replay those probes with the new
content before invalidating Taffy. Equal outputs preserve its caches and live
geometry, while publishing the new text context for subsequent layout. Font,
language or shaping changes, changed layout styles, changed measurements,
incomplete probe history, resizing and resource revisions use normal layout.
The synthetic unchanged surface-root snapshot is omitted from synchronization.
The initial raw-shaping-size comparison was too conservative: known dimensions
and intrinsic-width ceiling can make Taffy's outputs equal despite different
raw widths. The final comparison uses those exact outputs.

**Measured.** AMD Ryzen 7 7840HS, default-feature release with thin LTO and one
codegen unit, desktop compositor running. `affected_template_eval_beats_full_rebuild`:
100 alternating one-field audio updates over 1,027 authored nodes (1,028 including
the surface root), 640×160 CPU paint. Three interleaved before/after samples,
using the default-feature executable saved before editing; no builds or other
tests ran during these samples. The fixture also alternates selective/control
ordering within each sample and checks final pixel parity.

| | before | after |
| --- | --- | --- |
| selective total / 100 updates | 552.864–578.760 ms | 308.418–317.769 ms |
| full-template control / 100 updates | 1,198.879–1,227.000 ms | 1,158.253–1,237.959 ms |
| `affected_template_eval_speedup` | 2.071–2.219× | 3.645–3.999× |

The existing 4.5× gate **still fails**; its baseline and tolerance are unchanged.
Layout writes/extents/fit visits are zero for steady equal-size patches at both
16 and 1,024 siblings. Display building still examines siblings; that and
sparse geometry-changing write-back remain in the P2 backlog. The glass dock
and dashboard budgets were not changed.

**Validation.** Elements: 311 passed / 7 ignored. Full default-feature debug
shell suite, serial with `MESH_VERIFY_RETAINED=1`: 989 passed / 32 ignored.
The new shell regression also passes in release and checks each frame's pixels
against a full rebuild. Element parity covers natural, wrapping, flex-shrunk and
nowrap text at both sibling counts, growth and shrinkage, fractional widths
with equal rounded sizes, and a later ancestor resize. Both regressions are
included in `tools/check-performance`. Elements clippy completed with existing
warnings and no findings in the new implementation. Live UI inspection was
unavailable because no MESH shell was running in the session.


## 2026-10-24 — P2: sparse command updates and transactional binding patches clear the template gate

`working tree after 8e8b85f5` · area: retained display storage, interaction reuse,
template patch transactions

**Changes.** Stable text/semantic frames replace ordinary node commands through
retained paths and a persistent command tree with fanout bounded at 16. All
changed nodes must own a single ordinary command and preserve visual bounds;
scroll scopes and other command shapes retain the conservative rebuild path.
Ancestor sequences are patched coherently, clean subtree caches remain present,
and unchanged coverage, effect metadata and immutable frame snapshots are shared.
Text-only binding frames also skip focus-scope discovery, disclosure discovery
and interaction reconciliation after checking every reported dirty path for
unchanged style, state, geometry and non-text attributes. Pending menu focus
still requests its refresh.

These changes alone were not accepted as a reliable gate pass: an earlier
three-run check of the pre-staging version ranged 4.364–4.675× against 4.5×.
A CPU-clock profile then showed `try_patch_template_writes` copying sibling
`WidgetNode`/`ComputedStyle` data through `SharedWidgetChildren::deref_mut`.
Its rollback tree clone made each ancestor child vector shared before mutation.
The final transaction evaluates bindings on clones of only the target nodes,
checks all expression failures before publication, then commits sorted parent
paths before descendants. Failure leaves the live tree untouched. Genuine
external tree sharing keeps its existing copy-on-write semantics.
The profile is under ignored `profiles/2026-10-23-text-patch/` (1,999 Hz
`cpu-clock:u`, DWARF call graph, 3,489 samples); it was diagnostic, not a timing
sample in the table below.

**Measured.** AMD Ryzen 7 7840HS, default-feature release, thin LTO and one codegen
unit, desktop compositor running. `affected_template_eval_beats_full_rebuild`:
100 alternating one-field audio updates (20/21), 1,027 authored nodes in a column
(1,028 including the surface root), 640×160 CPU paint. Three round-robin samples
of each saved intermediate executable and the final executable, with no builds
or other tests running. Each fixture also alternates selective/control order
within a sample and checks final pixel parity.

| Version | selective / 100 updates | full control / 100 updates | `affected_template_eval_speedup` |
| --- | --- | --- | --- |
| equal-size layout reuse, before this continuation | 312.242–344.314 ms | 1,184.503–1,254.401 ms | 3.440–4.017× |
| plus sparse display commands | 251.003–273.928 ms | 1,148.770–1,170.011 ms | 4.194–4.601× |
| plus interaction-input reuse | 238.922–250.203 ms | 1,133.595–1,158.149 ms | 4.531–4.847× |
| plus staged binding transaction | 90.483–94.535 ms | 1,105.574–1,139.434 ms | 11.695–12.363× |

All three final samples pass the unchanged `affected_template_eval_speedup`
threshold: baseline 6.0, tolerance 25%, minimum 4.5×. This is an absolute reduction
in selective update cost; the full-template control also gets slightly faster.
The glass dock/dashboard budgets and `config/performance-baseline.tsv` were not
changed. The full glass workload sweep was not rerun.

**Structural checks.** At 16 and 1,024 siblings, repeated equal-size service
updates write no layout geometry, rebuild at most three display nodes (text and
semantic ancestors), preserve the parent's child-vector allocation, and perform
zero focus-scope/disclosure refreshes or interaction reconciliations. The
standalone renderer checks one/two changed text leaves with clipping, opacity
and blur ancestors, exact full-command/coverage parity, retained old snapshots,
and a later ancestor rebuild. Persistent sequence replacements preserve clean
branches and do not grow storage depth across repeated updates. Safety checks
reject disabled/reference/style/state/geometry/ancestor-policy changes from the
interaction reuse path. Parent-and-child bindings and expression-failure
atomicity have separate regressions. Structural/safety checks are included in
`tools/check-performance`.

**Validation.** Full default-feature debug shell suite, serial with
`MESH_VERIFY_RETAINED=1`: 991 passed / 32 ignored. Renderer: 337 passed / 13
ignored; interaction: 51 passed. Release narrow-script group with retained
verification: 20 passed / 1 ignored, including pixel parity, transactional
failure and parent/child commits. Renderer clippy completed with existing
warnings and no findings in the new implementation; touched Rust formatting and
`git diff --check` are clean. Live UI inspection remains pending.

**Left open.** Geometry-changing sparse layout write-back, other display command
shapes, shared dirty-path prefixes and write-time keyed-list deltas remain in the
single backlog. The broader pipeline design stays linked there.


## 2026-10-24 — P2/P3: geometry patches, shared path prefixes, sync-time keyed deltas

`working tree after 8e8b85f5` (on top of the uncommitted 2026-10-23/24 P2 work)
· area: sparse layout write-back, display entries and commands, path
resolution, keyed lists

**Changes.**
- *Geometry-changing patches.* Taffy's rounding pass visited every node on each
  layout; trees now disable it and write-back rounds only the nodes it projects,
  matching Taffy exactly (relative locations; sizes at cumulative unrounded
  edges, carried in the projection). Sparse display entries accept geometry
  and clip changes when no changed node with children has a scaling or rotating
  transform, so clean entries and recorded parent transforms stay valid. The
  sparse command patch now replaces moved single-command nodes too, deriving
  each world transform and clip from frames along the retained path. When
  bounds move, it updates ancestor coverage, child unions and spans in place;
  this requires single-command ancestors. A shrink that gives up an edge of
  its parent's child union re-unions that parent's children, skipping
  pre-clipped children. Boxes that clip or scale children, layer/scrollbar
  ancestors, scroll viewports and pruning transitions keep the rebuild.
- *Shared prefixes.* `FrameSnapshot::live_nodes`/`live_paths`, the batch
  index and the command patch sort targets and step through each shared prefix
  once; `for_each_node_at_paths_mut` does the same for mutable callers
  (annotation, diagnostics, binding commits).
- *Keyed deltas.* State sync already converts and compares each compound
  global; it now records which array items differ when the length is kept.
  Lists whose source is a plain member, not shadowed by an enclosing loop local,
  read the member from state and compare and key-check only those items.
  Consuming a delta drops memoized values of expressions reading the member;
  without that, a later rebuild painted first-render colors (caught by the new
  regression before landing).

**Workloads.** New `geometry_patch_frame_timing_profile`: 200 handler+paint
iterations per case, 320×100 CPU paint, keyed column of N items (`clean_column`:
4-item column beside an N-item column; `text_width`: middle text grows
2→12 chars; `box_width`: middle box 10↔20px; `box_height_last`: last box
1↔3px; `box_height_first`: first box 1↔3px, so every sibling moves). Most of
the 1,024 items are pre-clipped below the 100px surface in this shape.
AMD Ryzen 7 7840HS, default-feature release (thin LTO, one codegen unit),
`taskset -c 2`, desktop compositor running. Three interleaved runs per side;
the before binary is the saved build of the tree at session start (same probe).

| N = 1,024 (mean µs) | handler before | handler after | paint before | paint after |
| --- | --- | --- | --- | --- |
| `clean_column` | 138–235 | 138–142 | 817–1,186 | 232–243 |
| `text_width` | 2,396–2,472 | 1,041–1,094 | 3,284–3,499 | 2,320–2,372 |
| `box_width` | 2,306–2,336 | 1,005–1,047 | 2,037–2,116 | 1,222–1,303 |
| `box_height_last` | 2,353–2,489 | 1,000–1,018 | 2,129–2,366 | 1,205–1,274 |
| `box_height_first` | 2,974–3,054 | 1,350–1,400 | 11,768–12,240 | 11,378–11,447 |

At N = 16 handlers drop 73–83 → 57–59 µs and paints are unchanged
(`clean_column` 155–310 → 143–153). Per-paint visits at N = 1,024, before → after:
display entries 1,027–1,032 → 0 in every case except `box_height_first`;
keyed item compares 1,024 → 1; annotation steps 4,103 → 2,055 when every
sibling moves; `clean_column` display ancestors 15 → 10 and builds 8 → 4.
Layout write-back (1,027) and overflow annotation (~1,032) still visit every
child of the re-laid-out parent, and handler sync still converts the whole list
(~1 ms): both are in the backlog. With every item on screen
(`MESH_GEOMETRY_VISIBLE=1`, 320×1100, after only), paint means at 16 → 1,024 are
`clean_column` 142–293 → 240–370, `box_width` 140–144 → 1,502–1,576 and
`text_width` 217–294 → 2,278–2,383 µs. The remaining growth is Taffy's flex pass
over the parent's children, text remeasurement as the column width changes,
write-back and annotation; display work is constant (gate below).

**Existing gates.** `affected_template_eval_speedup` 11.44–11.72× → 11.64–12.44×
(3 interleaved runs; threshold 4.5×). D9, 10 alternating rounds, p90 metric
range (median): dashboard open 55.4–83.6 (64.0) → 66.8–92.0 (73.2) ms; dock frame
14.4–20.7 (16.0) → 15.8–19.3 (17.0) ms; bar clock tick 4.54–5.52 (4.82) →
5.00–6.11 (5.48) ms. Service update, hover and popover open overlap (5 rounds).
**This wall-time increase is not added work:** dashboard open executes
4.595 G user instructions in both binaries, and clock tick goes 2.648 G → 2.437 G.
User cycles rise 2.55–2.70 G → 2.95–3.02 G, all of it inside unchanged Skia
raster-pipeline stages (`copy_2_slots_unmasked` 11.7% → 15.8% of samples). In
both rebuilt binaries those stage functions sit 32 bytes off their previous
64-byte alignment. A build with temporary diagnostic env toggles had the same
alignment and cycles. Treat CPU-raster D9 differences of this size between
relinked binaries as code-placement noise unless instruction counts move.
Budgets and `config/performance-baseline.tsv` are unchanged.

**Structural gates** (added to `tools/check-performance`):
`geometry_patch_frames_scale_with_the_change` (16 vs 1,024 visible siblings;
display entries 0 in release, equal display builds, retained/layout sync and
keyed compares, rect/pixel/retained parity with a full-rebuild twin);
`keyed_list_item_writes_compare_only_written_items` (compares 1/2/1 for one,
two and reassigned items at both sizes; key change and resize rebuild; a later
structural rebuild must not read a stale memoized list);
`geometry_command_patches_skip_siblings_and_match_full_build` (commands, spans,
layers and subtree coverage equal a full build under plain, clip and opacity
roots); `display_list_rebuilds_descendants_only_when_a_moved_parent_defines_their_space`
(replaces `display_list_rebuilds_descendants_for_layout_dirty_parent`, which
asserted rebuilding unscaled children whose commands are unchanged);
`write_back_rounding_matches_taffy_rounding`; `live_paths_step_through_shared_prefixes_once`.

**Validation.** Shell release suite with `MESH_VERIFY_RETAINED=1`: 992 passed before
and after (identical failure sets, none). Full debug shell suite, serial, retained
verification: 994 passed / 34 ignored. Render 337, elements 313, scripting 364,
frontend 92, interaction 51. All 46 `cargo test` gate lines in
`tools/check-performance` pass. Clippy reports no findings in the new code.
The probe also found the stale structural text measurement recorded in
[October](2026-10.md). Profiles are under ignored `profiles/2026-10-24-geometry/`.
Not checked in a live shell.

## 2026-10-24 — glass first-open investigation (wallpaper picker, launcher)

`a163218a` + uncommitted `glass_first_open_profile` · area: surface show, first paint

**Report.** Opening the app launcher or the wallpaper picker took ~2 s.

**Method.** Live `tools/profile-shell session` capture of the `glass` profile
(Hyprland, AMD 780M, eDP 2880×1800 at scale 1, GPU scene worker), toggling
`@mesh/glass-wallpapers` 4× over IPC 1.5 s apart:
`profiles/session/20261024-105446` (`profiling` build). The same IPC script ran
against `target/debug/mesh-shell` (what `cargo run` builds), with the log kept
only in scratch. Times run from the IPC `toggle_surface` to the surface's
first pointer enter (mapped). Single sessions, so the numbers are indicative.

| Build | First open | Later opens |
| --- | --- | --- |
| debug | 1.28 s (renders 684 + 531 ms) | ~80 ms |
| profiling (release opt) | ~125 ms (renders 48 + 44 ms) | ~30 ms (16 ms render + ~10 ms GPU scene) |

Most of the reported 2 s is the debug build. In release the first open is
still 125 ms. Sampling the UI thread during the first release open gives
34 samples at 499 Hz (~68 ms):
`draw_box_shadow_impl` 8, `draw_linear_gradient_command` 8 (rounded-gradient
image cache), `raster_bitmap_variant` via
`raster_file_variant_on_render_thread` 7 (synchronous decode + Lanczos3 resize
+ per-pixel copy of the card thumbnails), and the rest in style/tree build.
The `icon_image_raster` stage (24 + 20 ms release, 956 + 737 ms debug) is that
synchronous file raster path. Each show renders twice, at the pre-configure
size and again after configure, and each render runs `tree_build` twice
(~3 + ~2 ms on later opens).

*Rejected hypothesis:* icon-font glyph rasterization. A scratch bench
(release, Material Symbols Rounded 15 MB, 8 glyphs at 20 px with four axes)
takes 0.03–0.05 ms per batch. Hinting off and a shared `ScaleContext` save
~0.01 ms. `rasterize()` with the cache cold takes 0.2 ms. It is not the cost.

`glass_first_open_profile` (offline, release, CPU raster, 5 rounds): launcher
open 41–74 ms with 150 apps (1,877 nodes), picker open 71–104 ms at 2880×1800,
almost all glass primitives in CPU raster. That does not represent the GPU live
path, so treat it only as a harness for the synchronous stages. The launcher
was not opened live, because no IPC command can open it. Follow-ups are in
the backlog under "Live glass session".

## 2026-10-24 — glass first open: off-thread file icons, GPU-native glass primitives, single-render show

`7ce4d1ba` + working tree · area: resource broker, Skia backend, surface show,
idle prewarm, glass launcher. Follows the first-open investigation above.

**Changes.**
- *File icons and images.* A cache miss no longer decodes, resizes and copies
  on the frame (`raster_file_variant_on_render_thread`). It queues the worker
  job and paints nothing; the job's completion repaints the requesting
  component. A submission the broker rejects (queue or byte budget full)
  subscribes the consumer to the next settled job, so 150 launcher icons
  against a 128-job queue still all arrive. Only when no worker can ever run
  the job is the file rasterized on the frame. Bitmap reductions of 2x and
  more use `imageops::thumbnail` instead of Lanczos3.
- *GPU-native glass primitives.* While recording a GPU scene, blurred box
  shadows draw as a mask-blurred rrect and rounded gradients as the gradient
  under an anti-aliased rrect clip. The per-size CPU image caches remain for
  CPU raster, where they keep damage-clip determinism and blur cost bounded.
- *Show renders once.* Showing a hidden surface restyles its retained tree
  (`INTERACTION_RESTYLE`) instead of `TREE_REBUILD`. A layer surface that has
  painted before (shown or prewarmed) sends its configure on the first pass
  and paints only after the compositor answers. Top-level surfaces no longer
  paint an entering frame: the follow-up frame already dropped the transition
  bootstrap and snapped to rest, so the entering frame was rebuilt, painted at
  an unconfigured size and never presented. A one-shot flag keeps that snap.
  Promoted popups keep their entering frame; their settle frame restyles
  instead of rebuilding. Auto-focus is seeded from the retained tree before the
  style pass, so it no longer raises a second pass.
- *Idle prewarm.* One second after startup, whenever the loop expects to sleep
  at least 50 ms, one never-shown hidden layer surface is painted into a canvas
  that is never presented (CPU canvases only record; replay is skipped). Its
  first show then takes the retained path, and its icons and thumbnails were
  queued while idle.
- *Launcher.* `{#if grid}` builds only the active view and `{#if app.has_file}`
  only one art node per app.

**Measured.** AMD Ryzen 7 7840HS, release (thin LTO, one codegen unit),
`taskset -c 2`, desktop compositor running. Baseline is a worktree of
`7ce4d1ba` with the same test-only harness changes; three interleaved runs
of each binary.

| Workload | Before | After |
| --- | --- | --- |
| glass dock nodes with 150 apps (launcher closed) | 1,877 | 676 |
| launcher open: dock paint (rounds 1–4) | 2.6–3.7 ms | 1.1–2.4 ms |
| launcher open p50, `glass_first_open_profile` | 63.9–68.0 ms | 50.0–69.8 ms |
| picker open to first resting frame, p50 | 80.0–86.5 ms | 75.2–91.0 ms |
| picker open stages: tree_build / style / layout | 1.47 / 0.20 / 0.063 ms | 0 / 0.002 / 0.012 ms |
| re-show through the shell (testing backend): configures / rebuilds / renders / paints | 1 / 2 / 2 / 3 | 1 / 0 / 1 / 1 |
| `d9_glass_dashboard_open` p90, 5 interleaved runs | 57.3–83.0 ms | 63.9–88.0 ms |
| bitmap thumbnail, 2560×1440 PNG and 3840×2160 JPEG → 264×165 | Lanczos3 41.7–89.5 ms | `thumbnail` 5.5–12.0 ms |

The thumbnail diff is a mean of 0.81–1.42/255 per channel. Open totals in the
offline harness are dominated by CPU glass raster (launcher popup paint
36–81 ms) and overlap. That harness rasterizes icons synchronously (`cfg(test)`)
and never records a GPU scene, so it cannot show the first two changes; the
investigation attributed 7 + 8 + 8 of 34 first-open UI-thread samples to them.
The re-show row is from `reshown_glass_surface_reuses_its_tree_and_paints_once`
(before: the same probe on the old path). D9 differences are within the CPU
raster code-placement noise noted earlier this month. Budgets and
`config/performance-baseline.tsv` are unchanged.

**Validation.** Full default-feature debug shell suite, serial with
`MESH_VERIFY_RETAINED=1`: 997 passed / 35 ignored. Renderer: 337 passed; the
two `prepared_scene` cache tests that fail under parallel execution fail
identically at `7ce4d1ba` and pass alone. New:
`prewarmed_glass_surface_first_show_paints_once`,
`rejected_requests_repaint_after_the_next_settled_job`. Not checked in a live
shell; that measurement is the remaining backlog item.

## 2026-10-24 — Settings scroll: every frame restyled the page twice

`eae4dd10` + working tree · area: shell render loop, surface size observation

**Live symptom.** Scrolling Settings → Appearance (glass profile, release
`profiling` build, Hyprland, eDP 2880×1800 at scale 1, 120 Hz, GPU scene
worker) lagged. `profiles/session/20261024-125601`: during 171 presented
Settings frames, `total_surface_render` was 34.0 ms p50 / 40.6 ms p90 / 94.9 ms
max. There were 342 paints (two per frame), and each pass walked all ~624 nodes through
restyle (4.7 ms mean), layout (1.9 ms), animation sampling, annotation,
accessibility and retained sync.

**Cause.** A second capture logged the call site of every STYLE/LAYOUT
invalidation (`profiles/session/20261024-132147`, temporary instrumentation,
since removed). Each frame called `observe_surface_size` twice from the render
loop. The top of the loop read the shell's surface record, which is already
content-sized (920×700), and passed it through `content_size_for_target`,
which subtracts the tooltip overlay reserve as if it were the compositor's
padded size (920×500). The configure path later in the loop restored 920×700.
Each change invalidated STYLE|LAYOUT|PAINT|METRICS, and the dirt the first
pass left asked for an immediate second pass. Any surface with both axes
measured and a reserve was affected. Spanning surfaces (width 0) took the
compositor branch and were not.

**Fix.** Only the compositor's size is converted to content size.

**Offline evidence.** The existing `d9_settings_scroll_frame_gate` drives the
component directly and could not see this. The testing presentation backend
now reports acked configure sizes when asked (`testing_report_configured_sizes`),
as Wayland does. `settings_scroll_frames_through_the_shell_stay_paint_only`
scrolls Settings through the shell runtime and fails before the fix (a
relayout on every frame) and passes after (at most the first frame's
scroll-ownership relayout; one paint per presented frame). The gate also takes
`MESH_SCROLL_GLASS=1`. With the glass material, offline CPU-raster scroll frames
are paint-only but cost 30–40 ms in release, dominated by glass
raster. That is not the live GPU path. Live after-numbers are not yet captured.

**Validation.** Shell suite, serial with `MESH_VERIFY_RETAINED=1`: 998 passed /
35 ignored. Presentation tests pass.

## 2026-10-24 — live Settings scroll recapture; stuck popover exit; glass rim band

`b52dedf6` + working tree · area: child-surface lifecycle, CPU glass rim raster

**Live Settings scroll after the size-flip fix.** Same setup as the entry
above (glass profile, `profiling` build, Hyprland, eDP 2880×1800 at scale 1,
120 Hz, GPU scene worker), `profiles/session/20261024-141319`, built before
the two changes below. During the 5.3 s scroll, 388 Settings frames had
`total_surface_render` 2.8 ms p50 / 4.8 ms p90 / 40.7 ms max, with one paint
per frame (589 renders, 590 paints over the session). Before the fix:
34.0 ms p50 / 40.6 ms p90, two paints per frame.

**Glass bar animating every frame.** The earlier "~4/s idle, ~47/s while
another surface scrolls" was a session average, not a steady rate. In
`20261024-125601`, `20261024-132147` and `20261024-141319` the bar rendered
~1/s (the clock) until the control center was opened and its Settings tile
clicked, then ~110/s (~30/s while Settings shared the frame) until shutdown:
9,305 bar renders in 128 s in the last capture, 29% of a core while idle. Each
frame sampled 41 nodes (1,608 `animation_lookup`, 421 `restyle_scan`, 184
`annotation` visits). A temporary log of the live animation set showed the
control-center subtree still `mesh-surface-exiting`, with its key in the bar's
`closing_child_keys`. `onSettings` sets `open = false`, starting the exit, and
~50 ms later the compositor dismissed the popup. The dismissal destroyed the
child; with no requests and no children left, reconciliation took its early
return, which cleared entering keys but not closing keys. Now it clears both.
Regression: `compositor_dismissal_mid_exit_clears_the_closing_key` (fails
before, passes after). Offline replays of clock ticks, title-only `mesh.wm`
updates, `WindowChanged` events and plain dismissals did not reproduce it. Not
re-captured live after the fix.

**CPU glass rim.** On surfaces above windows, `draw_rim` shaded the whole
element rect with the rim SkSL, which the CPU backend interprets per pixel;
~75% of the glass scroll gate's samples were raster-pipeline SkSL stages. The
shader is exactly zero where `dist <= -rim`, so two interior rects (clear of
the top/bottom and left/right corner arcs, radii clamped as the shader clamps
them, 2 px margin) are now clipped out. `rim_band_clip_matches_shading_the_whole_rect`
compares every pixel against shading the whole rect: symmetric, asymmetric,
zero and oversized radii, fractional rect.

Release (thin LTO, one codegen unit), `taskset -c 2`,
`MESH_SCROLL_GLASS=1 d9_settings_scroll_frame_gate` (Appearance, 240 icon
themes + 240 fonts, 60 finger frames + 30 momentum ticks), three runs each:

| | Before | After |
| --- | --- | --- |
| frame p50 | 31.0–32.5 ms | 8.4–8.5 ms |
| frame p90 | 43.1–48.5 ms | 11.8–12.9 ms |
| frames run | 74 | 90 |

Fewer frames ran before because frames overran the 16 ms tick and the momentum
tail ended early. `check-performance --workloads-only` after the change, one
run, all gates pass: `d9_glass_dashboard_open_p90_ms` 28.7, `d9_glass_dock_frame_p90_ms`
11.7, `d9_glass_clock_tick_p90_ms` 8.3, `d9_scroll_p90_ms` 4.3. No interleaved
baseline was taken for those three glass gates; earlier same-day runs were
63.9–88.0 (dashboard) and 16.0–20.1 (dock). The live GPU path draws the rim
in a GPU shader and is not affected.

**Validation.** Shell suite, serial with `MESH_VERIFY_RETAINED=1`: 999 passed /
35 ignored. Render suite, serial: 340 passed / 13 ignored.

## 2026-10-24 — glass bar recording ticks: clean frames no longer rebuild

`71d2bf0d` + working tree · area: component paint path selection

Backlog item: while recording, the glass bar's 1 s `elapsed` update was
reported to rebuild and relay out the whole bar (~920 validation and 184 layout
visits offline). Reproduced in-process with the real `glass-bar` module, a
`mesh.recorder` payload with `recording = true`, and a 16 ms frame loop that
runs due script tasks and paints only when `wants_render()` holds (as
`Shell::render_components` does):

- The timer tick itself was already sparse: one patched text node, retained
  path, 0 `validation`, 0 `layout_build` visits.
- The whole-bar work came from a paint with nothing dirty. `take_dirty_for_paint`
  only allowed the retained path when some dirty flag was set, so a clean frame
  fell through to a full template build: 1,005 `validation`, 184 `layout_build`,
  184 `annotation`, 207 `style_resolve` visits (debug build; visit counts do not
  depend on the build profile). The offline profile that produced the backlog
  numbers painted every tick, and the bar's clock and recorder tasks run on
  separate 1 s deadlines, so every other paint was clean. Live, a clean frame
  can reach `paint` when `wants_render` holds only because of a live animation,
  scroll or child-surface lifecycle; how often that happens was not measured.

Now a clean frame takes the retained path as a paint-only frame: 0 `validation`,
0 `layout_build`. Regression:
`glass_bar_recording_ticks_stay_off_the_rebuild_path` (fails before with the
counts above, passes after). Not measured live; no wall times were taken.

**Validation.** Shell suite, serial: 1000 passed / 35 ignored; serial with
`MESH_VERIFY_RETAINED=1`: 1003 passed / 35 ignored.

## 2026-10-24 — isolate promoted tooltip fade redraws

Area: shell component ticks and child-surface presentation.

Promoted tooltip ticks sample opacity/scale into the retained popup and request
child-only painting. The shell reconciles child targets without painting the
parent when it has no other work. Inline tooltips continue to invalidate their
owning surface. A terminal tick is retained past the animation deadline so the
last sample cannot be stranded; unsupported/dismissed targets discard their
pending paint generation. Existing user edits were preserved.

**Validation.** Test profile (unoptimized + debuginfo), CPU raster: a 120×26
tooltip in a 160×60 canvas, themed 200 ms opacity animation, sampled at 50, 100
and 250 ms. `promoted_tooltip_fade_keeps_parent_clean_and_paints_final_frame`
checks zero parent invalidations across those three ticks, pending child paint
until each sample is drawn, and no remaining render/tick work after the final
sample. It also checks discarded child work and independent parent resource
invalidation. `inline_tooltip_fade_still_invalidates_parent` checks the inline
path. Targeted tooltip and render suites passed. No before/after wall-time or
live-compositor measurement was made; this records redraw correctness, not a
measured latency speedup.

## 2026-10-24 — preserve the bar scene during popup-only animation

Area: component paint and prepared GPU scenes.

Shared-tree popover animation already reports empty parent damage once the
bar's own startup and trigger feedback settle. The scene path nevertheless
started a fresh complete recording and replayed the whole parent selection on
every frame. Empty-damage frames now open no canvas session, retain the complete
parent scene, and emit no parent Paint profiling record. Style/layout, retained
state, child requests and the popup animation still advance; explicit parent
repaint, resource invalidation and nonempty damage retain complete recordings.

**Measured structural work.** Test profile (unoptimized + debuginfo; scene
identity/count proof, not a timing claim), shipped violet-glass calendar and
controls on a 1920×100 glass bar, 58 parent display entries, 21 frames per popup.
Startup keyframes are finished before opening; a 200 ms wait lets the trigger's
180 ms feedback settle while the popup's 420 ms transition continues. Three
baseline runs temporarily restoring unconditional scene recording each produced
21–21 fresh parent scenes per popup despite zero parent damage. Three fixed
runs produced 0–0, with changed child paint generations and nonempty child raster
output. Gate: `glass_popup_animation_preserves_parent_scene`; it also asserts
parent Paint-record counts and that explicit parent repaint replaces the scene.
No GPU worker timing or live-compositor capture was taken.

**Validation.** Glass integration, interaction/reflow (including fractional-scale
scene/raster parity and explicit repaint), and shell render suites; touched Rust
files pass rustfmt.

## 2026-10-24 — timer patches through popover lifecycle; transform-based meters

Area: retained template binding updates; shipped glass-dashboard stats.

Covered binding writes no longer reject the patch path just because a child
is entering or exiting. Retained restyle already restores authored classes and
reapplies lifecycle markers, including when a binding overwrites a class. Root
surface lifecycle, recovery and structural/uncovered expression guards remain.
Dashboard system-meter fills and CPU-history bars now have fixed 92 px / 54 px
layout boxes and bottom-anchored `scaleY` transitions. Value rounding, meter
clamping and the history's 3 px minimum remain in the module.

**Structural measurements.** Test profile (unoptimized + debuginfo; operation
counts, not wall-time speedup claims), three runs before and after:

- `timer_binding_patches_survive_child_popover_lifecycle`: real Luau sleeping
  task, 300×100 surface, two bound text nodes and a bound popup-panel class,
  three ticks each with child entering and child exiting keys. Before: 3–3
  rebuild frames per phase; after: 0–0, three patched nodes per tick, zero
  structural validation visits. Rebuild counts use validation work rather than
  the nominal full-rebuild flag: the old fallback kept SCRIPT_NARROW while
  rebuilding. Both phases preserve new authored classes, apply their lifecycle
  marker, and remove it cleanly when the phase ends.
- `dashboard_meter_animation_preserves_layout`: shipped dashboard with violet
  glass, 1920×1080 scene canvas, CPU 20→80 and thirty history samples 20→80;
  stable memory/disk/temperature values, 21 frames after the update paint.
  Before: 21–21 frames with layout sync/write; after: 0–0. The gate verifies
  fixed meter/history heights and bottom transform origin, and applies reduced
  motion through the settings API to check the final quantized scales.

**Validation.** Invalidation and disclosure suites, glass integration, targeted
regressions and rustfmt for touched Rust files. No live-compositor or GPU-worker
timing measured.

## 2026-10-24 — spectrum transforms, selective popup fallback and live GPU proof

Area: glass dashboard Player, frontend binding patches, child-lifecycle service
fallback and the live replay harness.

The 24 spectrum bars now retain 44 px layout boxes and set bottom-anchored
`scaleY` directly from each sample, preserving their quantized 4 px minimum.
There is no queued height transition. Conversion alone still performed layout
on every sample: the Player's unchanged seek slider was rebound from its
annotation-canonicalized `0` to the equivalent JSON numeric spelling `0.0`.
Binding patches now compare numeric slider values numerically before writing;
text inputs retain their text semantics. The frontend regression covers an
equivalent numeric spelling and a real changed value.

A shape-stable service fallback can now reuse unaffected authored branches
while a child enters/exits. Prior lifecycle markers are stripped before reused
branches reach rebuild finalization, which captures authored classes and
reapplies the current markers. Root lifecycle, expression recovery, dynamic
structure and conservative retained synchronization guards remain.
`selective_service_fallback_preserves_child_lifecycle` forces a coverage miss on
every service update and compares pixels, text, lifecycle opacity/classes and
marker cleanup against forced full builds.

**Structural A/B, three runs each.** Test profile (unoptimized + debuginfo;
operation counts, not wall-time claims):

- `dashboard_spectrum_updates_preserve_layout`: shipped dashboard, 1920×1080
  scene canvas, 32 source values / 24 visible spectrum bars, 21 alternating
  0.2/0.8 audio updates. Before: 21–21 layout frames; after: 0–0. The gate also
  verifies fixed 44 px geometry, bottom origin and the final quantized scale.
- `selective_service_fallback_preserves_child_lifecycle`: 640×160 raster,
  128 static siblings and a popup, three updates per enter/exit phase. Before:
  0–0 authored nodes reused across six updates; after: 786–786 (131 per update),
  with full-build pixel and lifecycle parity. This narrows authored evaluation;
  fallback finalization/retained reconciliation remains conservative.

**Live GPU validation.** Release with `gpu`, thin LTO, one codegen unit,
Hyprland eDP-1 2880×1800 at scale 1 / 120 Hz; AMD vendor 0x1002, device 0x15bf,
renderD129 (0xe281), dmabuf format 0x34324241. Three fresh isolated processes:
`profiles/popup-live/1706244`, `1715003`, `1718751` (the summary records the
exact capture paths). Each opens/closes calendar and controls near bar timer
deadlines, observes 2.2 s of idle clock ticks, injects six CPU updates 500 ms
apart, then thirty spectrum updates at 33 ms. Config, IPC and state are private;
layer namespaces use `.mesh-replay`. The focused harness waits for actual
worker completions, captured independently of accepted scene submission.

| Window / target | Render frames per run | Validation frames | Layout frames |
| --- | --- | --- | --- |
| Calendar opening / bar | 158–170 | 0–0 | 5–5 |
| Calendar closing / bar | 22–26 | 0–0 | 4–4 |
| Controls opening / bar | 139–156 | 0–0 | 5–5 |
| Controls closing / bar | 25–26 | 0–0 | 4–4 |
| Idle clock / bar | 2–2 | 0–0 | 2–2 |
| Six CPU updates / dashboard | 231–239 | 0–0 | 6–6 |
| Thirty spectrum updates / dashboard | 30–30 | 0–0 | 0–0 |

There were 461–541 successful GPU worker commits per run, including commits
from both child popups. Raw records and the checked aggregate are in
`profiles/popup-live/summary.json`. These are final-path observations, not a
live A/B latency speedup. The first probe ran outside the Nix shell and fell
back because `libEGL.so` was unavailable; rerunning inside `nix develop`
resolved the runtime library path. The earlier GPU probe after only the module
transform conversion still had 30 spectrum layout frames; the numeric slider
comparison removed those in all final captures.

**Remaining finding.** A ready unchanged parent still advances shared-tree
animation preparation while its child waits for a frame callback: calendar
opening prepared the bar 158–170 times for 40–41 child commits, controls opening
139–156 times for 39–44. Parent paints/commits are separate counts; focus/trigger
feedback and clock changes legitimately paint the bar. Callback-aware shared-tree
preparation is a new backlog item, not a claim of a completely idle parent.

**Validation.** Invalidation (85), glass integration (16), interaction/reflow
(18), frontend attribute tests (12), structural A/B gates and three successful
live GPU replays. Touched Rust files pass rustfmt.

## 2026-10-24 — capture-directory correction

Area: evidence paths for the preceding spectrum/popup/live-GPU entry.

The third final capture directory is `profiles/popup-live/1723245`, replacing
`1718751` in that entry. `profiles/popup-live/summary.json` already contains all
three correct paths. The measurements and validation results are unchanged.

## 2026-10-24 — Align live replay and CPU fixture animation timing

On main after `d4604548` · area: replay gate methodology

The embedded CPU fixture previously slept 16 ms after each paint, stretching
tile dwells by raster time, while live input crossed tiles roughly every 100 ms.
It measured only a warmed second sweep against the live shell's first sweep.
Both now use one absolute-time schedule: ten pinned apps / 150 installed apps,
20 tile crossings over two seconds, six pointer positions per 100 ms dwell,
and the same out-and-back route. Late input samples coalesce to the current
position. CPU sampling uses absolute 16 ms ticks; live paint follows compositor
callbacks. Both warm up, leave and settle before measuring.

**Validated.** Release, GPU-feature shell binary, three fresh live GPU processes
and three fresh forced-CPU processes on the available Wayland compositor. All
six pass the unchanged per-class/per-pass maximum comparison with zero
`live_dock_visit_outliers`. GPU windows contain 217–239 measured frames; CPU
windows contain 154–156. The aligned CPU envelope samples seven overlapping
animation nodes instead of six. The previous single-process capture after
`202fbb9a` recorded 51 outliers; earlier repeated failures are in the October 21
live-attribution entry. No runtime speedup or latency comparison is claimed,
and no D9 budget was changed.

The pure schedule test covers late samples, dwell deadlines, turnaround and end
bounds. Debug GPU-feature shell suite: 1,017 passed / 36 ignored. Raw logs are
under ignored `profiles/backlog-2026-10-24/aligned-*`.

## 2026-10-24 — Fold full-layout projection into geometry write-back

On main after `d89e9b0b` · area: retained full layout

Fresh, structural and full retained passes now use the existing sparse writer
with reuse disabled. It rounds/project rects top-down, aggregates clipped fit
extents bottom-up, records visibility/projection entries, and reports geometry
changes in that traversal. The separate `seed_projection` walk is deleted.
Unknown/changed topology still reports an incomplete geometry delta; retained
sparse passes keep their branch reuse.

**Measured.** Standard release (thin LTO, one codegen unit), CPU 6, unchanged
`d4604548` checkout versus candidate. `full_layout_projection_profile`: a
1,365-node four-way/depth-five tree, percentage-width root, fixed-size box
descendants, no text measurer, retained resize between 1200/1201 × 800, 500
layouts per sample. Three alternating before/after process pairs, three samples
each (nine per side): 448.657–533.846 ms before versus 424.933–526.684 ms after;
medians 458.445 versus 440.602 ms. Timing ranges overlap; the deterministic
result is the removed sweep, not an end-to-end shell speedup claim.

Per resize, `layout_extents` falls from 2,730 to 1,365; `layout_write`,
`layout_fit` and `layout_sync` remain 1,365. Gate
`full_layout_write_records_projection_in_one_visit` requires one extent/fit/write
visit per node and compares geometry with stateless full layout across resizes,
including absolute fit contents, clipped overflow and a hidden subtree.

**Validation.** Elements: 314 passed / eight ignored. GPU-feature shell suite:
1,017 passed / 36 ignored. After avoiding unused fresh-tree change-set entries,
the full-layout/geometry-change regressions and complete elements suite pass
again. Formatting and diff checks pass. The older unpinned exploratory samples
were affected by concurrent compilation and are not used for the timing claim.
Raw final logs: ignored `profiles/backlog-2026-10-24/layout-pinned-*`.

## 2026-10-24 — Preserve blocked GPU scenes and retry export on release

On main after `f6a68c5a` · area: scene worker and Wayland backpressure

A `BuffersBusy` completion now parks the exact immutable frame/commit instead
of producing a failed shell completion and forcing a full repaint. Native
dmabuf release requeues export/copy from the retained raster target. No new
commit, script execution, scene recording or raster is needed. The worker keeps
at most one pending/parked frame per surface; supersession, removal and close
discard stale retries outside its queue lock. Non-transient failures retain the
existing full-replay/fallback contract.

Presentation preserves pending logical damage and surface-object checks, waits
for actual dmabuf availability, and preserves parent-before-popup commit order.
Queued/active jobs retain the two-second worker deadline; parked frames and
completed children awaiting their parent do not consume it. The release-before-
completion case retries immediately rather than relying on another wakeup.

**Measured structural control.** Standard release (thin LTO, one codegen unit),
three fresh required-GPU processes (last two pinned to CPU 6). The existing
32×24 native export/lease fixture holds all three slots, applies an 8×8 colour
patch, observes `BuffersBusy`, releases one lease and retries that same commit.
Normal render attempts increment the test raster counter by 1–1; resumed export
increments it by 0–0. All three runs pass pixel checks against the exported slot,
retain partial damage, and preserve the old-release isolation checks. This is
a native GPU test with test-held leases, not a live compositor-stall capture
or a wall-time speedup claim. Gate:
`scene_worker_preserves_partial_damage_and_owns_compositor_leases`.

**Validated.** Eleven generic worker tests pass, including retry identity/damage,
unrelated-surface progress, fatal-error rejection, stale retry rejection, resource
lifetime, and replacement/removal/close cleanup. Default renderer: 342 passed /
13 ignored. GPU-feature presentation: 115 passed. GPU-feature shell: 1,017 passed /
36 ignored. The GPU renderer suite reports 352 passed / one failed / 19 ignored;
its panel recorded-scene parity failure reproduces identically on the unchanged
`f6a68c5a` checkout (38 of 19,200 pixels, worst channel delta 36). It remains in
the backlog; no tolerance was changed. Raw logs are under ignored
`profiles/backlog-2026-10-24/scene-retry/`.

## 2026-10-26 — P2 producer-directed geometry write-back and exact overflow unions

Baseline `dcd94b5b`; isolated candidate branch `codex/p2-sparse-geometry`.
Reviewed the rejected-experiments table and the 2026-10-24 geometry-delta entry
before implementation. No Taffy fork or private API is used.

**Implementation.** A generational layout arena implements Taffy 0.10.1's public
low-level traits and continues to run its flex/grid/block/leaf algorithms and
cache policy. `set_unrounded_layout` records authoritative before/after layouts,
including clean siblings moved by flex, wrapping or alignment. An epoch-indexed
vector deduplicates writes without clearing a hash table sized for a previous
large update. Cache invalidation stops at an already dirty ancestor, matching
Taffy's policy. Sparse write-back follows producer deltas plus changed input
paths. A changed parent's size alone does not force a child scan: native layout
writes report affected child geometry. Origin, rounding-origin, visibility and
padding changes conservatively propagate the projection to descendants.

Layout fit extents and overflow annotation retain indexed child unions. Point
updates preserve exact shrinking bounds and tied extrema, without reconstructing
old contributions from scroll metrics. Full/structural passes rebuild indexes;
missing paths/history keep the full path. Overflow changes, including clamped
scroll offsets, join the frame dirty set. Large producer change sets use full
write-back, and large overflow dirty sets use full annotation. Dense frames
therefore avoid building sparse branch indexes for most of the tree.

**Structural measurements.** `geometry_patch_visit_profile`, three separate
release processes per side, four alternating updates per fixture, 16 and 1,024
siblings, CPU raster at 320×100. For 1,024 siblings, width/text/last-height
patches each measure **1,027–1,027 → 4–4 layout-write visits**, and
**1,032–1,032 → 10–10 annotation visits**. A first-child height change that
moves almost every sibling keeps **1,027** write visits; annotation falls from
2,055 to 1,030 through the dense full pass. The small changing column beside a
large clean column measures 8 → 6 layout-write visits and 15 → 14 annotation
visits. Fit/extents work was already bounded for sparse width changes.

`geometry_patch_frames_scale_with_the_change` additionally proves constant
write-back and annotation visits with every sibling visible (320×1100), on both
growth and shrink frames, with full-tree/retained/pixel parity. Bounds-index
operations grow logarithmically: 21 → 33 for a 16 → 1,024-sibling box-width
patch. Producer work is explicitly counted rather than hidden: that 1,024-item
candidate frame still records **1,027 unrounded writes and 9,239 producer
operations** (child traversal, layout dispatch and invalidation). This removes
redundant downstream scans; it does not make Taffy's flex algorithm sparse.

**Release timings.** Same thin-LTO, one-codegen-unit GPU-feature shell test
profile on both sides; explicit CPU `PixelBuffer` raster. Three alternating
baseline/candidate pairs, 200 updates per sample after three initialization
paints, ordinary unpinned desktop in use, no concurrent compilation. Rows below
are the range of per-run **paint p50 in milliseconds**. Paint includes bindings,
style/layout, retained updates and raster; handler sync is measured separately
in the raw data and is excluded here. The first four cases keep all siblings
visible at 320×1100; `box_height_first` uses the 320×100 clipped workload.

| Case | Siblings | Baseline paint p50, ms | Candidate paint p50, ms |
| --- | ---: | ---: | ---: |
| `clean_column` | 16 | 0.137–0.139 | 0.147–0.150 |
| `clean_column` | 1,024 | 0.236–0.236 | 0.233–0.235 |
| `box_width` | 16 | 0.144–0.145 | 0.141–0.144 |
| `box_width` | 1,024 | 1.417–1.515 | 1.275–1.332 |
| `text_width` | 16 | 0.216–0.217 | 0.208–0.215 |
| `text_width` | 1,024 | 2.122–2.295 | 1.900–1.992 |
| `box_height_last` | 16 | 0.142–0.145 | 0.145–0.146 |
| `box_height_last` | 1,024 | 1.444–1.623 | 1.257–1.292 |
| `box_height_first` | 16 | 0.220–0.223 | 0.224–0.230 |
| `box_height_first` | 1,024 | 9.648–12.037 | 9.679–10.140 |

Sparse 1,024-sibling cases improve; the already scoped clean-column workload is
largely unchanged. Small workloads can pay additional bookkeeping: notably
clean-column/16 adds about 0.01 ms. Dense-case ranges overlap; no dense-frame
speedup or live glass/GPU latency claim is made. An initial always-sparse
implementation measured 12.717–12.951 ms versus 12.425–12.527 ms on the dense
1,024-sibling case. The final candidate uses the full path for large change
sets instead; the initial data and executable are retained under
`/tmp/mesh-p2-evidence/initial-sparse/`.

**Validation.** Final release GPU-feature shell suite: **1,032 passed,
36 ignored, zero failures**. Elements: **320 passed, 8 ignored**; interaction:
**52 passed**. Native Taffy remains the independent full-layout oracle. The
geometry matrix covers early/middle/last growth and shrink at 16/1,024 siblings,
wrap, flex grow/shrink, alignment, fractional padding/coordinates, hidden nodes,
intrinsic text probes and nested absolute/fixed/promoted descendants. Exact
changed-node reports are checked against full geometry. Overflow tests compare
every metric and offset clamp, including clipping, negative bounds, tied extrema,
missing history and invalid paths. The additional optimized shell pixel matrix
compares layouts, scroll metrics, retained command geometry/clips and every pixel
for wrap/grow/shrink/alignment/fractional/clipped/fixed cases at both sizes and
all three target positions. Existing promoted-popup and lifecycle tests pass.
The structural gate and both visit profiles also pass separately.

**Scope remaining.** Display-list patches under layer/scrollbar/clip/scale/scroll
scopes, and sibling unions for shrinking display coverage, remain the P2 display
backlog item. Compound Lua synchronization remains D1. No public styling or
module contract changed. The completed producer-delta design note is removed;
this record retains its outcome and git retains its design.

**Evidence/reproduction.** `/tmp/mesh-p2-evidence/` contains the baseline and
candidate test executables, `bench.py`, `timings.json`, `visits.json`, all three
per-side visit logs, `candidate-structural-gate.log`, per-case timing logs,
`final-release-tests.log`, and the initial always-sparse trial. Element and
interaction output is `/tmp/mesh-p2-unit-tests.log`. Build:
`nix develop -c cargo test --release -p mesh-core-shell --features gpu --lib`.
Timing gate: `geometry_patch_frame_timing_profile --ignored --nocapture`, with
`MESH_GEOMETRY_CASE` selecting the case and `MESH_GEOMETRY_VISIBLE=1` for the
visible workloads. Separate worktrees and `/tmp/mesh-p2-target` keep the main
checkout and its build artifacts untouched. No live compositor replay was run.

## 2026-10-25 — gate shared-tree popup preparation by affected targets

Base `d02ba382`, isolated branch `codex/handler-popup-glass`. Reviewed the
rejected-experiments table and the preceding parent-scene preservation entry.

Animation-only frontend frames report their affected realized popovers through
retained snapshot paths. The scheduler and render loop wait when all of those
popovers lack a presentation slot, even if the parent is ready. Finished parent
keyframes retained for forwards fill do not widen that target set. Unknown or
parent animation paths, input/script/resource invalidation, initial configure
work and non-popover children keep conservative preparation. A ready child can
still advance while its parent waits for a callback or buffer release.

**Release measurement.** `blocked_popup_animation_preparation_profile`,
GPU-feature shell test binary, optimized with thin LTO; synthetic 120×36 parent
and 72×32 popup, parent ready, popup callback pending, 100 render attempts per
sample. Three interleaved broad-gate / target-gate pairs in one process:
preparation calls **100–100 → 0–0**; elapsed **1.135–1.178 ms → 0.030–0.037 ms**
per 100 attempts. The broad-gate control returns no target restriction and
therefore executes the pre-change readiness path. This fixture measures shell
preparation dispatch, not a real popup's tree/layout cost or GPU frame latency.
The gate asserts the preparation counts rather than a noisy timing ratio.

**Validation.** `cargo test --release -p mesh-core-shell --features gpu --lib --
--test-threads=1`: **1,024 passed, 37 ignored, zero failures**. Regressions cover
callback and buffer backpressure, input/script fallback, resumption, the inverse
ready-child/blocked-parent case, missing paths and completed parent keyframes.
The ignored release preparation benchmark also passes. Source and measurement
artifacts are isolated from the main checkout; scratch evidence is under
`/tmp/mesh-popup-glass/` (`shell-tests.log`, `popup-benchmark.log`).

## 2026-10-25 — live glass first open after popup readiness gating

Measured `e012da95`, normal **release** GPU build (thin LTO), separate branch
`codex/handler-popup-glass`; no additional runtime change in this measurement.
Binary SHA-256:
`e721e06a0fc84caee934df95969d5c868befa25676740d9f5eb8e3c25e1fb86e`.

**Environment and workload.** Main Hyprland 0.56.2 compositor, `wayland-1`,
2880×1800 at scale 1 / 120 Hz, AMD render node `renderD129` (PCI 1002:15bf).
The runtime reports GPU device `0xe281`, dmabuf format `0x34324241`, modifier
`0xffffffffffffff`; no CPU fallback or protocol error in the selected runs.
The desktop remains in normal use, with the existing quickshell shell present;
these are not pinned or otherwise isolated idle-machine latency guarantees.
The launcher indexes 68 listed desktop entries and requests a 596×512 popup;
the wallpaper directory contains five images. Shared OS/image caches are warm;
"first" means first show in a fresh MESH process, not cold disk caches.

A private copy of config/settings/profile/state and a private IPC socket keep
all profile changes out of the checkout and the user's saved configuration.
The hand-built test profile copies the shipped glass composition's roots and
resources and explicitly adds `@mesh/glass-wallpapers`: the current shipped
composition selects the newer `@mesh/wallpaper-carousel`, while this backlog
item follows the original picker investigated on 2026-10-24. Hidden-layer
prewarming is enabled and finishes before the selected first opens.

**Method.** Three fresh processes (scratch captures **2, 6, 8**), four opens of
each target per process. Picker: `shell:show_surface:@mesh/glass-wallpapers`
over its private IPC socket, followed by hide between opens. Launcher: actual
Wayland virtual-pointer clicks on its dock trigger, followed by an outside
click to dismiss; no synthetic application launch or template-state injection.
Stage profiling and `WAYLAND_DEBUG=1` are enabled throughout. Picker time starts
immediately before the IPC send; launcher time starts at the logged native
pointer press. The endpoint is the first non-null `wl_buffer` attachment and
following `wl_surface.commit` for that target, correlated through its layer role
or `get_xdg_surface` / `get_popup` requests. Protocol lines are timestamped on
receipt by the local collector, so small pipe/collector overhead is included.
This includes asynchronous GPU scene completion up to buffer commit, but is
not compositor presentation feedback or a measurement of physical scanout.

| Target | First open, 3 fresh processes | Later opens, 9 samples |
| --- | ---: | ---: |
| Original glass wallpaper picker | **42.541–45.403 ms** | **9.980–17.861 ms** |
| Dock launcher | **304.658–490.097 ms** | **42.209–66.410 ms** |

A separate build overlapped trial captures 3, 4 and 7; those samples are
excluded from the table. Compiler absence was checked after capture 2; captures
6 and 8 additionally sampled `rustc` activity every 0.5 s and recorded zero
processes throughout. Trial 1 used Escape for dismissal, which did not reliably
close the launcher; trial 5 was stopped by another shell runner's SIGTERM.
Both trials are excluded. Later probes use a distinct executable process name
and outside-click dismissal. No competing process was stopped for this work.

**Finding.** The first launcher child `total_surface_render` records are
322.453, 259.051 and 444.790 ms in the three selected runs, accounting for most
of the first-open delay. That synchronous timer covers canvas allocation,
backdrop preparation, child display-list/scene recording and present submission.
It does not establish which of those operations is responsible, so no shader,
font or icon attribution is claimed. The backlog now asks for that attribution;
the live-measurement item itself is complete. This is a current-behavior
measurement, not an A/B speedup claim for the preceding popup gate.

**Evidence and reproduction.** `/tmp/mesh-popup-glass/` retains `capture.py`,
`analyze.py`, the private `config/`, `monitors.json`, `workload.json`,
`run-{2,6,8}.log` / `.jsonl`, `marks-{2,6,8}.json`, `load-{6,8}.json` and
`results.json`. The collector runs inside `nix develop` with `MESH_BACKEND`,
`MESH_MODULE_GRAPH_PATH`, `MESH_SETTINGS_PATH`, `MESH_HOME`, `XDG_STATE_HOME` and
`MESH_IPC_SOCKET` explicitly scoped to the probe. The picker uses ordinary IPC;
the launcher uses `wlrctl pointer move/click`. The probes shut down after each
run. The popup implementation's release suite remains 1,024 passed / 37 ignored;
this commit changes only the backlog, status and this measurement record.

## 2026-10-26 — retain display commands beneath compositing and scroll scopes

Baseline `8adb3e02`; isolated candidate branch `codex/p2-display-scopes`.
Reviewed the rejected-experiments table and the preceding P2 geometry record.
This completes the display-scope fallback item; shrinking display coverage and
retained span-array copying remain separate backlog work.

**Implementation.** The sparse command index retains all of an owner's local
command positions, including trailing scrollbars and layer/scroll push/pop
commands. Topology-stable updates replace persistent command leaves along dirty
paths. Changed child bounds repair compositing and filter clips without
rebuilding clean paint payloads or child command topology. Filter replay regions
are refreshed, and old/new effect coverage is included in damage. Scope-owned
span coverage is repaired along with ordinary spans. Ordered spans use indexed
lookups; trailing scrollbar spans retain conservative span-array repair.

Path resolution carries live cumulative affine coordinates and retained scroll
content spaces. Entry bounds inside retained content stay anchored to its
viewport, and only viewports on updated paths receive sparse damage. A moved
viewport invalidates sparse entry collection. Stable overflow metrics update an
existing scrollbar owner, and retained offset-only frames replace the owner's
node and scroll commands without visiting content. Non-retained offset changes,
changes to descendant clip/transform spaces, visibility/topology changes and
missing history retain the conservative path. Scroll-content eligibility changes
from paint material/primitive updates also keep full entry collection.

**A/B method.** Normal release profile (thin LTO, one codegen unit), renderer
unit-test executables, same fixture source on both sides. The baseline worktree
adds only `scoped_display_patch_profile`; it does not change runtime code. Three
alternating baseline/candidate process pairs; each process contains three
alternating rounds per workload, each with 100 updates after three initialization
updates. There are nine p50 samples per side and row below. The ordinary desktop
is unpinned and remains in use. The harness checks that no `rustc` process is
active before each executable; all shell compilation/tests finished first.

The fixture has 16 or 1,024 box siblings in a 320×1100 surface, one width toggled
20↔40 px, and wider clean siblings that keep the child union's extrema stable.
This isolates command preparation from the separate shrinking-union problem.
Scopes are overflow clip, 0.95 scale, 0.7 opacity, 2 px foreground filter blur,
retained hidden-overflow scroll, or retained scroll with scrollbars. Scroll
metrics are explicit (100 px range, 5.5 px offset). The measured interval is
`RetainedDisplayList::update_with_dirty_nodes`, excluding retained-object dirty
classification, layout, handler synchronization and raster. These are display
preparation timings, not whole-frame, live GPU or first-open latency results.
The ignored harness also records a deliberately forced conservative control;
the table uses only actual baseline/candidate rows (`control == false`).

| Scope | Siblings | Baseline preparation p50, ms | Candidate preparation p50, ms |
| --- | ---: | ---: | ---: |
| `clip` | 16 | 0.004749–0.004749 | 0.004749–0.004749 |
| `clip` | 1,024 | 0.009498–0.009498 | 0.010685–0.010685 |
| `scale` | 16 | 0.004749–0.004749 | 0.004749–0.005936 |
| `scale` | 1,024 | 0.009498–0.009498 | 0.010685–0.011872 |
| `opacity` | 16 | 0.010685–0.010685 | 0.004749–0.005936 |
| `opacity` | 1,024 | 0.349048–0.390602 | 0.008310–0.008311 |
| `filter` | 16 | 0.010685–0.011872 | 0.005936–0.005936 |
| `filter` | 1,024 | 0.350235–0.381104 | 0.008310–0.008311 |
| `scroll` | 16 | 0.017808–0.017809 | 0.004749–0.004749 |
| `scroll` | 1,024 | 0.960476–0.981846 | 0.008310–0.008311 |
| `scrollbar` | 16 | 0.017808–0.017809 | 0.004749–0.005936 |
| `scrollbar` | 1,024 | 0.964037–0.987782 | 0.008310–0.008311 |

At 1,024 siblings, opacity/filter display-build visits are **1,026 → 1**;
scroll/scrollbar visits are **1,025 → 1**, with display-entry visits
**1,025 → 0**. All nine samples agree. Clip/scale were already sparse on the
baseline: build visits stay one and entry visits zero. They pay about 1–2.4 µs
more preparation in the large fixture; no speedup is claimed for those cases.
Timer quantization is visible at this scale. The principal win is removal of
layer and scroll fallbacks, not a universal latency improvement.

An initial repair implementation scanned every retained span to look up changed
owners. It passed parity but regressed the already-sparse large clip/scale cases
from about 0.0095–0.0107 ms to 0.0214 ms. It was replaced by indexed lookups on
ordered spans, leaving the final ranges above. Initial executable, raw output
and JSON remain in `/tmp/mesh-display-evidence/initial-span-scan/`.

**Validation.** Final renderer debug suite: **342 passed, 14 ignored**; final
renderer release suite: **341 passed, 14 ignored**. Final release GPU-feature
shell suite: **1,034 passed, 37 ignored**. Zero failures. The expanded
`geometry_command_patches_skip_siblings_and_match_full_build` gate covers
16/1,024 siblings, first/middle/last targets, growth and shrink, clip/scale,
opacity/filter layers, scroll and scrollbar ancestors. It also changes layer
owner geometry or scroll-owner overflow metrics with a leaf. Build visits stay
one for a single dirty node and two for the paired changes. Commands, entries,
subtree coverage, replay spans, layer/filter metadata, fresh full raster and
partial repaint pixels match the full oracle. Partial repaints use the shipped
blur-damage expansion. The retained-offset test additionally asserts one build
visit and immutable content-command reuse. Existing scroll shift/damage,
promoted-popup, animation and lifecycle tests pass. Formatting and diff checks
pass. No live compositor replay was run or claimed.

**Limits and remaining scope.** A scope change that actually changes descendant
coordinate spaces still rebuilds those descendants. A child giving up an edge of
an ordinary ancestor's bounds still triggers a sibling union scan. Persistent
commands reuse clean branches, but flat retained span arrays can still copy
unrelated entries; scrollbar-containing span repair still scans those arrays.
Neither those costs nor the independent P3/D1 table-write observation question
is presented as completed.

**Evidence/reproduction.** `/tmp/mesh-display-evidence/` retains both profile
executables, `bench.py`, `timings.json` (432 rows including the auxiliary
controls), six raw process logs and the printed summary. Run
`cargo test --release -p mesh-core-render --lib scoped_display_patch_profile --
--ignored --nocapture --test-threads=1` inside `nix develop`. Renderer suite logs
are `/tmp/mesh-display-final-debug.log` and
`/tmp/mesh-display-final-release-render.log`; the final shell log is
`/tmp/mesh-display-final-shell.log`. Builds use `/tmp/mesh-p2-target`, leaving
main-checkout build artifacts and the user's profile edits untouched. Temporary
implementation and baseline worktrees are removed after committing/integration.

## 2026-10-26 — indexed shrinking display coverage

Baseline `4005afb4`. The prior display-scope command work was already complete;
this change finishes the separate sibling-union scan item. Retained span-array
copying and scrollbar span repair remain in the backlog. Reviewed the rejected
experiments before implementation.

**Implementation.** Each built paint subtree retains a balanced child-coverage
index keyed by direct-child NodeId. Sparse updates replace only the changed
leaf and its ancestor index branches, preserving prior retained snapshots and
sharing clean branches. Inner-to-outer repair updates exact child unions even
when a child gives up an extremum; no live sibling walk or preclip recheck is
needed. A parent whose coverage is unchanged stops bounds propagation. Full or
structural rebuilds reconstruct the index from built children; omitted children
remain omitted. Layer/filter clips still repair from these exact bounds, and
retained scroll content still propagates its anchored viewport.

**A/B method.** Normal release profile (thin LTO, one codegen unit), built in
`nix develop` with `CARGO_BUILD_JOBS=4` and `/tmp/mesh-p2-target`. The saved
baseline executable adds only the ignored shrink-profile fixture, with no
runtime changes. Three alternating baseline/candidate pairs, pinned to CPU 2,
with no active rustc process during timing. Each executable runs three rounds
of 100 measured updates after three initialization updates, with an additional
conservative material-dirty control. The table excludes that control and gives
ranges across nine per-round p50 samples on each side.

The fixture has 16 or 1,024 direct box children in a 320×1100 surface. Clean
children are 80×1 px; the middle child alternates 160↔120 px width, uniquely
defining the child union's right edge. Half the measured updates therefore
shrink that union. Scopes are plain, clip, 0.95 scale, 0.7 opacity, 2 px foreground
blur, retained hidden-overflow scroll, and retained scroll with scrollbars.
Explicit scroll metrics have a 100 px range and 5.5 px offset. Timings cover
`update_with_dirty_nodes` only, excluding dirty classification, layout and raster.
Timer quantization is visible; these are preparation timings, not live frame
or launcher first-open results.

| Scope | Siblings | Baseline preparation p50, ms | Candidate preparation p50, ms |
| --- | ---: | ---: | ---: |
| `plain` | 16 | 0.004749–0.008311 | 0.004750–0.005936 |
| `plain` | 1,024 | 0.040368–0.043931 | 0.010686–0.016622 |
| `clip` | 16 | 0.004749–0.005937 | 0.004749–0.005936 |
| `clip` | 1,024 | 0.041555–0.047492 | 0.010686–0.011873 |
| `scale` | 16 | 0.004749–0.007123 | 0.004749–0.005936 |
| `scale` | 1,024 | 0.041555–0.045118 | 0.010686–0.013061 |
| `opacity` | 16 | 0.005936–0.007124 | 0.005936–0.007124 |
| `opacity` | 1,024 | 0.039181–0.043930 | 0.008311–0.009499 |
| `filter` | 16 | 0.005936–0.005937 | 0.004750–0.005936 |
| `filter` | 1,024 | 0.039181–0.040369 | 0.008311–0.009499 |
| `scroll` | 16 | 0.004750–0.005936 | 0.004749–0.007123 |
| `scroll` | 1,024 | 0.007124–0.008311 | 0.008311–0.009498 |
| `scrollbar` | 16 | 0.004750–0.005936 | 0.004749–0.005936 |
| `scrollbar` | 1,024 | 0.008311–0.009498 | 0.008311–0.009499 |

For plain/clip/scale/opacity/filter shrink updates, ancestor visits are
**18 → 2** at 16 siblings and **1,026 → 2** at 1,024 siblings; display-build
visits remain **1** and display-entry visits **0**. Bounds-index visits are
**5 / 11**, respectively: logarithmic branch repair rather than a sibling scan.
Retained scroll already bounded ancestor visits at two; its index bookkeeping
adds roughly 1–2 µs in the large fixture, with no scroll speedup claimed.
Flat span copying remains visible in the large ordinary cases.

**Validation.** The existing
`geometry_command_patches_skip_siblings_and_match_full_build` performance gate
(already run by `tools/check-performance`) now requires exactly three ancestor
visits at both 16 and 1,024 siblings for every tested scope and update, including
shrinks. It also bounds index visits logarithmically. First/middle/last growth
and shrink compare complete commands, entries, subtree bounds, replay spans,
layer/filter metadata, full raster and partial-repaint pixels against full
rebuilds. The index regression covers tied extrema, empty bounds, nonmonotonic
IDs, all-axis growth/shrink and immutable prior snapshots at 1/3/16/1,024 leaves.
Release baseline renderer suite: 341 passed, zero failures; candidate: 342
passed, zero failures. Release GPU-feature shell suite: **1,034 passed**,
37 ignored, zero failures. Formatting and diff checks pass.

**Evidence/reproduction.** `/tmp/mesh-coverage-evidence/` contains saved baseline
and candidate executables, `bench.py`, `timings.json`, six raw benchmark process
logs, the summary and build/test logs. Run
`cargo test --release -p mesh-core-render --lib shrinking_display_coverage_profile -- --ignored --nocapture --test-threads=1`
inside `nix develop`. No live compositor or hardware replay is claimed.

## 2026-10-26 — persistent replay spans and indexed scrollbar repairs

Baseline `0d2943a0`. This completes the P2 retained span-array item; P3/D1 stays
deferred pending runtime write observation. Reviewed the rejected-experiments
table and the previous display-scope and shrinking-coverage records first.

**Implementation.** Retained subtrees and frame plans now share a persistent
`PaintSpanSequence`. Leaves contain at most 16 spans; branches contain at most
16 child links. Links retain command offsets, so rebuilding an ancestor reuses
clean child span storage without shifting/copying every descendant entry.
Construction moves child links into the balanced tree without redundant Arc
increments. Leading spans, child branches and trailing scrollbar spans are in
command order. Sparse coverage repair uses retained command positions to look
up exact owner ranges, including trailing scrollbars, and copies only touched
leaves and branch metadata. Diagnostic `as_slice()` materializes only on demand;
preparation and replay use branch storage.

The parity gate exposed stale published metadata: sparse updates repaired the
retained spans but left the frame plan's replay spans at the prior generation.
The sparse publication path now shares the repaired sequence and refreshes
changed filter-layer regions. The full publication path also shares spans
instead of converting the complete retained span array. Commands, damage and
published replay metadata now advance together.

**A/B method.** Normal release profile (thin LTO, one codegen unit), builds/tests
inside `nix develop`, `CARGO_BUILD_JOBS=4`, `/tmp/mesh-p2-target`. The saved baseline
adds only `retained_display_span_profile` and its timing output to the unchanged
runtime. Three alternating baseline/candidate process pairs pinned to CPU 2;
no rustc process is active during timings. Each process runs three rounds of
100 measured updates after three initialization updates. Ranges below cover
nine round samples per side. The desktop remains active; timer quantization
and run-to-run noise are visible in microsecond samples.

Fixture: 16 or 1,024 direct box children in a 320×1100 surface. Clean children
are 80×1 px; the middle child alternates 160↔120 px width. Scopes are plain,
retained scroll with scrollbars, and an unretained scroll owner with 0.95 scale
and scrollbars. Explicit scroll metrics have a 100 px range and 5.5 px offset.
The timed interval is `update_with_dirty_nodes`, excluding dirty classification,
layout, replay selection and raster. These are display preparation timings,
not whole-frame, live GPU or launcher latency results.

| Scope | Siblings | Baseline mean, ms | Candidate mean, ms | Baseline p50, ms | Candidate p50, ms | Baseline p90, ms | Candidate p90, ms |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `plain` | 16 | 0.004797–0.005165 | 0.005189–0.007943 | 0.004749–0.004749 | 0.004749–0.008311 | 0.005936–0.005937 | 0.005937–0.008311 |
| `plain` | 1,024 | 0.011291–0.014722 | 0.009035–0.010508 | 0.010686–0.013060 | 0.008311–0.009499 | 0.011873–0.022559 | 0.009499–0.018996 |
| `scrollbar` | 16 | 0.005390–0.006851 | 0.005224–0.006589 | 0.004750–0.007124 | 0.004749–0.005936 | 0.005937–0.007124 | 0.005937–0.008311 |
| `scrollbar` | 1,024 | 0.008857–0.010104 | 0.008762–0.011766 | 0.008311–0.009498 | 0.008311–0.010686 | 0.009498–0.013060 | 0.009499–0.018996 |
| `unretained-scrollbar` | 16 | 0.004975–0.007088 | 0.005272–0.007622 | 0.004749–0.004750 | 0.004749–0.007124 | 0.005936–0.014247 | 0.005937–0.015435 |
| `unretained-scrollbar` | 1,024 | 0.013262–0.018142 | 0.009617–0.013559 | 0.011873–0.015435 | 0.009498–0.013060 | 0.015435–0.024933 | 0.010686–0.020184 |

The measured plain and unretained-scrollbar sparse frames copy exactly **2 span
entries** at both sizes; retained scroll copies **1**. Lookup visits are **4 / 5**
for plain, **3 / 5** for unretained scrollbars, and **2 / 2** for retained scroll.
Build visits remain one and entry visits zero. The baseline has no new span
counters; its full retained array copy and scrollbar scan are identified by the
old code, rather than presented as instrumented baseline counts. No speedup is
claimed for retained-scroll sparse updates or the small fixtures.

**Conservative controls and limits.** The harness also forces the existing
conservative path with a material-dirty summary, leaving live trees and dirty
IDs identical. Those updates still rebuild commands and traverse child lists;
this change does not claim proportionality for that path.

| Scope | Siblings | Baseline conservative p50, ms | Candidate conservative p50, ms |
| --- | ---: | ---: | ---: |
| `plain` | 16 | 0.010686–0.014248 | 0.010686–0.016623 |
| `plain` | 1,024 | 0.479668–0.560405 | 0.478481–0.522411 |
| `scrollbar` | 16 | 0.018997–0.028495 | 0.018997–0.020184 |
| `scrollbar` | 1,024 | 1.146931–1.485310 | 1.127934–1.355896 |
| `unretained-scrollbar` | 16 | 0.009499–0.010686 | 0.010685–0.014248 |
| `unretained-scrollbar` | 1,024 | 0.478482–0.544970 | 0.506977–0.593649 |

Large plain and retained-scroll control ranges overlap. Unretained-scrollbar
controls remain slower in some runs (paired process median p50 changes: about
+9%, +16%, then unchanged); no universal preparation improvement is claimed.
The metadata allocation/reference-count tradeoff remains visible on this
conservative workload. Replay selection still scans replay spans;
opaque retained-scroll replay remains a separate backlog item.

**Measured iterations.** A binary branch tree passed parity but made conservative
rebuild controls slower: the large plain p50 range was 0.529536–0.752749 ms vs
0.490356–0.530723 ms baseline. Switching to 16-way branches removed most branch
allocations; moving child links rather than cloning their references then
brought the plain control into the final overlapping ranges above. Raw sample
JSON, summaries and executables for both intermediate candidates are retained
under `binary-branches/` and `multiway-initial/` in the evidence directory.
The initial flat chunking approach was replaced before benchmarking so ancestor
rebuilds could share clean spans too.

**Validation.** Release baseline renderer: **342 passed**, 16 ignored, zero
failures; final renderer: **343 passed**, 16 ignored, zero failures. The expanded
`geometry_command_patches_skip_siblings_and_match_full_build` gate covers
16/1,024 siblings, first/middle/last growth, shrink and repeated moves, plain,
clip, scale, opacity/filter layers, retained scroll/scrollbar and unretained
scrollbar scopes. It compares commands, entries, subtree bounds, replay spans,
published frame spans/filter regions, full raster and partial-repaint pixels
against full rebuilds. It requires bounded span-copy/lookup work, ordered spans,
shared publication storage and immutable old snapshots. The new
`offset_span_branches_preserve_snapshots_and_bound_repairs` gate requires exactly
one copied leaf entry at both sizes, logarithmic lookup visits, correct nested
offsets and distinct leading/trailing owner spans; it is in
`tools/check-performance` alongside the existing geometry gate.
Release GPU-feature shell suite: **1,034 passed**, 37 ignored, zero failures.
Formatting, shell-script syntax and diff checks pass.

**Evidence/reproduction.** `/tmp/mesh-span-evidence/` contains baseline/candidate
executables, `bench.py`, `timings.json` (216 rows including controls), six process
logs, summary and build/test logs. Run
`cargo test --release -p mesh-core-render --lib retained_display_span_profile -- --ignored --nocapture --test-threads=1`
inside `nix develop`. No live compositor or hardware replay is claimed.

## 2026-10-26 — launcher first-open attribution: cold XDG icon lookup

Baseline `9122f5e3` plus profiling-only child preparation probes. This completes
attribution of the earlier 305–490 ms live launcher delay; the identified
optimization is a separate backlog item. No speedup is claimed in this entry.

**Finding.** The first child paint spends about **95–98%** of its synchronous
preparation in icon commands. Both CPU and GPU paths take roughly **250–269 ms**
there; later first-frame icon work is **0.135–0.395 ms**. The narrow icon raster
counter accounts for only **1.298–3.627 ms** in cold samples. Canvas creation,
backdrop context and display-list construction do not explain the long first
open. Resource lookup inside icon commands is the dominant cold path.

A separate CPU stack capture, correlated to the cold child's replay interval,
contains **24 of 29 UI-thread user-CPU samples** under
`mesh_core_icon::xdg::find_icon_in_theme_with_catalog` / named-icon resolution.
Twenty include `icon::theme::Theme::find_icon_here`; thirteen include filesystem
metadata calls, and two include theme-index parsing/search construction. These
are inclusive overlapping sample counts, not additive wall-time percentages.
`statx` is the most frequent leaf frame (11 samples). Kernel/off-CPU time is not
represented by those user-only samples.

The code explains why the async file-raster work did not remove this stall:
`resolve_icon_for_render` first calls `resolve_icon_for_module` synchronously.
An uncached semantic lookup reaches `find_icon_in_theme_with_catalog`, which
constructs `IconSearch::new_from(...).search().icons()` and searches the theme
filesystem before returning a target. The render-time async resolution queue
is only reached after that immediate lookup returns `Missing`. Per-query caches
make reopens cheap but do not prepare previously unseen names/sizes before the
first child frame. Moving cold semantic/XDG resolution off paint, or publishing
a prepared theme index/targets, must preserve icon fallback correctness and
consumer repaint subscriptions; no behavior change is implemented here.

**Environment and method.** Normal release CLI build, default GPU feature,
thin LTO / one codegen unit, `CARGO_BUILD_JOBS=4`, built in `nix develop`.
Binary SHA-256: `960960da0358fa9382b31df07c9a8cee9faba18c50a16e16a9fa84c5ef1cc321`.
Main Hyprland `wayland-1`, 2880×1800 at scale 1 / 120 Hz, AMD device `0xe281`,
dmabuf format `0x34324241`, modifier `0xffffffffffffff`. The desktop remains
active with quickshell present. No rustc process is observed in any selected
capture (sampled every 0.5 seconds); runs are not CPU-pinned.

A private copy of the historical glass profile/settings/module graph preserves
the earlier fixture's roots, icon/font preferences and wallpaper. Its module
path points to the current checkout. Config, home, state and IPC socket paths
are private to the probe; checkout preferences are not changed. Hidden-layer
prewarming is left enabled and boot settles for six seconds before profiling
is enabled. OS caches are shared/warm; "first" means first open in a fresh
process. Each launcher popup requests 596×512 logical pixels. Its first replay
has 42 primitive, 17 text, 17 icon, one control and one scrollbar command.

Three alternating CPU/GPU process pairs use the same executable with
`MESH_RENDER_GPU=0` or `1`. Runtime logs confirm CPU/SHM or the real GPU path,
with no GPU fallback. Each process gets four native Wayland virtual-pointer
clicks on the launcher trigger, with an outside click and 0.8 s dismissal wait
between opens. The endpoint is the first non-null buffer attachment followed
by `wl_surface.commit` for that newly created popup; the start is the logged
native pointer press. This includes asynchronous GPU completion up to buffer
commit, not compositor presentation feedback/physical scanout. A local pipe
collector timestamps protocol receipts, adding small collector/pipe overhead.

| Target/mode | First open, 3 processes | Later opens, 9 samples |
| --- | ---: | ---: |
| CPU/SHM, profiling enabled | 356.355–384.859 ms | 56.658–86.966 ms |
| GPU, profiling enabled | 311.699–345.658 ms | 35.688–85.145 ms |

**First child preparation breakdown** (three samples per mode; nested rows must
not be added to their parent):

| Operation | CPU, ms | GPU, ms |
| --- | ---: | ---: |
| Total synchronous child preparation | 262.351–273.701 | 259.120–277.844 |
| Canvas setup | 0.823–1.036 | 0.001–0.002 |
| Backdrop context | 0.002–0.003 | 0.002–0.003 |
| Component child paint | 261.172–272.305 | 258.859–277.566 |
| Root snapshot | 0.013–0.024 | 0.009–0.023 |
| Display list | 1.822–2.375 | 1.777–3.358 |
| Command selection | 0.002–0.002 | 0.002–0.007 |
| Canvas session | 0.001–0.002 | 0.001–0.001 |
| Command replay/recording | 259.279–269.848 | 257.020–274.109 |
| Icon commands (subset) | 249.975–259.866 | 252.703–268.947 |
| Text commands (subset) | 3.232–4.287 | 3.378–3.946 |
| Icon raster counter (subset) | 1.298–3.627 | 1.484–2.850 |
| Canvas finalization | 0.004–0.005 | 0.002–0.002 |
| Damage/blur metadata | 0.014–0.018 | 0.014–0.015 |

The first child timer can describe a preparation attempt before its first
successful buffer commit; the commit latency above is the user-visible
pipeline endpoint. Input handling, parent preparation, configure callbacks and
GPU completion account for the remainder outside this child timer. This entry
does not attribute that smaller remainder to one cause.

**Instrumentation check.** Repeated native-click captures with built-in
profiling disabled still reproduce the stall: CPU first open **314.704–340.105
ms** (2 fresh processes), GPU **307.211–361.935 ms** (3 fresh processes). Later
opens are **65.685–99.011 ms** (6 CPU samples) and **38.518–72.827 ms** (9 GPU
samples). These are separate runs, not paired instrumentation-overhead or
speedup estimates. A CPU control whose popup was already created before the
marked click is excluded. The separate stack capture is also excluded from
latency ranges; it samples the actual cold child replay window. The startup
pilot with invalid module-path syntax is not a measurement.

**Observability and validation.** Profiling-enabled runtime samples now split
child canvas setup, backdrop context, component paint and damage/blur work.
Child paint logs split snapshot/display-list/selection/session/replay/finalize,
plus command classes and nested text/raster counters. All timing/attribution
collection is gated by built-in profiling; normal painting keeps that work off.
The profiler guide documents the nested scopes and commit boundary. Live CPU
and GPU runs complete all four opens and orderly shutdown. Release GPU-feature
shell suite: **1,034 passed**, 37 ignored, zero failures. Formatting and diff
checks pass on the changed files. No renderer, scheduling or icon-resolution behavior is fixed
or benchmarked as an optimization in this commit.

**Evidence/reproduction.** Ignored `profiles/launcher-attribution/` contains
`capture.py`, `profile_extra.py`, `analyze.py`, `analyze_stacks.py`, private
config/state, instrumented and pre-probe binaries, metadata, `results.json`,
`unprofiled-results.json`, 12 process logs/JSONL files plus marks/load records,
the pilot, stack capture, raw `first-open.perf.data`, symbolized `stacks.txt`,
`stack-summary.json`, build/test logs and summaries. Native pointer injection
uses `wlrctl`; stack sampling uses `perf record -e cpu-clock -F 499 -k
CLOCK_REALTIME --call-graph dwarf -p <probe-pid>`. The source inspection points
are `render/child.rs`, `shell_component/mod.rs`, `surface/icon.rs`,
`ui/icon/src/registry.rs`, and `ui/icon/src/xdg.rs`.

## 2026-10-26 — launcher cold icon lookup leaves the paint thread

**Change.** Named-icon painting checks cached targets and prepared bindings
without filesystem access or waiting for the live registry mutex. Cold host
lookups run on bounded resource workers with shared immutable binding/catalog
snapshots; publication checks registry generation and resource revision. Pending
icons stay blank until their target or terminal missing fallback is ready, then
wake their subscribed consumers. Unprepared higher-priority candidates retain
precedence. XDG cache keys include captured catalog roots. Prepared font result
entries charge shared-target metadata rather than the entire font per glyph.
Synchronous APIs and the no-worker/no-future-completion availability fallback
remain available.

**Live comparison.** Normal release GPU-feature CLI, thin LTO, one codegen unit,
`CARGO_BUILD_JOBS=4`, on the main Hyprland `wayland-1` compositor (2880×1800,
scale 1, 120 Hz; AMD GPU device `0xe281`). A private copy of the attribution
fixture uses the glass profile and a 596×512 launcher. Its first replay has 42
primitive, 17 text, 17 icon, one control and one scrollbar commands. Three fresh
process pairs per mode run baseline then candidate; each process opens/closes
the launcher four times using native virtual-pointer clicks after startup
settles. CPU forces SHM (`MESH_RENDER_GPU=0`); GPU uses
`MESH_RENDER_GPU=1`. All 48 opens are valid; sampled rustc count is zero
throughout. The user left the pointer idle during capture.

| Mode / endpoint | Baseline first, 3 | Candidate first, 3 | Baseline later, 9 | Candidate later, 9 |
| --- | ---: | ---: | ---: | ---: |
| CPU input → first buffer commit, ms | 459.564–514.307 | 117.309–140.671 | 60.221–125.888 | 75.760–108.729 |
| GPU input → first buffer commit, ms | 482.192–507.992 | 57.929–69.839 | 62.399–80.300 | 44.820–80.934 |
| CPU synchronous child preparation, ms | 358.856–400.908 | 19.134–24.600 | 3.305–7.329 | 3.946–8.844 |
| GPU synchronous child preparation, ms | 427.386–448.893 | 10.133–11.062 | 3.905–6.242 | 2.774–6.442 |
| CPU icon commands, ms (nested) | 340.347–378.198 | 0.407–0.560 | 0.153–0.336 | 0.204–0.419 |
| GPU icon commands, ms (nested) | 418.843–439.554 | 0.357–0.419 | 0.354–0.534 | 0.186–0.457 |

Both sides enable the same built-in profiling and Wayland protocol capture.
The endpoint is the newly created popup's first non-null buffer attachment and
following `wl_surface.commit`, timed from native pointer press; it includes GPU
completion up to commit, not physical presentation. Collector/pipe receipt
adds small overhead. This measures initial UI responsiveness: asynchronous
icons can arrive after the first commit. No separate uninstrumented speedup is
claimed, and the overlapping later-open ranges establish no warm-path gain.
The excluded CPU pilot missed the launcher while the pointer was moving.

**Validation.** Release renderer: **343 passed**, 16 ignored; release
GPU-feature shell: **1,034 passed**, 37 ignored, including the corrected
resource-completion pixel oracle. Final icon suite: **35 passed**; production
icon integration suite: **5 passed**. The final release GPU-feature CLI builds
successfully; formatting, shell syntax and diff checks pass. The
production renderer tests exercise cold lookup, blank pending pixels, eventual
file pixels, consumer-only wakeups, separate catalog identities, terminal
missing settlement, a busy committed-registry mutex, prepared Material glyphs
and relative file names. The Settings scroll pixel oracle now delivers resource
completion through the runtime repaint path before comparing retained and full
frames: the first broad run caught a reference-capture race (156 pixels), while
the isolated and broad reruns passed before that fixture correction. Pixel
equality and the minimum shifted-frame assertion remain unchanged.

**Evidence.** Ignored `profiles/launcher-fix/` holds `compare.py`,
`analyze_compare.py`, private fixture/config/state, all 12 logs and protocol
JSONL/marks/load records, `results.json`, `summary.txt`, build/test logs and
preserved binaries. Baseline is `7cfff4c6`, SHA-256
`960960da0358fa9382b31df07c9a8cee9faba18c50a16e16a9fa84c5ef1cc321`;
the as-measured candidate SHA-256 is
`fb470d2ca326ac93db7ebc9d89b9e8d5f7f94942626abbd5d9a79cccadcb7606`.
After capture, file-name compatibility, shared-font cache accounting and
redundant ready-delivery reservation fixes and live-registry warning deduplication
were added; they do not change the
measured cold host-name dispatch. The final binary is kept separately as
`candidate-final` (SHA-256
`7fafb453c708ba17febe296dd95d46626d8489bc43905ed49cc3163d4dc88c0f`).
Capture/analysis reuse the helpers in
`profiles/launcher-attribution/`; this is a new paired comparison, not a reuse
of the earlier attribution timing ranges. There is no automated latency budget
for this live launcher endpoint; `tools/check-performance` now includes the
production icon regression suite.

## 2026-10-26 — hiding surfaces unmap when their exit settles

**Change.** A hiding surface previously stayed mapped for its root's longest
opacity transition, which on violet-glass is the theme's default
`--animation-duration-medium` (280 ms), whether or not anything was still
animating. The component now reports `exit_settled` once it has painted its
`mesh-surface-exiting` styles with no transition or keyframe animation still
running, and the shell unmaps it then; the root duration remains the upper
bound. The glass dashboard and wallpaper picker scrims now exit with
`--animation-duration-fast`, like their panels; their 280 ms scrim fade was
the longest exit transition actually running.

**Measurement.** `glass_exit_settles_before_the_root_opacity_duration`
(release, GPU feature; the test paints every 8 ms against the real violet-glass
theme), three runs: dashboard 115.4–116.6 ms, picker 115.2–116.9 ms from
`set_surface_exiting(true)` to settled, against the previous fixed 280 ms
unmap delay. With the core change alone (scrim still 280 ms), the dashboard
settled at 309 ms in a debug run, so the bound still applied. Not measured
live; the compositor sees the unmap one loop turn after the settling paint.

Validation: release GPU-feature shell lib 1,036 passed / 37 ignored / 0 failed;
`hide_surface_unmaps_once_the_exit_settles_before_its_deadline` covers the
shell wake and unmap.

## 2026-10-26 — no tooltip overlay reserve when popups are available

**Change.** Every non-window layer surface was inflated by a tooltip overlay
reserve (200 px below the content, and up to 352 px of width) so inline
tooltips could paint outside the content box. Tooltips that leave the content
box already promote to `xdg_popup`s (the dock, whose bottom anchor drops the
reserve, relied on that). `surface_geometry_with_overlay_reserve` now takes
`popup_supported` and adds no reserve when the presentation backend supports
popups; compositors without `xdg_popup` keep it as the fallback.

**Measurement** (layer sizes the shell configured; nested Hyprland 0.56.2,
1280×800 headless output at scale 1, release, CPU/SHM, glass profile; one run
each, sizes are deterministic):

| surface | before (`61d390af`) | after | pixels |
| --- | --- | --- | --- |
| `@mesh/glass-bar` | 1272×248 | 1272×48 | −80.6% (315,456 → 61,056) |
| `@mesh/glass-widgets` | 1280×520 | 1280×320 | −38.5% (665,600 → 409,600) |
| `@mesh/glass-dock` | 1280×114 | 1280×114 | unchanged (bottom anchor) |

Each SHM slot and GPU target scales with those pixels (bar: 1.26 MB → 0.24 MB
per 4-byte buffer at scale 1, times the slot count and scale²), as do
full-buffer clears/presents and the compositor's blending of the transparent
area. Steady-state damage is unchanged: content damage never covered the
reserve, except while an inline tooltip showed. No frame-time claim was
measured.

Live check: hovering the bar clock shows its tooltip as a child popup
(`::mesh-tooltip` child surface) below the 48 px bar; the calendar still opens,
closes and reopens on three clicks without pointer motion.

Validation: shell lib 1,036 passed / 37 ignored (new
`no_reserve_when_tooltips_can_be_popups`; the settings mount test now expects
its 700 px layer instead of 900 px).


## 2026-10-27 — Settings touchpad scrolling: docked versus window role

area: Settings scroll attribution; no rendering optimization or speedup claim

After the surface-state correctness fix, release, CPU raster, stage profiling
disabled, `d9_settings_scroll_frame_gate` with `MESH_SCROLL_GLASS=1`. Three
interleaved docked/window pairs on the development desktop.
`MESH_SCROLL_WINDOWED=1` promotes the same component and binds paint to the
920×900 extent. The docked role retains its authored 920×700 panel and margin;
these are role comparisons with different visible extents, not an isolated
before/after optimization measurement. The Appearance fixture seeds 240 icon
themes and font families, displays bounded resource rows and theme cards, and
drives 60 touchpad finger frames plus 30 momentum frames. Retained plans report
168 nodes and 187 commands.

| Role | Frame p50 across runs | Frame p90 across runs | Passes / frames | Restyles after ownership |
| --- | --- | --- | --- | --- |
| Docked | 4.522–6.772 ms | 7.047–10.597 ms | 90 / 90 | 0 |
| Window | 6.182–9.891 ms | 8.450–14.131 ms | 90 / 90 | 0 |

Every run passes the gate's one-paint-per-frame and no-restyle assertions. No
display entries are rebuilt; commands rebuilt over the whole gesture are 324
for docked and 320–324 for window. The ranges overlap and the viewport differs;
these results do not diagnose or resolve the user's live GPU/presentation lag.
That follow-up stays in `docs/BACKLOG.md`. Logs are under
`/tmp/mesh-settings-scroll-{docked,window}-{1,2,3}.log`.

## 2026-10-27 — S04 shared CSS/value/keyframe lowering and reload reuse

The CSS parser already served themes and components; the remaining repeated
work was source lowering during validation/reload and duplicate value/stop
representations. `parse_stylesheet` now returns immutable shared lowerings from
a per-thread LRU bounded to 16 entries and 8 MiB of retained source/AST payload.
Source text and absolute source offset identify an entry; input is limited to
2 MiB and 64 nested delimiters. Reference classification is stored once per
lowered declaration. Components re-export that classification and both theme
and component stops use one generic keyframe representation, with runtime
projections retaining authored recipes or validated easing as appropriate.
The duplicate component keyframe-property check is removed; shared lowering
owns that validation. Existing typed token resolution and clone-detachment
semantics remain in place rather than adding another graph/cache authority.

**Measurement:** `nix develop`, Rust 1.98.1, workspace release profile (thin LTO,
one codegen unit), three repeated runs of
`cargo test --release -p mesh-core-theme --lib measurement -- --ignored --nocapture --test-threads=1`.
The control forces the current lowering implementation to parse each time;
the comparison warms one lowering then checks shared pointer identity on every
read. These compare uncached/cached lowerings, not old/new full shell reloads.
Each source has 30 control and 30 warmed reads per run. All seven tracked theme
packs are included (both mesh-default modes: eight CSS files), plus generated
1/10/100 KiB sources containing distinct simple selectors and two declarations.

| Source/workload | Uncached p50 | Shared p50 | Uncached p95 | Shared p95 | Retained source/AST payload |
| --- | --- | --- | --- | --- | --- |
| Eight shipped CSS files, 11,332–13,833 bytes | 506.989–632.846 µs | 1.188–2.375 µs | 533.111–979.546 µs | 2.374–2.375 µs | 76,793–81,770 bytes/file |
| Synthetic 1,062 bytes | 61.741–64.116 µs | 1.188 µs | 68.865–87.863 µs | 2.375 µs | 19,191 bytes |
| Synthetic 10,243 bytes | 1.292–1.371 ms | 1.188–2.375 µs | 1.321–1.776 ms | 2.375 µs | 177,091 bytes |
| Synthetic 102,446 bytes | 84.623–87.183 ms | 3.562 µs | 93.851–99.505 ms | 3.562–4.749 µs | 1,695,123 bytes |

Warmed batches execute zero parser/lowering passes versus 30 in each control
batch. Byte accounting uses string/vector capacities, excluding allocator
headers and cache administrative storage. Allocation counts were not measured.
The large synthetic cold parse remains expensive; this change removes repeated
lowering rather than claiming to optimize that cold path. No speed threshold
was added; `css_reload_reuse_measurement` is an ignored measurement workload.

`typed_token_dependency_measurement` compares recursive scalar alias resolution
with existing shared typed reads on cloned immutable themes. Matrix: 10/100/1,000
tokens; requested chains 1/5/20 (clipped to nine links for the ten-token graph);
10/50/200 synthetic consumers, each reading the same inherited alias 100 times.
Consumers are repeated readers, not rendered/layout trees. Representative
three-run elapsed ranges:

| Tokens / links / consumers / reads | Recursive control | Shared typed reads |
| --- | --- | --- |
| 10 / 1 / 10 / 1,000 | 306–334 µs | 94–103 µs |
| 100 / 5 / 50 / 5,000 | 4.065–4.131 ms | 263–277 µs |
| 1,000 / 20 / 200 / 20,000 | 86.038–88.902 ms | 1.068–1.081 ms |

Each warmed graph retains one alias entry and all measured reads hit it;
results preserve numeric type. Mutation detaches the resolution cache and the
unchanged clone retains its original typed result. These are parser/lookup
measurements, not end-to-end theme switching, layout, frame-gap or allocation
measurements. The rejected-experiments table and the October 13 D7 token-cache
record were reviewed; no rejected computed-style sharing approach was revived.

Validation: theme/component debug suites 48/109 passed, no failures; generated
selector/alias/keyframe projection parity and invalid-source tests pass, with
both entry-count and retained-byte eviction, exact offsets, invalid-edit reuse,
source limits, clone sharing and mutation detachment covered. Both release
measurement workloads passed three times. Shell theme tests: 32 passed, one
ignored; component interaction/animation tests: 47 passed, two ignored.

Final recovery verification: `package_recovery_restores_the_published_theme_change_document`
passed, proving committed detail cursors survive rollback and candidate cursors
become stale. The three transport regressions also passed after adding negative
cursor rejection. `git diff --check` is clean for this change.

## 2026-10-27 — Locale catalog baseline (partial S05 measurement)

Release `locale_catalog_workloads`, three consecutive runs, 30 samples each:
JSON parse plus compilation for 100 / 1,000 / 4,000 keys (2,691 / 27,891 /
114,891 bytes) had p50 ranges 0.025–0.045 / 0.335–0.359 / 1.440–1.478 ms
and p95 ranges 0.037–0.069 / 0.385–0.559 / 1.775–1.856 ms. Bulk module
projection with a two-locale chain at 4,000 keys took p50 0.590–0.595 ms
(one layer) or 0.874–0.933 ms (five overlapping layers); p95 ranges were
0.663–1.044 and 1.165–1.337 ms respectively. Preparing 4 / 40 / 400 catalog
sources, each reading the same warm 100-key file, took p50 0.158–0.165 /
1.532–1.591 / 16.454–19.375 ms over ten samples; p95 0.205–0.265 /
1.642–2.087 / 19.230–22.653 ms. No before/after improvement or gate is claimed.

The 2026-09 audit's proposed 10,000/100,000-key single catalogs exceed the
current 4,096-entry admission bound. Module translators now borrow their IDs,
and module projection traverses layers in precedence order instead of rebuilding
an all-key set. Allocation counts, per-component mount costs, more fallback
shapes and separate unchanged/one-file-edit reloads remain unmeasured; S05
stays open.

## 2026-10-27 — Locale translator and reload baseline

Release `locale_catalog_workloads`, three runs. Constructing a borrowed module
translator and resolving one key for 1,000 instances took p50 0.057–0.062 ms
and p95 0.064–0.071 ms, with 10 or 100 catalog keys (30 samples each).
Preparing 400 separate warm 100-key files took p50 17.378–19.578 ms and p95
20.685–23.448 ms; rewriting one file before each preparation took p50
18.580–19.168 ms and p95 20.988–25.299 ms (10 samples each; write excluded).
Four and 40 files took unchanged p50 0.163–0.165 / 1.596–1.704 ms and edited
p50 0.169–0.180 / 1.609–1.663 ms. The loader currently reparses every source
in either case. This supplements the preceding baseline; allocation counts,
peak memory and additional fallback shapes remain open. No optimization or
gate is claimed.

## 2026-10-27 — Locale workload process memory

Three release runs of the complete `locale_catalog_workloads` test process
under GNU `time` reported peak RSS of 9,924–10,060 KiB. This is a whole-process
high-water mark across the combined workload, not per-case allocation or peak
attribution; those measurements and additional fallback shapes remain open.

## 2026-10-27 — Locale allocation and fallback measurements

Release `locale_catalog_workloads`, three runs of 30 samples per case unless
noted. A synthetic 100-key projection over 10 fallback locales and 20
fully overlapping layers per locale took p50 0.268–0.277 ms and p95
0.285–0.439 ms; the harness also measures 2/5/10 fallbacks and 1/5/20 layers.
A 100,000-unique-key projection spread across 25 catalogs of 4,000 keys took
p50 54.047–55.664 ms and p95 65.123–67.189 ms (10 samples). Those synthetic
chains are stress shapes, not a claim about normal locale selection.

With the opt-in counting allocator, each 4,000-key parse/compile made 16,679
allocation/reallocation calls; a five-layer 4,000-key projection made 8,024,
and the 100,000-key projection made 200,032. Constructing and querying 1,000
borrowed translators made zero. Preparing 4/40/400 distinct 100-key catalog
files made 1,771/17,686/176,809 calls for both unchanged and one-edited cases.
Counts were stable at p50 and p95 across three runs. Timing baselines above
were measured without allocator instrumentation. GNU `time` reported whole-
process peak RSS 41,752–42,748 KiB for the expanded combined workload, not
per-case peaks. These measurements close S05's parsing and projection baseline;
full component-mount and per-case RSS attribution were not measured. No
performance improvement or gate is claimed.

## 2026-10-27 — S05 measurement scope clarification

The preceding entry closes S05-PERF-001 and S05-PERF-003's catalog projection
and reload measurements. S05-PERF-002 remains in the backlog: the borrowed
translator microbenchmark does not measure full component mounting.

## 2026-10-27 — Localized child-component mount baseline (S05-PERF-002)

Release `localized_component_mount_workload`, three consecutive runs, five
samples each: one / 100 / 1,000 sibling components, each resolving one key from
its owning module's 10- or 100-key English catalog. The root runtime and
catalog setup are excluded; the timed `build_tree` creates child runtimes,
executes their templates, and builds the tree. For 10 keys, per-run medians
were 0.578–1.197 / 52.024–112.936 / 1,164.502–1,176.720 ms; for 100 keys,
1.256–1.554 / 121.496–135.730 / 1,234.663–1,245.880 ms. These are complete
child-mount costs, not isolated translator costs. Samples were unpinned and the
first one-component sample included cold font setup; no speedup or gate is
claimed.

Three separate allocation-profiling builds/runs, five samples each, counted
thread-local allocation/reallocation calls during `build_tree`. At 1,000
components, the per-run medians were 4,976,212–4,976,239 calls and
1,078,710,888–1,079,152,608 allocated bytes with 10 keys, versus
5,789,132–5,789,232 calls and 1,124,364,088–1,126,016,520 bytes with 100 keys.
Those are cumulative allocated bytes, not resident or retained memory. The
translator-only benchmark above showed zero allocations; these figures include
all child runtime and tree setup and do not isolate the catalog's contribution.

## 2026-10-27 — Share per-module locale resolvers within a surface

Release `localized_component_mount_workload`: 1,000 sibling components each
resolve one key from a shared 100-key module catalog. Three runs with five
interleaved uncached/shared samples each had uncached per-run medians
1,183.351–1,207.838 ms and shared medians 1,141.926–1,162.812 ms; individual
sample ranges overlapped (uncached 1,164.731–1,251.901 ms; shared
1,129.312–1,188.303 ms). Timing was unpinned and shows a modest, noisy
improvement. Three separate allocation-instrumented runs of the same workload
reported 5,791,159–5,791,190 allocation calls and 1,125,062,240–1,125,569,400
cumulative allocated bytes uncached, versus 4,879,179–4,879,241 calls and
1,071,845,440–1,072,843,328 bytes shared. The checked gate is
`localized_mount_shared_resolver_allocation_gate`; cumulative allocated bytes
are not retained memory. Cache entries are capped at 128 per surface and
cleared when its locale/catalog snapshot changes. A test checks same-locale
catalog replacement and module separation. Other child-runtime and tree setup
still dominates mount cost; the broader attribution item stays open.

## 2026-10-28 — Desktop widget resize investigation

Area: glass widgets, component rebuild and CPU raster. A temporary release-mode
probe dragged the media resize handle over 100 one-pixel diagonal steps on a
1920×1000 test surface, painting after each move. In two runs with 99 tree
builds, input p50/p90 was 0.72–0.74/1.26–1.53 ms and CPU raster frame p50/p90
was 33.47–35.10/40.27–42.94 ms. Profiling attributed 23.37–24.03 ms per frame
to paint and 3.70–4.13 ms to tree build; layout was 0.29 ms in the second run.
A third run reached the persisted size limit and is not comparable. This is
an offline CPU test, not a live GPU or compositor measurement; no causal claim
about the observed desktop lag or performance improvement is established.
No gate or implementation change accompanies this investigation.

## 2026-10-28 — Bound filesystem wildcard matching and recheck CPU gates

Area: filesystem capability patterns. Recursive partition search is replaced by
constant-space iterative wildcard matching; retries advance the input, bounding
work polynomially. Literal path segments retain direct string equality.
Generated short Unicode patterns match the former recursive reference, and
32/128/512-length adversarial cases assert a comparison-count bound.

Release (thin LTO, one codegen unit), this development machine, CPU 2 pinned,
three runs of 100 synthetic 16-wildcard nonmatches: segment matching took
148.465–151.891 ms with the recursive reference versus 0.003562 ms iterative;
path matching took 72.252–77.016 ms versus 0.007124–0.008312 ms. The segment
pattern repeats `*a` sixteen times followed by `b`, against sixteen `a`s;
the path pattern uses sixteen `**/a` pairs followed by `b`, against sixteen
`a` segments. Desktop and concurrent builds were active. These are adversarial
measurements, not normal filesystem throughput. The capability release suite
passes 17 tests (two ignored); `filesystem_glob_nonmatches_have_polynomial_work`
is the structural gate.

Separately, checkpoint `36b012fe` passes the existing CPU raster gates in three
release runs pinned to CPU 2: `d9_glass_dashboard_open_p90_ms` 43.511–47.044 ms
(budget 100), `d9_glass_dock_frame_p90_ms` 9.611–9.941 ms (budget 16.7), clock
p90 4.354–5.003 ms. Dashboard uses twelve mount/service/first-raster samples at
1920×1080; dock uses ten pinned and 150 installed apps, 2880×180, 53–55 frames.
These are current baselines, not improvements attributed to this change.
The stale dock/dashboard failure items leave the backlog. A grain-interior
renderer candidate was discarded before performance claims because the current
material has no grain; no renderer optimization is shipped here.

## 2026-10-28 — Attribute missing-instance storage recovery allocations

Area: localized component mounting and scoped storage. Sampling the combined
mount workload attributed about 31% to opening storage and 27.5% to missing-file
recovery (inclusive samples, not isolated phase timings). Recovery previously
constructed a full path and lossy filename for every sibling before rejecting
unrelated entries. It now checks the filename bytes first and only constructs
paths for matching temporary recovery candidates. Recovery order, revision
selection and validation are unchanged. Fifteen scripting storage tests pass,
including unrelated newer temporary files and non-UTF-8 sibling filenames.

Three paired allocation-count runs opening 1,000 missing frontend instance
stores beside 1,000 sibling lock files counted exactly 4,055,900 calls and
766,875,900 cumulative bytes before versus 2,055,900 calls and 178,875,900 bytes
after. This isolated harness uses the actual storage filesystem implementation
with the unused Luau adapter omitted, optimized rustc with debug dependency
rlibs, and identical allocator instrumentation on both sides. These are
allocation measurements, not release latency or retained-memory claims.
The debug shell `allocation-profiling` gate
`missing_instance_storage_dense_directory_allocation_gate` passes three fresh
processes at 2,056,900 calls / 178,560,900 cumulative bytes; minor path-length
variation changes byte totals. Its 2.5-million-call threshold excludes the
old loop. Broader runtime and tree setup costs remain open in the backlog.

## 2026-10-28 — Live widget resize GPU replay

Added `live_desktop_widget_resize`: isolated shell, fixed service data, 100
one-pixel diagonal moves at intervals of at least 16 ms, with handle and card
geometry assertions. Release with GPU feature, current development desktop,
Hyprland 0.56.2-b: one successful run recorded 92 tree rebuilds, 70 GPU commits,
and 34.477 ms inter-commit p90. Widget render p90 was 11.270 ms; mean tree build
2.710 ms, layout 0.271 ms and scene paint preparation 0.830 ms. This is a single
baseline, not an improvement measurement. The replay namespace does not match
the installed hyprglass rule, so hyprglass cost and physical display latency
remain unmeasured and the live-resize investigation stays open. A first run
outside the Nix environment used CPU/SHM because libEGL was unavailable; it
must not be interpreted as GPU evidence. Running through `nix develop` enabled
the successful GPU path. The profiling guide documents the command and limits.

## 2026-10-28 — Integration verification after storage recovery change

Current debug shell component memo suite passes 16 tests (two ignored), and
interaction/animation passes 48 (two ignored). The Python performance-gate
suite passes six tests and profile-report passes one. All pending planning
notes remain linked from the backlog. The live-resize and broader component
mount items remain open; this work fixes a measured allocation hotspot and an
animation geometry defect rather than claiming those whole areas are complete.

Machine for the preceding October 28 verification records: x86_64 AMD Ryzen 7
7840HS with Radeon 780M Graphics, 8 cores / 16 threads, boost enabled.

Supplementary whole-mount verification: three alternating release processes per
side, fresh `XDG_STATE_HOME` for each process, five interleaved resolver modes
per case, 1/100/1,000 siblings and 10/100 catalog keys. Shared-resolver 1,000-child,
100-key per-run medians were 1,188.455–1,269.459 ms before versus
1,036.672–1,259.845 ms after; sample ranges were 1,152.025–1,308.750 versus
1,017.680–2,138.091 ms. Unpinned with concurrent compilation/desktop activity;
the before binary was CPU-only and the after binary GPU-enabled, although this
workload only builds trees. The overlap and feature difference preclude a
whole-mount latency improvement claim. All six workload processes passed.
