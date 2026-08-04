---
status: resolved
trigger: "2026-08-03T22:59:19 MESH exited on KDE with `buffer allocation failed: all 3 SHM buffers are busy for 1920x201 surface`, while the navigation was outside the screen."
created: "2026-08-04T00:00:00+02:00"
updated: "2026-08-04T09:35:00+02:00"
---

# KDE Navigation SHM Buffers

## Symptoms

- Expected behavior: the top navigation bar remains within the KDE output and presents frames without exhausting its SHM pool.
- Actual behavior: the navigation is outside the screen and the shell exits after exhausting all three SHM buffers for a 1920x201 surface.
- Error messages: `buffer allocation failed: all 3 SHM buffers are busy for 1920x201 surface`.
- Timeline: observed immediately on 2026-08-04 local time; whether this KDE path previously worked is unknown.
- Reproduction: run the current MESH shell on KDE with a 1920-pixel-wide output until the navigation surface is configured and rendered.

## Current Focus

- hypothesis: the combined fix removes the stale 1920x201 configure and prevents compositor-owned SHM pool saturation from terminating MESH
- test: user runs the current tree on the original KDE 1920px output and confirms the navigation appears at the top edge and remains alive through repeated frames
- expecting: no 1920x201 configure/allocation error, navigation content occupies 1920x56 with its transparent reserve, and MESH does not exit if KDE temporarily retains all three buffers
- next_action: await human verification on KDE; if confirmed, archive the debug session and complete backlog/log/status tracking
- reasoning_checkpoint:
    hypothesis: "three compositor-owned SHM buffers cause process exit because temporary pool saturation is misclassified as allocation failure"
    confirming_evidence:
      - "the exact fatal branch is reached only after SlotPool::canvas returns None for every buffer; create_buffer is not called and has not failed"
      - "the 50ms callback timeout deliberately permits retry while frame_pending remains true, so all-busy is an expected reachable state on a throttled compositor"
      - "PresentStatus::NotReady already preserves pending present damage and requests another paint in the shell"
    falsification_test: "if the busy state represents an invalid protocol condition rather than temporary compositor ownership, or mapping it to NotReady loses pending damage, the proposed classification is wrong"
    fix_rationale: "returning NotReady expresses backpressure at the existing presentation boundary; it changes no allocation, pool-depth, damage, or release behavior and leaves genuine create_buffer errors fatal"
    blind_spots: "the live KDE release cadence cannot be exercised in this environment; verification covers the exact geometry sequence and the typed retry path rather than compositor timing"
- reasoning_checkpoint:
- tdd_checkpoint:

## Evidence

- timestamp: 2026-08-04T00:10:00+02:00
  checked: navigation module manifest and root component style
  found: the layer surface declares exclusive_zone 56 and the root .nav-shell declares height 56px; neither declares 201px
  implication: 201px is introduced after authoring, in measured/padded surface geometry or compositor configuration

- timestamp: 2026-08-04T00:11:00+02:00
  checked: SurfaceEntry::copy_into_shm_buffer error path
  found: the exact error is returned only when SlotPool::canvas reports every existing SHM buffer busy and the per-surface pool has already reached SHM_BUFFER_POOL_MAX (3)
  implication: the crash requires a fourth presentation attempt before any of three submitted buffers becomes reusable; 201px alone cannot produce the fatal error

- timestamp: 2026-08-04T00:16:00+02:00
  checked: shell render gate, SurfaceEntry frame callback state, and Wayland present result handling
  found: frame callbacks suppress rendering only for 50ms; after that the shell retries even while frame_pending remains true. If all three buffers are still compositor-owned, copy_into_shm_buffer has no backpressure result and returns fatal BufferAlloc. The shell already preserves pending damage and requests another paint for PresentStatus::NotReady.
  implication: a compositor that throttles callbacks/releases for an offscreen or occluded surface deterministically turns ordinary Wayland backpressure into process termination; a nonfatal busy result fits the existing retry contract

- timestamp: 2026-08-04T00:23:00+02:00
  checked: tooltip reserve calculation, render loop ordering, and prior investigation log
  found: tooltip_overlay_extra_for_content adds exactly 200px for any nonzero-height layer. On the first loop the dynamic navigation height falls back to 1, so configure is immediately called with 201 before paint; first_layer_configure only forces a second iteration and does not defer that first protocol configure. The second iteration can then send the measured 256px size.
  implication: the reported 1920x201 allocation is a precise fingerprint of the unmeasured first configure, not the authored 56px navigation geometry; two geometry configures create a KDE-sensitive stale-configure race

- timestamp: 2026-08-04T00:32:00+02:00
  checked: focused first-dynamic-layer configure-history regression
  found: before any behavior change the regression fails with actual history [(1920, 201), (1920, 256)] versus expected [(1920, 256)]
  implication: the stale KDE geometry request is directly reproduced and the primary hypothesis is confirmed

- timestamp: 2026-08-04T00:40:00+02:00
  checked: focused configure-history regression after deferring only the first unmeasured configure
  found: the regression passes with exactly one 1920x256 configure; the transient 1920x201 request is gone
  implication: the minimal ordering change directly removes the reproduced KDE geometry trigger without changing measured final geometry

