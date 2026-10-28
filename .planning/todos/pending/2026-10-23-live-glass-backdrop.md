# Live glass backdrop: replace the wallpaper-image source

**Status:** proposed 2026-10-23; phase 1 landed the same day (October log).
Backlog line under "Rendering and paint" in
[`docs/BACKLOG.md`](../../../docs/BACKLOG.md). Touches spec 04 §10–§11.

Phase 1 as built: eligibility is decided per surface by layer
(`BackdropFrame::source_beneath`, set for background/bottom layers) and lowered
into the display list's command topology. On other surfaces a refracting
element's blur takes the ordinary backdrop route and `ApplyBackdropRim` paints
the rim highlight with shader-computed coverage, so partial and full repaints
match. Promoted popups never sample the source. The live A/B over a moving
window has not been run yet.

The 2026-10-27 widget correction also routes the bottom layer through compositor
blur and rim lighting. A static wallpaper image is not a trusted backdrop there
when another client owns the visible wallpaper. Only background-layer wallpaper
surfaces keep image sampling; phases 2–4 remain pending.

2026-10-28: §4 is met on Hyprland by the third-party hyprglass plugin rather
than a MESH-owned one. It reads `ext-background-effect-v1` state and, in
`contour` mask mode, traces each drawn shape for its own lens edge. The
`liquid-glass` theme targets it (spec 04 §11, "Compositor glass"). A
bottom-layer shortcut that sampled the wallpaper image for desktop widgets was
tried and reverted the same day: with the wallpaper surface not showing that
image, the widgets refracted a picture that was not on screen.

## Problem

`backdrop-filter: blur() refract()` (04 §11) samples one static image: the
file at `shell.render.backdrop.image`, cover-fit to the output and offset by a
*predicted* surface origin (`runtime/render/backdrop.rs`,
`frontend/render/src/surface/backdrop.rs`). An element with `refract()` is
never exported as a compositor blur region, so in the glass theme **every**
frosted surface shows the image, not what is actually behind it. The two drift
apart when:

1. a window, another client's layer surface, or another MESH surface is
   behind the glass. The bar, dock, dashboard, notifications and popovers sit on
   the top/overlay layers, above windows.
2. the wallpaper is drawn by something else (swww, hyprpaper), or MESH's own
   wallpaper is mid-transition, scaled differently, or not the configured file.
3. the predicted origin is wrong: foreign exclusive zones, multi-output, or
   compositor-side layer animations.
4. the compositor animates workspaces, fades or moves windows. The image is
   static.

## The constraint

A Wayland client cannot read what the compositor shows behind its own surface.
Every route to the real backdrop goes through the compositor:

| Route | What it sees | Synced? | Why it is or is not enough |
| --- | --- | --- | --- |
| `ext-background-effect-v1` / `org_kde_kwin_blur` (shipped) | everything beneath | yes, zero lag | Blur only. It cannot bend the backdrop or disperse colour. |
| Output capture (`ext-image-copy-capture-v1`, `zwlr_screencopy`; both bound on Hyprland 0.56) | the composited output, **including MESH's own surfaces** | one frame late | Feedback loop: the glass samples itself. Inverting our own compositing, `B = (C − αS)/(1 − α)`, fails inside the glass. There MESH paints the refracted sample under the fill, so the pixel is near-opaque. It also costs a 20 MiB copy per changed frame on the SHM path, and Hyprland's permission system can gate it. |
| Per-toplevel capture (`ext-foreign-toplevel-image-capture-source`) + wallpaper | windows without layers | lagging | Needs window positions and stacking from compositor IPC. It misses compositor decorations, animations and foreign layer surfaces, and has per-window capture cost. It is a second fake. |
| Compositor effect (plugin) | everything beneath | yes, zero lag | Exact. Compositor-specific code outside the MESH process. |

So MESH can sample exactly **only what it painted itself**. Everything else must
be the compositor's job. The plan follows that line instead of improving the
guess.

## Design

### 1. Split blur from refraction (all compositors)

Treat `blur()` in a refracting `backdrop-filter` as an ordinary backdrop blur:
export the element's shape as a compositor region (04 §10 routing). Then the
frosted interior, which is most of a glass pane, is the real, synced backdrop on
any compositor with a blur protocol. Refraction becomes a layer drawn *over*
that region, and only where an exact source exists (§2). Without a source the
element keeps the real blur, the rim highlight and the fill, and drops the
bending. Visual-regression fixtures for the glass theme cover both cases.

This change alone fixes problems 1, 2 and 4 for blur. It is the largest
visible improvement and touches only routing.

### 2. Owned backdrop scene in place of the image file

