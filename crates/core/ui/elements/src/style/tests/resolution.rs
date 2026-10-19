use super::super::*;
use super::common::*;
use crate::tree::ElementState;
use mesh_core_component::style::{Declaration, Selector, StyleRule, StyleValue};

#[test]
fn aspect_ratio_resolves_from_a_number_a_fraction_and_auto() {
    let theme = mesh_core_theme::default_theme();
    let resolver = StyleResolver::new(&theme);

    let resolve = |value: &str| {
        let rules = vec![StyleRule {
            selector: Selector::Tag("button".to_string()),
            declarations: vec![mesh_core_component::style::Declaration {
                property: "aspect-ratio".to_string(),
                value: StyleValue::Literal(value.to_string()),
            }],
            container_query: None,
        }];
        resolver
            .resolve_node_style(
                &rules,
                "button",
                &[],
                None,
                StyleContext::default(),
                ElementState::default(),
            )
            .aspect_ratio
    };

    assert_eq!(resolve("1"), Some(1.0));
    assert_eq!(resolve("1 / 1"), Some(1.0));
    assert_eq!(resolve("16 / 9"), Some(16.0 / 9.0));
    assert_eq!(resolve("auto"), None);
    // A malformed or degenerate ratio must not collapse the box.
    assert_eq!(resolve("1 / 0"), None);
    assert_eq!(resolve("wide"), None);
}

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
            rules: Vec::new(),
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
fn restyling_embedded_surface_preserves_compiler_layout_style() {
    let theme = mesh_core_theme::default_theme();
    let resolver = StyleResolver::new(&theme);
    let rules = vec![
        StyleRule {
            selector: Selector::Class("host".to_string()),
            declarations: vec![Declaration {
                property: "--accent".to_string(),
                value: StyleValue::Literal("#123456".to_string()),
            }],
            container_query: None,
        },
        StyleRule {
            selector: Selector::Tag("button".to_string()),
            declarations: vec![Declaration {
                property: "color".to_string(),
                value: StyleValue::Var("--accent".to_string()),
            }],
            container_query: None,
        },
    ];

    let mut host = crate::tree::WidgetNode::new("row");
    host.attributes.insert("class".into(), "host".into());
    let mut wrapper = crate::tree::WidgetNode::new("surface");
    wrapper.computed_style.direction = crate::style::FlexDirection::Column;
    wrapper.computed_style.align_self = crate::style::AlignSelf::Stretch;
    wrapper.computed_style.justify_content = crate::style::JustifyContent::Center;
    wrapper.computed_style.width = crate::style::Dimension::Px(999.0);
    let contribution = crate::tree::WidgetNode::new("button");
    wrapper.children.push(contribution);
    host.children.push(wrapper);

    resolver.restyle_subtree_cached(&mut host, &rules, StyleContext::default(), &mut None);

    let wrapper = &host.children[0];
    assert_eq!(
        wrapper.computed_style.direction,
        crate::style::FlexDirection::Column
    );
    assert_eq!(
        wrapper.computed_style.align_self,
        crate::style::AlignSelf::Stretch
    );
    assert_eq!(
        wrapper.computed_style.justify_content,
        crate::style::JustifyContent::Center
    );
    assert_eq!(
        wrapper.computed_style.width,
        crate::style::Dimension::Px(999.0)
    );
    assert_eq!(
        wrapper.computed_style.custom_properties.get("--accent"),
        Some(&StyleValue::Literal("#123456".to_string()))
    );
    assert_eq!(
        wrapper.children[0].computed_style.color,
        Color::from_hex("#123456").unwrap()
    );

    let wrapper_id = wrapper.id;
    resolver.restyle_subtree_for_ids(
        &mut host,
        &rules,
        StyleContext::default(),
        &std::collections::HashSet::from([wrapper_id]),
    );
    assert_eq!(
        host.children[0].computed_style.align_self,
        crate::style::AlignSelf::Stretch
    );
    assert_eq!(
        host.children[0].computed_style.justify_content,
        crate::style::JustifyContent::Center
    );
}

