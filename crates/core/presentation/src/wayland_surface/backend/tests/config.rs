use super::*;

// ---------------------------------------------------------------------------
// layer-surface config tests
// ---------------------------------------------------------------------------

fn base_cfg() -> SurfaceConfig {
    SurfaceConfig {
        role: SurfaceRole::Layer,
        window: WindowOptions::default(),
        edge: Some(Edge::Left),
        layer: MeshLayer::Overlay,
        size_policy: LayerSurfaceSizePolicy::Fixed,
        width: 280,
        height: 164,
        exclusive_zone: 0,
        keyboard_mode: KeyboardMode::OnDemand,
        namespace: "@mesh/audio-popover".into(),
        margin_top: 24,
        margin_right: 0,
        margin_bottom: 0,
        margin_left: 24,
        blur: false,
    }
}

fn window_cfg() -> SurfaceConfig {
    SurfaceConfig {
        role: SurfaceRole::Window,
        window: WindowOptions {
            title: "Settings".into(),
            app_id: "mesh.settings".into(),
            resizable: false,
            decorations: WindowDecorations::Client,
        },
        width: 920,
        height: 700,
        namespace: "@mesh/settings".into(),
        ..SurfaceConfig::default()
    }
}

#[test]
fn window_config_is_not_clamped_to_an_output() {
    // A window is placed by the compositor and may legitimately exceed one
    // output's size or live on another; clamping it would fight the WM.
    // The same numbers on a layer surface *are* clamped.
    let mut layer = base_cfg();
    layer.edge = Some(Edge::Top);
    layer.width = 3000;
    layer.height = 2000;
    let clamped_layer = clamp_surface_config_to_output(layer, Some((1920, 1080)));
    assert_eq!((clamped_layer.width, clamped_layer.height), (1920, 1080));

    let window = window_cfg();
    let clamped_window = clamp_surface_config_to_output(window, Some((640, 480)));
    assert_eq!(
        (clamped_window.width, clamped_window.height),
        (920, 700),
        "window surfaces must keep their requested size regardless of output geometry"
    );
}

#[test]
fn window_identity_and_role_participate_in_the_config_fingerprint() {
    let cfg = window_cfg();
    let baseline = surface_config_fingerprint(&cfg, KeyboardMode::None);

    let mut retitled = cfg.clone();
    retitled.window.title = "Settings — Audio".into();
    assert_ne!(
        baseline,
        surface_config_fingerprint(&retitled, KeyboardMode::None),
        "a title change must reach the toplevel instead of being deduplicated away"
    );

    let mut resizable = cfg.clone();
    resizable.window.resizable = true;
    assert_ne!(
        baseline,
        surface_config_fingerprint(&resizable, KeyboardMode::None)
    );

    let mut as_layer = cfg.clone();
    as_layer.role = SurfaceRole::Layer;
    assert_ne!(
        baseline,
        surface_config_fingerprint(&as_layer, KeyboardMode::None),
        "a role change must be visible to the reconfigure gate"
    );
}

#[test]
fn keyboard_mode_only_reconfigure_keeps_surface_configured() {
    let previous = base_cfg();
    let mut next = previous.clone();
    next.keyboard_mode = KeyboardMode::Exclusive;

    assert!(
        !surface_change_requires_fresh_configure(&previous, &next, true),
        "keyboard interactivity-only changes must not force a fresh configure for an already-visible surface"
    );
}

#[test]
fn geometry_reconfigure_still_requires_fresh_configure() {
    let previous = base_cfg();
    let mut next = previous.clone();
    next.width = 320;

    assert!(surface_change_requires_fresh_configure(
        &previous, &next, true
    ));
}

#[test]
fn unconfigured_surface_still_requires_initial_configure() {
    let previous = base_cfg();
    let next = previous.clone();

    assert!(surface_change_requires_fresh_configure(
        &previous, &next, false
    ));
}

#[test]
fn dynamic_top_surface_uses_output_width_when_configure_width_is_unspecified() {
    let mut cfg = base_cfg();
    cfg.edge = Some(Edge::Top);
    cfg.width = 0;
    cfg.height = 50;

    assert_eq!(
        resolved_surface_size_for_config(&cfg, 1, 50, Some((1920, 1080))),
        (1920, 50),
        "top bars with width=0 must paint across the output even when the compositor leaves configure width unspecified"
    );
}

#[test]
fn dynamic_left_surface_uses_output_height_when_configure_height_is_unspecified() {
    let mut cfg = base_cfg();
    cfg.edge = Some(Edge::Left);
    cfg.width = 56;
    cfg.height = 0;

    assert_eq!(
        resolved_surface_size_for_config(&cfg, 56, 1, Some((1920, 1080))),
        (56, 1080),
        "left rails with height=0 must paint across the output height when the compositor leaves configure height unspecified"
    );
}

