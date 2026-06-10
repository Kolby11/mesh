# Phase 101: Per-Region Damage - Context

**Gathered:** 2026-06-10
**Status:** Ready for planning
**Mode:** Auto-generated (infrastructure phase)

<domain>
## Phase Boundary

Thread `Vec<DamageRect>` from the retained renderer output through the shell render path and into `wl_surface::damage_buffer` calls — replacing the single unioned bounding rect that currently covers the full surface on every frame commit.

**In scope:**
- Change `take_present_damage()` trait method to return `Vec<DamageRect>` instead of `Option<DamageRect>`
- Store per-region damage rects instead of a single merged rect in `FrontendSurfaceComponent`
- Change `PresentationEngine::present_with_damage()` to accept `Vec<DamageRect>`
- Loop in the Wayland backend calling `wl_surface.damage_buffer()` once per rect, capped at 16 rects per commit
- Add damage rect count to the debug overlay alongside existing damage metrics

**Out of scope:**
- Sub-rect damage within individual node rectangles
- GPU-level damage tracking
- Changes to layout or rendering logic

</domain>

<decisions>
## Implementation Decisions

### Claude's Discretion
All implementation choices are at Claude's discretion — pure infrastructure phase.

Key constraints from ROADMAP and requirements:
- Cap at 16 rects per commit (DMGE-02); when >16 rects exist, fall back to the full-surface unioned rect to bound protocol overhead
- `Vec<DamageRect>` flows from `effective_damage.rects` (already computed in `select_effective_damage_rects`) through `take_present_damage()` to the Wayland commit
- Empty `Vec` means no changed pixels → skip present entirely (current `None` → skip behavior must be preserved)
- Debug overlay damage rect count goes in profiling/debug section alongside existing damage area metrics (DMGE-03)

</decisions>

<code_context>
## Existing Code Insights

### Current Damage Flow
- `select_effective_damage_rects()` in `shell_component.rs` already produces `effective_damage.rects: Vec<DamageRect>`
- `effective_damage_scratch: Vec<DamageRect>` is already stored and reused as a scratch buffer
- `last_present_damage: Option<DamageRect>` is the current single-rect present damage state
- `take_present_damage()` returns `Option<DamageRect>` — defined in `crates/core/frontend/host/src/lib.rs:281`
- `merge_optional_damage()` currently merges paint damage into a single rect

### Presentation Layer
- `PresentationEngine::present_with_damage()` in `crates/core/presentation/src/lib.rs:120` takes `Option<DamageRect>`
- Wayland backend `present_with_damage()` in `crates/core/presentation/src/wayland_surface/backend.rs:637` calls `wl_surface.damage_buffer()` once
- `commit_damage()` helper in the backend at line ~256 does the single `damage_buffer()` call

### Render Dispatch
- `crates/core/shell/src/shell/runtime/render.rs:245` calls `take_present_damage()` and passes to `present_with_damage()`

### Integration Points
- `ShellComponent` trait in `crates/core/frontend/host/src/lib.rs` — change `take_present_damage` signature
- `FrontendSurfaceComponent` in `crates/core/shell/src/shell/component/shell_component.rs` — change storage
- `PresentationEngine` in `crates/core/presentation/src/lib.rs` — change API
- `WaylandSurfaceBackend` in `crates/core/presentation/src/wayland_surface/backend.rs` — change commit loop
- Debug overlay in `crates/core/foundation/debug/src/lib.rs` or render — add rect count field

</code_context>

<specifics>
## Specific Ideas

- When vec length > 16, fall back to the full-surface unioned DamageRect as a single element rather than truncating to the first 16 partial rects (which could leave stale pixels outside the first 16 rects)
- Empty vec = skip present (preserve existing skip behavior)
- Tests in `crates/core/shell/src/shell/component/tests/integration/real_surfaces.rs` call `take_present_damage().is_some()` — update these to check vec non-empty

</specifics>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope.

</deferred>