#[test]
fn retained_restyle_preserves_explicit_transparent_and_default_inherited_values() {
    let theme = mesh_core_theme::default_theme();
    let resolver = StyleResolver::new(&theme);
    let rules = vec![
        StyleRule {
            selector: Selector::Class("parent".to_string()),
            declarations: vec![Declaration {
                property: "color".to_string(),
                value: StyleValue::Literal("#123456".to_string()),
            }],
            container_query: None,
        },
        StyleRule {
            selector: Selector::State("row".to_string(), "hover".to_string()),
            declarations: vec![Declaration {
                property: "color".to_string(),
                value: StyleValue::Literal("#654321".to_string()),
            }],
            container_query: None,
        },
        StyleRule {
            selector: Selector::Class("child".to_string()),
            declarations: vec![
                Declaration {
                    property: "color".to_string(),
                    value: StyleValue::Literal("transparent".to_string()),
                },
                Declaration {
                    property: "font-size".to_string(),
                    value: StyleValue::Literal("14px".to_string()),
                },
                Declaration {
                    property: "font-weight".to_string(),
                    value: StyleValue::Literal("400".to_string()),
                },
                Declaration {
                    property: "line-height".to_string(),
                    value: StyleValue::Literal("1.4".to_string()),
                },
            ],
            container_query: None,
        },
    ];
    let mut root = crate::tree::WidgetNode::new("row");
    root.id = 1;
    root.attributes.insert("class".into(), "parent".into());
    let mut child = crate::tree::WidgetNode::new("text");
    child.id = 2;
    child.attributes.insert("class".into(), "child".into());
    root.children.push(child);

    resolver.restyle_subtree_cached(&mut root, &rules, StyleContext::default(), &mut None);
    let initial_child = &root.children[0].computed_style;
    assert_eq!(initial_child.color, Color::TRANSPARENT);
    assert_eq!(initial_child.font_size, 14.0);
    assert_eq!(initial_child.font_weight, 400);
    assert!((initial_child.line_height - 1.4).abs() < f32::EPSILON);
    assert!(initial_child.explicit_properties.color);
    assert!(initial_child.explicit_properties.font_size);
    assert!(initial_child.explicit_properties.font_weight);
    assert!(initial_child.explicit_properties.line_height);
    assert_eq!(
        initial_child.inherited_properties,
        StylePropertyMask {
            font_family: true,
            ..StylePropertyMask::default()
        }
    );
    let initial_explicit = initial_child.explicit_properties;

    root.state.hovered = true;
    let root_id = root.id;
    resolver.restyle_subtree_for_ids(
        &mut root,
        &rules,
        StyleContext::default(),
        &std::collections::HashSet::from([root_id]),
    );

    let restyled_child = &root.children[0].computed_style;
    assert_eq!(restyled_child.color, Color::TRANSPARENT);
    assert_eq!(restyled_child.font_size, 14.0);
    assert_eq!(restyled_child.font_weight, 400);
    assert!((restyled_child.line_height - 1.4).abs() < f32::EPSILON);
    assert_eq!(restyled_child.explicit_properties, initial_explicit);
    assert_eq!(
        restyled_child.inherited_properties,
        StylePropertyMask {
            font_family: true,
            ..StylePropertyMask::default()
        }
    );
}

