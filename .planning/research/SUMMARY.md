# Research Summary: MESH v1.20 Compositor Integration

**Project:** MESH v1.20 Compositor Integration
**Domain:** Wayland compositor protocol — HiDPI/fractional scale, blur offload, per-region damage
**Researched:** 2026-06-10
**Confidence:** HIGH (protocol mechanics and MESH codebase state); MEDIUM (blur compositor support matrix)

---

## Executive Summary

MESH v1.20 adds three Wayland compositor protocol integrations: HiDPI/fractional scale rendering, compositor-offloaded blur for `backdrop-filter`, and per-region damage commits. All three are additive and protocol-gated — each degrades gracefully to the current behavior when the compositor does not advertise the relevant global. None requires changes to the `.mesh` authoring surface or the Luau API.

The research consensus across all four files is that the correct build order is **damage first, HiDPI second, blur last**. Pitfalls research is definitive on the key dependency: blur region `wl_region` coordinates are in surface-local (logical) pixels and must be derived from `node.layout` rects that are correctly bounded by a known scale factor, which only exists after Phase 2. Per-region damage can be wired first because at scale 1.0 (the current state) logical == physical, so the coordinate migration is a no-op; damage rects become truly physical after Phase 2 automatically.

The most significant protocol naming issue in the milestone spec: `wp_blur_v1` does not exist in `wayland-protocols` 0.32 or any available crate. The correct protocol for production blur offload is `org_kde_kwin_blur` from `wayland-protocols-plasma 0.3`, which is what KWin and Hyprland actually ship. The `ext_background_effect_v1` staging protocol exists in `wayland-protocols` 0.32 but carries an explicit "testing phase" warning with no confirmed compositor implementation; it should be deferred to v1.21+. Implementation must proceed with `org_kde_kwin_blur` as the primary path.

---

## Recommended Build Order

### Phase 1: Per-Region Damage (lowest risk, do first)

**Rationale:** The retained renderer already produces `EffectiveDamage::rects: Vec<DamageRect>` internally. The only missing link is that `take_present_damage()` flattens this to a single `Option<DamageRect>` before it reaches `Shell::render_components`. At scale 1.0, damage rects are already correct for `damage_buffer`. Zero new crate dependencies. Worst-case regression is the current whole-surface damage behavior.

**Scope:** `shell_component.rs`, `component.rs` (trait), `render.rs` (shell), `lib.rs` (presentation), `backend.rs` (presentation).

**Key change:** `take_present_damage()` returns `Option<Vec<DamageRect>>`; `PresentationEngine::present_with_damage_rects` replaces the single-rect variant; `attach_shm_buffer` loops `damage_buffer` once per rect. Cap submission at 16–32 rects; fall back to bounding box for denser patterns.

**Top pitfall to avoid:** `SurfaceShmBuffer::pending_damage` must become `Vec<DamageRect>` — the union-across-slots triple-buffer logic must preserve the full rect list, not collapse it to a single bounding box.

---

### Phase 2: HiDPI / Fractional Scale (establishes scale authority, do second)

**Rationale:** Establishes `SurfaceEntry::scale_factor: f64` as the single authoritative scale source. Must come before blur because blur `wl_region` coordinates must be in surface-local (logical) pixels derived from `node.layout` rects — the scale boundary must be explicit for this to be correct. After Phase 2, `PixelBuffer` is allocated at physical pixels, damage rects are inherently physical-pixel-correct, and blur region derivation has a clean logical-pixel boundary to work from.

**Scope:** `Cargo.toml` (presentation), `state.rs`, `backend.rs`, `handlers.rs`, `lib.rs` (presentation), `render.rs` (shell).

**New dep (direct, already transitive):**
```toml
wayland-protocols = { version = "0.32", features = ["client", "staging"] }
```
No version conflict — the transitive copy is already 0.32.12.

**Key change:** `SurfaceEntry` gains `scale_factor: f64`, `fractional_scale: Option<WpFractionalScaleV1>`, `viewport: Option<WpViewport>`. `Shell::render_components` queries `surface_scale()` and multiplies logical dimensions before `PixelBuffer::new`. The existing `scale` parameter on `paint_frontend_tree_at_for_module` (currently hardcoded `1.0`) is wired to the real per-surface value.

**Top pitfalls to avoid:**
- `scale_factor_changed` in `CompositorHandler` is currently an empty no-op — implement it first.
- Never call both `set_buffer_scale` and `wp_viewport.set_destination` together — use viewporter only when fractional scale is active.
- Use `ceil` (not `round`) for buffer dimensions from fractional scale.
- `preferred_scale` event arrives before first configure per spec — store scale at bind time, not at configure time.
- `surface_enter`/`surface_leave` handlers are currently empty — update `SurfaceEntry::scale_factor` when a surface moves monitors.
- Verify `SlotPool` on-demand growth for 2x panels (1920×32 logical = 3840×64 physical = ~983 KB, larger than the 262 KB initial pool).

---

### Phase 3: Compositor Blur Offload (purely additive, do last)

