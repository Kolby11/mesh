use super::super::declaration::*;
use super::super::*;
use crate::style::parse::*;
use mesh_core_component::style::{StyleValue, prop_variable_key};
use mesh_core_theme::TokenValue;
use std::collections::HashMap;

#[test]
fn numeric_resolution_matches_string_resolution_for_simple_references() {
    let mut theme = mesh_core_theme::default_theme();
    theme
        .tokens_mut()
        .insert("spacing.large".into(), TokenValue::Number(18.0));
    theme
        .tokens_mut()
        .insert("opacity.enabled".into(), TokenValue::Bool(true));
    let resolver = StyleResolver::new(&theme);
    let mut variables = HashMap::new();
    variables.insert("--local-size".into(), StyleValue::Literal("14px".into()));
    variables.insert(
        prop_variable_key("gap"),
        StyleValue::Var("--spacing-large".into()),
    );

    for value in [
        StyleValue::Literal("12px".into()),
        StyleValue::Var("--local-size".into()),
        StyleValue::Var("--spacing-large".into()),
        StyleValue::Var("--opacity-enabled".into()),
        StyleValue::Prop("gap".into()),
    ] {
        let string_resolved = parse_px(&resolver.resolve_value_with_variables(&value, &variables));
        let numeric_resolved = resolver.resolve_number_with_variables(&value, &variables);
        assert_eq!(numeric_resolved, string_resolved);
    }
}

#[test]
fn color_resolution_matches_string_resolution_for_simple_references() {
    let mut theme = mesh_core_theme::default_theme();
    theme
        .tokens_mut()
        .insert("color.primary".into(), TokenValue::String("#112233".into()));
    theme
        .tokens_mut()
        .insert("spacing.large".into(), TokenValue::Number(18.0));
    let resolver = StyleResolver::new(&theme);
    let mut variables = HashMap::new();
    variables.insert(
        "--local-color".into(),
        StyleValue::Literal("#445566".into()),
    );
    variables.insert(
        prop_variable_key("accent"),
        StyleValue::Var("--color-primary".into()),
    );

    for value in [
        StyleValue::Literal("#abcdef".into()),
        StyleValue::Literal("not-a-color".into()),
        StyleValue::Var("--local-color".into()),
        StyleValue::Var("--color-primary".into()),
        StyleValue::Var("--spacing-large".into()),
        StyleValue::Prop("accent".into()),
    ] {
        let resolved = resolver.resolve_value_with_variables(&value, &variables);
        let string_resolved = Color::from_css(&resolved).unwrap_or(Color::TRANSPARENT);
        let color_resolved = resolver.resolve_color_with_variables(&value, &variables);
        assert_eq!(color_resolved, string_resolved);
    }
}

#[test]
fn keyword_resolution_matches_string_resolution_for_simple_references() {
    let mut theme = mesh_core_theme::default_theme();
    theme
        .tokens_mut()
        .insert("display.hidden".into(), TokenValue::String("none".into()));
    let resolver = StyleResolver::new(&theme);
    let mut variables = HashMap::new();
    variables.insert("--local-display".into(), StyleValue::Literal("none".into()));
    variables.insert(
        prop_variable_key("display"),
        StyleValue::Var("--display-hidden".into()),
    );

    for value in [
        StyleValue::Literal("none".into()),
        StyleValue::Literal("flex".into()),
        StyleValue::Var("--local-display".into()),
        StyleValue::Var("--display-hidden".into()),
        StyleValue::Prop("display".into()),
    ] {
        let string_resolved = match resolver
            .resolve_value_with_variables(&value, &variables)
            .as_str()
        {
            "none" => Display::None,
            _ => Display::Flex,
        };
        let borrowed_resolved =
            resolver.with_resolved_str(&value, &variables, |resolved| match resolved {
                "none" => Display::None,
                _ => Display::Flex,
            });
        assert_eq!(borrowed_resolved, string_resolved);
    }
}

