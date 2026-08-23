use super::*;

/// Build and configure an `xdg_positioner` from a [`PopupPlacement`]. Every
/// field maps 1:1 onto a positioner request, so the compositor performs the
/// anchoring and flip-at-edge math. Reactive mode is used only for the initial
/// popup positioner; later explicit reposition requests must remain
/// token-correlated.
pub(super) fn build_positioner(
    xdg_shell: &XdgShell,
    placement: &PopupPlacement,
    reactive: bool,
) -> Result<XdgPositioner, PresentationError> {
    let positioner = XdgPositioner::new(xdg_shell)
        .map_err(|e| PresentationError::SurfaceCreate(format!("xdg_positioner: {e}")))?;
    let (ax, ay, aw, ah) = placement.anchor_rect;
    positioner.set_anchor_rect(ax, ay, aw.max(1), ah.max(1));
    positioner.set_size(
        placement.size.0.max(1) as i32,
        placement.size.1.max(1) as i32,
    );
    positioner.set_anchor(popup::map_anchor(placement.anchor));
    positioner.set_gravity(popup::map_gravity(placement.gravity));
    positioner.set_constraint_adjustment(popup::map_constraint(placement.constraint));
    positioner.set_offset(placement.offset.0, placement.offset.1);
    if reactive {
        positioner.set_reactive();
    }
    Ok(positioner)
}

pub(in crate::wayland_surface) fn apply_config(layer_surface: &LayerSurface, cfg: &SurfaceConfig) {
    let (protocol_width, protocol_height) = layer_protocol_size(cfg);
    layer_surface.set_layer(map_layer(cfg.layer));
    layer_surface.set_anchor(map_anchor(cfg));
    layer_surface.set_exclusive_zone(cfg.exclusive_zone);
    layer_surface.set_keyboard_interactivity(map_keyboard(cfg.keyboard_mode));
    layer_surface.set_size(protocol_width, protocol_height);
    layer_surface.set_margin(
        cfg.margin_top,
        cfg.margin_right,
        cfg.margin_bottom,
        cfg.margin_left,
    );
}

/// Map MESH's decoration preference onto SCTK's creation-time request.
///
/// `Client` asks the compositor to let the module draw its own chrome but
/// still creates the decoration object, so a compositor that insists on
/// server-side decorations can say so through `WindowConfigure` rather than
/// being contradicted. `ClientOnly` would skip the negotiation entirely.
pub(super) fn map_window_decorations(decorations: WindowDecorations) -> SctkWindowDecorations {
    match decorations {
        WindowDecorations::Client => SctkWindowDecorations::RequestClient,
        WindowDecorations::Server => SctkWindowDecorations::RequestServer,
    }
}

pub(super) fn map_layer(layer: MeshLayer) -> Layer {
    match layer {
        MeshLayer::Background => Layer::Background,
        MeshLayer::Bottom => Layer::Bottom,
        MeshLayer::Top => Layer::Top,
        MeshLayer::Overlay => Layer::Overlay,
    }
}

pub(super) fn map_anchor(cfg: &SurfaceConfig) -> Anchor {
    match cfg.edge {
        // Treat a single edge as a normal shell placement, not a centered popup.
        // Top/bottom bars stretch across the output width, and left/right rails
        // pin to the top corner instead of floating in the vertical center.
        // If a left/right rail requests `height == 0`, layer-shell expects it
        // to be anchored to both top and bottom so the compositor can stretch
        // it vertically across the output.
        Some(Edge::Top) => Anchor::TOP | Anchor::LEFT | Anchor::RIGHT,
        Some(Edge::Bottom) => Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT,
        Some(Edge::Left) if cfg.height == 0 => Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT,
        Some(Edge::Right) if cfg.height == 0 => Anchor::TOP | Anchor::BOTTOM | Anchor::RIGHT,
        Some(Edge::Left) => Anchor::TOP | Anchor::LEFT,
        Some(Edge::Right) => Anchor::TOP | Anchor::RIGHT,
        None => Anchor::empty(),
    }
}

/// Map a MESH surface config onto the wire `zwlr_layer_surface_v1::set_size`.
///
/// CRITICAL layer-shell semantics: a dimension of `0` does NOT mean "empty" —
/// it means "stretch to the output edges on that axis" (and is only protocol-
/// valid when the surface is anchored to both opposing edges of that axis).
/// Passing measured-content sizes of `0` straight through has repeatedly
/// produced invisible output-spanning surfaces that swallow pointer and
/// keyboard input shell-wide.
///
/// Zeros are resolved here as follows:
/// - Top/bottom surfaces: width `0` is the intended output-wide bar span
///   (their horizontal both-edge anchor is unconditional); height `0` falls
///   back to the exclusive zone.
/// - Left/right surfaces with a positive exclusive zone are docked rails:
///   width falls back to the exclusive zone and height `0` spans — intended.
/// - Left/right (and unanchored) surfaces WITHOUT an exclusive zone are
///   floating popover-style surfaces. `map_anchor` derives the vertical
///   both-edge anchor FROM `height == 0`, so an unmeasured `0x0` popover
///   would silently become a full-output-height input sink (this shipped
///   twice: an invisible surface swallowing all pointer/keyboard input).
///   That case is clamped to 1x1 and logged as an error — a broken 1px
///   surface plus a log line beats a screen-wide input blackout.
pub(super) fn layer_protocol_size(cfg: &SurfaceConfig) -> (u32, u32) {
    let anchor = map_anchor(cfg);
    if cfg.width == 0
        && cfg.height == 0
        && cfg.exclusive_zone <= 0
        && !matches!(cfg.edge, Some(Edge::Top | Edge::Bottom))
    {
        tracing::error!(
            namespace = %cfg.namespace,
            edge = ?cfg.edge,
            "non-docked layer surface configured 0x0: zero means \"span the \
             output\" in layer-shell, which would map an invisible \
             output-spanning surface that blocks input; clamping to 1x1"
        );
        return (1, 1);
    }
    let width = if cfg.width == 0 && !anchor.contains(Anchor::LEFT | Anchor::RIGHT) {
        layer_protocol_fallback_size(cfg)
    } else {
        cfg.width
    };
    let height = if cfg.height == 0 && !anchor.contains(Anchor::TOP | Anchor::BOTTOM) {
        layer_protocol_fallback_size(cfg)
    } else {
        cfg.height
    };
    (width, height)
}

pub(super) fn layer_protocol_fallback_size(cfg: &SurfaceConfig) -> u32 {
    u32::try_from(cfg.exclusive_zone).unwrap_or(0).max(1)
}

pub(super) fn map_keyboard(mode: KeyboardMode) -> KeyboardInteractivity {
    match mode {
        KeyboardMode::None => KeyboardInteractivity::None,
        KeyboardMode::Exclusive => KeyboardInteractivity::Exclusive,
        KeyboardMode::OnDemand => KeyboardInteractivity::OnDemand,
    }
}
