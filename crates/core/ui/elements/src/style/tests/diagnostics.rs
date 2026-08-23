use super::super::*;
use super::common::*;
use crate::tree::ElementState;
use mesh_core_component::style::{Declaration, Selector, StyleRule, StyleValue};

fn resolve_cycle_rules(declarations: Vec<Declaration>) -> (ComputedStyle, Vec<StyleDiagnostic>) {
    let theme = mesh_core_theme::default_theme();
    let resolver = StyleResolver::new(&theme);
    resolver.resolve_node_style_with_diagnostics(
        &[StyleRule {
            selector: Selector::Class("cycle".to_string()),
            declarations,
            container_query: None,
        }],
        "box",
        &["cycle".to_string()],
        None,
        StyleContext::default(),
        ElementState::default(),
    )
}

#[test]
fn direct_custom_property_cycle_is_diagnostic_instead_of_recursing() {
    let (style, diagnostics) = resolve_cycle_rules(vec![
        Declaration {
            property: "color".to_string(),
            value: StyleValue::Literal("#00ff00".to_string()),
        },
        Declaration {
            property: "--a".to_string(),
            value: StyleValue::Var("--a".to_string()),
        },
        Declaration {
            property: "color".to_string(),
            value: StyleValue::Var("--a".to_string()),
        },
    ]);

    assert_eq!(style.color, Color::from_hex("#00ff00").unwrap());
    assert_eq!(diagnostics.len(), 1);
    assert!(
        diagnostics[0]
            .message
            .contains("cyclic CSS custom-property")
    );
    assert!(diagnostics[0].message.contains("--a -> --a"));
}

#[test]
fn indirect_custom_property_cycle_reports_the_dependency_path() {
    let (_style, diagnostics) = resolve_cycle_rules(vec![
        Declaration {
            property: "--a".to_string(),
            value: StyleValue::Var("--b".to_string()),
        },
        Declaration {
            property: "--b".to_string(),
            value: StyleValue::Var("--a".to_string()),
        },
        Declaration {
            property: "font-size".to_string(),
            value: StyleValue::Var("--a".to_string()),
        },
    ]);

    assert_eq!(diagnostics.len(), 1);
    assert!(diagnostics[0].message.contains("--a -> --b -> --a"));
}

#[test]
fn custom_property_cycle_uses_var_fallback_and_remains_diagnostic() {
    let (style, diagnostics) = resolve_cycle_rules(vec![
        Declaration {
            property: "--a".to_string(),
            value: StyleValue::Var("--b".to_string()),
        },
        Declaration {
            property: "--b".to_string(),
            value: StyleValue::Var("--a".to_string()),
        },
        Declaration {
            property: "color".to_string(),
            value: StyleValue::Var("--a, var(--cycle-fallback, #ff0000)".to_string()),
        },
    ]);

    assert_eq!(style.color, Color::from_hex("#ff0000").unwrap());
    assert_eq!(diagnostics.len(), 1);
    assert!(
        diagnostics[0]
            .message
            .contains("cyclic CSS custom-property")
    );
}

#[test]
fn style_diagnostics_unsupported_property_produces_style_diagnostic() {
    let theme = mesh_core_theme::default_theme();
    let resolver = StyleResolver::new(&theme);
    let rules = vec![StyleRule {
        selector: Selector::Class("panel".to_string()),
        declarations: vec![mesh_core_component::style::Declaration {
            property: "grid-template-columns".to_string(),
            value: StyleValue::Literal("1fr 1fr".to_string()),
        }],
        container_query: None,
    }];

    let (_style, diagnostics) = resolver.resolve_node_style_with_diagnostics(
        &rules,
        "box",
        &["panel".to_string()],
        None,
        StyleContext::default(),
        ElementState::default(),
    );

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].property, "grid-template-columns");
    assert_eq!(diagnostics[0].selector.as_deref(), Some(".panel"));
    assert!(
        diagnostics[0]
            .message
            .contains("unsupported CSS property 'grid-template-columns'")
    );
}

#[test]
fn style_diagnostics_transform_origin_is_accepted_and_lowered() {
    let theme = mesh_core_theme::default_theme();
    let resolver = StyleResolver::new(&theme);
    let rules = vec![StyleRule {
        selector: Selector::Class("panel".to_string()),
        declarations: vec![Declaration {
            property: "transform-origin".to_string(),
            value: StyleValue::Literal("center".to_string()),
        }],
        container_query: None,
    }];

    let (style, diagnostics) = resolve_class(&resolver, &rules, "panel");

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    assert_eq!(
        style.transform_origin.x,
        TransformOriginValue::Percent(50.0)
    );
    assert_eq!(
        style.transform_origin.y,
        TransformOriginValue::Percent(50.0)
    );
}

#[test]
fn style_diagnostics_browser_layout_properties_are_unsupported() {
    let theme = mesh_core_theme::default_theme();
    let resolver = StyleResolver::new(&theme);
    let rules = vec![StyleRule {
        selector: Selector::Class("panel".to_string()),
        declarations: vec![
            Declaration {
                property: "container-type".to_string(),
                value: StyleValue::Literal("inline-size".to_string()),
            },
            Declaration {
                property: "text-wrap".to_string(),
                value: StyleValue::Literal("nowrap".to_string()),
            },
        ],
        container_query: None,
    }];

    let (_style, diagnostics) = resolve_class(&resolver, &rules, "panel");
    let properties: std::collections::BTreeSet<_> = diagnostics
        .iter()
        .map(|diagnostic| diagnostic.property.as_str())
        .collect();

    assert_eq!(properties.len(), 2);
    assert!(properties.contains("container-type"));
    assert!(properties.contains("text-wrap"));
    for diagnostic in diagnostics {
        assert!(diagnostic.message.contains("unsupported"));
        assert!(diagnostic.message.contains(&diagnostic.property));
    }
}