#[test]
fn dimension_resolution_matches_string_resolution_for_simple_references() {
    let mut theme = mesh_core_theme::default_theme();
    theme
        .tokens_mut()
        .insert("size.panel".into(), TokenValue::String("320px".into()));
    let resolver = StyleResolver::new(&theme);
    let mut variables = HashMap::new();
    variables.insert("--local-width".into(), StyleValue::Literal("75%".into()));
    variables.insert(
        prop_variable_key("width"),
        StyleValue::Var("--size-panel".into()),
    );

    for value in [
        StyleValue::Literal("auto".into()),
        StyleValue::Literal("240px".into()),
        StyleValue::Literal("50%".into()),
        StyleValue::Var("--local-width".into()),
        StyleValue::Var("--size-panel".into()),
        StyleValue::Prop("width".into()),
    ] {
        let string_resolved =
            parse_dimension(&resolver.resolve_value_with_variables(&value, &variables));
        let borrowed_resolved =
            resolver.with_resolved_str(&value, &variables, |resolved| parse_dimension(resolved));
        assert_eq!(borrowed_resolved, string_resolved);
    }
}

#[test]
fn overflow_resolution_matches_string_resolution_for_simple_references() {
    let mut theme = mesh_core_theme::default_theme();
    theme.tokens_mut().insert(
        "overflow.panel".into(),
        TokenValue::String("hidden auto".into()),
    );
    let resolver = StyleResolver::new(&theme);
    let mut variables = HashMap::new();
    variables.insert(
        "--local-overflow".into(),
        StyleValue::Literal("scroll".into()),
    );
    variables.insert(
        prop_variable_key("overflow"),
        StyleValue::Var("--overflow-panel".into()),
    );

    for value in [
        StyleValue::Literal("hidden".into()),
        StyleValue::Literal("hidden auto".into()),
        StyleValue::Var("--local-overflow".into()),
        StyleValue::Var("--overflow-panel".into()),
        StyleValue::Prop("overflow".into()),
    ] {
        let string_resolved =
            parse_overflow_shorthand(&resolver.resolve_value_with_variables(&value, &variables));
        let borrowed_resolved = resolver.with_resolved_str(&value, &variables, |resolved| {
            parse_overflow_shorthand(resolved)
        });
        assert_eq!(borrowed_resolved, string_resolved);
    }
}

#[test]
fn time_resolution_matches_string_resolution_for_simple_references() {
    let mut theme = mesh_core_theme::default_theme();
    theme.tokens_mut().insert(
        "animation.duration.fast".into(),
        TokenValue::String("120ms".into()),
    );
    let resolver = StyleResolver::new(&theme);
    let mut variables = HashMap::new();
    variables.insert(
        "--local-duration".into(),
        StyleValue::Literal("0.2s".into()),
    );
    variables.insert(
        prop_variable_key("duration"),
        StyleValue::Var("--animation-duration-fast".into()),
    );

    for value in [
        StyleValue::Literal("120ms".into()),
        StyleValue::Literal("0.2s".into()),
        StyleValue::Literal("300".into()),
        StyleValue::Var("--local-duration".into()),
        StyleValue::Var("--animation-duration-fast".into()),
        StyleValue::Prop("duration".into()),
    ] {
        let string_resolved =
            parse_first_time_ms(&resolver.resolve_value_with_variables(&value, &variables));
        let borrowed_resolved = resolver
            .with_resolved_str(&value, &variables, |resolved| parse_first_time_ms(resolved));
        assert_eq!(borrowed_resolved, string_resolved);
    }
}

#[test]
fn transition_property_resolution_matches_string_resolution_for_simple_references() {
    let mut theme = mesh_core_theme::default_theme();
    theme.tokens_mut().insert(
        "transition.properties.common".into(),
        TokenValue::String("opacity, transform, width".into()),
    );
    let resolver = StyleResolver::new(&theme);
    let mut variables = HashMap::new();
    variables.insert(
        "--local-properties".into(),
        StyleValue::Literal("all".into()),
    );
    variables.insert(
        prop_variable_key("properties"),
        StyleValue::Var("--transition-properties-common".into()),
    );

    for value in [
        StyleValue::Literal("opacity".into()),
        StyleValue::Literal("opacity, transform, width".into()),
        StyleValue::Var("--local-properties".into()),
        StyleValue::Var("--transition-properties-common".into()),
        StyleValue::Prop("properties".into()),
    ] {
        let string_resolved =
            parse_transition_properties(&resolver.resolve_value_with_variables(&value, &variables));
        let borrowed_resolved = resolver.with_resolved_str(&value, &variables, |resolved| {
            parse_transition_properties(resolved)
        });
        assert_eq!(borrowed_resolved, string_resolved);
    }
}

