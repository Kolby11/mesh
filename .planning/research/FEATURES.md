# Features Research: MESH v1.20 Compositor Integration

**Domain:** Wayland compositor protocol integration for a shell framework
**Researched:** 2026-06-10
**Overall confidence:** HIGH for protocol mechanics and behavior; MEDIUM for compositor support matrices (cross-checked against known compositor changelogs and wayland.app protocol index; web search was unavailable due to API outage during research)

---

## Summary

This milestone adds three compositor-facing Wayland protocol features to MESH: HiDPI/fractional scale rendering, compositor-offloaded blur for backdrop-filter, and per-region damage reporting. All three are additive and protocol-gated — the shell must detect global availability at bind time, apply the feature when the compositor supports it, and fall back gracefully when it does not. None of the three requires changes to the module authoring surface; they are infrastructure improvements that make the shell look and perform better on capable compositors.

The features are independent of each other. Per-region damage is the lowest risk (already partially implemented in the damage pipeline). HiDPI/fractional scale carries the most cross-cutting impact (affects render buffer sizing, SHM allocation, layout coordinate system, and surface configuration). Blur offload is the narrowest in scope (purely additive protocol attachment with no renderer coupling when unavailable).

---

## HiDPI / Fractional Scale

### What It Does and User-Visible Outcome

Without HiDPI support, MESH renders at logical pixel density (1:1 logical-to-buffer pixels). On a 2x HiDPI display the compositor upscales the SHM buffer by 2x, producing blurry shell surfaces — text edges are fuzzy, icons look interpolated, and the panel looks visually softer than native GTK/Qt surfaces on the same desktop.

With HiDPI support, MESH:
1. Receives the output scale factor from the compositor (integer via `wl_output::scale` / `wl_surface::preferred_buffer_scale`, or fractional via `wp_fractional_scale_v1::preferred_scale`)
2. Allocates the SHM buffer at physical pixel dimensions (`logical_size * scale_factor`)
3. Renders the widget tree at full physical resolution (all measurements stay in logical pixels inside the renderer; only the output buffer is sized at physical pixels)
4. Sets `wl_surface::set_buffer_scale(factor)` for integer scales, or uses `wp_viewporter::set_destination(logical_w, logical_h)` to tell the compositor the logical size for non-integer fractional scales
5. Commits a crisp, natively-scaled buffer

User-visible outcome: panel text and icons are sharp on 1.25x, 1.5x, 1.75x, and 2x displays. On a 4K display at 125% scaling (common on HiDPI laptops), the shell no longer looks blurry relative to native applications.

### Protocol Mechanics

Two parallel paths:

**Integer scale (`wl_output::scale` + `wl_surface::set_buffer_scale`)**
- Compositor emits `wl_output::scale(factor: int)` on output bind and on change
- From Wayland 1.22+, `wl_surface::preferred_buffer_scale(factor: int)` is also emitted per surface (wl_surface version 6)
- Client attaches buffer at `logical * factor` dimensions, calls `set_buffer_scale(factor)` before commit
- This path is universally supported; all compositors that implement `wl_output` at version 2+ support it

**Fractional scale (`wp_fractional_scale_v1` + `wp_viewporter`)**
- `wp_fractional_scale_manager_v1` is a global; check at bind time
- `wp_fractional_scale_v1::preferred_scale(scale: uint)` is emitted as `scale / 120` (e.g., 150 = 1.25x)
- Client renders buffer at nearest integer multiple, then uses `wp_viewporter::set_destination(logical_w, logical_h)` to tell the compositor the intended logical size — compositor handles the fractional resampling
- Preferred implementation: render at `ceil(scale * logical)` in buffer pixels, viewport back to logical dimensions. This avoids sub-pixel rendering artifacts.
- `wp_viewporter` is a stable protocol; `wp_fractional_scale_v1` is staging

### Table Stakes

Rendering sharp on HiDPI displays is table stakes. Any shell surface that appears blurry on a HiDPI desktop looks broken relative to native GTK/Qt applications. This is not optional polish — it is a correctness requirement for usability on modern hardware.

