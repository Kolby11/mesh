use super::*;
use mesh_core_render::DamageRect;

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
        padding: SurfacePadding::default(),
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
fn window_identity_and_role_have_typed_change_kinds() {
    let cfg = window_cfg();

    let mut retitled = cfg.clone();
    retitled.window.title = "Settings — Audio".into();
    assert_eq!(
        surface_config_change(&cfg, KeyboardMode::None, &retitled, KeyboardMode::None),
        SurfaceConfigChange::Live,
        "a title change must reach the toplevel without requiring a fresh configure"
    );

    let mut resizable = cfg.clone();
    resizable.window.resizable = true;
    assert_eq!(
        surface_config_change(&cfg, KeyboardMode::None, &resizable, KeyboardMode::None),
        SurfaceConfigChange::Live
    );

    let mut as_layer = cfg.clone();
    as_layer.role = SurfaceRole::Layer;
    assert_eq!(
        surface_config_change(&cfg, KeyboardMode::None, &as_layer, KeyboardMode::None),
        SurfaceConfigChange::Recreate,
        "a role change must replace the compositor object"
    );

    let mut server_decorations = cfg.clone();
    server_decorations.window.decorations = WindowDecorations::Server;
    assert_eq!(
        surface_config_change(
            &cfg,
            KeyboardMode::None,
            &server_decorations,
            KeyboardMode::None,
        ),
        SurfaceConfigChange::Recreate,
        "decoration negotiation is a creation-time window property"
    );
}

#[test]
fn keyboard_mode_only_change_is_live() {
    let previous = base_cfg();
    let mut next = previous.clone();
    next.keyboard_mode = KeyboardMode::Exclusive;

    assert_eq!(
        surface_config_change(&previous, previous.keyboard_mode, &next, next.keyboard_mode,),
        SurfaceConfigChange::Live,
        "keyboard interactivity-only changes must not force a fresh configure for an already-visible surface"
    );
}

#[test]
fn layer_diff_ignores_toplevel_only_fields() {
    let previous = base_cfg();
    let mut next = previous.clone();
    next.window.title = "ignored for layer surfaces".into();
    next.window.app_id = "ignored.layer".into();
    next.window.resizable = !next.window.resizable;
    next.size_policy = LayerSurfaceSizePolicy::Flexible;
    next.keyboard_mode = KeyboardMode::Exclusive;

    assert_eq!(
        surface_config_change(
            &previous,
            previous.keyboard_mode,
            &next,
            previous.keyboard_mode,
        ),
        SurfaceConfigChange::Unchanged,
        "layer diffs must not react to toplevel-only or desired keyboard fields"
    );
}

#[test]
fn window_diff_ignores_layer_only_fields() {
    let previous = window_cfg();
    let mut next = previous.clone();
    next.edge = Some(Edge::Bottom);
    next.layer = MeshLayer::Background;
    next.size_policy = LayerSurfaceSizePolicy::Flexible;
    next.exclusive_zone = 48;
    next.margin_top = 12;
    next.margin_right = 8;
    next.margin_bottom = 4;
    next.margin_left = 16;
    next.namespace = "ignored.window.namespace".into();
    next.blur = !next.blur;
    next.keyboard_mode = KeyboardMode::Exclusive;

    assert_eq!(
        surface_config_change(&previous, KeyboardMode::None, &next, KeyboardMode::None),
        SurfaceConfigChange::Unchanged,
        "window diffs must not react to layer-only placement fields"
    );
}

#[test]
fn layer_geometry_change_requires_fresh_configure() {
    let previous = base_cfg();
    let mut next = previous.clone();
    next.width = 320;

    assert_eq!(
        surface_config_change(&previous, previous.keyboard_mode, &next, next.keyboard_mode,),
        SurfaceConfigChange::Configure
    );
}

#[test]
fn unchanged_config_has_no_semantic_diff() {
    let previous = base_cfg();
    let next = previous.clone();

    assert_eq!(
        surface_config_change(&previous, previous.keyboard_mode, &next, next.keyboard_mode,),
        SurfaceConfigChange::Unchanged
    );
    assert!(!SurfaceConfigChange::Unchanged.requires_fresh_configure());
}

