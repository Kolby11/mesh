# Requirements: MESH v1.20 Compositor Integration

**Milestone:** v1.20 — Compositor Integration
**Goal:** Use Wayland compositor protocols to offload work and support HiDPI displays without upscaling.
**Created:** 2026-06-10

---

## v1.20 Requirements

### Per-Region Damage

- [ ] **DMGE-01**: Shell passes a `Vec<DamageRect>` from the retained renderer through the present path instead of a single unioned rect
- [ ] **DMGE-02**: Presentation calls `wl_surface::damage_buffer` once per dirty rect (capped at 16) per frame commit
- [ ] **DMGE-03**: Debug/profiling exposes damage rect count per frame alongside existing damage metrics

### HiDPI / Fractional Scale

- [ ] **HDPI-01**: `SurfaceEntry` stores a `scale_factor: f64` updated from `wl_output::scale` and `wp_fractional_scale_v1` events
- [ ] **HDPI-02**: `PixelBuffer` is allocated at `ceil(logical × scale_factor)` physical pixels; layout remains in logical CSS pixels
- [ ] **HDPI-03**: `wp_viewporter` sets destination size to logical dimensions for correct compositing at non-integer scale ratios
- [ ] **HDPI-04**: Scale factor changes trigger a surface resize and full redraw without visual glitches or stale pixels
- [ ] **HDPI-05**: Integer `wl_output::scale` path is used as fallback when `wp_fractional_scale_v1` is unavailable

### Compositor Blur Offload

- [ ] **BLUR-01**: Shell binds `org_kde_kwin_blur` as an optional global at startup; surfaces proceed without blur on non-KDE compositors
- [ ] **BLUR-02**: For surfaces with `backdrop-filter: blur(...)` nodes, shell sends `kde_blur.set_region` + `kde_blur.commit` before `wl_surface.commit` using logical pixel coordinates
- [ ] **BLUR-03**: CPU software blur is not implemented as a fallback; unsupported compositors render a flat background
- [ ] **BLUR-04**: Blur region commits are skipped cleanly when no backdrop-filter nodes exist in the display list

---

## Future Requirements

- `ext_background_effect_v1` blur protocol support — deferred until compositor adoption is confirmed (as of 2026-06 no shipping compositor implements it)
- Per-surface damage rect count telemetry in the production monitoring path — deferred, debug only for v1.20
- Widget-level opaque rect analysis (OPAQUE-02) — deferred from v1.19, not in scope for v1.20

---

## Out of Scope

- **GPU rendering backend** — Skia CPU path stays; GPU work is a separate milestone (v1.25)
- **`ext_background_effect_v1`** — no confirmed compositor implementation; using `org_kde_kwin_blur` only
- **CPU software blur fallback** — clients cannot read the compositor framebuffer; software blur would be a fake effect on the client's own pixels
- **Compositor-global shortcuts or XDG portal work** — unrelated to compositor integration
- **Retained Taffy tree across passes** — v1.21 scope
- **Rope-style display list storage** — v1.21 scope

---

## Traceability

| REQ-ID | Phase | Status |
|--------|-------|--------|
| DMGE-01 | TBD | Pending |
| DMGE-02 | TBD | Pending |
| DMGE-03 | TBD | Pending |
| HDPI-01 | TBD | Pending |
| HDPI-02 | TBD | Pending |
| HDPI-03 | TBD | Pending |
| HDPI-04 | TBD | Pending |
| HDPI-05 | TBD | Pending |
| BLUR-01 | TBD | Pending |
| BLUR-02 | TBD | Pending |
| BLUR-03 | TBD | Pending |
| BLUR-04 | TBD | Pending |
