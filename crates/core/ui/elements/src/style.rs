mod parse;
mod resolve;
mod types;

pub use resolve::*;
pub use types::*;

#[cfg(test)]
mod tests {
    use super::parse::parse_transition_properties;
    use super::*;
    use crate::tree::ElementState;
    use mesh_core_component::{
        parser::parse_component,
        style::{Declaration, Selector, StyleRule, StyleValue},
    };

    fn parse_fixture_style(source: &str) -> Vec<StyleRule> {
        parse_component(source)
            .expect("fixture parses")
            .style
            .expect("fixture has style")
            .rules
    }

    fn selector_has_class(selector: &Selector, class: &str) -> bool {
        match selector {
            Selector::Class(name) => name == class,
            Selector::Compound(parts) => parts.iter().any(|part| selector_has_class(part, class)),
            Selector::Tag(_) | Selector::Id(_) | Selector::State(_, _) | Selector::Universal => {
                false
            }
        }
    }

    fn resolve_class(
        resolver: &StyleResolver<'_>,
        rules: &[StyleRule],
        class: &str,
    ) -> (ComputedStyle, Vec<StyleDiagnostic>) {
        resolver.resolve_node_style_with_diagnostics(
            rules,
            "box",
            &[class.to_string()],
            None,
            StyleContext::default(),
            ElementState::default(),
        )
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
    fn resolve_theme_token() {
        let theme = mesh_core_theme::default_theme();
        let resolver = StyleResolver::new(&theme);
        let value = StyleValue::Token("color.primary".to_string());
        let resolved = resolver.resolve_value(&value);
        assert_eq!(resolved, "#6750A4");
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
            ("box-shadow", StyleProfileStatus::Implemented),
            ("filter", StyleProfileStatus::Implemented),
            ("display", StyleProfileStatus::Implemented),
            ("font-size", StyleProfileStatus::Implemented),
            ("animation-duration", StyleProfileStatus::Implemented),
            ("transition-property", StyleProfileStatus::Implemented),
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
            "white-space",
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
    fn style_diagnostics_transform_origin_is_accepted_but_unlowered() {
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

        let (_style, diagnostics) = resolve_class(&resolver, &rules, "panel");

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].property, "transform-origin");
        assert!(diagnostics[0].message.contains("accepted by the parser"));
        assert!(diagnostics[0].message.contains("not lowered"));
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
            "../../../../../modules/frontend/navigation-bar/src/main.mesh"
        ));
        rules.extend(parse_fixture_style(include_str!(
            "../../../../../modules/frontend/navigation-bar/src/components/volume-button.mesh"
        )));

        let (_nav_style, nav_diagnostics) = resolve_class(&resolver, &rules, "nav-shell");
        let (_status_style, status_diagnostics) =
            resolve_class(&resolver, &rules, "status-primary");
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
            "../../../../../modules/frontend/navigation-bar/src/components/volume-button.mesh"
        ));
        let docs = include_str!("../../../../../docs/css-coverage.md");

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
    fn animation_token_duration_resolves_from_theme() {
        let theme = mesh_core_theme::default_theme();
        let resolver = StyleResolver::new(&theme);
        let rules = vec![StyleRule {
            selector: Selector::Class("panel".to_string()),
            declarations: vec![mesh_core_component::style::Declaration {
                property: "animation-duration".to_string(),
                value: StyleValue::Token("animation.duration.fast".to_string()),
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
        assert_eq!(style.animation.duration_ms, 90);
    }

    #[test]
    fn invalid_animation_token_produces_diagnostic_and_skips_declaration() {
        let theme = mesh_core_theme::default_theme();
        let resolver = StyleResolver::new(&theme);
        let rules = vec![StyleRule {
            selector: Selector::Class("panel".to_string()),
            declarations: vec![mesh_core_component::style::Declaration {
                property: "animation-duration".to_string(),
                value: StyleValue::Token("animation.duration.fastest".to_string()),
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

        assert_eq!(style.animation.duration_ms, 0);
        assert_eq!(diagnostics.len(), 1);
        assert!(
            diagnostics[0]
                .message
                .contains("animation.duration.fastest")
        );
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
                    value: StyleValue::Token("color.primary".to_string()),
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
    fn module_component_defaults_are_subtree_scoped() {
        let mut theme = mesh_core_theme::Theme {
            id: "scoped".into(),
            name: "Scoped".into(),
            tokens: std::collections::HashMap::from([
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
            ]),
            defaults: mesh_core_theme::ThemeDefaults {
                components: std::collections::HashMap::from([(
                    "base".into(),
                    std::collections::HashMap::from([(
                        "color".into(),
                        "token(color.on-background)".into(),
                    )]),
                )]),
            },
            modules: std::collections::HashMap::new(),
        };
        theme.modules.insert(
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
                            std::collections::HashMap::from([(
                                "transition".into(),
                                "background-color token(animation.duration.short) token(animation.curves.bezier.standard)"
                                    .into(),
                            )]),
                        ),
                        (
                            "button".into(),
                            std::collections::HashMap::from([(
                                "background".into(),
                                "token(@mesh/weather.weather.color.sunny)".into(),
                            )]),
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
        assert_eq!(outside.transition.duration_ms, 0);

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
        assert_eq!(inside.transition.duration_ms, 150);
        assert!(inside.transition.properties.animates_background_color());
    }

    #[test]
    fn container_query_rules_apply_against_context() {
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
    fn pseudo_state_rules_apply_when_state_matches() {
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
        previous
            .attributes
            .insert("_mesh_key".into(), "prev".into());
        previous.state.hovered = false;
        previous.computed_style.background_color = Color::from_hex("#ff0000").unwrap();
        let mut current = crate::tree::WidgetNode::new("button");
        current.attributes.insert("_mesh_key".into(), "next".into());
        current.state.hovered = true;
        root.children.push(previous);
        root.children.push(current);

        let target_keys = std::collections::HashSet::from(["prev".to_string(), "next".to_string()]);
        resolver.restyle_subtree_for_keys(&mut root, &rules, StyleContext::default(), &target_keys);

        assert_eq!(
            root.children[0].computed_style.background_color,
            ComputedStyle::default().background_color
        );
        assert_eq!(
            root.children[1].computed_style.background_color,
            Color::from_hex("#ff0000").unwrap()
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
        root.children = vec![btn];
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

    #[test]
    fn padding_inline_and_block_tokens_resolve_to_computed_edges() {
        use mesh_core_component::parser::parse_component;

        let source = r#"
<style>
.panel {
    padding-inline: token(spacing.lg);
    padding-block: token(spacing.sm);
}
</style>
"#;
        let file = parse_component(source).unwrap();
        let rules = file.style.unwrap().rules;

        let theme = mesh_core_theme::default_theme();
        let resolver = StyleResolver::new(&theme);

        let style = resolver.resolve_node_style(
            &rules,
            "div",
            &["panel".to_owned()],
            None,
            StyleContext::default(),
            ElementState::default(),
        );

        // spacing.lg = 24, spacing.sm = 8
        assert_eq!(style.padding.left, 24.0, "padding-inline left");
        assert_eq!(style.padding.right, 24.0, "padding-inline right");
        assert_eq!(style.padding.top, 8.0, "padding-block top");
        assert_eq!(style.padding.bottom, 8.0, "padding-block bottom");
    }

    #[test]
    fn padding_shorthand_and_overrides_resolve_correctly() {
        use mesh_core_component::parser::parse_component;

        let source = r#"
<style>
.card {
    padding: 16px;
    padding-top: 4px;
}
</style>
"#;
        let file = parse_component(source).unwrap();
        let rules = file.style.unwrap().rules;

        let theme = mesh_core_theme::default_theme();
        let resolver = StyleResolver::new(&theme);

        let style = resolver.resolve_node_style(
            &rules,
            "div",
            &["card".to_owned()],
            None,
            StyleContext::default(),
            ElementState::default(),
        );

        assert_eq!(style.padding.top, 4.0, "padding-top override");
        assert_eq!(style.padding.right, 16.0, "shorthand right");
        assert_eq!(style.padding.bottom, 16.0, "shorthand bottom");
        assert_eq!(style.padding.left, 16.0, "shorthand left");
    }

    #[test]
    fn padding_margin_four_value_shorthands_expand_to_edges() {
        let theme = mesh_core_theme::default_theme();
        let resolver = StyleResolver::new(&theme);
        let rules = vec![StyleRule {
            selector: Selector::Class("card".to_string()),
            declarations: vec![
                mesh_core_component::style::Declaration {
                    property: "padding".to_string(),
                    value: StyleValue::Literal("1px 2px 3px 4px".to_string()),
                },
                mesh_core_component::style::Declaration {
                    property: "margin".to_string(),
                    value: StyleValue::Literal("5px 6px 7px 8px".to_string()),
                },
            ],
            container_query: None,
        }];

        let style = resolver.resolve_node_style(
            &rules,
            "box",
            &["card".to_string()],
            None,
            StyleContext::default(),
            ElementState::default(),
        );

        assert_eq!(style.padding.top, 1.0);
        assert_eq!(style.padding.right, 2.0);
        assert_eq!(style.padding.bottom, 3.0);
        assert_eq!(style.padding.left, 4.0);
        assert_eq!(style.margin.top, 5.0);
        assert_eq!(style.margin.right, 6.0);
        assert_eq!(style.margin.bottom, 7.0);
        assert_eq!(style.margin.left, 8.0);
    }

    #[test]
    fn border_shorthand_sets_width_and_color() {
        let theme = mesh_core_theme::default_theme();
        let resolver = StyleResolver::new(&theme);
        let rules = vec![StyleRule {
            selector: Selector::Tag("box".to_string()),
            declarations: vec![mesh_core_component::style::Declaration {
                property: "border".to_string(),
                value: StyleValue::Literal("2px solid #ffffff".to_string()),
            }],
            container_query: None,
        }];

        let style = resolver.resolve_node_style(
            &rules,
            "box",
            &[],
            None,
            StyleContext::default(),
            ElementState::default(),
        );

        assert_eq!(style.border_width, Edges::all(2.0));
        assert_eq!(style.border_color, Color::WHITE);
    }

    #[test]
    fn overflow_two_value_shorthand_sets_axes() {
        let theme = mesh_core_theme::default_theme();
        let resolver = StyleResolver::new(&theme);
        let rules = vec![StyleRule {
            selector: Selector::Tag("box".to_string()),
            declarations: vec![mesh_core_component::style::Declaration {
                property: "overflow".to_string(),
                value: StyleValue::Literal("hidden auto".to_string()),
            }],
            container_query: None,
        }];

        let style = resolver.resolve_node_style(
            &rules,
            "box",
            &[],
            None,
            StyleContext::default(),
            ElementState::default(),
        );

        assert_eq!(style.overflow_x, Overflow::Hidden);
        assert_eq!(style.overflow_y, Overflow::Auto);
    }

    #[test]
    fn flex_triple_shorthand_sets_grow_shrink_basis() {
        let theme = mesh_core_theme::default_theme();
        let resolver = StyleResolver::new(&theme);
        let rules = vec![StyleRule {
            selector: Selector::Tag("box".to_string()),
            declarations: vec![mesh_core_component::style::Declaration {
                property: "flex".to_string(),
                value: StyleValue::Literal("1 0 12px".to_string()),
            }],
            container_query: None,
        }];

        let style = resolver.resolve_node_style(
            &rules,
            "box",
            &[],
            None,
            StyleContext::default(),
            ElementState::default(),
        );

        assert_eq!(style.flex_grow, 1.0);
        assert_eq!(style.flex_shrink, 0.0);
        assert!(matches!(style.flex_basis, Dimension::Px(px) if px == 12.0));
    }

    #[test]
    fn font_shorthand_sets_text_fields() {
        let theme = mesh_core_theme::default_theme();
        let resolver = StyleResolver::new(&theme);
        let rules = vec![StyleRule {
            selector: Selector::Tag("text".to_string()),
            declarations: vec![mesh_core_component::style::Declaration {
                property: "font".to_string(),
                value: StyleValue::Literal("italic 600 16px/1.4 Inter".to_string()),
            }],
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

        assert_eq!(style.font_style, FontStyle::Italic);
        assert_eq!(style.font_weight, 600);
        assert_eq!(style.font_size, 16.0);
        assert_eq!(style.line_height, 1.4);
        assert_eq!(style.font_family, "Inter");
    }

    #[test]
    fn css_variable_resolves_local_literal_value() {
        let theme = mesh_core_theme::default_theme();
        let resolver = StyleResolver::new(&theme);
        let rules = vec![StyleRule {
            selector: Selector::Class("panel".to_string()),
            declarations: vec![
                mesh_core_component::style::Declaration {
                    property: "--surface".to_string(),
                    value: StyleValue::Literal("#ffffff".to_string()),
                },
                mesh_core_component::style::Declaration {
                    property: "background".to_string(),
                    value: StyleValue::Var("--surface".to_string()),
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

        assert_eq!(style.background_color, Color::WHITE);
    }

    #[test]
    fn css_variable_resolves_token_value_before_computed_style() {
        let theme = mesh_core_theme::default_theme();
        let resolver = StyleResolver::new(&theme);
        let rules = vec![StyleRule {
            selector: Selector::Class("panel".to_string()),
            declarations: vec![
                mesh_core_component::style::Declaration {
                    property: "--surface".to_string(),
                    value: StyleValue::Token("color.primary".to_string()),
                },
                mesh_core_component::style::Declaration {
                    property: "background".to_string(),
                    value: StyleValue::Var("--surface".to_string()),
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

        assert_eq!(style.background_color, Color::from_hex("#6750A4").unwrap());
    }

    #[test]
    fn missing_css_variable_produces_style_diagnostic() {
        let theme = mesh_core_theme::default_theme();
        let resolver = StyleResolver::new(&theme);
        let rules = vec![StyleRule {
            selector: Selector::Class("panel".to_string()),
            declarations: vec![mesh_core_component::style::Declaration {
                property: "background".to_string(),
                value: StyleValue::Var("--missing".to_string()),
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
        assert!(diagnostics[0].message.contains("--missing"));
    }

    #[test]
    fn shipped_navigation_style_token_resolution_uses_theme_pipeline() {
        let theme = mesh_core_theme::default_theme();
        let resolver = StyleResolver::new(&theme);
        let rules = vec![StyleRule {
            selector: Selector::Class("nav-shell".to_string()),
            declarations: vec![
                Declaration {
                    property: "background".to_string(),
                    value: StyleValue::Token("color.surface".to_string()),
                },
                Declaration {
                    property: "color".to_string(),
                    value: StyleValue::Token("color.on-surface".to_string()),
                },
                Declaration {
                    property: "padding-inline".to_string(),
                    value: StyleValue::Token("spacing.lg".to_string()),
                },
                Declaration {
                    property: "border-radius".to_string(),
                    value: StyleValue::Token("radius.md".to_string()),
                },
                Declaration {
                    property: "transition-duration".to_string(),
                    value: StyleValue::Token("animation.duration.short".to_string()),
                },
                Declaration {
                    property: "animation-duration".to_string(),
                    value: StyleValue::Token("animation.duration.long".to_string()),
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
        assert_eq!(style.transition.duration_ms, 150);
        assert_eq!(style.animation.duration_ms, 360);
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
                    value: StyleValue::Token("color.surface-container".to_string()),
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
        assert_eq!(
            style.background_color,
            Color::from_hex("#211F26").unwrap()
        );
    }

    #[test]
    fn shipped_navigation_style_animation_token_failures_are_actionable() {
        let theme = mesh_core_theme::default_theme();
        let resolver = StyleResolver::new(&theme);
        let rules = vec![StyleRule {
            selector: Selector::Class("nav-shell".to_string()),
            declarations: vec![Declaration {
                property: "transition-duration".to_string(),
                value: StyleValue::Token("animation.duration.not-real".to_string()),
            }],
            container_query: None,
        }];

        let (_style, diagnostics) = resolve_class(&resolver, &rules, "nav-shell");

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].property, "transition-duration");
        assert!(diagnostics[0].message.contains("animation.duration.not-real"));
    }

    #[test]
    fn shipped_navigation_style_fixtures_parse_without_syntax_regression() {
        let nav_rules = parse_fixture_style(include_str!(
            "../../../../../modules/frontend/navigation-bar/src/main.mesh"
        ));
        let volume_rules = parse_fixture_style(include_str!(
            "../../../../../modules/frontend/navigation-bar/src/components/volume-button.mesh"
        ));

        assert!(nav_rules.iter().any(|rule| selector_has_class(&rule.selector, "nav-shell")));
        assert!(volume_rules
            .iter()
            .any(|rule| selector_has_class(&rule.selector, "nav-button")));
    }

    #[test]
    fn shipped_navigation_style_expected_diagnostics_do_not_block_tokens() {
        let theme = mesh_core_theme::default_theme();
        let resolver = StyleResolver::new(&theme);
        let mut rules = parse_fixture_style(include_str!(
            "../../../../../modules/frontend/navigation-bar/src/main.mesh"
        ));
        rules.extend(parse_fixture_style(include_str!(
            "../../../../../modules/frontend/navigation-bar/src/components/volume-button.mesh"
        )));

        let (nav_style, nav_diagnostics) = resolve_class(&resolver, &rules, "nav-shell");
        let (status_style, status_diagnostics) =
            resolve_class(&resolver, &rules, "status-primary");
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
        assert_eq!(nav_style.background_color, Color::from_hex("#1C1B1F").unwrap());
        assert_eq!(nav_style.padding.left, 16.0);
        assert_eq!(status_style.font_size, 12.0);
        assert_eq!(button_style.border_width, Edges::all(2.0));
        assert_eq!(
            button_style.background_color,
            Color::from_hex("#211F26").unwrap()
        );
    }

    #[test]
    fn shipped_audio_style_fixture_resolves_painter_relevant_values() {
        let theme = mesh_core_theme::default_theme();
        let resolver = StyleResolver::new(&theme);
        let rules = parse_fixture_style(include_str!(
            "../../../../../modules/frontend/audio-popover/src/main.mesh"
        ));

        let (style, diagnostics) = resolve_class(&resolver, &rules, "audio-popover");

        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        assert_eq!(style.background_color, Color::from_hex("#211F26").unwrap());
        assert_eq!(style.color, Color::from_hex("#E6E1E5").unwrap());
        assert_eq!(style.padding, Edges::all(16.0));
        assert_eq!(style.border_radius, Corners::all(16.0));
        assert_eq!(style.gap, 16.0);
    }

    #[test]
    fn token_resolution_still_works_after_variable_support() {
        let theme = mesh_core_theme::default_theme();
        let resolver = StyleResolver::new(&theme);
        let value = StyleValue::Token("color.primary".to_string());
        assert_eq!(resolver.resolve_value(&value), "#6750A4");
    }

    #[test]
    fn transition_shorthand_parses_comma_separated_items() {
        let theme = mesh_core_theme::default_theme();
        let resolver = StyleResolver::new(&theme);
        let rules = vec![StyleRule {
            selector: Selector::Class("panel".to_string()),
            declarations: vec![mesh_core_component::style::Declaration {
                property: "transition".to_string(),
                value: StyleValue::Literal(
                    "opacity 150ms ease-in 25ms, border-color 250ms ease-out".to_string(),
                ),
            }],
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

        assert_eq!(style.transition.duration_ms, 150);
        assert_eq!(style.transition.delay_ms, 25);
        assert_eq!(style.transition.easing, TransitionEasing::EaseIn);
        assert!(style.transition.properties.animates_opacity());
        assert!(style.transition.properties.animates_border_color());
    }

    #[test]
    fn transition_property_supports_phase_8_visual_properties() {
        let properties = parse_transition_properties(
            "all, opacity, background, background-color, color, border-color, border-radius",
        );

        assert!(properties.animates_opacity());
        assert!(properties.animates_background_color());
        assert!(properties.animates_border_color());
        assert!(properties.animates_color());
        assert!(properties.animates_border_radius());
    }

    #[test]
    fn animation_longhands_store_metadata_only() {
        let theme = mesh_core_theme::default_theme();
        let resolver = StyleResolver::new(&theme);
        let rules = vec![StyleRule {
            selector: Selector::Class("panel".to_string()),
            declarations: vec![
                mesh_core_component::style::Declaration {
                    property: "animation-name".to_string(),
                    value: StyleValue::Literal("pulse".to_string()),
                },
                mesh_core_component::style::Declaration {
                    property: "animation-duration".to_string(),
                    value: StyleValue::Literal("320ms".to_string()),
                },
                mesh_core_component::style::Declaration {
                    property: "animation-delay".to_string(),
                    value: StyleValue::Literal("40ms".to_string()),
                },
                mesh_core_component::style::Declaration {
                    property: "animation-timing-function".to_string(),
                    value: StyleValue::Literal("ease-in-out".to_string()),
                },
                mesh_core_component::style::Declaration {
                    property: "animation-iteration-count".to_string(),
                    value: StyleValue::Literal("infinite".to_string()),
                },
                mesh_core_component::style::Declaration {
                    property: "animation-direction".to_string(),
                    value: StyleValue::Literal("alternate".to_string()),
                },
                mesh_core_component::style::Declaration {
                    property: "animation-fill-mode".to_string(),
                    value: StyleValue::Literal("both".to_string()),
                },
                mesh_core_component::style::Declaration {
                    property: "animation-play-state".to_string(),
                    value: StyleValue::Literal("paused".to_string()),
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

        assert_eq!(style.animation.name.as_deref(), Some("pulse"));
        assert_eq!(style.animation.duration_ms, 320);
        assert_eq!(style.animation.delay_ms, 40);
        assert_eq!(style.animation.easing, TransitionEasing::EaseInOut);
        assert_eq!(
            style.animation.iteration_count,
            AnimationIterationCount::Infinite
        );
        assert_eq!(style.animation.direction, AnimationDirection::Alternate);
        assert_eq!(style.animation.fill_mode, AnimationFillMode::Both);
        assert_eq!(style.animation.play_state, AnimationPlayState::Paused);
    }

    #[test]
    fn animation_shorthand_stores_metadata_only() {
        let theme = mesh_core_theme::default_theme();
        let resolver = StyleResolver::new(&theme);
        let rules = vec![StyleRule {
            selector: Selector::Class("panel".to_string()),
            declarations: vec![mesh_core_component::style::Declaration {
                property: "animation".to_string(),
                value: StyleValue::Literal(
                    "pulse 250ms ease-in-out 50ms 2 alternate both paused".to_string(),
                ),
            }],
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

        assert_eq!(style.animation.name.as_deref(), Some("pulse"));
        assert_eq!(style.animation.duration_ms, 250);
        assert_eq!(style.animation.delay_ms, 50);
        assert_eq!(style.animation.easing, TransitionEasing::EaseInOut);
        assert_eq!(
            style.animation.iteration_count,
            AnimationIterationCount::Number(2)
        );
        assert_eq!(style.animation.direction, AnimationDirection::Alternate);
        assert_eq!(style.animation.fill_mode, AnimationFillMode::Both);
        assert_eq!(style.animation.play_state, AnimationPlayState::Paused);
    }

    #[test]
    fn shell_card_css_subset_resolves_for_layout() {
        use mesh_core_component::parser::parse_component;

        let source = r#"
<style>
.shell-card {
    --pad: token(spacing.md);
    padding: var(--pad);
    margin: 4px 8px;
    border: 1px solid token(color.outline);
    display: flex;
    flex-direction: column;
    gap: 6px;
    position: relative;
    overflow: hidden;
}
</style>
"#;
        let file = parse_component(source).unwrap();
        let rules = file.style.unwrap().rules;

        let theme = mesh_core_theme::default_theme();
        let resolver = StyleResolver::new(&theme);
        let style = resolver.resolve_node_style(
            &rules,
            "box",
            &["shell-card".to_string()],
            None,
            StyleContext::default(),
            ElementState::default(),
        );

        assert_eq!(style.padding, Edges::all(16.0));
        assert_eq!(style.margin.top, 4.0);
        assert_eq!(style.margin.right, 8.0);
        assert_eq!(style.margin.bottom, 4.0);
        assert_eq!(style.margin.left, 8.0);
        assert_eq!(style.border_width, Edges::all(1.0));
        assert_eq!(style.border_color.a, 255);
        assert_eq!(style.direction, FlexDirection::Column);
        assert_eq!(style.gap, 6.0);
        assert_eq!(style.position, Position::Relative);
        assert_eq!(style.overflow_x, Overflow::Hidden);
        assert_eq!(style.overflow_y, Overflow::Hidden);
    }

    #[test]
    fn pseudo_state_rules_still_apply_after_variable_support() {
        pseudo_state_rules_apply_when_state_matches();
    }

    #[test]
    fn container_query_rules_still_apply_after_variable_support() {
        container_query_rules_apply_against_context();
    }
}