- timestamp: 2026-08-04T00:47:00+02:00
  checked: full mesh-core-presentation library suite after mapping a saturated busy pool to NotReady
  found: 64 passed, 0 failed, 12 ignored; genuine buffer creation errors still use PresentationError while the saturated branch returns before attach/commit
  implication: the nonfatal backpressure classification compiles across the real Wayland backend and preserves all existing presentation behavior covered by the crate

- timestamp: 2026-08-04T00:52:00+02:00
  checked: complete shell surface-layout test group
  found: 12 passed, 0 failed, including the new KDE-shaped configure-history regression and existing reserve/input/configure retry coverage
  implication: deferring the first configure preserves adjacent layer-surface geometry and lifecycle behavior

- timestamp: 2026-08-04T00:55:00+02:00
  checked: full mesh-core-shell library suite and repository baseline
  found: 627 passed, 9 failed, 125 ignored; failures are the eight documented deterministic baseline failures plus the separately documented parallel-only phase26 raster-cache failure. No new failure is associated with surface configure, SHM presentation, or the changed files.
  implication: the fix introduces no detected regression beyond the repository's known red baseline; live KDE remains the only unavailable verification boundary

- timestamp: 2026-08-04T09:12:00+02:00
  checked: the live shell on Hyprland (the tree with the deferred configure), against `hyprctl layers` and a `grim` capture
  found: the navigation layer is 1920x201 and paints nothing; the single configure carries height=201 on the deferred second pass, so the 1920x201 fingerprint from the KDE report is reproduced here, on a second compositor, with the deferral in place
  implication: deferring the first configure did not remove the 201; the 201 is measured content of 1px plus the 200px reserve, and the earlier evidence read a symptom (configure ordering) as the cause

- timestamp: 2026-08-04T09:14:00+02:00
  checked: instrumented paint measurement for the shipped navigation bar (content extent, root/child styles, measured size)
  found: content extent 1x1 on the first pass and 1920x1 after, while the `.nav-shell` child carries `height: Px(56.0)` and lays out at height 1.0; measure_content_size therefore reports 1
  implication: the surface is not misconfigured — it is measured against a fabricated 1px box, and the collapse is what gets reported

- timestamp: 2026-08-04T09:20:00+02:00
  checked: isolated Taffy repro in mesh-core-elements (surface root Px(1)xPx(1), 56px child with children)
  found: a definite 1px root collapses the child to 1x1; the same tree with an `auto` root axis measures 56 regardless of available space
  implication: the fix is to lay an unknown axis out as `auto`, not to invent a viewport size for it

## Eliminated

- the 201px height as a compositor/KDE placement problem: reproduced identically on Hyprland
- first-configure ordering as the root cause: with the first configure deferred, the second configure still carried the collapsed 201

## Resolution

- root_cause: a surface's first frame has no size on either axis, and the shell handed `paint` a 1px stand-in that `finalize_tree` stamped onto the synthetic surface root as a definite box. A 56px bar laid out in a 1px surface measures 1px; `render_layout` sends that 1px on as the surface height, and a nonzero height is no longer dynamic, so the compositor's size is never consulted again — the bar is pinned at 1px of content inside a 1920x201 buffer (1 + the 200px tooltip reserve) for the life of the process. Repeated presents of that permanently-wrong surface are what saturated the three SHM buffers on KDE
- fix: `SurfaceExtent.content` carries `0` for an axis the shell has no size for, `paint` records those axes, and `finalize_tree` gives the surface root `Dimension::Auto` there instead of `Px(stand-in)`, so the first frame measures the content. Kept from the earlier pass: the deferred first unmeasured layer configure (one configure instead of two) and `PresentStatus::NotReady` rather than a fatal `BufferAlloc` when all three SHM buffers are compositor-owned
- verification: live on Hyprland the navigation layer is 1920x256 (56 content + 200 reserve) with the bar fully painted, from exactly one configure, and no busy-buffer error in 25s — against 1920x201 and an empty bar before. Regression `unmeasured_navigation_bar_measures_its_own_height_not_the_placeholder` paints the shipped bar with `SurfaceExtent::padded((0,0),(1,201))` and fails 1 vs 56 without the fix. mesh-core-shell 628 passed / 9 failed / 125 ignored — the documented eight plus the documented parallel-only phase26; presentation 64/64; elements 210/210. Live KDE re-run still worth doing, but the KDE fingerprint is reproduced and fixed on Hyprland
- files_changed:
    - crates/core/frontend/host/src/lib.rs
    - crates/core/shell/src/shell/component.rs
    - crates/core/shell/src/shell/component/rendering/mod.rs
    - crates/core/shell/src/shell/component/shell_component/mod.rs
    - crates/core/shell/src/shell/component/tests/integration/real_surfaces/layout.rs
    - crates/core/shell/src/shell/runtime/render/mod.rs
    - crates/core/shell/src/shell/tests/surface_layout.rs
    - crates/core/presentation/src/lib.rs
    - crates/core/presentation/src/wayland_surface/backend/entry.rs
    - crates/core/presentation/src/wayland_surface/backend/present.rs