#[test]
fn filter_resolution_matches_string_resolution_for_simple_references() {
    let mut theme = mesh_core_theme::default_theme();
    theme.tokens_mut().insert(
        "filter.blur.medium".into(),
        TokenValue::String("blur(12px)".into()),
    );
    let resolver = StyleResolver::new(&theme);
    let mut variables = HashMap::new();
    variables.insert("--local-filter".into(), StyleValue::Literal("none".into()));
    variables.insert(
        prop_variable_key("filter"),
        StyleValue::Var("--filter-blur-medium".into()),
    );

    for value in [
        StyleValue::Literal("none".into()),
        StyleValue::Literal("blur(4px)".into()),
        StyleValue::Var("--local-filter".into()),
        StyleValue::Var("--filter-blur-medium".into()),
        StyleValue::Prop("filter".into()),
    ] {
        let string_resolved =
            parse_filter(&resolver.resolve_value_with_variables(&value, &variables));
        let borrowed_resolved =
            resolver.with_resolved_str(&value, &variables, |resolved| parse_filter(resolved));
        assert_eq!(borrowed_resolved, string_resolved);
    }
}

#[test]
fn background_image_resolution_matches_string_resolution_for_simple_references() {
    let mut theme = mesh_core_theme::default_theme();
    theme.tokens_mut().insert(
        "background.gradient.accent".into(),
        TokenValue::String("linear-gradient(#112233, #445566)".into()),
    );
    let resolver = StyleResolver::new(&theme);
    let mut variables = HashMap::new();
    variables.insert(
        "--local-background".into(),
        StyleValue::Literal("none".into()),
    );
    variables.insert(
        prop_variable_key("background"),
        StyleValue::Var("--background-gradient-accent".into()),
    );

    for value in [
        StyleValue::Literal("none".into()),
        StyleValue::Literal("url(assets/panel.png)".into()),
        StyleValue::Literal("linear-gradient(#112233, #445566)".into()),
        StyleValue::Var("--local-background".into()),
        StyleValue::Var("--background-gradient-accent".into()),
        StyleValue::Prop("background".into()),
    ] {
        let string_resolved =
            parse_background_image(&resolver.resolve_value_with_variables(&value, &variables));
        let borrowed_resolved = resolver.with_resolved_str(&value, &variables, |resolved| {
            parse_background_image(resolved)
        });
        assert_eq!(borrowed_resolved, string_resolved);
    }
}

#[test]
fn edge_shorthand_resolution_matches_string_resolution_for_simple_references() {
    let mut theme = mesh_core_theme::default_theme();
    theme.tokens_mut().insert(
        "spacing.inset.panel".into(),
        TokenValue::String("4px 8px 12px 16px".into()),
    );
    let resolver = StyleResolver::new(&theme);
    let mut variables = HashMap::new();
    variables.insert(
        "--local-inset".into(),
        StyleValue::Literal("2px 6px".into()),
    );
    variables.insert(
        prop_variable_key("inset"),
        StyleValue::Var("--spacing-inset-panel".into()),
    );

    for value in [
        StyleValue::Literal("4px".into()),
        StyleValue::Literal("4px 8px".into()),
        StyleValue::Literal("4px 8px 12px 16px".into()),
        StyleValue::Var("--local-inset".into()),
        StyleValue::Var("--spacing-inset-panel".into()),
        StyleValue::Prop("inset".into()),
    ] {
        let string_resolved =
            parse_edges_shorthand(&resolver.resolve_value_with_variables(&value, &variables));
        let borrowed_resolved = resolver.with_resolved_str(&value, &variables, |resolved| {
            parse_edges_shorthand(resolved)
        });
        assert_eq!(borrowed_resolved, string_resolved);
    }
}

