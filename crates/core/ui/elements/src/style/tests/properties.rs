use super::super::*;
use super::common::*;
use crate::tree::NodeState;
use mesh_core_component::style::StyleValue;

#[test]
fn resolves_prop_reference_from_resolver_props() {
    use mesh_core_component::style::prop_variable_key;
    let theme = mesh_core_theme::default_theme();
    let mut props = std::collections::HashMap::new();
    props.insert(
        prop_variable_key("track_width"),
        StyleValue::Literal("20px".to_string()),
    );
    let resolver = StyleResolver::new(&theme).with_props(props);
    let rules = parse_fixture_style(
        r#"
<props>
{ track_width = { type = "size", default = "32px" }, }
</props>
<style>
.slider { width: prop(track_width); }
</style>
"#,
    );
    let (style, diagnostics) = resolve_class(&resolver, &rules, "slider");
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    assert_eq!(style.width, crate::Dimension::Px(20.0));
}

#[test]
fn unresolved_prop_reference_is_empty() {
    let theme = mesh_core_theme::default_theme();
    let resolver = StyleResolver::new(&theme);
    let rules = parse_fixture_style(
        r#"
<props>
{ track_width = { type = "size", default = "32px" }, }
</props>
<style>
.slider { width: prop(track_width); }
</style>
"#,
    );
    let (style, _) = resolve_class(&resolver, &rules, "slider");
    // No props attached → prop() resolves to empty → 0px (not the 20px above).
    // In practice every declared prop carries a default, so the resolver always
    // seeds a value; this only covers the unseeded edge.
    assert_eq!(style.width, crate::Dimension::Px(0.0));
}

#[test]
fn pack_qualified_font_family_uses_runtime_theme_binding() {
    let mut theme = mesh_core_theme::default_theme();
    theme.set_token(
        "mesh.font.default.body",
        mesh_core_theme::TokenValue::String("Inter".into()),
        mesh_core_theme::ThemeProvenance::BaseRecovery,
    );
    let resolver = StyleResolver::new(&theme);
    let rules = parse_fixture_style(
        r#"
<style>
.text { font-family: default/body; }
</style>
"#,
    );
    let (style, diagnostics) = resolve_class(&resolver, &rules, "text");
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    assert_eq!(&*style.font_family, "Inter");
}

#[test]
fn indexed_module_style_resolution_matches_non_indexed_resolution() {
    let theme = mesh_core_theme::default_theme();
    let resolver = StyleResolver::new(&theme);
    let rules = parse_fixture_style(
        r#"
<style>
box { width: 10px; }
.card { height: 20px; }
box.card { padding: 3px; }
</style>
"#,
    );
    let classes = vec!["card".to_string()];
    let index = StyleRuleIndex::new(&rules);

    let expected = resolver.resolve_node_style_for_module(
        &rules,
        "box",
        &classes,
        None,
        StyleContext::default(),
        NodeState::default(),
        Some("@mesh/test"),
    );
    let actual = resolver.resolve_node_style_for_module_indexed(
        &rules,
        &index,
        "box",
        &classes,
        None,
        StyleContext::default(),
        NodeState::default(),
        Some("@mesh/test"),
    );

    assert_eq!(actual.width, expected.width);
    assert_eq!(actual.height, expected.height);
    assert_eq!(actual.padding.left, expected.padding.left);
}

#[test]
fn resolves_surface_exclusive_zone_as_a_root_css_policy() {
    let theme = mesh_core_theme::default_theme();
    let resolver = StyleResolver::new(&theme);
    let rules = parse_fixture_style(
        r#"
<style>
.panel { exclusive-zone: none; }
</style>
"#,
    );

    let (style, diagnostics) = resolve_class(&resolver, &rules, "panel");
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    assert_eq!(style.surface_exclusive_zone, SurfaceExclusiveZone::None);
}

#[test]
fn parse_hex_colors() {
    assert_eq!(
        Color::from_hex("#fff"),
        Some(Color {
            r: 255,
            g: 255,
            b: 255,
            a: 255
        })
    );
    assert_eq!(
        Color::from_hex("#6750A4"),
        Some(Color {
            r: 103,
            g: 80,
            b: 164,
            a: 255
        })
    );
    assert_eq!(
        Color::from_hex("#00000080"),
        Some(Color {
            r: 0,
            g: 0,
            b: 0,
            a: 128
        })
    );
    assert_eq!(Color::from_hex("invalid"), None);
}

