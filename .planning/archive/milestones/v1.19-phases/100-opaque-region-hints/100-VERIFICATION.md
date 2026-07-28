# Phase 100: Opaque Region Hints — VERIFICATION

**Date:** 2026-06-09
**Status:** PASS

## Build

- `cargo check --workspace --all-features` — **PASS** (zero errors)
- `cargo check -p mesh-core-presentation` — **PASS**
- `cargo check -p mesh-core-shell` — **PASS**
- `cargo check -p mesh-core-frontend-host` — **PASS**

## Tests

- `cargo test -p mesh-core-presentation` — **8/8 passed** (zero failures)
- `cargo test -p mesh-core-shell` — 2 pre-existing failures (unrelated: `Color::from_rgba8` removed, `ProfilingInvalidationSnapshot` fields added before this phase)

## Clippy

- `cargo clippy -p mesh-core-presentation -p mesh-core-shell -p mesh-core-frontend-host` — **zero new warnings** (60 pre-existing warnings in mesh-core-shell, none from this phase)

## Acceptance Criteria

### Plan 100-01 (PresentationEngine API + wl_region lifecycle)

| Criterion | Status | Notes |
|-----------|--------|-------|
| `PresentationEngine::update_opaque_region` exists | ✓ | `crates/core/presentation/src/lib.rs:136` |
| Delegates to `LayerShellBackend` for Wayland | ✓ | `if let Backend::WaylandSurface(bridge)` dispatches to `bridge.update_opaque_region()` |
| DevWindow no-ops | ✓ | `if let` arm silently skips for `Backend::DevWindow` |
| `LayerShellBackend::update_opaque_region` exists | ✓ | `crates/core/presentation/src/wayland_surface/backend.rs:703` |
| `set_opaque_region(Some(...))` sent | ✓ | After creating region and adding rect |
| `set_opaque_region(None)` sent for clear case | ✓ | Both `opaque_rect is None` and `rect.width/height == 0` paths |
| wl_region destroyed per present | ✓ | Smithay `Region` wrapper auto-destroys on `Drop` (verified in smithay-client-toolkit 0.19.2 compositor.rs) |
| Unknown surface silently skipped | ✓ | `let Some(entry) = self.state.surfaces.get(surface_id) else { return; }` |

### Plan 100-02 (Shell-side opaque rect computation + loop integration)

| Criterion | Status | Notes |
|-----------|--------|-------|
| `display_list_paint_commands()` accessor | ✓ | `crates/core/shell/src/shell/component/shell_component.rs:956` |
| `DisplayPaintCommand` imported | ✓ | In `component.rs` mesh_core_render import |
| Default trait method on `ShellComponent` | ✓ | `crates/core/frontend/host/src/lib.rs:305` returns `&[]` |
| `compute_opaque_rect_for_root` exists | ✓ | `crates/core/shell/src/shell/runtime/render.rs:351` |
| Guard: `background_color.a != 255` | ✓ | Exact equality to 255 (PITFALLS.md Pitfall 3) |
| Guard: `BackgroundPaint::None` | ✓ | Skips Image/LinearGradient (D-02) |
| Guard: `border_radius > 0.0` | ✓ | f32 comparison (D-03) |
| Guard: `overflow clips_contents` | ✓ | Both overflow_x and overflow_y must clip (D-03) |
| `update_opaque_region` wired in render loop | ✓ | Called between paint and present, guarded by `if visible` |
| Uses `known_surface_size` for dimensions | ✓ | Falls back silently if None |

## Guard Conditions (per CONTEXT.md D-02/D-03)

All four guard conditions verified present in `compute_opaque_rect_for_root`:

1. **`background_color.a != 255`** — returns `None` (alpha threshold exactly 255, per PITFALLS.md Pitfall 3)
2. **`background_paint != BackgroundPaint::None`** — returns `None` (solid color fill only, no images/gradients)
3. **`border_radius > 0.0`** — returns `None` (rounded corners invalidate rectangle-based opaque region)
4. **`!overflow_x.clips_contents() \|\| !overflow_y.clips_contents()`** — returns `None` (both axes must be `Hidden`)

## Files Modified

| File | Changes |
|------|---------|
| `crates/core/presentation/src/lib.rs` | Added `PresentationEngine::update_opaque_region()` dispatch method |
| `crates/core/presentation/src/wayland_surface/mod.rs` | Imported `Region` from smithay-client-toolkit compositor |
| `crates/core/presentation/src/wayland_surface/backend.rs` | Added `LayerShellBackend::update_opaque_region()` with wl_region create→add→set→destroy lifecycle |
| `crates/core/frontend/host/src/lib.rs` | Added `display_list_paint_commands` default method to `ShellComponent` trait |
| `crates/core/shell/src/shell/component.rs` | Added `DisplayPaintCommand` to mesh_core_render imports |
| `crates/core/shell/src/shell/component/shell_component.rs` | Added `display_list_paint_commands()` accessor to `FrontendSurfaceComponent` |
| `crates/core/shell/src/shell/runtime/render.rs` | Added `compute_opaque_rect_for_root()` and wired `update_opaque_region` into render loop |

## wl_region Lifecycle

Per PITFALLS.md Pitfall 2: wl_region created via `Region::new()`, add rect via `region.add()`, set via `wl_surface.set_opaque_region(Some(region.wl_region()))`, then the `Region` wrapper auto-destroys on `Drop` (verified: smithay-client-toolkit 0.19.2 `compositor.rs:465-468`). No explicit `destroy()` call needed — the Rust Drop semantics handle this per frame.

## Requirements Satisfied

- **OPAQUE-01**: Walk retained display list for fully-opaque root background rects, compute union as wl_region, send wl_surface::set_opaque_region ✓