| Requirement | Why Table Stakes | Complexity |
|-------------|-----------------|------------|
| Render at native pixel density on integer-scale outputs | Blurry text/icons on 2x display is a visible defect | Medium — SHM reallocation, buffer size threading through render pipeline |
| Fractional scale via wp_fractional_scale_v1 + wp_viewporter | Required for 125%/150%/175% scaling (common on HiDPI laptops) | Medium — adds a second scale path, needs viewporter attachment |
| React to output scale changes at runtime | User can move surface to different monitor or change scaling in settings | Medium — event-driven reallocation, preserve surface continuity |
| Keep logical coordinate system intact inside MESH | Module authors write in logical pixels; physical size is an output concern only | Architectural — coordinate space must not leak through to Luau or .mesh authoring |

### Compositor Support Matrix

| Compositor | `wl_output::scale` (integer) | `wl_surface::preferred_buffer_scale` | `wp_fractional_scale_v1` | `wp_viewporter` |
|------------|------------------------------|--------------------------------------|--------------------------|-----------------|
| KWin (KDE Plasma 6) | YES | YES | YES (Plasma 5.27+) | YES |
| KWin (KDE Plasma 5.x) | YES | NO (added in KWin 6) | YES (5.27+) | YES |
| Mutter (GNOME 45+) | YES | YES | YES (GNOME 45) | YES |
| sway (wlroots 0.17+) | YES | YES | PARTIAL (advertised; experimental) | YES |
| Hyprland | YES | YES | YES (first-class feature) | YES |
| cosmic-comp (COSMIC DE) | YES | YES | YES | YES |
| River (wlroots-based) | YES | YES | PARTIAL (wlroots version dependent) | YES |
| Wayfire | YES | YES | PARTIAL (may have rendering artifacts) | YES |

Confidence: HIGH for integer scale (universally supported). MEDIUM for fractional scale matrix (based on Plasma 5.27/GNOME 45 release notes and Hyprland documentation; wlroots compositor versions need verification against current releases).

### Graceful Degradation

- `wp_fractional_scale_manager_v1` absent: fall back to integer `wl_output::scale` path. Surface appears at correct integer scale; may look slightly soft at non-integer display scaling but is never wrong.
- `wl_surface::preferred_buffer_scale` absent (old compositor): use `wl_output::scale` from whichever output the surface's center is on.
- Surface spans multiple outputs with different scales: use the largest scale (avoids downscaling artifacts on the higher-DPI output).
- Compositor sends scale = 1 or no scale event: render at 1:1 — identical to current behavior, zero regression.
- No fallback should cause a crash or buffer mismatch. Worst outcome is a 1:1 buffer upscaled by the compositor, which is the current behavior.

### Anti-Features

| Anti-Feature | Why Avoid |
|--------------|-----------|
| Exposing scale factor to module authors as a settable property | Scale is determined by the compositor and output; user override would break rendering correctness |
| Fractional buffer sizes (e.g., 1.25 * 100 = 125.0 as a float) | Always use integer physical buffer sizes; fractional coordinates cause sub-pixel misalignment |
| Per-element DPI awareness in Luau scripts | Module authors work in logical pixels only; DPI is a rendering pipeline concern |
| Using `wl_surface::damage` (surface coordinates) on scaled surfaces | Deprecated for scaled surfaces; `damage_buffer` in buffer coordinates must be used when buffer scale > 1 |
| Rebuilding the entire SHM pool on every scale change | Scale changes are rare; reallocate the buffer but preserve other state |

---

## Compositor Blur Offload

### What It Does and User-Visible Outcome

`backdrop-filter: blur(Npx)` in CSS blurs the content behind a transparent or translucent surface. Without compositor offload, blur behind shell surfaces is not achievable — Wayland clients cannot sample the compositor framebuffer behind them. With compositor offload, MESH attaches a blur region to the surface via protocol; the compositor applies a Gaussian blur pass to the composited framebuffer behind the surface region before compositing the client pixels on top.