#[test]
fn style_diagnostics_border_style_is_diagnostic_only() {
    let theme = mesh_core_theme::default_theme();
    let resolver = StyleResolver::new(&theme);
    let rules = vec![StyleRule {
        selector: Selector::Class("panel".to_string()),
        declarations: vec![Declaration {
            property: "border-style".to_string(),
            value: StyleValue::Literal("solid".to_string()),
        }],
        container_query: None,
    }];

    let (_style, diagnostics) = resolve_class(&resolver, &rules, "panel");

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].property, "border-style");
    assert!(diagnostics[0].message.contains("diagnostic-only"));
    assert!(diagnostics[0].message.contains("not lowered"));
}

#[test]
fn style_diagnostics_shipped_navigation_fixture_expected_properties_are_exact() {
    let theme = mesh_core_theme::default_theme();
    let resolver = StyleResolver::new(&theme);
    let mut rules = parse_fixture_style(include_str!(
        "../../../../../../../modules/frontend/navigation-bar/src/main.mesh"
    ));
    rules.extend(parse_fixture_style(include_str!(
        "../../../../../../../modules/frontend/navigation-bar/src/components/volume-button.mesh"
    )));

    let (_nav_style, nav_diagnostics) = resolve_class(&resolver, &rules, "nav-shell");
    let (_status_style, status_diagnostics) = resolve_class(&resolver, &rules, "status-primary");
    let (_button_style, button_diagnostics) = resolve_class(&resolver, &rules, "nav-button");
    let properties: std::collections::BTreeSet<_> = nav_diagnostics
        .iter()
        .chain(status_diagnostics.iter())
        .chain(button_diagnostics.iter())
        .map(|diagnostic| diagnostic.property.as_str())
        .collect();

    assert_eq!(
        properties,
        std::collections::BTreeSet::from(["border-style", "container-type", "text-wrap"])
    );
}

#[test]
fn style_diagnostics_descendant_selector_out_of_scope_documented() {
    let rules = parse_fixture_style(include_str!(
        "../../../../../../../modules/frontend/navigation-bar/src/components/volume-button.mesh"
    ));
    let docs = include_str!("../../../../../../../docs/css-coverage.md");

    assert!(
        rules.iter().any(|rule| {
            selector_has_class(&rule.selector, "nav-button")
                && selector_has_class(&rule.selector, "nav-button-glyph")
        }),
        "fixture should preserve current descendant-like selector lowering shape"
    );
    assert!(docs.contains("Descendant"));
    assert!(docs.contains("out-of-scope"));
}

#[test]
fn animation_variable_duration_resolves_from_theme() {
    let theme = mesh_core_theme::default_theme();
    let resolver = StyleResolver::new(&theme);
    let rules = vec![StyleRule {
        selector: Selector::Class("panel".to_string()),
        declarations: vec![mesh_core_component::style::Declaration {
            property: "animation-duration".to_string(),
            value: StyleValue::Var("--animation-duration-fast".to_string()),
        }],
        container_query: None,
    }];

    let (style, diagnostics) = resolver.resolve_node_style_with_diagnostics(
        &rules,
        "box",
        &["panel".to_string()],
        None,
        StyleContext::default(),
        ElementState::default(),
    );

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    assert_eq!(style.animations[0].duration_ms, 90);
}

#[test]
fn transition_hyphenated_animation_curve_resolves_from_theme() {
    let mut theme = mesh_core_theme::Theme::new("animation-test", "Animation test");
    theme.tokens_mut().extend([
        (
            "animation.duration.short".into(),
            mesh_core_theme::TokenValue::Number(150.0),
        ),
        (
            "animation.curves.bezier.emphasized-decelerate".into(),
            mesh_core_theme::TokenValue::String("cubic-bezier(0.05, 0.7, 0.1, 1)".into()),
        ),
    ]);
    let resolver = StyleResolver::new(&theme);
    let rules = vec![StyleRule {
        selector: Selector::Class("panel".to_string()),
        declarations: vec![Declaration {
            property: "transition".to_string(),
            value: StyleValue::Literal(
                "opacity var(--animation-duration-short) var(--animation-curves-bezier-emphasized-decelerate)"
                    .into(),
            ),
        }],
        container_query: None,
    }];

    let (style, diagnostics) = resolver.resolve_node_style_with_diagnostics(
        &rules,
        "box",
        &["panel".to_string()],
        None,
        StyleContext::default(),
        ElementState::default(),
    );

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    assert_eq!(style.transitions[0].duration_ms, 150);
    assert_eq!(
        style.transitions[0].easing,
        TransitionEasing::CubicBezier(0.05, 0.7, 0.1, 1.0)
    );
}

#[test]
fn invalid_animation_variable_produces_diagnostic_and_skips_declaration() {
    let theme = mesh_core_theme::default_theme();
    let resolver = StyleResolver::new(&theme);
    let rules = vec![StyleRule {
        selector: Selector::Class("panel".to_string()),
        declarations: vec![mesh_core_component::style::Declaration {
            property: "animation-duration".to_string(),
            value: StyleValue::Var("--animation-duration-fastest".to_string()),
        }],
        container_query: None,
    }];

    let (style, diagnostics) = resolver.resolve_node_style_with_diagnostics(
        &rules,
        "box",
        &["panel".to_string()],
        None,
        StyleContext::default(),
        ElementState::default(),
    );

    assert_eq!(style.animations[0].duration_ms, 0);
    assert_eq!(diagnostics.len(), 1);
    assert!(
        diagnostics[0]
            .message
            .contains("animation.duration.fastest")
    );
}