#[test]
fn corner_shorthand_resolution_matches_string_resolution_for_simple_references() {
    let mut theme = mesh_core_theme::default_theme();
    theme.tokens_mut().insert(
        "radius.panel".into(),
        TokenValue::String("4px 8px 12px 16px".into()),
    );
    let resolver = StyleResolver::new(&theme);
    let mut variables = HashMap::new();
    variables.insert(
        "--local-radius".into(),
        StyleValue::Literal("2px 6px".into()),
    );
    variables.insert(
        prop_variable_key("radius"),
        StyleValue::Var("--radius-panel".into()),
    );

    for value in [
        StyleValue::Literal("4px".into()),
        StyleValue::Literal("4px 8px".into()),
        StyleValue::Literal("4px 8px 12px 16px".into()),
        StyleValue::Var("--local-radius".into()),
        StyleValue::Var("--radius-panel".into()),
        StyleValue::Prop("radius".into()),
    ] {
        let string_resolved =
            parse_corners_shorthand(&resolver.resolve_value_with_variables(&value, &variables));
        let borrowed_resolved = resolver.with_resolved_str(&value, &variables, |resolved| {
            parse_corners_shorthand(resolved)
        });
        assert_eq!(borrowed_resolved, string_resolved);
    }
}

#[test]
fn border_resolution_matches_string_resolution_for_simple_references() {
    let mut theme = mesh_core_theme::default_theme();
    theme.tokens_mut().insert(
        "border.panel".into(),
        TokenValue::String("2px solid #112233".into()),
    );
    let resolver = StyleResolver::new(&theme);
    let mut variables = HashMap::new();
    variables.insert("--local-border".into(), StyleValue::Literal("none".into()));
    variables.insert(
        prop_variable_key("border"),
        StyleValue::Var("--border-panel".into()),
    );

    for value in [
        StyleValue::Literal("none".into()),
        StyleValue::Literal("1px solid #445566".into()),
        StyleValue::Var("--local-border".into()),
        StyleValue::Var("--border-panel".into()),
        StyleValue::Prop("border".into()),
    ] {
        let mut string_style = ComputedStyle::default();
        apply_border_shorthand(
            &mut string_style,
            &resolver.resolve_value_with_variables(&value, &variables),
        );
        let mut borrowed_style = ComputedStyle::default();
        resolver.with_resolved_str(&value, &variables, |resolved| {
            apply_border_shorthand(&mut borrowed_style, resolved);
        });
        assert_eq!(borrowed_style.border_width, string_style.border_width);
        assert_eq!(borrowed_style.border_color, string_style.border_color);
    }

    let color = StyleValue::Var("--border-panel".into());
    let string_color =
        parse_border_color_shorthand(&resolver.resolve_value_with_variables(&color, &variables));
    let borrowed_color = resolver.with_resolved_str(&color, &variables, |resolved| {
        parse_border_color_shorthand(resolved)
    });
    assert_eq!(borrowed_color, string_color);
}

#[test]
fn transform_origin_resolution_matches_string_resolution_for_simple_references() {
    let mut theme = mesh_core_theme::default_theme();
    theme.tokens_mut().insert(
        "transform.origin.panel".into(),
        TokenValue::String("25% 75%".into()),
    );
    let resolver = StyleResolver::new(&theme);
    let mut variables = HashMap::new();
    variables.insert(
        "--local-origin".into(),
        StyleValue::Literal("left top".into()),
    );
    variables.insert(
        prop_variable_key("origin"),
        StyleValue::Var("--transform-origin-panel".into()),
    );

    for value in [
        StyleValue::Literal("center".into()),
        StyleValue::Literal("10px 20px".into()),
        StyleValue::Var("--local-origin".into()),
        StyleValue::Var("--transform-origin-panel".into()),
        StyleValue::Prop("origin".into()),
    ] {
        let string_resolved =
            parse_transform_origin(&resolver.resolve_value_with_variables(&value, &variables));
        let borrowed_resolved = resolver.with_resolved_str(&value, &variables, |resolved| {
            parse_transform_origin(resolved)
        });
        assert_eq!(borrowed_resolved, string_resolved);
    }
}

#[test]
fn transform_resolution_matches_string_resolution_for_simple_references() {
    let mut theme = mesh_core_theme::default_theme();
    theme.tokens_mut().insert(
        "transform.panel".into(),
        TokenValue::String("translate(12px, 8px) scale(1.2) rotate(15deg)".into()),
    );
    let resolver = StyleResolver::new(&theme);
    let mut variables = HashMap::new();
    variables.insert(
        "--local-transform".into(),
        StyleValue::Literal("translateX(4px)".into()),
    );
    variables.insert(
        prop_variable_key("transform"),
        StyleValue::Var("--transform-panel".into()),
    );

    for value in [
        StyleValue::Literal("none".into()),
        StyleValue::Literal("translate(10px, 20px) rotate(0.25turn)".into()),
        StyleValue::Var("--local-transform".into()),
        StyleValue::Var("--transform-panel".into()),
        StyleValue::Prop("transform".into()),
    ] {
        let string_resolved =
            parse_transform(&resolver.resolve_value_with_variables(&value, &variables));
        let borrowed_resolved =
            resolver.with_resolved_str(&value, &variables, |resolved| parse_transform(resolved));
        assert_eq!(borrowed_resolved, string_resolved);
    }
}