#[test]
fn child_color_inherit_follows_parent_hover_color_instead_of_going_transparent() {
    let theme = mesh_core_theme::default_theme();
    let resolver = StyleResolver::new(&theme);
    let rules = vec![
        StyleRule {
            selector: Selector::Class("parent".to_string()),
            declarations: vec![Declaration {
                property: "color".to_string(),
                value: StyleValue::Literal("#123456".to_string()),
            }],
            container_query: None,
        },
        StyleRule {
            selector: Selector::State("row".to_string(), "hover".to_string()),
            declarations: vec![Declaration {
                property: "color".to_string(),
                value: StyleValue::Literal("#654321".to_string()),
            }],
            container_query: None,
        },
        StyleRule {
            selector: Selector::Class("child".to_string()),
            declarations: vec![Declaration {
                property: "color".to_string(),
                value: StyleValue::Literal("inherit".to_string()),
            }],
            container_query: None,
        },
    ];
    let mut root = crate::tree::WidgetNode::new("row");
    root.id = 1;
    root.attributes.insert("class".into(), "parent".into());
    let mut child = crate::tree::WidgetNode::new("text");
    child.id = 2;
    child.attributes.insert("class".into(), "child".into());
    root.children.push(child);

    resolver.restyle_subtree_cached(&mut root, &rules, StyleContext::default(), &mut None);
    let initial_child = &root.children[0].computed_style;
    assert_eq!(
        initial_child.color,
        Color {
            r: 0x12,
            g: 0x34,
            b: 0x56,
            a: 255
        }
    );
    assert!(
        !initial_child.explicit_properties.color,
        "color: inherit must not be treated as an explicit override"
    );

    root.state.hovered = true;
    let root_id = root.id;
    resolver.restyle_subtree_for_ids(
        &mut root,
        &rules,
        StyleContext::default(),
        &std::collections::HashSet::from([root_id]),
    );

    let restyled_child = &root.children[0].computed_style;
    assert_eq!(
        restyled_child.color,
        Color {
            r: 0x65,
            g: 0x43,
            b: 0x21,
            a: 255
        },
        "child with color: inherit must pick up the parent's hover color, not stay transparent"
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
fn a_later_rules_custom_property_applies_to_the_same_nodes_var() {
    let theme = mesh_core_theme::default_theme();
    let resolver = StyleResolver::new(&theme);
    let rules = vec![
        StyleRule {
            selector: Selector::Tag("button".to_string()),
            declarations: vec![
                Declaration {
                    property: "--bg".to_string(),
                    value: StyleValue::Literal("#0000ff".to_string()),
                },
                Declaration {
                    property: "background-color".to_string(),
                    value: StyleValue::Var("--bg".to_string()),
                },
            ],
            container_query: None,
        },
        StyleRule {
            selector: Selector::State("button".to_string(), "hover".to_string()),
            declarations: vec![Declaration {
                property: "--bg".to_string(),
                value: StyleValue::Literal("#ff0000".to_string()),
            }],
            container_query: None,
        },
    ];
    let resolve = |hovered| {
        resolver
            .resolve_node_style(
                &rules,
                "button",
                &[],
                None,
                StyleContext::default(),
                ElementState {
                    hovered,
                    ..ElementState::default()
                },
            )
            .background_color
    };

    assert_eq!(resolve(false), Color::from_hex("#0000ff").unwrap());
    assert_eq!(resolve(true), Color::from_hex("#ff0000").unwrap());
}

fn validate_finalized(node: &mut crate::tree::WidgetNode) {
    for child in &mut node.children {
        validate_finalized(child);
    }
    node.validate_finalized_subtree();
}

#[test]
fn finalized_memo_styles_preserve_full_resolution_on_input_changes() {
    let theme = mesh_core_theme::default_theme();
    let rules = vec![
        StyleRule {
            selector: Selector::Class("parent".into()),
            declarations: vec![Declaration {
                property: "color".into(),
                value: StyleValue::Literal("#ff0000".into()),
            }],
            container_query: None,
        },
        StyleRule {
            selector: Selector::Compound(vec![
                Selector::Class("item".into()),
                Selector::Class("selected".into()),
                Selector::State("*".into(), "hover".into()),
            ]),
            declarations: vec![Declaration {
                property: "width".into(),
                value: StyleValue::Literal("80px".into()),
            }],
            container_query: None,
        },
    ];
    let resolver = StyleResolver::new(&theme)
        .with_finalized_memo_styles(true)
        .with_incremental_matching();
    let mut root = crate::tree::WidgetNode::new("box");
    let mut child = crate::tree::WidgetNode::new("text");
    child.attributes.insert("class".into(), "item".into());
    root.children.push(child);
    let mut index = None;
    let context = StyleContext::default();
    resolver.restyle_subtree_cached(&mut root, &rules, context, &mut index);
    for change in 0..7 {
        match change {
            0 => {
                root.children[0]
                    .attributes
                    .insert("class".into(), "item unrelated".into());
            }
            1 => {
                root.children[0]
                    .attributes
                    .insert("class".into(), "item selected".into());
                root.children[0].state.hovered = true;
            }
            2 => {
                root.attributes.insert("class".into(), "parent".into());
            }
            3 => {
                root.children[0]
                    .attributes
                    .insert("style".into(), "font-size: 31px".into());
            }
            4 => {
                root.children[0].state.hovered = false;
            }
            5 => {
                root.attributes.remove("class");
            }
            _ => {
                root.children[0].attributes.remove("style");
            }
        }
        let mut full = root.clone();
        validate_finalized(&mut root);
        let mut attribution = StyleRuleAttribution::new(&rules);
        resolver.restyle_subtree_cached_profiled(
            &mut root,
            &rules,
            context,
            &mut index,
            &mut attribution,
        );
        StyleResolver::new(&theme).restyle_subtree_cached(&mut full, &rules, context, &mut None);
        assert_eq!(root.computed_style, full.computed_style, "change {change}");
        assert_eq!(
            root.children[0].computed_style, full.children[0].computed_style,
            "change {change}"
        );
        if change == 0 {
            assert_eq!(attribution.entries().count(), 0);
        }
    }
    let old_cache = root.finalized_style.clone().unwrap();
    validate_finalized(&mut root);
    resolver.restyle_subtree_cached(&mut root, &rules, context, &mut index);
    assert!(std::sync::Arc::ptr_eq(
        &old_cache,
        root.finalized_style.as_ref().unwrap()
    ));
    // A new index identity must invalidate snapshots even if its allocator
    // reuses the old rules vector's address.
    let mut index = Some(StyleRuleIndex::new(&rules));
    resolver.restyle_subtree_cached(&mut root, &rules, context, &mut index);
    assert!(!std::sync::Arc::ptr_eq(
        &old_cache,
        root.finalized_style.as_ref().unwrap()
    ));
}

#[test]
fn incremental_class_and_state_matching_checks_all_compound_dependencies() {
    let theme = mesh_core_theme::default_theme();
    let rules = vec![StyleRule {
        selector: Selector::Compound(vec![
            Selector::Class("first".into()),
            Selector::Compound(vec![
                Selector::Class("last".into()),
                Selector::State("*".into(), "hover".into()),
            ]),
        ]),
        declarations: vec![Declaration {
            property: "width".into(),
            value: StyleValue::Literal("80px".into()),
        }],
        container_query: None,
    }];
    let resolver = StyleResolver::new(&theme).with_incremental_matching();
    let mut node = crate::tree::WidgetNode::new("button");
    let mut index = None;
    resolver.restyle_subtree_cached(&mut node, &rules, StyleContext::default(), &mut index);
    for (classes, hovered, matches) in [
        ("first", false, false),
        ("first last", false, false),
        ("first last", true, true),
        ("last", true, false),
        ("first last", true, true),
        ("first last unrelated", true, true),
        ("first last", false, false),
    ] {
        node.attributes.insert("class".into(), classes.into());
        node.state.hovered = hovered;
        let mut full = node.clone();
        let mut attribution = StyleRuleAttribution::new(&rules);
        resolver.restyle_subtree_for_ids_cached_profiled(
            &mut node,
            &rules,
            StyleContext::default(),
            &mut index,
            &std::collections::HashSet::from([full.id]),
            &mut attribution,
        );
        StyleResolver::new(&theme).restyle_subtree_cached(
            &mut full,
            &rules,
            StyleContext::default(),
            &mut None,
        );
        assert_eq!(node.computed_style, full.computed_style);
        assert_eq!(node.computed_style.width == Dimension::Px(80.0), matches);
        if classes.ends_with("unrelated") {
            assert_eq!(attribution.entries().count(), 0);
        }
    }
}

#[test]
#[ignore = "release-only finalized memo and incremental selector benchmark"]
fn finalized_memo_and_selector_dependency_profile() {
    use std::hint::black_box;
    use std::time::Instant;
    let theme = mesh_core_theme::default_theme();
    let rules = (0..256)
        .map(|i| StyleRule {
            selector: Selector::Compound(vec![
                Selector::Class(format!("item{i}")),
                Selector::State("*".into(), "hover".into()),
            ]),
            declarations: vec![Declaration {
                property: "background".into(),
                value: StyleValue::Literal("#123456".into()),
            }],
            container_query: None,
        })
        .collect::<Vec<_>>();
    let mut raw = crate::tree::WidgetNode::new("box");
    for i in 0..512 {
        let mut child = crate::tree::WidgetNode::new("text");
        child
            .attributes
            .insert("class".into(), format!("item{}", i % 256));
        child
            .attributes
            .insert("content".into(), format!("row {i}"));
        raw.children.push(child);
    }
    let resolver = StyleResolver::new(&theme)
        .with_finalized_memo_styles(true)
        .with_incremental_matching();
    let mut index = None;
    let mut finalized = raw.clone();
    resolver.restyle_subtree_cached(&mut finalized, &rules, StyleContext::default(), &mut index);
    crate::normalize_accessibility_finalized_memo(&mut finalized);
    raw.retain_finalization_from(&finalized);
    let iterations = 200;
    let full_started = Instant::now();
    for _ in 0..iterations {
        let mut tree = raw.clone();
        StyleResolver::new(&theme).restyle_subtree_cached(
            &mut tree,
            &rules,
            StyleContext::default(),
            &mut index,
        );
        crate::normalize_accessibility(&mut tree);
        black_box(tree);
    }
    let full = full_started.elapsed();
    let memo_started = Instant::now();
    for _ in 0..iterations {
        let mut tree = raw.clone();
        validate_finalized(&mut tree);
        resolver.restyle_subtree_cached(&mut tree, &rules, StyleContext::default(), &mut index);
        crate::normalize_accessibility_finalized_memo(&mut tree);
        black_box(tree);
    }
    let memo = memo_started.elapsed();
    let speedup = full.as_secs_f64() / memo.as_secs_f64();
    println!("MESH_PERF metric=finalized_memo_speedup value={speedup:.4}");
    eprintln!("finalized memo: 513 nodes, {iterations} rebuilds: full {full:?}, memo {memo:?}");
    assert!(
        speedup >= 1.5,
        "finalized memo must avoid full finalization"
    );

    let old_classes = vec!["unrelated".to_string()];
    let new_classes = vec!["unrelated".to_string(), "item128".to_string()];
    let old = StyleNodeAttrs::new("button", &old_classes, None, ElementState::default());
    let new = StyleNodeAttrs::new("button", &new_classes, None, ElementState::default());
    let index = index.unwrap();
    let iterations = 100_000;
    let scan_started = Instant::now();
    for _ in 0..iterations {
        black_box(rules.iter().all(|rule| {
            selector_matches_attrs(&rule.selector, black_box(&old))
                == selector_matches_attrs(&rule.selector, black_box(&new))
        }));
    }
    let scan = scan_started.elapsed();
    let deps_started = Instant::now();
    for _ in 0..iterations {
        assert!(black_box(index.changed_dependencies_preserve_matches(
            &rules,
            black_box(&old),
            black_box(&new),
            StyleContext::default()
        )));
    }
    let deps = deps_started.elapsed();
    let speedup = scan.as_secs_f64() / deps.as_secs_f64();
    println!("MESH_PERF metric=selector_dependency_update_speedup value={speedup:.4}");
    eprintln!(
        "selector dependencies: 256 rules, {iterations} unmatched class changes: scan {scan:?}, index {deps:?}"
    );
    assert!(
        speedup >= 4.0,
        "dependency lookup must avoid unrelated rules"
    );
}

#[test]
fn finalized_memo_environment_and_container_changes_invalidate_styles() {
    use mesh_core_component::style::ContainerQuery;
    let theme = mesh_core_theme::default_theme();
    let rules = vec![
        StyleRule {
            selector: Selector::Tag("text".into()),
            declarations: vec![Declaration {
                property: "font-size".into(),
                value: StyleValue::Prop("size".into()),
            }],
            container_query: None,
        },
        StyleRule {
            selector: Selector::Class("wide".into()),
            declarations: vec![Declaration {
                property: "width".into(),
                value: StyleValue::Literal("120px".into()),
            }],
            container_query: Some(ContainerQuery {
                min_width: Some(200.0),
                ..Default::default()
            }),
        },
    ];
    let mut node = crate::tree::WidgetNode::new("text");
    node.attributes.insert("class".into(), "wide".into());
    let mut index = None;
    for (width, size) in [
        (100.0, "14px"),
        (300.0, "14px"),
        (300.0, "24px"),
        (100.0, "24px"),
    ] {
        let props = std::collections::HashMap::from([(
            "--mesh-prop-size".into(),
            StyleValue::Literal(size.into()),
        )]);
        let context = StyleContext {
            container_width: width,
            container_height: 80.0,
        };
        let mut full = node.clone();
        validate_finalized(&mut node);
        StyleResolver::new(&theme)
            .with_props(props.clone())
            .with_finalized_memo_styles(true)
            .restyle_subtree_cached(&mut node, &rules, context, &mut index);
        StyleResolver::new(&theme)
            .with_props(props)
            .restyle_subtree_cached(&mut full, &rules, context, &mut None);
        assert_eq!(node.computed_style, full.computed_style);
        assert_eq!(
            node.computed_style.width == Dimension::Px(120.0),
            width >= 200.0
        );
    }
    let old = node.finalized_style.clone().unwrap();
    validate_finalized(&mut node);
    let changed_theme = mesh_core_theme::default_theme();
    StyleResolver::new(&changed_theme)
        .with_finalized_memo_styles(true)
        .restyle_subtree_cached(&mut node, &rules, StyleContext::default(), &mut index);
    assert!(!std::sync::Arc::ptr_eq(
        &old,
        node.finalized_style.as_ref().unwrap()
    ));
}

#[test]
fn finalized_memo_does_not_reuse_sampled_retained_styles() {
    let theme = mesh_core_theme::default_theme();
    let resolver = StyleResolver::new(&theme).with_finalized_memo_styles(true);
    let mut node = crate::tree::WidgetNode::new("text");
    let mut index = None;
    resolver.restyle_subtree_cached(&mut node, &[], StyleContext::default(), &mut index);
    let authored = node.computed_style.clone();
    node.computed_style.opacity = 0.25;
    node.mark_style_sampled();
    validate_finalized(&mut node);
    resolver.restyle_subtree_cached(&mut node, &[], StyleContext::default(), &mut index);
    assert_eq!(node.computed_style, authored);
}

#[test]
fn incremental_matching_includes_module_theme_selector_dependencies() {
    let mut theme = mesh_core_theme::default_theme();
    theme.modules_mut().insert(
        "@test/theme-deps".into(),
        mesh_core_theme::ThemeModule {
            rules: vec![mesh_core_theme::ThemeStyleRule {
                selector: Selector::Compound(vec![
                    Selector::Class("themed".into()),
                    Selector::State("*".into(), "hover".into()),
                ]),
                declarations: [("width".into(), "83px".into())].into_iter().collect(),
            }],
            ..Default::default()
        },
    );
    let resolver = StyleResolver::new(&theme).with_incremental_matching();
    let mut node = crate::tree::WidgetNode::new("button");
    node.set_module_id("@test/theme-deps");
    let mut index = None;
    resolver.restyle_subtree_cached(&mut node, &[], StyleContext::default(), &mut index);
    for (class, hovered, matched) in [
        ("themed", false, false),
        ("themed", true, true),
        ("other", true, false),
        ("themed", true, true),
        ("themed", false, false),
    ] {
        node.attributes.insert("class".into(), class.into());
        node.state.hovered = hovered;
        let mut full = node.clone();
        resolver.restyle_subtree_for_ids_cached(
            &mut node,
            &[],
            StyleContext::default(),
            &mut index,
            &std::collections::HashSet::from([full.id]),
        );
        StyleResolver::new(&theme).restyle_subtree_cached(
            &mut full,
            &[],
            StyleContext::default(),
            &mut None,
        );
        assert_eq!(node.computed_style, full.computed_style);
        assert_eq!(node.computed_style.width == Dimension::Px(83.0), matched);
    }
}

#[test]
fn css_profile_gradients_shorthand_and_shadow_lists() {
    let theme = mesh_core_theme::default_theme();
    let resolver = StyleResolver::new(&theme);
    let resolve = |declarations: &[(&str, &str)]| {
        resolver.resolve_node_style(
            &[StyleRule {
                selector: Selector::Tag("box".into()),
                declarations: declarations
                    .iter()
                    .map(|(property, value)| Declaration {
                        property: (*property).into(),
                        value: StyleValue::Literal((*value).into()),
                    })
                    .collect(),
                container_query: None,
            }],
            "box",
            &[],
            None,
            StyleContext::default(),
            ElementState::default(),
        )
    };
    let gradient =
        "linear-gradient(to bottom, red, rgb(0, 255, 0), hsl(240, 100%, 50%), transparent)";
    let expected = BackgroundPaint::MultiStopLinearGradient(vec![
        Color::from_css("red").unwrap(),
        Color::from_css("rgb(0, 255, 0)").unwrap(),
        Color::from_css("hsl(240, 100%, 50%)").unwrap(),
        Color::TRANSPARENT,
    ]);
    assert_eq!(
        resolve(&[("background", gradient)]).background_paint,
        expected
    );
    assert_eq!(
        resolve(&[("background-image", gradient)]).background_paint,
        expected
    );
    let reset = resolve(&[("background-image", gradient), ("background", "red")]);
    assert_eq!(reset.background_paint, BackgroundPaint::None);
    assert_eq!(reset.background_color, Color::from_css("red").unwrap());
    assert_eq!(
        resolve(&[("background", "red"), ("background", gradient)]).background_color,
        Color::TRANSPARENT
    );
    assert_eq!(
        resolve(&[("background", gradient), ("background-color", "red")]).background_paint,
        expected
    );
    let shadow = resolve(&[(
        "box-shadow",
        "rgba(255, 0, 0, 0.5) 1px 2px 3px, 4px 5px blue, inset 0 0 2px hsl(120, 100%, 50%)",
    )]);
    assert_eq!(
        shadow.box_shadow.color,
        Color::from_css("rgba(255, 0, 0, 0.5)").unwrap()
    );
    assert_eq!(shadow.box_shadow.offset_y, 2.0);
    assert_eq!(shadow.additional_box_shadows.len(), 2);
    assert_eq!(
        shadow.additional_box_shadows[0].color,
        Color::from_css("blue").unwrap()
    );
    assert!(shadow.additional_box_shadows[1].inset);
    let reset = resolve(&[
        ("box-shadow", "0 2px red, 0 4px blue"),
        ("box-shadow", "none"),
    ]);
    assert!(reset.box_shadow.is_none());
    assert!(reset.additional_box_shadows.is_empty());
    for invalid in [
        "0 2px red, invalid",
        "0 2px rgb(1, 2)",
        "0 red",
        "0 0 red blue",
    ] {
        let style = resolve(&[("box-shadow", invalid)]);
        assert!(style.box_shadow.is_none(), "{invalid}");
        assert!(style.additional_box_shadows.is_empty(), "{invalid}");
    }
}

#[test]
fn css_profile_paints_accept_theme_color_syntax() {
    use crate::style::parse::{parse_background_image, parse_box_shadows};
    for value in [
        "rebeccapurple",
        "#1234",
        "rgb(20% 40% 60% / 50%)",
        "hsl(120 50% 40% / 0.6)",
        "hwb(120 20% 30%)",
        "oklch(60% 0.1 120)",
        "color(display-p3 0.2 0.4 0.6)",
    ] {
        let [r, g, b, a] = mesh_core_theme::css::parse_color_rgba(value).expect(value);
        let expected = Color { r, g, b, a };
        assert_eq!(
            parse_background_image(&format!("linear-gradient({value}, {value})")),
            BackgroundPaint::LinearGradient(StyleLinearGradient {
                from: expected,
                to: expected
            }),
            "{value}"
        );
        let shadows = parse_box_shadows(&format!("0 2px {value}, 0 4px {value}"));
        assert_eq!(shadows.len(), 2, "{value}");
        assert!(
            shadows.iter().all(|shadow| shadow.color == expected),
            "{value}"
        );
    }
    for invalid in [
        "linear-gradient(red, blue,)",
        "linear-gradient(red)",
        "linear-gradient(red,,blue)",
        "linear-gradient(red, rgb(0, 0, 0)",
    ] {
        assert_eq!(
            parse_background_image(invalid),
            BackgroundPaint::None,
            "{invalid}"
        );
    }
    for invalid in [
        "0 2px red,",
        "0 2px red,,0 4px blue",
        "0 2px rgb(0, 0, 0",
        "0 2px #ééé",
    ] {
        assert!(parse_box_shadows(invalid).is_empty(), "{invalid}");
    }
}