User-visible outcome on supporting compositors: translucent panels and popovers show a frosted-glass effect. This is the visual style used by KDE Plasma, GNOME (via blur-my-shell), and macOS. A shell panel with `background: rgba(0,0,0,0.5)` and `backdrop-filter: blur(20px)` renders as a translucent frosted panel rather than a semi-transparent flat overlay.

User-visible outcome on non-supporting compositors: the surface renders with a flat/opaque background instead of the frosted effect. The surface must remain fully functional and visually coherent — it just lacks the visual flourish.

### Protocol Mechanics

Three protocol layers exist, in order of maturity:

**`org_kde_kwin_blur` (KDE-specific, stable, widely deployed)**
- Bind `org_kde_kwin_blur_manager`; call `create(surface)` to get an `org_kde_kwin_blur` object
- Call `set_region(wl_region)` to define the blur area in surface-local coordinates; null region = entire surface
- Call `commit()` on the blur object to apply; the blur region is also double-buffered via `wl_surface::commit`
- The compositor applies blur when the KWin blur effect is enabled in compositor settings

**`ext-background-effect-v1` (freedesktop staging, newer, cross-compositor path)**
- Bind `ext_background_effect_manager_v1`; compositor emits `capabilities` on bind with a `blur` flag
- If `blur` capability is present: call `get_background_effect(surface)` to get `ext_background_effect_surface_v1`
- Call `set_blur_region(wl_region)` with surface-local coordinates; double-buffered via `wl_surface::commit`
- This is the correct cross-compositor path going forward; protocol adoption is newer than the KDE protocol

**Note on `wp_blur_v1`**: The milestone context references `wp_blur_v1` but this name does not correspond to a shipped freedesktop protocol as of the research date. The `ext-background-effect-v1` is the current staging candidate. MESH should target `ext-background-effect-v1` for the generic path and `org_kde_kwin_blur` for KDE compatibility.

Confidence: HIGH for `org_kde_kwin_blur` mechanics. HIGH for `ext-background-effect-v1` (documented on wayland.app protocols index). MEDIUM for `wp_blur_v1` name — if this is a different protocol than `ext-background-effect-v1`, verification is needed before implementation.

### Table Stakes vs Differentiator

Compositor blur offload is a **differentiator**, not table stakes.

- Table stakes: the surface functions and looks visually coherent without blur (flat background color is acceptable on any compositor)
- Differentiator: frosted glass effect on compositors that support it; specifically required for a high-quality visual appearance on KDE Plasma

Implementing it is worthwhile because `backdrop-filter: blur()` is already part of the `.mesh` CSS authoring surface — wiring the protocol makes the property work as expected on KDE Plasma instead of silently doing nothing.

| Requirement | Classification | Complexity |
|-------------|---------------|------------|
| Wire `org_kde_kwin_blur` for KDE Plasma support | Differentiator but high-value | Low — simple protocol with a clear object lifecycle |
| Wire `ext-background-effect-v1` for generic support | Differentiator + future-proofing | Low — same pattern as KDE protocol, slightly different lifecycle |
| Graceful no-op fallback when neither global is available | Table stakes of the fallback | Trivial — no-op when globals absent at bind time |
| Derive blur wl_region from CSS backdrop-filter layout region | Ties protocol to CSS authoring contract | Medium — requires tracking which nodes have backdrop-filter and computing their surface-local bounding rect |

### Compositor Support Matrix

| Compositor | `org_kde_kwin_blur` | `ext-background-effect-v1` |
|------------|--------------------|-----------------------------|
| KWin (KDE Plasma 5.12+) | YES — core KDE blur effect; works when blur is enabled in compositor settings | NO (protocol is newer than KWin implementations to date) |
| Mutter (GNOME) | NO | NO — GNOME does not expose client-region blur |
| sway | NO | NO |
| Hyprland | YES — implements org_kde_kwin_blur for compatibility | PARTIAL — in active development |
| cosmic-comp (COSMIC DE) | UNKNOWN | UNKNOWN |
| River | NO | NO |
| Wayfire | PARTIAL — requires blur plugin to be active | NO |

