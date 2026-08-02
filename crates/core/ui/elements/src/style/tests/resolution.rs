use super::super::*;
use super::common::*;
use crate::tree::ElementState;
use mesh_core_component::style::{Declaration, Selector, StyleRule, StyleValue};

#[test]
fn resolve_node_style_from_rules() {
    let theme = mesh_core_theme::default_theme();
    let resolver = StyleResolver::new(&theme);

    let rules = vec![StyleRule {
        selector: Selector::Tag("text".to_string()),
        declarations: vec![
            mesh_core_component::style::Declaration {
                property: "font-size".to_string(),
                value: StyleValue::Literal("20px".to_string()),
            },
            mesh_core_component::style::Declaration {
                property: "color".to_string(),
                value: StyleValue::Var("--color-primary".to_string()),
            },
        ],
        container_query: None,
    }];

    let style = resolver.resolve_node_style(
        &rules,
        "text",
        &[],
        None,
        StyleContext::default(),
        ElementState::default(),
    );
    assert_eq!(style.font_size, 20.0);
    assert_eq!(
        style.color,
        Color {
            r: 103,
            g: 80,
            b: 164,
            a: 255
        }
    );
}

#[test]
fn resolve_paint_effects_from_rules() {
    let theme = mesh_core_theme::default_theme();
    let resolver = StyleResolver::new(&theme);

    let rules = vec![StyleRule {
        selector: Selector::Class("panel".to_string()),
        declarations: vec![
            mesh_core_component::style::Declaration {
                property: "box-shadow".to_string(),
                value: StyleValue::Literal("2px 4px 8px 1px #00000080".to_string()),
            },
            mesh_core_component::style::Declaration {
                property: "filter".to_string(),
                value: StyleValue::Literal("blur(3px)".to_string()),
            },
            mesh_core_component::style::Declaration {
                property: "backdrop-filter".to_string(),
                value: StyleValue::Literal("blur(5px)".to_string()),
            },
        ],
        container_query: None,
    }];

    let style = resolver.resolve_node_style(
        &rules,
        "box",
        &["panel".to_string()],
        None,
        StyleContext::default(),
        ElementState::default(),
    );

    assert_eq!(style.box_shadow.offset_x, 2.0);
    assert_eq!(style.box_shadow.offset_y, 4.0);
    assert_eq!(style.box_shadow.blur_radius, 8.0);
    assert_eq!(style.box_shadow.spread_radius, 1.0);
    assert_eq!(
        style.box_shadow.color,
        Color::from_hex("#00000080").unwrap()
    );
    assert_eq!(style.filter.blur_radius, 3.0);
    assert_eq!(style.backdrop_filter.blur_radius, 5.0);
}

