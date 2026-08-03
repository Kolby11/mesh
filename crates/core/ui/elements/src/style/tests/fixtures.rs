use super::super::*;
use super::common::*;
use crate::tree::ElementState;
use mesh_core_component::style::{Declaration, Selector, StyleRule, StyleValue, prop_variable_key};

#[test]
fn shipped_navigation_style_variable_resolution_uses_theme_pipeline() {
    let theme = mesh_core_theme::default_theme();
    let resolver = StyleResolver::new(&theme);
    let rules = vec![StyleRule {
        selector: Selector::Class("nav-shell".to_string()),
        declarations: vec![
            Declaration {
                property: "background".to_string(),
                value: StyleValue::Var("--color-surface".to_string()),
            },
            Declaration {
                property: "color".to_string(),
                value: StyleValue::Var("--color-on-surface".to_string()),
            },
            Declaration {
                property: "padding-inline".to_string(),
                value: StyleValue::Var("--spacing-lg".to_string()),
            },
            Declaration {
                property: "border-radius".to_string(),
                value: StyleValue::Var("--radius-md".to_string()),
            },
            Declaration {
                property: "transition-duration".to_string(),
                value: StyleValue::Var("--animation-duration-short".to_string()),
            },
            Declaration {
                property: "animation-duration".to_string(),
                value: StyleValue::Var("--animation-duration-long".to_string()),
            },
        ],
        container_query: None,
    }];

    let (style, diagnostics) = resolve_class(&resolver, &rules, "nav-shell");

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    assert_eq!(style.background_color, Color::from_hex("#1C1B1F").unwrap());
    assert_eq!(style.color, Color::from_hex("#E6E1E5").unwrap());
    assert_eq!(style.padding.left, 24.0);
    assert_eq!(style.padding.right, 24.0);
    assert_eq!(style.border_radius, Corners::all(8.0));
    assert_eq!(style.transitions[0].duration_ms, 150);
    assert_eq!(style.animations[0].duration_ms, 360);
}

#[test]
fn shipped_navigation_style_custom_properties_remain_local_variables() {
    let theme = mesh_core_theme::default_theme();
    let resolver = StyleResolver::new(&theme);
    let rules = vec![StyleRule {
        selector: Selector::Class("panel".to_string()),
        declarations: vec![
            Declaration {
                property: "--surface".to_string(),
                value: StyleValue::Var("--color-surface-container".to_string()),
            },
            Declaration {
                property: "background".to_string(),
                value: StyleValue::Var("--surface".to_string()),
            },
        ],
        container_query: None,
    }];

    let (style, diagnostics) = resolve_class(&resolver, &rules, "panel");

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    assert_eq!(style.background_color, Color::from_hex("#211F26").unwrap());
}

#[test]
fn shipped_navigation_style_animation_variable_failures_are_actionable() {
    let theme = mesh_core_theme::default_theme();
    let resolver = StyleResolver::new(&theme);
    let rules = vec![StyleRule {
        selector: Selector::Class("nav-shell".to_string()),
        declarations: vec![Declaration {
            property: "transition-duration".to_string(),
            value: StyleValue::Var("--animation-duration-not-real".to_string()),
        }],
        container_query: None,
    }];

    let (_style, diagnostics) = resolve_class(&resolver, &rules, "nav-shell");

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].property, "transition-duration");
    assert!(
        diagnostics[0]
            .message
            .contains("animation.duration.not-real")
    );
}

#[test]
fn shipped_navigation_style_fixtures_parse_without_syntax_regression() {
    let nav_rules = parse_fixture_style(include_str!(
        "../../../../../../../modules/frontend/navigation-bar/src/main.mesh"
    ));
    let volume_rules = parse_fixture_style(include_str!(
        "../../../../../../../modules/frontend/navigation-bar/src/components/volume-button.mesh"
    ));

    assert!(
        nav_rules
            .iter()
            .any(|rule| selector_has_class(&rule.selector, "nav-shell"))
    );
    assert!(
        volume_rules
            .iter()
            .any(|rule| selector_has_class(&rule.selector, "nav-button"))
    );
}