Confidence: HIGH for KWin (core product feature with documented API). HIGH for sway/Mutter absence. MEDIUM for Hyprland (known org_kde_kwin_blur support; ext-background-effect status uncertain). LOW for COSMIC and Wayfire.

### Graceful Degradation

This is the most important behavioral constraint for blur:

- If neither blur global is advertised at bind time: no blur is applied. Surfaces with `backdrop-filter: blur()` render with a flat background. The surface must remain functional and visually acceptable. Module authors should provide a fallback background-color token.
- If the KDE blur global is present but the user has disabled the blur compositor effect in KWin settings: the blur region is submitted but the compositor silently ignores it. No error; surface composited without blur.
- Blur region must be resubmitted on every surface resize and whenever the set of backdrop-filtered nodes changes geometry.
- Never make blur a hard dependency of surface correctness. A shell that requires blur to look acceptable is a bad design.

### Anti-Features

| Anti-Feature | Why Avoid |
|--------------|-----------|
| CPU software blur fallback | Requires sampling the compositor framebuffer behind the surface, which is not available to Wayland clients. Any CPU "blur" would be fake — only the client's own pixels, not what's behind it |
| Making `backdrop-filter: blur()` silently no-op with no diagnostics | Module authors should be able to discover via debug output whether blur is active on the current compositor |
| Submitting blur region on every `wl_surface::commit` unconditionally | Only resubmit when the blur-covered geometry changes; the region is double-buffered and does not need to be re-sent when geometry is stable |
| Separate Luau API surface for blur control | Blur is a CSS authoring concern; `backdrop-filter: blur(Npx)` in `.mesh` style blocks is the full author contract |
| Hardcoding a blur radius in the protocol attachment | The radius implied by the CSS property should drive the protocol parameter where the protocol supports it |

---

## Per-Region Damage

### What It Does and User-Visible Outcome

On every surface commit, the Wayland client must report which rectangular regions of the buffer changed. The compositor uses this to avoid re-compositing unchanged regions — it can skip GPU texture uploads, skip scanout of unchanged display tiles on hardware overlays, and reduce memory bandwidth on tiled GPU architectures.

MESH currently calls `wl_surface::damage_buffer(0, 0, full_width, full_height)` — whole-surface damage — on every present. This is always correct but never optimal. The presentation layer already accepts a `DamageRect` parameter via `present_with_damage`, meaning the API shape is right; the gap is that the retained renderer unions all dirty paint regions into a single bounding rectangle before passing it upward, rather than preserving the list of individual dirty rects.

User-visible outcome: per-region damage is not directly visible. The benefit is reduced compositor GPU and memory bandwidth:
- A shell panel where only a clock region updates every second damages a small rect, not the full surface
- A notification that animates in from one edge damages only the growing notification area
- A popover that is fully static between interaction events commits zero damage on frame-callback wakeups

On compositors with hardware overlay support (KWin on AMD/Intel, Sway with DRM/KMS backend), accurate damage reduces scanout processing and can reduce idle power draw on battery-powered laptops.

### Protocol Mechanics

`wl_surface::damage_buffer(x, y, width, height)` is called once per damaged rectangle, before `wl_surface::commit`. Multiple calls accumulate additively for the upcoming commit. Coordinates are in buffer pixels (physical coordinates), not surface/logical coordinates.

Rules:
- Multiple `damage_buffer` calls before one commit mark the union of all those rects as damaged
- Under-damaging (reporting less than actually changed) causes stale pixels — compositor may use a cached buffer region
- Over-damaging (reporting more than changed) is always safe but wastes compositor bandwidth
- The existing MESH code in `backend.rs` already calls `damage_buffer` with buffer coordinates — the upgrade is to call it multiple times with smaller rects instead of once with the full surface

The `DamageRect` type in `mesh-core-render` is currently a single rectangle. For multi-rect damage: the type needs to become `Vec<DamageRect>` (or a `DamageSet`), and the retained renderer's dirty tracking needs to propagate individual per-node dirty rects rather than unioning them into a single bounding box before surfacing to the presentation layer.

