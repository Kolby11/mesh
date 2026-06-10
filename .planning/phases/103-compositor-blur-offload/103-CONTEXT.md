# Phase 103: Compositor Blur Offload - Context

**Gathered:** 2026-06-10
**Status:** Ready for planning

<domain>
## Phase Boundary

Wire `org_kde_kwin_blur` Wayland protocol so surfaces with `backdrop-filter: blur(...)` nodes delegate blur rendering to the KDE compositor. On non-KDE compositors, surfaces render a flat background without error. Remove the CPU software blur path per BLUR-03. This phase depends on Phase 102 (scale factor must be authoritative before blur region coordinates are correct).
</domain>

<decisions>
## Implementation Decisions

### Protocol Binding Strategy
- Bind `org_kde_kwin_blur` as an optional global during compositor startup, producing `Option<KdeBlurManager>` in shared `State`
- Non-KDE compositors get `None` and proceed without error
- Per-surface `kde_blur` objects created lazily on first frame with backdrop-filter nodes

### Blur Region Computation
- Walk display items after list construction to find nodes with `backdrop_filter.blur_radius > 0.0`
- Compute union of logical-coordinate rectangles from those nodes
- Send blur region at commit time in the present path, alongside damage rect computation
- Use logical pixel coordinates for protocol calls (per spec)

### Cargo Dependencies
- Add `wayland-protocols-plasma` crate for `org_kde_kwin_blur` protocol XML
- No Rust feature flag — protocol binding is optional at runtime

### CPU Blur Removal
- `apply_backdrop_filter` becomes a no-op for software blur (skip rendering when `blur_radius <= 0.0`)
- Display list still tracks `backdrop_filter` for region computation
- Keep function structure for future `backdrop-filter` effects

### OpenCode's Discretion
- Exact naming conventions (e.g., `KdeBlurState`, `BlurManager`)
- Method organization within modules
- Test fixture design and coverage strategy
</decisions>

<code_context>
## Existing Code Insights

### Reusable Assets
- `wayland-protocols` v0.32 already a dependency of presentation crate
- `State` struct pattern for optional protocol globals (`Option<ActivationState>`, `Option<WpViewporter>`, `Option<HyprlandFocusGrabManagerV1>`)
- `SurfaceEntry` pattern for per-surface protocol state
- `backdrop-filter` CSS property already parsed into `VisualFilter` with `blur_radius: f32`
- Display list items carry `backdrop_filter: VisualFilter` field
- `push_backdrop_filter_command` and `apply_backdrop_filter` exist in painter tree
- Visual bounds already expand for blur effects in `shell_component.rs`

### Established Patterns
- Protocol globals bound optionally during `WaylandSurface::new()` or compositor init
- Per-surface protocol objects stored in `SurfaceEntry`, managed by surface lifecycle
- Protocol calls sequenced before `wl_surface.commit()` in the present path
- Static `protocol_damage_rects` helper pattern for testable protocol logic
- `nix develop -c cargo check` for builds; `nix develop -c cargo test` for tests

### Integration Points
- Presentation crate: `backend.rs` (protocol binding, surface creation, commit path), `state.rs` (State struct, SurfaceEntry)
- Shell render integration: `render.rs` (access display list blur data), `shell_component.rs` (wire info to presentation)
- Painter: `painter.rs` and `painter/tree.rs` (remove CPU blur)
- Protocol XML: `wayland-protocols-plasma` crate for `org_kde_kwin_blur.xml`
</code_context>

<specifics>
## Specific Ideas

No specific requirements beyond ROADMAP success criteria — open to standard approaches.
</specifics>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope.
</deferred>