#[test]
fn parse_css_rgb_colors_used_by_runtime_props() {
    assert_eq!(
        Color::from_css("rgba(1, 2, 3, 0.5)"),
        Some(Color {
            r: 1,
            g: 2,
            b: 3,
            a: 128,
        })
    );
    assert_eq!(
        Color::from_css("rgb(100%, 0%, 50%)"),
        Some(Color {
            r: 255,
            g: 0,
            b: 128,
            a: 255,
        })
    );
}

#[test]
fn resolve_theme_var() {
    let theme = mesh_core_theme::default_theme();
    let resolver = StyleResolver::new(&theme);
    let value = StyleValue::Var("--color-primary".to_string());
    let resolved = resolver.resolve_value(&value);
    assert_eq!(Color::from_hex(&resolved), Color::from_hex("#E0AF68"));
}

#[test]
fn supported_css_properties_cover_phase_8_contract() {
    for property in [
        "background",
        "background-color",
        "color",
        "border",
        "border-color",
        "border-width",
        "border-radius",
        "display",
        "visibility",
        "opacity",
        "overflow",
        "overflow-x",
        "overflow-y",
        "width",
        "height",
        "min-width",
        "max-width",
        "min-height",
        "max-height",
        "padding",
        "padding-inline",
        "padding-block",
        "margin",
        "margin-inline",
        "margin-block",
        "font",
        "font-family",
        "font-size",
        "font-weight",
        "font-style",
        "line-height",
        "letter-spacing",
        "text-align",
        "text-overflow",
        "white-space",
        "direction",
        "flex",
        "flex-direction",
        "flex-wrap",
        "flex-grow",
        "flex-shrink",
        "flex-basis",
        "justify-content",
        "align-items",
        "align-self",
        "align-content",
        "gap",
        "row-gap",
        "column-gap",
        "position",
        "exclusive-zone",
        "z-index",
        "inset",
        "top",
        "right",
        "bottom",
        "left",
        "transition",
        "transition-property",
        "transition-duration",
        "transition-delay",
        "transition-timing-function",
        "background-image",
        "box-shadow",
        "filter",
        "backdrop-filter",
        "animation",
        "animation-name",
        "animation-duration",
        "animation-delay",
        "animation-timing-function",
        "animation-iteration-count",
        "animation-direction",
        "animation-fill-mode",
        "animation-play-state",
    ] {
        assert!(is_supported_css_property(property), "{property}");
    }
    assert!(is_supported_css_property("--local-token"));
    assert!(!is_supported_css_property("grid-template-columns"));
    assert!(is_supported_css_property("transform"));
}

#[test]
fn style_profile_matrix_classifies_supported_visual_properties() {
    for (property, expected_status) in [
        ("background-color", StyleProfileStatus::Implemented),
        ("width", StyleProfileStatus::Implemented),
        ("padding", StyleProfileStatus::Implemented),
        ("border-width", StyleProfileStatus::Implemented),
        ("border-radius", StyleProfileStatus::Implemented),
        ("opacity", StyleProfileStatus::Implemented),
        ("transform", StyleProfileStatus::Implemented),
        ("background-image", StyleProfileStatus::Implemented),
        ("box-shadow", StyleProfileStatus::Implemented),
        ("filter", StyleProfileStatus::Implemented),
        ("display", StyleProfileStatus::Implemented),
        ("font-size", StyleProfileStatus::Implemented),
        ("animation-duration", StyleProfileStatus::Implemented),
        ("transition-property", StyleProfileStatus::Implemented),
        ("exclusive-zone", StyleProfileStatus::Implemented),
    ] {
        assert_eq!(
            style_profile_status(property),
            Some(expected_status),
            "{property}"
        );
    }
}

#[test]
fn style_profile_matrix_matches_supported_css_properties() {
    for property in supported_css_properties() {
        if property.starts_with("--") {
            continue;
        }

        assert!(
            style_profile_status(property).is_some(),
            "missing style profile row for {property}"
        );
    }
}

#[test]
fn style_profile_marks_browser_css_out_of_scope() {
    for property in [
        "grid-template-columns",
        "float",
        "container-type",
        "text-wrap",
    ] {
        assert_eq!(
            style_profile_status(property),
            Some(StyleProfileStatus::OutOfScope),
            "{property}"
        );
        assert!(
            !is_supported_css_property(property),
            "{property} must not be accepted as implemented shell CSS"
        );
    }
}