**Rationale:** Reads existing `DisplayPaintStyle::backdrop_filter` data already computed in the display list. Must come last because blur `wl_region` must be in surface-local (logical) pixels derived from `node.layout` — this is only unambiguous after Phase 2 establishes the physical-pixel boundary. On compositors without `org_kde_kwin_blur` this phase is a complete no-op.

**Scope:** `Cargo.toml` (presentation, new dep), `state.rs`, `backend.rs`, `lib.rs` (presentation), `render.rs` (shell), `shell_component.rs` (new `blur_region_rect()` method).

**New dep (not currently in tree):**
```toml
wayland-protocols-plasma = { version = "0.3", features = ["client"] }
```
Resolves to the same `wayland-client 0.31.14` and `wayland-protocols 0.32.12` already in the lock file. Zero version conflicts.

**Protocol name correction — `wp_blur_v1` does not exist:** The milestone spec references `wp_blur_v1` but this name matches no protocol in `wayland-protocols` 0.32 or any available crate. The correct import path for production blur is:
```rust
use wayland_protocols_plasma::blur::client::{
    org_kde_kwin_blur::OrgKdeKwinBlur,
    org_kde_kwin_blur_manager::OrgKdeKwinBlurManager,
};
```
`ext_background_effect_v1` (the `wayland-protocols` staging candidate) carries an explicit "testing phase" warning and has no confirmed compositor implementation — defer to v1.21+.

**Top pitfalls to avoid:**
- `blur.commit()` is a separate call from `wl_surface::commit` — must be called after every `blur.set_region(...)`; without it, region changes are silently ignored by the compositor.
- CPU Skia blur must be suppressed when compositor blur is active — add `compositor_blur_active` flag per surface; when set, render backdrop-filter regions as transparent. Otherwise the compositor blurs already-blurred client pixels (double-blur).
- Blur region must be re-sent after `hide()`/remap — treat alongside `update_opaque_region`.
- `OrgKdeKwinBlurManager::unset(surface)` must only be called if the surface currently has an active blur object — track per `SurfaceEntry`.
- `WpViewport` and `OrgKdeKwinBlur` objects are tied to `wl_surface` lifetime, not buffer lifetime — do not destroy in `hide()`; only destroy on `SurfaceEntry` drop.

---

## Stack Additions

| Crate | Status | Purpose |
|-------|--------|---------|
| `wayland-protocols 0.32` | Add as direct dep to `mesh-core-presentation` with `features = ["client", "staging"]` (already transitive) | `wp_fractional_scale_v1` + `wp_viewporter` bindings |
| `wayland-protocols-plasma 0.3` | New direct dep in `mesh-core-presentation` | `org_kde_kwin_blur` bindings |

No other crate additions are needed. Per-region damage uses `wl_surface::damage_buffer` already in `wayland-client 0.31`.

**What not to add:**
- `ext_background_effect_v1` — staging, no compositor ships it yet; deferred to v1.21+.
- No `smithay-client-toolkit` version bump — SCT 0.19 integer scale path is sufficient.
- No `wl_surface::set_buffer_scale` when viewporter is active — the spec prohibits mixing these.

---

## Feature Readiness

| Feature | Current State | Gap |
|---------|--------------|-----|
| Per-region damage | `EffectiveDamage::rects` already populated in retained renderer; `damage_buffer` already called in `attach_shm_buffer` | `take_present_damage()` exposes only a single bounding rect; `SurfaceShmBuffer::pending_damage` is `Option<DamageRect>` |
| HiDPI integer scale | `CompositorHandler::scale_factor_changed` hook exists in `handlers.rs` | Handler body is empty no-op; no scale stored on `SurfaceEntry`; `PixelBuffer` allocated at logical size; paint scale param hardcoded to 1.0 |
| Fractional scale | `wayland-protocols 0.32` already transitive | Not a direct dep; no `WpFractionalScaleManagerV1` or `WpViewporter` bound; no `preferred_scale` dispatch impl |
| Compositor blur (KDE) | `backdrop_filter: VisualFilter` with `blur_radius: f32` on `DisplayPaintStyle`; display list walker identifies backdrop-filter nodes | `wayland-protocols-plasma` not in dep tree; no `OrgKdeKwinBlurManager` global bound; no `blur_region_rect()` method; CPU Skia blur would double-blur without a suppression flag |

All three features have existing prerequisite plumbing in place. No greenfield work is required — each phase is wiring and extension.

---

## Top Pitfalls

1. **`scale_factor_changed` is an empty no-op (HiDPI phase)** — The handler in `handlers.rs` stores nothing. Every scale-dependent operation will silently produce wrong results. Implement this first in Phase 2; store scale on `SurfaceEntry` and trigger repaint.

2. **`wp_blur_v1` does not exist — use `org_kde_kwin_blur` (Blur phase)** — The correct crate is `wayland-protocols-plasma 0.3`. Do not block on `wp_blur_v1`. Ship the KDE path; add a feature flag for `wp_blur_v1` when a compositor lands it.