#[test]
fn transform_parses_skew_functions_in_degrees_and_turns() {
    let transform = parse_transform("translateX(4px) skewX(-15deg) skewY(0.05turn)");
    assert_eq!(transform.translate_x, 4.0);
    assert!((transform.skew_x + 15f32.to_radians()).abs() < 1e-6);
    assert!((transform.skew_y - 0.05 * std::f32::consts::TAU).abs() < 1e-6);
    assert!(!transform.is_translation_only());

    let both = parse_transform("skew(10deg, 5deg)");
    assert!((both.skew_x - 10f32.to_radians()).abs() < 1e-6);
    assert!((both.skew_y - 5f32.to_radians()).abs() < 1e-6);
}

#[test]
fn box_shadow_resolution_matches_string_resolution_for_simple_references() {
    let mut theme = mesh_core_theme::default_theme();
    theme.tokens_mut().insert(
        "shadow.panel".into(),
        TokenValue::String("2px 4px 8px 1px #112233".into()),
    );
    let resolver = StyleResolver::new(&theme);
    let mut variables = HashMap::new();
    variables.insert("--local-shadow".into(), StyleValue::Literal("none".into()));
    variables.insert(
        prop_variable_key("shadow"),
        StyleValue::Var("--shadow-panel".into()),
    );

    for value in [
        StyleValue::Literal("none".into()),
        StyleValue::Literal("1px 2px 3px #445566".into()),
        StyleValue::Var("--local-shadow".into()),
        StyleValue::Var("--shadow-panel".into()),
        StyleValue::Prop("shadow".into()),
    ] {
        let string_resolved =
            parse_box_shadow(&resolver.resolve_value_with_variables(&value, &variables));
        let borrowed_resolved =
            resolver.with_resolved_str(&value, &variables, |resolved| parse_box_shadow(resolved));
        assert_eq!(borrowed_resolved, string_resolved);
    }
}

#[test]
fn flex_resolution_matches_string_resolution_for_simple_references() {
    fn apply_flex_value(style: &mut ComputedStyle, value: &str) {
        let value = value.trim();
        if value == "none" {
            style.flex_grow = 0.0;
            style.flex_shrink = 0.0;
            style.flex_basis = Dimension::Auto;
        } else if value == "auto" {
            style.flex_grow = 1.0;
            style.flex_shrink = 1.0;
            style.flex_basis = Dimension::Auto;
        } else if let Ok(n) = value.parse::<f32>() {
            style.flex_grow = n;
            style.flex_shrink = 1.0;
            style.flex_basis = Dimension::Px(0.0);
        } else {
            apply_flex_shorthand(style, value);
        }
    }

    let mut theme = mesh_core_theme::default_theme();
    theme
        .tokens_mut()
        .insert("flex.panel".into(), TokenValue::String("2 1 240px".into()));
    let resolver = StyleResolver::new(&theme);
    let mut variables = HashMap::new();
    variables.insert("--local-flex".into(), StyleValue::Literal("auto".into()));
    variables.insert(
        prop_variable_key("flex"),
        StyleValue::Var("--flex-panel".into()),
    );

    for value in [
        StyleValue::Literal("none".into()),
        StyleValue::Literal("1".into()),
        StyleValue::Literal("2 1 240px".into()),
        StyleValue::Var("--local-flex".into()),
        StyleValue::Var("--flex-panel".into()),
        StyleValue::Prop("flex".into()),
    ] {
        let mut string_style = ComputedStyle::default();
        apply_flex_value(
            &mut string_style,
            &resolver.resolve_value_with_variables(&value, &variables),
        );
        let mut borrowed_style = ComputedStyle::default();
        resolver.with_resolved_str(&value, &variables, |resolved| {
            apply_flex_value(&mut borrowed_style, resolved);
        });
        assert_eq!(borrowed_style.flex_grow, string_style.flex_grow);
        assert_eq!(borrowed_style.flex_shrink, string_style.flex_shrink);
        assert_eq!(borrowed_style.flex_basis, string_style.flex_basis);
    }
}

