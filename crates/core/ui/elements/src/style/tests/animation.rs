use super::super::parse::parse_transition_properties;
use super::super::*;
use super::resolution::*;
use crate::tree::ElementState;
use mesh_core_component::style::{Selector, StyleRule, StyleValue};

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

    assert_eq!(style.transitions[0].duration_ms, 150);
    assert_eq!(style.transitions[0].delay_ms, 25);
    assert_eq!(style.transitions[0].easing, TransitionEasing::EaseIn);
    assert!(style.transitions[0].properties.animates_opacity());
    assert!(style.transitions[1].properties.animates_border_color());
}

fn resolve_single_decl(property: &str, value: &str) -> ComputedStyle {
    let theme = mesh_core_theme::default_theme();
    let resolver = StyleResolver::new(&theme);
    let rules = vec![StyleRule {
        selector: Selector::Class("panel".to_string()),
        declarations: vec![mesh_core_component::style::Declaration {
            property: property.to_string(),
            value: StyleValue::Literal(value.to_string()),
        }],
        container_query: None,
    }];
    resolver.resolve_node_style(
        &rules,
        "box",
        &["panel".to_string()],
        None,
        StyleContext::default(),
        ElementState::default(),
    )
}

#[test]
fn transition_shorthand_parses_steps_with_position() {
    let style = resolve_single_decl("transition", "opacity 200ms steps(4, jump-end)");
    assert_eq!(
        style.transitions[0].easing,
        TransitionEasing::Steps(4, StepPosition::JumpEnd)
    );
    assert!(style.transitions[0].properties.animates_opacity());
}

#[test]
fn transition_shorthand_parses_step_end_keyword() {
    let style = resolve_single_decl("transition", "transform 100ms step-start");
    assert_eq!(
        style.transitions[0].easing,
        TransitionEasing::Steps(1, StepPosition::JumpStart)
    );
}

#[test]
fn animation_shorthand_parses_steps_with_inner_space() {
    let style = resolve_single_decl("animation", "pulse 1s steps(3, jump-none) infinite");
    assert_eq!(
        style.animations[0].easing,
        TransitionEasing::Steps(3, StepPosition::JumpNone)
    );
    assert_eq!(style.animations[0].name.as_deref(), Some("pulse"));
    assert_eq!(style.animations[0].duration_ms, 1000);
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
fn animation_property_bucket_classifies_paint_only_properties() {
    for (name, properties) in [
        (
            "opacity",
            TransitionProperties {
                opacity: true,
                ..TransitionProperties::none()
            },
        ),
        (
            "background-color",
            TransitionProperties {
                background_color: true,
                ..TransitionProperties::none()
            },
        ),
        (
            "border-color",
            TransitionProperties {
                border_color: true,
                ..TransitionProperties::none()
            },
        ),
        (
            "color",
            TransitionProperties {
                color: true,
                ..TransitionProperties::none()
            },
        ),
        (
            "border-radius",
            TransitionProperties {
                border_radius: true,
                ..TransitionProperties::none()
            },
        ),
        (
            "transform",
            TransitionProperties {
                transform: true,
                ..TransitionProperties::none()
            },
        ),
        (
            "border-width",
            TransitionProperties {
                border_width: true,
                ..TransitionProperties::none()
            },
        ),
    ] {
        assert_eq!(
            properties.animation_bucket(),
            AnimationPropertyBucket::PaintOnly,
            "{name}"
        );
        assert!(properties.has_paint_only_animation(), "{name}");
    }
}

#[test]
fn animation_property_bucket_classifies_layer_effect_properties() {
    for (name, properties) in [
        (
            "box-shadow",
            TransitionProperties {
                box_shadow: true,
                ..TransitionProperties::none()
            },
        ),
        (
            "filter",
            TransitionProperties {
                filter: true,
                ..TransitionProperties::none()
            },
        ),
        (
            "backdrop-filter",
            TransitionProperties {
                backdrop_filter: true,
                ..TransitionProperties::none()
            },
        ),
    ] {
        assert_eq!(
            properties.animation_bucket(),
            AnimationPropertyBucket::LayerEffect,
            "{name}"
        );
        assert!(properties.has_layer_effect_animation(), "{name}");
    }
}

#[test]
fn animation_property_bucket_classifies_layout_affecting_properties() {
    for (name, properties) in [
        (
            "width",
            TransitionProperties {
                width: true,
                ..TransitionProperties::none()
            },
        ),
        (
            "height",
            TransitionProperties {
                height: true,
                ..TransitionProperties::none()
            },
        ),
        (
            "padding",
            TransitionProperties {
                padding: true,
                ..TransitionProperties::none()
            },
        ),
        (
            "font-size",
            TransitionProperties {
                font_size: true,
                ..TransitionProperties::none()
            },
        ),
        (
            "inset-left",
            TransitionProperties {
                inset_left: true,
                ..TransitionProperties::none()
            },
        ),
    ] {
        assert_eq!(
            properties.animation_bucket(),
            AnimationPropertyBucket::LayoutAffecting,
            "{name}"
        );
        assert!(properties.has_layout_affecting_animation(), "{name}");
    }
}

#[test]
fn animation_property_bucket_treats_all_as_layout_affecting() {
    let properties = TransitionProperties::all();

    assert_eq!(
        properties.animation_bucket(),
        AnimationPropertyBucket::LayoutAffecting
    );
    assert!(properties.has_layout_affecting_animation());
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

    assert_eq!(style.animations[0].name.as_deref(), Some("pulse"));
    assert_eq!(style.animations[0].duration_ms, 320);
    assert_eq!(style.animations[0].delay_ms, 40);
    assert_eq!(style.animations[0].easing, TransitionEasing::EaseInOut);
    assert_eq!(
        style.animations[0].iteration_count,
        AnimationIterationCount::Infinite
    );
    assert_eq!(style.animations[0].direction, AnimationDirection::Alternate);
    assert_eq!(style.animations[0].fill_mode, AnimationFillMode::Both);
    assert_eq!(style.animations[0].play_state, AnimationPlayState::Paused);
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

    assert_eq!(style.animations[0].name.as_deref(), Some("pulse"));
    assert_eq!(style.animations[0].duration_ms, 250);
    assert_eq!(style.animations[0].delay_ms, 50);
    assert_eq!(style.animations[0].easing, TransitionEasing::EaseInOut);
    assert_eq!(
        style.animations[0].iteration_count,
        AnimationIterationCount::Number(2)
    );
    assert_eq!(style.animations[0].direction, AnimationDirection::Alternate);
    assert_eq!(style.animations[0].fill_mode, AnimationFillMode::Both);
    assert_eq!(style.animations[0].play_state, AnimationPlayState::Paused);
}

#[test]
fn shell_card_css_subset_resolves_for_layout() {
    use mesh_core_component::parser::parse_component;

    let source = r#"
<style>
.shell-card {
--pad: var(--spacing-md);
padding: var(--pad);
margin: 4px 8px;
border: 1px solid var(--color-outline);
display: flex;
flex-direction: column;
gap: 6px;
position: relative;
overflow: hidden;
mix-blend-mode: multiply;
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
    assert_eq!(style.mix_blend_mode, BlendMode::Multiply);
}

#[test]
fn pseudo_state_rules_still_apply_after_variable_support() {
    pseudo_state_rules_apply_when_state_matches();
}

#[test]
fn container_query_rules_still_apply_after_variable_support() {
    container_query_rules_apply_against_context();
}