3. **Double-blur when CPU and compositor blur are both active (Blur phase)** — If the Skia `backdrop-filter: blur()` path remains active while `org_kde_kwin_blur` is committed, the compositor blurs pre-blurred pixels. Add `compositor_blur_active` flag; when set, skip CPU blur and render the region as transparent.

4. **Blur `wl_region` vs damage `damage_buffer` coordinate spaces (Cross-cutting)** — `org_kde_kwin_blur` `wl_region` is in surface-local logical pixels; `damage_buffer` is in buffer physical pixels. Mixing these silently produces wrong results only on HiDPI. Enforce the distinction at the type level or audit all rect construction sites before each phase ships.

5. **`wl_surface::commit` is atomic — all pending state must precede it (Cross-cutting)** — `viewport.set_destination(...)`, `blur.commit()`, `damage_buffer(...)`, and buffer attach are all pending state activated by `wl_surface::commit`. Placing any of these after `layer_surface.commit()` queues them for the next frame. Review the commit sequence in `attach_shm_buffer` when each new protocol object is added.

6. **SHM pool initial size too small at 2x scale (HiDPI phase)** — Initial pool is 262 KB; a 2x-scaled full-width panel is ~983 KB. Verify `SlotPool` on-demand growth does not silently fail; increase initial size or make it scale-aware.

7. **`union_damage` flattening defeats multi-rect (Damage phase)** — `SurfaceShmBuffer::pending_damage` must become `Vec<DamageRect>`. The triple-buffer catch-up accumulation must append rect lists, not union to a single bounding box.

---

## Open Questions

| Question | Confidence | Resolution Path |
|----------|------------|-----------------|
| Does `SlotPool` grow on demand without silent error below one 2x frame? | MEDIUM | Inspect SCT source or test on 2x display during Phase 2 QA |
| Is `preferred_scale` guaranteed to arrive before first configure on KWin, Mutter, Hyprland? | MEDIUM | Test against each compositor during Phase 2 verification |
| Does Hyprland's `org_kde_kwin_blur` require separate `blur.commit()` or does it apply on `wl_surface::commit`? | MEDIUM | Test during Phase 3 verification |
| `ext_background_effect_v1` Hyprland partial support status | LOW | Defer to v1.21+ research; do not block v1.20 |
| Fractional scale on wlroots-based compositors (sway, river, wayfire) | MEDIUM | Accepted risk; integer scale fallback exists; test during Phase 2 QA |

---

## Confidence Assessment

| Area | Confidence | Notes |
|------|------------|-------|
| Stack | HIGH | Direct Cargo.toml/Cargo.lock inspection; crate registry confirmed |
| Features | HIGH (protocol mechanics), MEDIUM (support matrix) | Protocol specs from wayland.app; support matrix from release notes, not live testing |
| Architecture | HIGH | Direct codebase inspection with line-level confirmation |
| Pitfalls | HIGH | Codebase state confirmed (empty handlers, missing fields); winit-wayland reference used as ground truth |

**Overall confidence:** HIGH for implementation decisions. MEDIUM for compositor compatibility edge cases.

### Gaps to Address

- **`wp_blur_v1` scope change:** The milestone spec names `wp_blur_v1`; implementation ships `org_kde_kwin_blur`. Record the decision explicitly at phase start so roadmap entries use the correct protocol name.
- **`ext_background_effect_v1`:** Defer explicitly to v1.21+; do not leave open during v1.20 planning.
- **Fractional scale wlroots support:** Treat as accepted risk; integer scale fallback exists. Validate experimentally during Phase 2 QA.

---

## Sources

### Primary (HIGH confidence)

- `wayland-protocols 0.32.12` crate source (cargo registry) — fractional scale, viewporter protocol definitions and event signatures
- `wayland-protocols-plasma 0.3.12` crate source (cargo registry) — `org_kde_kwin_blur` object lifecycle and method signatures
- `winit-wayland/src/types/kwin_blur.rs` — reference implementation for `OrgKdeKwinBlurManager` binding and `blur.commit()` sequencing
- `winit-wayland/src/window/state.rs` — reference for `set_buffer_scale` / viewporter mutual exclusion gate
- MESH codebase (`crates/core/presentation/`, `crates/core/shell/`) — confirmed handler state, `EffectiveDamage` struct, `take_present_damage` signature, `attach_shm_buffer` commit sequence
- wayland.app protocol index — `wp_fractional_scale_v1`, `wp_viewporter`, `org_kde_kwin_blur`, `ext_background_effect_v1`

### Secondary (MEDIUM confidence)

- KDE Plasma 5.27 / GNOME 45 release notes — fractional scale support matrix
- Hyprland documentation — `org_kde_kwin_blur` and `wp_fractional_scale_v1` support

### Tertiary (LOW confidence)

- COSMIC DE blur support — status unknown
- Wayfire blur plugin — version-dependent; noted as partial

---

*Research completed: 2026-06-10*
*Ready for roadmap: yes*