#[test]
fn animation_keyword_properties_resolve_borrowed_tokens() {
    let mut theme = mesh_core_theme::default_theme();
    theme.tokens_mut().insert(
        "animation.easing".into(),
        TokenValue::String("ease-in-out".into()),
    );
    theme.tokens_mut().insert(
        "animation.direction".into(),
        TokenValue::String("reverse".into()),
    );
    theme
        .tokens_mut()
        .insert("animation.fill".into(), TokenValue::String("both".into()));
    theme
        .tokens_mut()
        .insert("animation.play".into(), TokenValue::String("paused".into()));
    let resolver = StyleResolver::new(&theme);
    let variables = HashMap::new();
    let mut style = ComputedStyle::default();

    apply_declaration(
        &mut style,
        "transition-timing-function",
        &StyleValue::Var("--animation-easing".into()),
        &resolver,
        &variables,
    );
    apply_declaration(
        &mut style,
        "animation-timing-function",
        &StyleValue::Var("--animation-easing".into()),
        &resolver,
        &variables,
    );
    apply_declaration(
        &mut style,
        "animation-direction",
        &StyleValue::Var("--animation-direction".into()),
        &resolver,
        &variables,
    );
    apply_declaration(
        &mut style,
        "animation-fill-mode",
        &StyleValue::Var("--animation-fill".into()),
        &resolver,
        &variables,
    );
    apply_declaration(
        &mut style,
        "animation-play-state",
        &StyleValue::Var("--animation-play".into()),
        &resolver,
        &variables,
    );

    assert_eq!(style.transitions[0].easing, TransitionEasing::EaseInOut);
    let animation = first_animation_mut(&mut style.animations);
    assert_eq!(animation.easing, TransitionEasing::EaseInOut);
    assert_eq!(animation.direction, AnimationDirection::Reverse);
    assert_eq!(animation.fill_mode, AnimationFillMode::Both);
    assert_eq!(animation.play_state, AnimationPlayState::Paused);
}

#[test]
fn size_constraints_accept_every_dimension_form() {
    let mut theme = mesh_core_theme::default_theme();
    theme
        .tokens_mut()
        .insert("size.panel".into(), TokenValue::String("320px".into()));
    let resolver = StyleResolver::new(&theme);
    let mut variables = HashMap::new();
    variables.insert("--fill".into(), StyleValue::Literal("100%".into()));

    let cases = [
        (
            "min-width",
            StyleValue::Literal("240px".into()),
            Dimension::Px(240.0),
        ),
        (
            "max-width",
            StyleValue::Literal("100%".into()),
            Dimension::Percent(100.0),
        ),
        (
            "min-height",
            StyleValue::Literal("auto".into()),
            Dimension::Auto,
        ),
        // `none` is the CSS initial value for the max-* properties and has
        // to clear the constraint, not clamp it to zero.
        (
            "max-height",
            StyleValue::Literal("none".into()),
            Dimension::Auto,
        ),
        (
            "max-width",
            StyleValue::Var("--fill".into()),
            Dimension::Percent(100.0),
        ),
        (
            "min-width",
            StyleValue::Var("--size-panel".into()),
            Dimension::Px(320.0),
        ),
        (
            "max-width",
            StyleValue::Literal("fit-content".into()),
            Dimension::Fit,
        ),
    ];

    for (property, value, expected) in cases {
        let mut style = ComputedStyle::default();
        apply_declaration(&mut style, property, &value, &resolver, &variables);
        let actual = match property {
            "min-width" => style.min_width,
            "max-width" => style.max_width,
            "min-height" => style.min_height,
            _ => style.max_height,
        };
        assert_eq!(actual, expected, "{property}: {value:?}");
    }
}

#[test]
fn fit_content_uses_the_content_bounds_layout_mode() {
    let theme = mesh_core_theme::default_theme();
    let resolver = StyleResolver::new(&theme);
    let mut style = ComputedStyle::default();

    apply_declaration(
        &mut style,
        "width",
        &StyleValue::Literal("fit-content".into()),
        &resolver,
        &HashMap::new(),
    );

    assert_eq!(style.width, Dimension::Fit);
}