Replace `BackdropSource { image }` with a scene MESH composes from its **own
presented frames**: on each output, every visible MESH surface on a layer below
the sampling surface, in layer and z order, each at its origin. It is exact
whenever nothing foreign lies between those surfaces and the glass. That holds
for the background and bottom layers, which sit below every window
(`@mesh/glass-widgets` over `@mesh/glass-wallpaper`).

- **Source:** the wallpaper surface's actual output, crossfades included, not a
  separately decoded file. `mesh.theme.set_backdrop_image` stays as the
  wallpaper module's own setting. It stops being a renderer input, and
  `shell.render.backdrop.image` is removed outright (no compat).
- **Invalidation:** a frame-generation dependency edge. A refracting surface
  records the generations of the surfaces under its read region. A lower
  surface presenting changed pixels inside that region damages the region in
  the dependants. The "nothing changed beneath" case costs nothing, as low-idle
  requires.
- **Ownership:** on the GPU path the render worker owns the scene textures. It
  already holds every surface's retained target, so this is a texture reference,
  not a readback ([renderer ownership](../../../docs/renderer-ownership.md)). The
  CPU path keeps the presented `PixelBuffer`. Prepared blur copies are keyed by
  (generation, blur) and keep the existing downsample rule.
- **Eligibility:** a refracting element samples the owned scene only when its
  surface is on the background or bottom layer. On the top and overlay layers
  windows can be beneath, so it falls back to §1 (real blur, no bending) unless
  §4 is bound. A glass popover over the bar is the one top-layer case that
  qualifies by construction (parent surface beneath, exactly known). It is
  included as a follow-up.

### 3. Real placement in place of prediction

Origins stay predicted (04 §11) but become correctable. An optional
`mesh.surface-geometry` interface is supplied by a backend module.
`hyprland-wm` already talks to Hyprland IPC, and `hyprctl layers -j` reports
real layer rectangles per output. When a provider is selected, its geometry is
authoritative and prediction is the fallback. This is a domain integration
behind a typed interface, so it fits 00 §2. Core consumes geometry, and the
module owns the compositor-specific way of getting it.

### 4. Compositor glass protocol (optional, exact)

This is the only route to synced refraction over windows. Define a small staging
protocol, `mesh_glass_effect_v1`, modelled on `ext-background-effect-v1`:
per-surface regions carrying a rounded rect, blur, depth, edge and dispersion.
Implement it as a Hyprland plugin that runs the existing refraction shader
(`ApplyBackdropRefraction`'s SkSL ported to GLSL) inside Hyprland's blur pass.
When the global is bound, refracting elements export glass regions instead of
painting client-side, and neither §1's fallback nor §2's eligibility applies.

The plugin lives outside the shell process and is tied to Hyprland's ABI.
MESH must work fully without it.

### Rejected: live output capture

The table above gives the reasons. The one narrow use is a **pre-map snapshot**
for a transient overlay (dashboard, wallpaper picker): capture the output once
before the surface maps, so the capture cannot contain the surface. Content that
changes while the overlay is open goes stale. Revisit it only if §1's fallback
proves visually unacceptable for those overlays.

## Phases

1. **Split routing (§1).** Export a refracting element's blur to the compositor.
   Refraction paints only with a source. Add a pixel test for the no-source
   fallback, a live A/B over a moving window, and update 04 §11.
2. **Owned scene (§2).** Add the scene source, generation dependencies and
   layer eligibility. Remove the image source and `shell.render.backdrop.image`.
   Pixel tests: a wallpaper crossfade under a glass widget matches a reference
   composite, and an unchanged lower surface causes no dependant damage. Record
   GPU and CPU timings for widgets over the wallpaper in the performance log.
3. **Geometry provider (§3).** Add the interface, the `hyprland-wm`
   implementation and the fallback. Test against a foreign exclusive zone.
4. **Compositor protocol (§4).** Write the protocol XML, client binding and
   region export in `presentation/.../blur.rs`, then the plugin. Gated on the
   decision below.

## Open decisions

- Whether MESH should own a compositor plugin at all (§4), and where it lives:
  under `tools/`, or as a separate repository with its own release cadence.
- Whether dropping the bending on top-layer surfaces without §4 is acceptable,
  or the pre-map snapshot is worth adding for overlays.

## Risks

- Phase 1 changes the glass look on the top layer. The bending disappears until
  §4 is bound. That is the honest result, but users will notice it.
- `ext-background-effect-v1` blurs with the compositor's kernel and passes, not
  the CSS radius. The blur radius becomes advisory there (already true for
  plain `backdrop-filter`).
- Owned-scene dependencies across surfaces add a cross-surface invalidation
  edge. Keep it in the frame dirty set; do not add a second scheduler.