#[test]
fn keyframe_property_helper_accepts_transition_safe_properties() {
    for property in [
        "opacity",
        "transform",
        "border-radius",
        "padding",
        "font-size",
        "inset",
        "box-shadow",
        "filter",
        "backdrop-filter",
    ] {
        assert!(is_transition_safe_keyframe_property(property), "{property}");
    }
}

#[test]
fn keyframe_property_helper_rejects_unsupported_properties() {
    for property in ["grid-template-columns", "display"] {
        assert!(
            !is_transition_safe_keyframe_property(property),
            "{property}"
        );
    }
}

#[test]
fn backdrop_filter_parses_blur_refraction_and_dispersion() {
    let filter =
        parse::parse_backdrop_filter("blur(12px) refract(24px 18px) dispersion(2px) grain(0.12)");
    assert_eq!(filter.blur_radius, 12.0);
    assert_eq!(filter.grain, 0.12);
    assert!(filter.has_refraction());
    assert_eq!(
        (filter.refract_depth, filter.refract_edge, filter.dispersion),
        (24.0, 18.0, 2.0)
    );

    let plain = parse::parse_backdrop_filter("refract(0px 0px)");
    assert!(
        plain.has_refraction(),
        "refract(0 0) still samples the backdrop source"
    );
    assert!(!plain.is_none());

    assert!(!parse::parse_backdrop_filter("blur(8px)").has_refraction());
    assert_eq!(parse::parse_backdrop_filter("grain(2)").grain, 1.0);
    assert!(!parse::parse_backdrop_filter("grain(0.1)").is_none());
    assert!(parse::parse_backdrop_filter("grain(NaN)").is_none());
    assert!(parse::parse_backdrop_filter("grain(1px)").is_none());
    assert!(parse::parse_backdrop_filter("blur(8px) wobble(2px)").is_none());
    assert!(
        !parse::parse_filter("refract(20px)").has_refraction(),
        "`filter` has no backdrop to refract"
    );
}

#[test]
fn transform_functions_resolve_nested_motion_tokens() {
    let theme = mesh_core_theme::default_theme();
    let resolver = StyleResolver::new(&theme);
    let rules = parse_fixture_style(
        r#"
<style>
.press { transform: scale(var(--animation-scale-press)); }
.lift { --lift: -3px; transform: translateY(var(--lift)); }
</style>
"#,
    );
    let (press, diagnostics) = resolve_class(&resolver, &rules, "press");
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    assert!(
        press.transform.scale_x > 0.5 && press.transform.scale_x < 1.0,
        "--animation-scale-press resolved inside scale(): {}",
        press.transform.scale_x
    );
    let (lift, _) = resolve_class(&resolver, &rules, "lift");
    assert_eq!(lift.transform.translate_y, -3.0);
}

#[test]
fn corner_shape_shorthand_and_longhands_resolve_per_corner() {
    let source = r#"
<template><box /></template>
<style>
.foot { corner-shape: scoop round; }
.foot.left { corner-bottom-right-shape: scoop; corner-top-right-shape: bevel; }
</style>
"#;
    let rules = mesh_core_component::parse_component(source)
        .unwrap()
        .style
        .unwrap()
        .rules;
    let theme = mesh_core_theme::default_theme();
    let resolver = StyleResolver::new(&theme);
    let style = resolver.resolve_node_style(
        &rules,
        "box",
        &["foot".to_string(), "left".to_string()],
        None,
        StyleContext::default(),
        NodeState::default(),
    );

    assert_eq!(
        style.corner_shape,
        CornerShapes {
            top_left: CornerShape::Scoop,
            top_right: CornerShape::Round,
            bottom_right: CornerShape::Scoop,
            bottom_left: CornerShape::Round,
        },
        "two values expand like border-radius; unsupported keywords are round"
    );
    assert!(!style.corner_shape.scoops(Corners::zero()));
    assert_eq!(
        style.corner_shape.round_radii(Corners::all(8.0)),
        Corners {
            top_left: 0.0,
            top_right: 8.0,
            bottom_right: 0.0,
            bottom_left: 8.0,
        }
    );
}