#[test]
fn shipped_navigation_style_expected_diagnostics_do_not_block_tokens() {
    let theme = mesh_core_theme::default_theme();
    let resolver = StyleResolver::new(&theme).with_props(std::collections::HashMap::from([(
        prop_variable_key("blur_background"),
        StyleValue::Var("--effect-backdrop-blur-surface-background".into()),
    )]));
    let mut rules = parse_fixture_style(include_str!(
        "../../../../../../../modules/frontend/navigation-bar/src/main.mesh"
    ));
    rules.extend(parse_fixture_style(include_str!(
        "../../../../../../../modules/frontend/navigation-bar/src/components/volume-button.mesh"
    )));
    // The hover lift lives in the settings button since the navigation triggers
    // stopped sharing one `.nav-button:hover` rule.
    rules.extend(parse_fixture_style(include_str!(
        "../../../../../../../modules/frontend/navigation-bar/src/components/settings-button.mesh"
    )));

    let (nav_style, nav_diagnostics) = resolve_class(&resolver, &rules, "nav-shell");
    let (status_style, status_diagnostics) = resolve_class(&resolver, &rules, "status-primary");
    let (button_style, button_diagnostics) = resolve_class(&resolver, &rules, "nav-button");
    let diagnostic_properties: std::collections::BTreeSet<_> = nav_diagnostics
        .iter()
        .chain(status_diagnostics.iter())
        .chain(button_diagnostics.iter())
        .map(|diagnostic| diagnostic.property.as_str())
        .collect();

    assert!(diagnostic_properties.contains("container-type"));
    assert!(diagnostic_properties.contains("text-wrap"));
    assert!(diagnostic_properties.contains("border-style"));
    assert_eq!(
        nav_style.background_color,
        Color {
            r: 10,
            g: 10,
            b: 14,
            a: 92,
        }
    );
    assert_eq!(nav_style.padding.left, 16.0);
    assert_eq!(status_style.font_size, 14.0);
    assert_eq!(button_style.border_width, Edges::all(2.0));
    assert_eq!(
        button_style.background_color,
        Color::from_hex("#211F26").unwrap()
    );

    let (hovered_button_style, hovered_button_diagnostics) = resolver
        .resolve_node_style_with_diagnostics(
            &rules,
            "button",
            &["nav-button".to_string()],
            None,
            StyleContext::default(),
            ElementState {
                hovered: true,
                ..ElementState::default()
            },
        );
    assert!(
        hovered_button_diagnostics
            .iter()
            .all(|diagnostic| diagnostic.property != "transform")
    );
    assert_eq!(hovered_button_style.transform.translate_y, -1.0);
    assert!((hovered_button_style.transform.scale_x - 1.04).abs() < 0.001);
}

#[test]
fn shipped_audio_style_fixture_resolves_painter_relevant_values() {
    let theme = mesh_core_theme::default_theme();
    let resolver = StyleResolver::new(&theme).with_props(std::collections::HashMap::from([(
        prop_variable_key("blur_background"),
        StyleValue::Var("--effect-backdrop-blur-popup-background".into()),
    )]));
    let rules = parse_fixture_style(include_str!(
        "../../../../../../../modules/frontend/audio-popover/src/main.mesh"
    ));

    let (style, diagnostics) = resolve_class(&resolver, &rules, "audio-popover");

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    assert_eq!(
        style.background_color,
        Color {
            r: 24,
            g: 26,
            b: 34,
            a: 71,
        }
    );
    assert_eq!(style.color, Color::from_hex("#E6E1E5").unwrap());
    assert_eq!(style.padding, Edges::all(8.0));
    assert_eq!(style.border_radius, Corners::all(16.0));
    assert_eq!(style.gap, 4.0);
}