### Table Stakes

Per-region damage is a **performance optimization**. It is table stakes for a production-quality retained renderer — any serious retained rendering pipeline tracks damage as a rect set — but is not user-visible on typical consumer hardware. The benefit is most pronounced on:
- Low-power ARM hardware with limited memory buses
- Compositors using hardware overlay planes (KWin on Intel/AMD, Sway with dmabuf backend)
- Battery-powered laptops where framebuffer bandwidth contributes to idle power draw

| Requirement | Classification | Complexity |
|-------------|---------------|------------|
| Track dirty paint regions as a rect set, not a single bounding box | Core pipeline improvement | Medium — `DamageRect` → `DamageSet` type; propagation through paint pipeline |
| Pass rect set from renderer through presentation to `damage_buffer` calls | Plumbing | Low — presentation loop already calls `damage_buffer`; replace single call with a loop |
| Conservative fallback: full-surface damage when rect set unavailable | Safety net | Trivial — current behavior preserved as fallback |
| Cap rect set size (e.g., max 16 rects, merge adjacent/overlapping beyond that) | Performance guard | Low — prevents per-rect compositor overhead from exceeding savings |

### Compositor Support Matrix

`wl_surface::damage_buffer` is part of core Wayland (wl_surface version 4, available since 2016). Every compositor MESH targets supports it. There is no compatibility concern — this is not a protocol extension.

| Compositor | `wl_surface::damage_buffer` | Uses damage for optimization |
|------------|----------------------------|-----------------------------|
| KWin | YES | YES — GPU texture update optimization |
| Mutter | YES | YES — compositor damage tracking |
| sway | YES | YES — wlroots DRM/KMS optimization |
| Hyprland | YES | YES |
| All others | YES | Varies by implementation |

Confidence: HIGH — core Wayland protocol, universal support.

### Anti-Features

| Anti-Feature | Why Avoid |
|--------------|-----------|
| Using `wl_surface::damage` (surface coordinates) on scaled surfaces | Surface-coord damage is wrong at buffer scale > 1; always use `damage_buffer` in buffer coordinates |
| Merging all dirty rects into a single bounding box before submission | Defeats the purpose of per-region damage when dirty areas are spatially separated (e.g., clock and network indicator both update simultaneously) |
| Submitting zero damage with a buffer attach | Compositor may skip compositing entirely; any actual visual change requires at least one damage call |
| Computing damage from pixel-level diff of two buffers | Per-frame pixel comparison is expensive and slow; damage must come from the retained tree's dirty markers, not pixel comparison |
| Emitting more than ~16 rects per commit | Diminishing returns beyond ~16 rects; compositor per-rect overhead starts to exceed the savings; merge adjacent rects for dense dirty patterns |

---

## Confidence Assessment

| Area | Confidence | Source |
|------|------------|--------|
| HiDPI protocol mechanics | HIGH | wayland.app protocol docs via context7 |
| Integer scale compositor support | HIGH | Core Wayland protocol, universally supported |
| Fractional scale compositor support matrix | MEDIUM | KDE/GNOME release notes; wlroots versions need on-target verification |
| Blur protocol mechanics (KDE) | HIGH | wayland.app kde-blur protocol docs |
| Blur protocol mechanics (ext-background-effect) | HIGH | wayland.app ext-background-effect-v1 docs |
| wp_blur_v1 protocol name | MEDIUM-LOW | Name does not match any shipped freedesktop protocol; likely refers to ext-background-effect-v1 |
| Blur compositor support matrix | MEDIUM (KDE), LOW (others) | KDE is well-documented; Hyprland/COSMIC status needs verification |
| Per-region damage protocol mechanics | HIGH | Core Wayland protocol documentation |
| Per-region damage compositor behavior | HIGH | Core protocol, no uncertainty |
| MESH current damage/scale implementation state | HIGH | Direct codebase inspection |