#[test]
fn layer_namespace_and_blur_require_surface_recreation() {
    let cfg = base_cfg();

    let mut renamed = cfg.clone();
    renamed.namespace = "@mesh/other-popover".into();
    assert_eq!(
        surface_config_change(&cfg, cfg.keyboard_mode, &renamed, renamed.keyboard_mode),
        SurfaceConfigChange::Recreate
    );

    let mut blurred = cfg.clone();
    blurred.blur = true;
    assert_eq!(
        surface_config_change(&cfg, cfg.keyboard_mode, &blurred, blurred.keyboard_mode),
        SurfaceConfigChange::Recreate
    );
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
fn dynamic_surface_padding_uses_the_resolved_extent() {
    let mut cfg = base_cfg();
    cfg.edge = Some(Edge::Top);
    cfg.width = 0;
    cfg.height = 50;
    cfg.padding = SurfacePadding::trailing(0, 12);

    let extent = resolved_surface_size_for_config(&cfg, 1, 50, Some((1920, 1080)));
    assert_eq!(extent, (1920, 50));
    assert_eq!(
        cfg.padding.content_rect(extent.0, extent.1),
        Some(DamageRect {
            x: 0,
            y: 0,
            width: 1920,
            height: 38,
        })
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

// ---------------------------------------------------------------------------
// input region / surface padding
//
// A MESH surface is routinely larger than its content — a bar reserves room
// below itself for tooltips, a popover reserves a ring for shadow overshoot
// — and the compositor hands MESH every click over that reserve unless the
// input region excludes it. These pin the derivation that excludes it.
// ---------------------------------------------------------------------------

#[test]
fn trailing_padding_confines_input_to_the_content_rect() {
    // A 56px bar inflated by the 200px tooltip overlay reserve.
    let padding = SurfacePadding::trailing(0, 200);
    assert_eq!(
        padding.content_rect(1920, 256),
        Some(DamageRect {
            x: 0,
            y: 0,
            width: 1920,
            height: 56,
        }),
        "the 200px strip below the bar must fall through to the windows under it"
    );
}

#[test]
fn ring_padding_confines_input_to_the_inset_content_rect() {
    // A popover whose buffer is padded on all sides for shadow overshoot.
    let padding = SurfacePadding {
        left: 24,
        top: 8,
        right: 24,
        bottom: 40,
    };
    assert_eq!(
        padding.content_rect(348, 212),
        Some(DamageRect {
            x: 24,
            y: 8,
            width: 300,
            height: 164,
        }),
    );
}

#[test]
fn zero_padding_leaves_the_whole_surface_taking_input() {
    assert!(SurfacePadding::default().is_zero());
    assert_eq!(SurfacePadding::default().content_rect(640, 480), None);
}

/// A zero-area input region makes a surface completely unclickable, which is
/// a worse failure than an oversized one and can legitimately happen for a
/// frame while a surface is still being measured. Degrade to whole-surface
/// input instead of collapsing.
#[test]
fn padding_larger_than_the_surface_does_not_collapse_the_region() {
    let padding = SurfacePadding::trailing(0, 200);
    assert_eq!(padding.content_rect(1920, 200), None);
    assert_eq!(padding.content_rect(1920, 120), None);
}

/// The reserve carries no protocol request of its own, so `apply_config` is
/// the only thing that copies it onto the live surface — and it only runs
/// when the typed diff says the config changed.
#[test]
fn changing_only_the_padding_still_counts_as_a_config_change() {
    let cfg = base_cfg();
    let mut padded = cfg.clone();
    padded.padding = SurfacePadding::trailing(0, 200);
    assert_eq!(
        surface_config_change(
            &cfg,
            KeyboardMode::OnDemand,
            &padded,
            KeyboardMode::OnDemand,
        ),
        SurfaceConfigChange::Live,
    );
}