#[test]
fn clamp_skips_when_surface_output_is_unknown() {
    // Multi-monitor regression: `wl_surface::enter` for a freshly-mapped
    // layer surface arrives *after* the compositor's own `configure`, so
    // at clamp time on a surface's first couple configures the real
    // output is still unknown. A compositor-verified width (already
    // resolved against the surface's real output) must pass through
    // unclamped rather than being shrunk to an unrelated guessed output.
    let mut cfg = base_cfg();
    cfg.edge = Some(Edge::Top);
    cfg.width = 3840; // resolved against the real (external, larger) output
    cfg.height = 136;
    cfg.margin_left = 0;
    cfg.margin_right = 0;

    let clamped = clamp_surface_config_to_output(cfg.clone(), None);
    assert_eq!(
        clamped.width, 3840,
        "width must pass through unclamped when the surface's own output isn't known yet"
    );
    assert_eq!(clamped.margin_left, 0);
    assert_eq!(clamped.margin_right, 0);
}

#[test]
fn clamp_does_not_shrink_top_surface_to_a_smaller_unrelated_output() {
    // Same scenario as above, but exercising what used to happen when the
    // clamp fell back to "whatever output is first in the registry"
    // instead of skipping: a bar living on a 3840-wide external monitor
    // must never be clamped down to a smaller 2880-wide laptop panel just
    // because that happened to enumerate first. This shipped as a real
    // bug — the bar rendered narrower than its output and got centered
    // with dead space on both sides.
    let mut cfg = base_cfg();
    cfg.edge = Some(Edge::Top);
    cfg.width = 3840;
    cfg.height = 136;

    let wrong_smaller_output = Some((2880, 1800));
    let clamped = clamp_surface_config_to_output(cfg.clone(), wrong_smaller_output);
    assert_eq!(
        clamped.width, 2880,
        "sanity check: clamping against a smaller output does shrink width (this is the behavior that must never fire for an output the surface isn't actually on)"
    );

    let real_matching_output = Some((3840, 2160));
    let clamped = clamp_surface_config_to_output(cfg, real_matching_output);
    assert_eq!(
        clamped.width, 3840,
        "clamping against the surface's real, matching output must leave a spanning width untouched"
    );
    assert_eq!(clamped.margin_left, 0);
    assert_eq!(clamped.margin_right, 0);
}

#[test]
fn top_surface_protocol_size_keeps_only_spanning_width_dynamic() {
    let mut cfg = base_cfg();
    cfg.edge = Some(Edge::Top);
    cfg.width = 0;
    cfg.height = 0;
    cfg.exclusive_zone = 56;

    assert_eq!(
        layer_protocol_size(&cfg),
        (0, 56),
        "top surfaces are left+right anchored, so only width may be sent as zero; height falls back to the exclusive zone"
    );
}

#[test]
fn bottom_surface_protocol_size_keeps_only_spanning_width_dynamic() {
    let mut cfg = base_cfg();
    cfg.edge = Some(Edge::Bottom);
    cfg.width = 0;
    cfg.height = 0;
    cfg.exclusive_zone = 56;

    assert_eq!(
        layer_protocol_size(&cfg),
        (0, 56),
        "bottom surfaces are left+right anchored, so only width may be sent as zero; height falls back to the exclusive zone"
    );
}

#[test]
fn left_surface_protocol_size_keeps_only_spanning_height_dynamic() {
    let mut cfg = base_cfg();
    cfg.edge = Some(Edge::Left);
    cfg.width = 0;
    cfg.height = 0;
    cfg.exclusive_zone = 48;

    assert_eq!(
        layer_protocol_size(&cfg),
        (48, 0),
        "left surfaces with dynamic height are top+bottom anchored, so only height may be sent as zero; width falls back to the exclusive zone"
    );
}

#[test]
fn undocked_side_surface_never_spans_the_output() {
    // Regression guard: a floating (exclusive_zone == 0) left/right
    // surface whose content is not measured yet must NOT map as an
    // output-height-spanning surface — that shipped twice as an invisible
    // full-height overlay swallowing all pointer/keyboard input.
    let mut cfg = base_cfg();
    cfg.edge = Some(Edge::Left);
    cfg.width = 0;
    cfg.height = 0;
    cfg.exclusive_zone = 0;

    assert_eq!(
        layer_protocol_size(&cfg),
        (1, 1),
        "an unmeasured popover-style side surface must map tiny, never output-spanning"
    );
}

#[test]
fn unanchored_surface_protocol_size_replaces_dynamic_axes() {
    let mut cfg = base_cfg();
    cfg.edge = None;
    cfg.width = 0;
    cfg.height = 0;

    assert_eq!(
        layer_protocol_size(&cfg),
        (1, 1),
        "unanchored surfaces cannot use zero size on either axis"
    );
}

#[test]
fn overlay_surface_without_exclusive_zone_uses_minimal_protocol_fallback() {
    let mut cfg = base_cfg();
    cfg.edge = Some(Edge::Top);
    cfg.width = 0;
    cfg.height = 0;
    cfg.exclusive_zone = 0;

    assert_eq!(layer_protocol_size(&cfg), (0, 1));
}