#[test]
fn style_background_image_url_resolves_backend_neutral_source() {
    let theme = mesh_core_theme::default_theme();
    let resolver = StyleResolver::new(&theme);
    let rules = vec![StyleRule {
        selector: Selector::Class("panel".to_string()),
        declarations: vec![mesh_core_component::style::Declaration {
            property: "background-image".to_string(),
            value: StyleValue::Literal("url(\"assets/panel.png\")".to_string()),
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
    assert_eq!(
        style.background_paint,
        BackgroundPaint::Image(StyleImageSource {
            path: "assets/panel.png".to_string(),
        })
    );
}

#[test]
fn style_background_linear_gradient_resolves_two_colors() {
    let theme = mesh_core_theme::default_theme();
    let resolver = StyleResolver::new(&theme);
    let rules = vec![StyleRule {
        selector: Selector::Class("panel".to_string()),
        declarations: vec![mesh_core_component::style::Declaration {
            property: "background-image".to_string(),
            value: StyleValue::Literal("linear-gradient(to bottom, #112233, #445566)".to_string()),
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
    assert_eq!(
        style.background_paint,
        BackgroundPaint::LinearGradient(StyleLinearGradient {
            from: Color::from_hex("#112233").unwrap(),
            to: Color::from_hex("#445566").unwrap(),
        })
    );
}

#[test]
fn style_background_image_unsupported_value_records_diagnostic() {
    let theme = mesh_core_theme::default_theme();
    let resolver = StyleResolver::new(&theme);
    let rules = vec![StyleRule {
        selector: Selector::Class("panel".to_string()),
        declarations: vec![mesh_core_component::style::Declaration {
            property: "background-image".to_string(),
            value: StyleValue::Literal("radial-gradient(#000, #fff)".to_string()),
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

    assert_eq!(style.background_paint, BackgroundPaint::None);
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.property == "background-image"
            && diagnostic.message.contains("unsupported background-image")
    }));
}

#[test]
fn module_component_defaults_are_subtree_scoped() {
    let mut theme = mesh_core_theme::Theme::new("scoped", "Scoped");
    *theme.tokens_mut() = std::collections::HashMap::from([
        (
            "color.on-background".into(),
            mesh_core_theme::TokenValue::String("#112233".into()),
        ),
        (
            "animation.duration.short".into(),
            mesh_core_theme::TokenValue::Number(150.0),
        ),
        (
            "animation.curves.bezier.standard".into(),
            mesh_core_theme::TokenValue::String("ease".into()),
        ),
    ]);
    *theme.defaults_mut() = mesh_core_theme::ThemeDefaults {
        components: std::collections::HashMap::from([(
            "base".into(),
            [("color".into(), "var(--color-on-background)".into())]
                .into_iter()
                .collect(),
        )]),
    };
    theme.modules_mut().insert(
        "@mesh/weather".into(),
        mesh_core_theme::ThemeModule {
            tokens: std::collections::HashMap::from([(
                "weather.color.sunny".into(),
                mesh_core_theme::TokenValue::String("#f6b73c".into()),
            )]),
            defaults: mesh_core_theme::ThemeDefaults {
                components: std::collections::HashMap::from([
                    (
                        "base".into(),
                        [(
                            "transition".into(),
                            "background-color var(--animation-duration-short) var(--animation-curves-bezier-standard)"
                                .into(),
                        )]
                        .into_iter()
                        .collect(),
                    ),
                    (
                        "button".into(),
                        [(
                            "background".into(),
                            "var(--weather-color-sunny)".into(),
                        )]
                        .into_iter()
                        .collect(),
                    ),
                ]),
            },
        },
    );

    let resolver = StyleResolver::new(&theme);

    let outside = resolver.resolve_node_style_for_module(
        &[],
        "button",
        &[],
        None,
        StyleContext::default(),
        ElementState::default(),
        None,
    );
    assert_eq!(outside.color, Color::from_hex("#112233").unwrap());
    assert_eq!(outside.background_color, Color::TRANSPARENT);
    assert_eq!(outside.transitions[0].duration_ms, 0);

    let inside = resolver.resolve_node_style_for_module(
        &[],
        "button",
        &[],
        None,
        StyleContext::default(),
        ElementState::default(),
        Some("@mesh/weather"),
    );
    assert_eq!(inside.color, Color::from_hex("#112233").unwrap());
    assert_eq!(inside.background_color, Color::from_hex("#f6b73c").unwrap());
    assert_eq!(inside.transitions[0].duration_ms, 150);
    assert!(inside.transitions[0].properties.animates_background_color());
}

#[test]
pub(super) fn container_query_rules_apply_against_context() {
    let theme = mesh_core_theme::default_theme();
    let resolver = StyleResolver::new(&theme);
    let rules = vec![StyleRule {
        selector: Selector::Class("panel".to_string()),
        declarations: vec![mesh_core_component::style::Declaration {
            property: "overflow-y".to_string(),
            value: StyleValue::Literal("auto".to_string()),
        }],
        container_query: Some(mesh_core_component::style::ContainerQuery {
            min_width: Some(480.0),
            ..Default::default()
        }),
    }];

    let narrow = resolver.resolve_node_style(
        &rules,
        "column",
        &["panel".into()],
        None,
        StyleContext {
            container_width: 320.0,
            container_height: 240.0,
        },
        ElementState::default(),
    );
    assert_eq!(narrow.overflow_y, Overflow::Visible);

    let wide = resolver.resolve_node_style(
        &rules,
        "column",
        &["panel".into()],
        None,
        StyleContext {
            container_width: 640.0,
            container_height: 240.0,
        },
        ElementState::default(),
    );
    assert_eq!(wide.overflow_y, Overflow::Auto);
}

#[test]
pub(super) fn pseudo_state_rules_apply_when_state_matches() {
    use crate::tree::ElementState;
    use mesh_core_component::style::{Declaration, Selector};

    let theme = mesh_core_theme::default_theme();
    let resolver = StyleResolver::new(&theme);

    let rules = vec![
        StyleRule {
            selector: Selector::Tag("button".to_string()),
            declarations: vec![Declaration {
                property: "background-color".to_string(),
                value: StyleValue::Literal("#333333".to_string()),
            }],
            container_query: None,
        },
        StyleRule {
            selector: Selector::State("button".to_string(), "hover".to_string()),
            declarations: vec![Declaration {
                property: "background-color".to_string(),
                value: StyleValue::Literal("#ffffff".to_string()),
            }],
            container_query: None,
        },
    ];

    let idle = resolver.resolve_node_style(
        &rules,
        "button",
        &[],
        None,
        StyleContext::default(),
        ElementState::default(),
    );
    assert_eq!(idle.background_color, Color::from_hex("#333333").unwrap());

    let hovered = resolver.resolve_node_style(
        &rules,
        "button",
        &[],
        None,
        StyleContext::default(),
        ElementState {
            hovered: true,
            ..Default::default()
        },
    );
    assert_eq!(
        hovered.background_color,
        Color::from_hex("#ffffff").unwrap()
    );
}

#[test]
fn targeted_restyle_recomputes_only_named_stateful_nodes() {
    let theme = mesh_core_theme::default_theme();
    let resolver = StyleResolver::new(&theme);
    let rules = vec![StyleRule {
        selector: Selector::State("button".to_string(), "hover".to_string()),
        declarations: vec![mesh_core_component::style::Declaration {
            property: "background-color".to_string(),
            value: StyleValue::Literal("#ff0000".to_string()),
        }],
        container_query: None,
    }];
    let mut root = crate::tree::WidgetNode::new("row");
    let mut previous = crate::tree::WidgetNode::new("button");
    previous.id = 1;
    previous
        .attributes
        .insert("_mesh_key".into(), "prev".into());
    previous.state.hovered = false;
    previous.computed_style.background_color = Color::from_hex("#ff0000").unwrap();
    let mut current = crate::tree::WidgetNode::new("button");
    current.id = 2;
    current.attributes.insert("_mesh_key".into(), "next".into());
    current.state.hovered = true;
    root.children.push(previous);
    root.children.push(current);

    let target_ids = std::collections::HashSet::from([1, 2]);
    resolver.restyle_subtree_for_ids(&mut root, &rules, StyleContext::default(), &target_ids);

    let idle_button = resolver.resolve_node_style(
        &[],
        "button",
        &[],
        None,
        StyleContext::default(),
        ElementState::default(),
    );
    assert_eq!(
        root.children[0].computed_style.background_color,
        idle_button.background_color
    );
    assert_eq!(
        root.children[1].computed_style.background_color,
        Color::from_hex("#ff0000").unwrap()
    );
}

#[test]
fn targeted_restyle_skips_descendant_rule_resolution_for_non_inherited_change() {
    let theme = mesh_core_theme::default_theme();
    let resolver = StyleResolver::new(&theme);
    let rules = vec![StyleRule {
        selector: Selector::State("row".into(), "hover".into()),
        declarations: vec![Declaration {
            property: "background-color".into(),
            value: StyleValue::Literal("#ff0000".into()),
        }],
        container_query: None,
    }];
    let mut root = crate::tree::WidgetNode::new("row");
    root.id = 1;
    let mut child = crate::tree::WidgetNode::new("text");
    child.id = 2;
    root.children.push(child);
    resolver.restyle_subtree_cached(&mut root, &rules, StyleContext::default(), &mut None);
    root.children[0].computed_style.background_color = Color::from_hex("#123456").unwrap();
    root.state.hovered = true;

    resolver.restyle_subtree_for_ids(
        &mut root,
        &rules,
        StyleContext::default(),
        &std::collections::HashSet::from([1]),
    );

    assert_eq!(
        root.children[0].computed_style.background_color,
        Color::from_hex("#123456").unwrap(),
        "a non-inherited parent change must not re-resolve clean descendants"
    );
}

#[test]
fn profiled_restyle_matches_normal_style_and_attributes_matching_rules() {
    let theme = mesh_core_theme::default_theme();
    let resolver = StyleResolver::new(&theme);
    let rules = parse_fixture_style(
        r#"
<style>
.tracked { background-color: #123456; }
.unmatched { color: #ff0000; }
</style>
"#,
    );
    let mut normal = crate::tree::WidgetNode::new("box");
    normal.attributes.insert("class".into(), "tracked".into());
    let mut profiled = normal.clone();
    resolver.restyle_subtree_cached(&mut normal, &rules, StyleContext::default(), &mut None);
    let mut attribution = StyleRuleAttribution::new(&rules);
    resolver.restyle_subtree_cached_profiled(
        &mut profiled,
        &rules,
        StyleContext::default(),
        &mut None,
        &mut attribution,
    );

    assert_eq!(
        profiled.computed_style.background_color,
        normal.computed_style.background_color
    );
    assert_eq!(profiled.computed_style.color, normal.computed_style.color);
    assert_eq!(profiled.computed_style.width, normal.computed_style.width);
    assert_eq!(
        profiled.computed_style.font_family,
        normal.computed_style.font_family
    );
    let entries: Vec<_> = attribution.entries().collect();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].selector, ".tracked");
    assert_eq!(entries[0].match_count, 1);
}

#[test]
fn targeted_restyle_propagates_changed_inherited_fields() {
    let theme = mesh_core_theme::default_theme();
    let resolver = StyleResolver::new(&theme);
    let rules = vec![StyleRule {
        selector: Selector::State("row".into(), "hover".into()),
        declarations: vec![Declaration {
            property: "color".into(),
            value: StyleValue::Literal("#ff0000".into()),
        }],
        container_query: None,
    }];
    let mut root = crate::tree::WidgetNode::new("row");
    root.id = 1;
    let mut child = crate::tree::WidgetNode::new("box");
    child.id = 2;
    root.children.push(child);
    resolver.restyle_subtree_cached(&mut root, &rules, StyleContext::default(), &mut None);
    let mut full_tree = root.clone();
    full_tree.children[0].computed_style.background_color = Color::from_hex("#123456").unwrap();
    root.children[0].computed_style.background_color = Color::from_hex("#123456").unwrap();
    full_tree.state.hovered = true;
    root.state.hovered = true;

    resolver.restyle_subtree_cached(&mut full_tree, &rules, StyleContext::default(), &mut None);

    resolver.restyle_subtree_for_ids(
        &mut root,
        &rules,
        StyleContext::default(),
        &std::collections::HashSet::from([1]),
    );

    assert_eq!(
        (
            root.children[0].computed_style.color,
            root.children[0].computed_style.font_family.clone(),
            root.children[0].computed_style.font_size,
            root.children[0].computed_style.font_weight,
            root.children[0].computed_style.line_height,
            root.children[0].computed_style.background_color,
        ),
        (
            full_tree.children[0].computed_style.color,
            full_tree.children[0].computed_style.font_family.clone(),
            full_tree.children[0].computed_style.font_size,
            full_tree.children[0].computed_style.font_weight,
            full_tree.children[0].computed_style.line_height,
            full_tree.children[0].computed_style.background_color,
        ),
        "targeted inherited-field propagation must match a full restyle"
    );
    assert_eq!(
        root.children[0].computed_style.background_color,
        Color::TRANSPARENT,
        "changed inherited fields must still restyle descendants"
    );
}

// cargo test -p mesh-core-elements --release -- non_inherited_targeted_restyle_beats_full_descendant_restyle_benchmark --ignored --nocapture
#[test]
#[ignore = "release-only targeted interaction restyle microbenchmark"]
fn non_inherited_targeted_restyle_beats_full_descendant_restyle_benchmark() {
    use std::time::Instant;

    let theme = mesh_core_theme::default_theme();
    let resolver = StyleResolver::new(&theme);
    let rules = vec![
        StyleRule {
            selector: Selector::Class("plain".into()),
            declarations: vec![Declaration {
                property: "background-color".into(),
                value: StyleValue::Literal("#222222".into()),
            }],
            container_query: None,
        },
        StyleRule {
            selector: Selector::State("row".into(), "hover".into()),
            declarations: vec![Declaration {
                property: "background-color".into(),
                value: StyleValue::Literal("#444444".into()),
            }],
            container_query: None,
        },
    ];
    let mut root = crate::tree::WidgetNode::new("row");
    root.id = 1;
    root.children = (0..2_048)
        .map(|index| {
            let mut child = crate::tree::WidgetNode::new("box");
            child.id = index + 2;
            child.attributes.insert("class".into(), "plain".into());
            child
        })
        .collect();
    let mut initial_index = None;
    resolver.restyle_subtree_cached(
        &mut root,
        &rules,
        StyleContext::default(),
        &mut initial_index,
    );

    let mut full_tree = root.clone();
    let mut targeted_tree = root;
    let target_ids = std::collections::HashSet::from([1]);
    let iterations = 1_000;

    let mut full_index = None;
    let full_started = Instant::now();
    for iteration in 0..iterations {
        full_tree.state.hovered = iteration % 2 == 0;
        resolver.restyle_subtree_cached(
            &mut full_tree,
            &rules,
            StyleContext::default(),
            &mut full_index,
        );
    }
    let full_elapsed = full_started.elapsed();

    let mut targeted_index = None;
    let targeted_started = Instant::now();
    for iteration in 0..iterations {
        targeted_tree.state.hovered = iteration % 2 == 0;
        resolver.restyle_subtree_for_ids_cached(
            &mut targeted_tree,
            &rules,
            StyleContext::default(),
            &mut targeted_index,
            &target_ids,
        );
    }
    let targeted_elapsed = targeted_started.elapsed();

    assert_eq!(
        full_tree.computed_style.background_color,
        targeted_tree.computed_style.background_color
    );
    assert_eq!(
        full_tree.children[1_024].computed_style.background_color,
        targeted_tree.children[1_024]
            .computed_style
            .background_color
    );
    assert_eq!(
        full_tree.children[1_024].computed_style.color,
        targeted_tree.children[1_024].computed_style.color
    );
    eprintln!(
        "non-inherited targeted restyle over {iterations} passes and 2,049 nodes: full {full_elapsed:?}; targeted {targeted_elapsed:?}; ratio {:.1}x",
        full_elapsed.as_secs_f64() / targeted_elapsed.as_secs_f64()
    );
    assert!(targeted_elapsed * 2 < full_elapsed);
}

#[test]
fn style_rule_index_matches_full_scan_for_selector_mix() {
    use mesh_core_component::style::{Declaration, Selector};

    let theme = mesh_core_theme::default_theme();
    let resolver = StyleResolver::new(&theme);
    let rules = vec![
        StyleRule {
            selector: Selector::Tag("button".to_string()),
            declarations: vec![Declaration {
                property: "color".to_string(),
                value: StyleValue::Literal("#111111".to_string()),
            }],
            container_query: None,
        },
        StyleRule {
            selector: Selector::Class("primary".to_string()),
            declarations: vec![Declaration {
                property: "background-color".to_string(),
                value: StyleValue::Literal("#222222".to_string()),
            }],
            container_query: None,
        },
        StyleRule {
            selector: Selector::Id("submit".to_string()),
            declarations: vec![Declaration {
                property: "border-color".to_string(),
                value: StyleValue::Literal("#333333".to_string()),
            }],
            container_query: None,
        },
        StyleRule {
            selector: Selector::Compound(vec![
                Selector::Class("primary".to_string()),
                Selector::State("*".to_string(), "hover".to_string()),
            ]),
            declarations: vec![Declaration {
                property: "opacity".to_string(),
                value: StyleValue::Literal("0.5".to_string()),
            }],
            container_query: None,
        },
    ];

    let style = resolver.resolve_node_style(
        &rules,
        "button",
        &["primary".to_string()],
        Some("submit"),
        StyleContext::default(),
        ElementState {
            hovered: true,
            ..Default::default()
        },
    );

    assert_eq!(style.color, Color::from_hex("#111111").unwrap());
    assert_eq!(style.background_color, Color::from_hex("#222222").unwrap());
    assert_eq!(style.border_color, Color::from_hex("#333333").unwrap());
    assert_eq!(style.opacity, 0.5);
}

#[test]
fn focus_visible_requires_focus_visible_state() {
    use crate::tree::ElementState;
    use mesh_core_component::style::{Declaration, Selector};

    let theme = mesh_core_theme::default_theme();
    let resolver = StyleResolver::new(&theme);
    let rules = vec![StyleRule {
        selector: Selector::State("input".to_string(), "focus-visible".to_string()),
        declarations: vec![Declaration {
            property: "color".to_string(),
            value: StyleValue::Literal("#abcdef".to_string()),
        }],
        container_query: None,
    }];

    let focused_only = resolver.resolve_node_style(
        &rules,
        "input",
        &[],
        None,
        StyleContext::default(),
        ElementState {
            focused: true,
            ..Default::default()
        },
    );
    assert_ne!(
        focused_only.color,
        Color::from_hex("#abcdef").unwrap(),
        ":focus-visible should no longer alias plain focused state"
    );

    let focus_visible = resolver.resolve_node_style(
        &rules,
        "input",
        &[],
        None,
        StyleContext::default(),
        ElementState {
            focused: true,
            focus_visible: true,
            ..Default::default()
        },
    );
    assert_eq!(focus_visible.color, Color::from_hex("#abcdef").unwrap());
}

#[test]
fn input_state_sets_hover_flags_on_nodes() {
    use crate::events::{InputState, RawInputEvent, UiEvent};
    use crate::layout::LayoutEngine;
    use crate::style::Dimension;
    use crate::tree::WidgetNode;

    let mut root = WidgetNode::new("root");
    root.computed_style.width = Dimension::Px(200.0);
    root.computed_style.height = Dimension::Px(100.0);

    let mut btn = WidgetNode::new("button");
    btn.computed_style.width = Dimension::Px(100.0);
    btn.computed_style.height = Dimension::Px(50.0);
    let btn_id = btn.id;
    root.children = vec![btn].into();
    LayoutEngine::compute(&mut root, 200.0, 100.0);

    let mut input = InputState::new();

    // Move pointer over the button.
    let events = input.process(
        &mut root,
        &RawInputEvent::PointerMotion { x: 50.0, y: 25.0 },
    );
    assert!(root.children[0].state.hovered, "button should be hovered");
    assert!(!root.state.hovered, "root should not be hovered");
    assert!(
        events
            .iter()
            .any(|e| matches!(e, UiEvent::PointerEnter { node_id } if *node_id == btn_id))
    );

    // Move pointer off the button onto the root.
    let events = input.process(
        &mut root,
        &RawInputEvent::PointerMotion { x: 150.0, y: 75.0 },
    );
    assert!(
        !root.children[0].state.hovered,
        "button hover should be cleared"
    );
    assert!(root.state.hovered, "root should now be hovered");
    assert!(
        events
            .iter()
            .any(|e| matches!(e, UiEvent::PointerLeave { node_id } if *node_id == btn_id))
    );
}
