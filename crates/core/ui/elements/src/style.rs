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
    use mesh_core_component::style::{Selector, StyleRule, StyleValue};

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
    fn keyframe_property_helper_accepts_transition_safe_properties() {
        for property in [
            "opacity",
            "transform",
            "border-radius",
            "padding",
            "font-size",
            "inset",
        ] {
            assert!(is_transition_safe_keyframe_property(property), "{property}");
        }
    }

    #[test]
    fn keyframe_property_helper_rejects_unsupported_properties() {
        for property in ["filter", "box-shadow", "grid-template-columns", "display"] {
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
