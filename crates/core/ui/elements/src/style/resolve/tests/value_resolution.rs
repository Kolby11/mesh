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

// cargo test -p mesh-core-elements --release -- numeric_theme_token_resolution_beats_string_roundtrip --ignored --nocapture
#[test]
#[ignore = "release-only numeric token resolution microbenchmark"]
fn numeric_theme_token_resolution_beats_string_roundtrip() {
    let mut theme = mesh_core_theme::default_theme();
    theme
        .tokens_mut()
        .insert("spacing.large".into(), TokenValue::Number(18.0));
    let resolver = StyleResolver::new(&theme);
    let variables = HashMap::new();
    let value = StyleValue::Var("--spacing-large".into());
    let iterations = 500_000;

    let old_started = std::time::Instant::now();
    let mut old_accumulator = 0.0f32;
    for _ in 0..iterations {
        old_accumulator += parse_px(
            &resolver.resolve_value_with_variables(std::hint::black_box(&value), &variables),
        );
    }
    let old_time = old_started.elapsed();

    let new_started = std::time::Instant::now();
    let mut new_accumulator = 0.0f32;
    for _ in 0..iterations {
        new_accumulator +=
            resolver.resolve_number_with_variables(std::hint::black_box(&value), &variables);
    }
    let new_time = new_started.elapsed();

    eprintln!(
        "numeric token resolution: string roundtrip {old_time:?}; typed fast path {new_time:?}; ratio {:.1}x; accumulators={old_accumulator}/{new_accumulator}",
        old_time.as_secs_f64() / new_time.as_secs_f64()
    );
    assert_eq!(old_accumulator, new_accumulator);
    assert!(new_time < old_time);
}

// cargo test -p mesh-core-elements --release -- time_theme_token_resolution_beats_string_clone --ignored --nocapture
#[test]
#[ignore = "release-only time token resolution microbenchmark"]
fn time_theme_token_resolution_beats_string_clone() {
    let mut theme = mesh_core_theme::default_theme();
    theme.tokens_mut().insert(
        "animation.duration.fast".into(),
        TokenValue::String("120ms".into()),
    );
    let resolver = StyleResolver::new(&theme);
    let variables = HashMap::new();
    let value = StyleValue::Var("--animation-duration-fast".into());
    let iterations = 500_000;

    let old_started = std::time::Instant::now();
    let mut old_accumulator = 0u32;
    for _ in 0..iterations {
        let resolved =
            resolver.resolve_value_with_variables(std::hint::black_box(&value), &variables);
        old_accumulator =
            old_accumulator.wrapping_add(std::hint::black_box(parse_first_time_ms(&resolved)));
    }
    let old_time = old_started.elapsed();

    let new_started = std::time::Instant::now();
    let mut new_accumulator = 0u32;
    for _ in 0..iterations {
        let parsed =
            resolver.with_resolved_str(std::hint::black_box(&value), &variables, |resolved| {
                parse_first_time_ms(resolved)
            });
        new_accumulator = new_accumulator.wrapping_add(std::hint::black_box(parsed));
    }
    let new_time = new_started.elapsed();

    eprintln!(
        "time token resolution: string clone {old_time:?}; borrowed fast path {new_time:?}; ratio {:.1}x; accumulators={old_accumulator}/{new_accumulator}",
        old_time.as_secs_f64() / new_time.as_secs_f64()
    );
    assert_eq!(old_accumulator, new_accumulator);
    assert!(new_time < old_time);
}

// cargo test -p mesh-core-elements --release -- transition_property_token_resolution_beats_string_clone --ignored --nocapture
#[test]
#[ignore = "release-only transition property token resolution microbenchmark"]
fn transition_property_token_resolution_beats_string_clone() {
    let mut theme = mesh_core_theme::default_theme();
    theme.tokens_mut().insert(
        "transition.properties.common".into(),
        TokenValue::String("opacity, transform, width".into()),
    );
    let resolver = StyleResolver::new(&theme);
    let variables = HashMap::new();
    let value = StyleValue::Var("--transition-properties-common".into());
    let iterations = 500_000;

    let old_started = std::time::Instant::now();
    let mut old_accumulator = 0u32;
    for _ in 0..iterations {
        let resolved =
            resolver.resolve_value_with_variables(std::hint::black_box(&value), &variables);
        old_accumulator = old_accumulator.wrapping_add(std::hint::black_box(
            transition_property_score(parse_transition_properties(&resolved)),
        ));
    }
    let old_time = old_started.elapsed();

    let new_started = std::time::Instant::now();
    let mut new_accumulator = 0u32;
    for _ in 0..iterations {
        let properties =
            resolver.with_resolved_str(std::hint::black_box(&value), &variables, |resolved| {
                parse_transition_properties(resolved)
            });
        new_accumulator = new_accumulator
            .wrapping_add(std::hint::black_box(transition_property_score(properties)));
    }
    let new_time = new_started.elapsed();

    eprintln!(
        "transition property token resolution: string clone {old_time:?}; borrowed fast path {new_time:?}; ratio {:.1}x; accumulators={old_accumulator}/{new_accumulator}",
        old_time.as_secs_f64() / new_time.as_secs_f64()
    );
    assert_eq!(old_accumulator, new_accumulator);
    assert!(new_time < old_time);
}

fn transition_property_score(properties: TransitionProperties) -> u32 {
    u32::from(properties.all)
        + u32::from(properties.opacity)
        + u32::from(properties.transform)
        + u32::from(properties.width)
        + u32::from(properties.height)
        + u32::from(properties.background_color)
        + u32::from(properties.color)
}

// cargo test -p mesh-core-elements --release -- filter_token_resolution_beats_string_clone --ignored --nocapture
#[test]
#[ignore = "release-only filter token resolution microbenchmark"]
fn filter_token_resolution_beats_string_clone() {
    let mut theme = mesh_core_theme::default_theme();
    theme.tokens_mut().insert(
        "filter.blur.medium".into(),
        TokenValue::String("blur(12px)".into()),
    );
    let resolver = StyleResolver::new(&theme);
    let variables = HashMap::new();
    let value = StyleValue::Var("--filter-blur-medium".into());
    let iterations = 500_000;

    let old_started = std::time::Instant::now();
    let mut old_accumulator = 0.0f32;
    for _ in 0..iterations {
        let resolved =
            resolver.resolve_value_with_variables(std::hint::black_box(&value), &variables);
        old_accumulator += std::hint::black_box(parse_filter(&resolved).blur_radius);
    }
    let old_time = old_started.elapsed();

    let new_started = std::time::Instant::now();
    let mut new_accumulator = 0.0f32;
    for _ in 0..iterations {
        let filter =
            resolver.with_resolved_str(std::hint::black_box(&value), &variables, |resolved| {
                parse_filter(resolved)
            });
        new_accumulator += std::hint::black_box(filter.blur_radius);
    }
    let new_time = new_started.elapsed();

    eprintln!(
        "filter token resolution: string clone {old_time:?}; borrowed fast path {new_time:?}; ratio {:.1}x; accumulators={old_accumulator}/{new_accumulator}",
        old_time.as_secs_f64() / new_time.as_secs_f64()
    );
    assert_eq!(old_accumulator, new_accumulator);
    assert!(new_time < old_time);
}

// cargo test -p mesh-core-elements --release -- background_image_token_resolution_beats_string_clone --ignored --nocapture
#[test]
#[ignore = "release-only background-image token resolution microbenchmark"]
fn background_image_token_resolution_beats_string_clone() {
    let mut theme = mesh_core_theme::default_theme();
    theme.tokens_mut().insert(
        "background.gradient.accent".into(),
        TokenValue::String("linear-gradient(#112233, #445566)".into()),
    );
    let resolver = StyleResolver::new(&theme);
    let variables = HashMap::new();
    let value = StyleValue::Var("--background-gradient-accent".into());
    let iterations = 500_000;

    let old_started = std::time::Instant::now();
    let mut old_accumulator = 0u32;
    for _ in 0..iterations {
        let resolved =
            resolver.resolve_value_with_variables(std::hint::black_box(&value), &variables);
        old_accumulator = old_accumulator.wrapping_add(std::hint::black_box(
            background_paint_score(&parse_background_image(&resolved)),
        ));
    }
    let old_time = old_started.elapsed();

    let new_started = std::time::Instant::now();
    let mut new_accumulator = 0u32;
    for _ in 0..iterations {
        let paint =
            resolver.with_resolved_str(std::hint::black_box(&value), &variables, |resolved| {
                parse_background_image(resolved)
            });
        new_accumulator =
            new_accumulator.wrapping_add(std::hint::black_box(background_paint_score(&paint)));
    }
    let new_time = new_started.elapsed();

    eprintln!(
        "background-image token resolution: string clone {old_time:?}; borrowed fast path {new_time:?}; ratio {:.1}x; accumulators={old_accumulator}/{new_accumulator}",
        old_time.as_secs_f64() / new_time.as_secs_f64()
    );
    assert_eq!(old_accumulator, new_accumulator);
    assert!(new_time < old_time);
}

fn background_paint_score(paint: &BackgroundPaint) -> u32 {
    match paint {
        BackgroundPaint::None => 1,
        BackgroundPaint::Image(source) => 2_u32.saturating_add(source.path.len() as u32),
        BackgroundPaint::LinearGradient(gradient) => 3_u32
            .saturating_add(u32::from(gradient.from.r))
            .saturating_add(u32::from(gradient.to.r)),
    }
}

// cargo test -p mesh-core-elements --release -- edge_shorthand_token_resolution_beats_string_clone --ignored --nocapture
#[test]
#[ignore = "release-only edge shorthand token resolution microbenchmark"]
fn edge_shorthand_token_resolution_beats_string_clone() {
    let mut theme = mesh_core_theme::default_theme();
    theme.tokens_mut().insert(
        "spacing.inset.panel".into(),
        TokenValue::String("4px 8px 12px 16px".into()),
    );
    let resolver = StyleResolver::new(&theme);
    let variables = HashMap::new();
    let value = StyleValue::Var("--spacing-inset-panel".into());
    let iterations = 500_000;

    let old_started = std::time::Instant::now();
    let mut old_accumulator = 0.0f32;
    for _ in 0..iterations {
        let resolved =
            resolver.resolve_value_with_variables(std::hint::black_box(&value), &variables);
        old_accumulator += std::hint::black_box(edge_score(parse_edges_shorthand(&resolved)));
    }
    let old_time = old_started.elapsed();

    let new_started = std::time::Instant::now();
    let mut new_accumulator = 0.0f32;
    for _ in 0..iterations {
        let edges =
            resolver.with_resolved_str(std::hint::black_box(&value), &variables, |resolved| {
                parse_edges_shorthand(resolved)
            });
        new_accumulator += std::hint::black_box(edge_score(edges));
    }
    let new_time = new_started.elapsed();

    eprintln!(
        "edge shorthand token resolution: string clone {old_time:?}; borrowed fast path {new_time:?}; ratio {:.1}x; accumulators={old_accumulator}/{new_accumulator}",
        old_time.as_secs_f64() / new_time.as_secs_f64()
    );
    assert_eq!(old_accumulator, new_accumulator);
    assert!(new_time < old_time);
}

fn edge_score(edges: Edges) -> f32 {
    edges.top + edges.right + edges.bottom + edges.left
}

// cargo test -p mesh-core-elements --release -- corner_shorthand_token_resolution_beats_string_clone --ignored --nocapture
#[test]
#[ignore = "release-only corner shorthand token resolution microbenchmark"]
fn corner_shorthand_token_resolution_beats_string_clone() {
    let mut theme = mesh_core_theme::default_theme();
    theme.tokens_mut().insert(
        "radius.panel".into(),
        TokenValue::String("4px 8px 12px 16px".into()),
    );
    let resolver = StyleResolver::new(&theme);
    let variables = HashMap::new();
    let value = StyleValue::Var("--radius-panel".into());
    let iterations = 500_000;

    let old_started = std::time::Instant::now();
    let mut old_accumulator = 0.0f32;
    for _ in 0..iterations {
        let resolved =
            resolver.resolve_value_with_variables(std::hint::black_box(&value), &variables);
        old_accumulator += std::hint::black_box(corner_score(parse_corners_shorthand(&resolved)));
    }
    let old_time = old_started.elapsed();

    let new_started = std::time::Instant::now();
    let mut new_accumulator = 0.0f32;
    for _ in 0..iterations {
        let corners =
            resolver.with_resolved_str(std::hint::black_box(&value), &variables, |resolved| {
                parse_corners_shorthand(resolved)
            });
        new_accumulator += std::hint::black_box(corner_score(corners));
    }
    let new_time = new_started.elapsed();

    eprintln!(
        "corner shorthand token resolution: string clone {old_time:?}; borrowed fast path {new_time:?}; ratio {:.1}x; accumulators={old_accumulator}/{new_accumulator}",
        old_time.as_secs_f64() / new_time.as_secs_f64()
    );
    assert_eq!(old_accumulator, new_accumulator);
    assert!(new_time < old_time);
}

fn corner_score(corners: Corners) -> f32 {
    corners.top_left + corners.top_right + corners.bottom_right + corners.bottom_left
}

// cargo test -p mesh-core-elements --release -- border_token_resolution_beats_string_clone --ignored --nocapture
#[test]
#[ignore = "release-only border token resolution microbenchmark"]
fn border_token_resolution_beats_string_clone() {
    let mut theme = mesh_core_theme::default_theme();
    theme.tokens_mut().insert(
        "border.panel".into(),
        TokenValue::String("2px solid #112233".into()),
    );
    let resolver = StyleResolver::new(&theme);
    let variables = HashMap::new();
    let value = StyleValue::Var("--border-panel".into());
    let iterations = 500_000;

    let old_started = std::time::Instant::now();
    let mut old_accumulator = 0.0f32;
    for _ in 0..iterations {
        let resolved =
            resolver.resolve_value_with_variables(std::hint::black_box(&value), &variables);
        let mut style = ComputedStyle::default();
        apply_border_shorthand(&mut style, &resolved);
        old_accumulator += std::hint::black_box(border_score(&style));
    }
    let old_time = old_started.elapsed();

    let new_started = std::time::Instant::now();
    let mut new_accumulator = 0.0f32;
    for _ in 0..iterations {
        let mut style = ComputedStyle::default();
        resolver.with_resolved_str(std::hint::black_box(&value), &variables, |resolved| {
            apply_border_shorthand(&mut style, resolved);
        });
        new_accumulator += std::hint::black_box(border_score(&style));
    }
    let new_time = new_started.elapsed();

    eprintln!(
        "border token resolution: string clone {old_time:?}; borrowed fast path {new_time:?}; ratio {:.1}x; accumulators={old_accumulator}/{new_accumulator}",
        old_time.as_secs_f64() / new_time.as_secs_f64()
    );
    assert_eq!(old_accumulator, new_accumulator);
    assert!(new_time < old_time);
}

fn border_score(style: &ComputedStyle) -> f32 {
    edge_score(style.border_width) + f32::from(style.border_color.r)
}

// cargo test -p mesh-core-elements --release -- transform_origin_token_resolution_beats_string_clone --ignored --nocapture
#[test]
#[ignore = "release-only transform-origin token resolution microbenchmark"]
fn transform_origin_token_resolution_beats_string_clone() {
    let mut theme = mesh_core_theme::default_theme();
    theme.tokens_mut().insert(
        "transform.origin.panel".into(),
        TokenValue::String("25% 75%".into()),
    );
    let resolver = StyleResolver::new(&theme);
    let variables = HashMap::new();
    let value = StyleValue::Var("--transform-origin-panel".into());
    let iterations = 500_000;

    let old_started = std::time::Instant::now();
    let mut old_accumulator = 0.0f32;
    for _ in 0..iterations {
        let resolved =
            resolver.resolve_value_with_variables(std::hint::black_box(&value), &variables);
        old_accumulator +=
            std::hint::black_box(transform_origin_score(parse_transform_origin(&resolved)));
    }
    let old_time = old_started.elapsed();

    let new_started = std::time::Instant::now();
    let mut new_accumulator = 0.0f32;
    for _ in 0..iterations {
        let origin =
            resolver.with_resolved_str(std::hint::black_box(&value), &variables, |resolved| {
                parse_transform_origin(resolved)
            });
        new_accumulator += std::hint::black_box(transform_origin_score(origin));
    }
    let new_time = new_started.elapsed();

    eprintln!(
        "transform-origin token resolution: string clone {old_time:?}; borrowed fast path {new_time:?}; ratio {:.1}x; accumulators={old_accumulator}/{new_accumulator}",
        old_time.as_secs_f64() / new_time.as_secs_f64()
    );
    assert_eq!(old_accumulator, new_accumulator);
    assert!(new_time < old_time);
}

fn transform_origin_score(origin: TransformOrigin) -> f32 {
    fn axis_score(value: TransformOriginValue) -> f32 {
        match value {
            TransformOriginValue::Percent(value) | TransformOriginValue::Px(value) => value,
        }
    }
    axis_score(origin.x) + axis_score(origin.y)
}

// cargo test -p mesh-core-elements --release -- transform_token_resolution_beats_string_clone --ignored --nocapture
#[test]
#[ignore = "release-only transform token resolution microbenchmark"]
fn transform_token_resolution_beats_string_clone() {
    let mut theme = mesh_core_theme::default_theme();
    theme.tokens_mut().insert(
        "transform.panel".into(),
        TokenValue::String("translate(12px, 8px) scale(1.2) rotate(15deg)".into()),
    );
    let resolver = StyleResolver::new(&theme);
    let variables = HashMap::new();
    let value = StyleValue::Var("--transform-panel".into());
    let iterations = 500_000;

    let old_started = std::time::Instant::now();
    let mut old_accumulator = 0.0f32;
    for _ in 0..iterations {
        let resolved =
            resolver.resolve_value_with_variables(std::hint::black_box(&value), &variables);
        old_accumulator += std::hint::black_box(transform_score(parse_transform(&resolved)));
    }
    let old_time = old_started.elapsed();

    let new_started = std::time::Instant::now();
    let mut new_accumulator = 0.0f32;
    for _ in 0..iterations {
        let transform =
            resolver.with_resolved_str(std::hint::black_box(&value), &variables, |resolved| {
                parse_transform(resolved)
            });
        new_accumulator += std::hint::black_box(transform_score(transform));
    }
    let new_time = new_started.elapsed();

    eprintln!(
        "transform token resolution: string clone {old_time:?}; borrowed fast path {new_time:?}; ratio {:.1}x; accumulators={old_accumulator}/{new_accumulator}",
        old_time.as_secs_f64() / new_time.as_secs_f64()
    );
    assert_eq!(old_accumulator, new_accumulator);
    assert!(new_time < old_time);
}

fn transform_score(transform: Transform2D) -> f32 {
    transform.translate_x
        + transform.translate_y
        + transform.scale_x
        + transform.scale_y
        + transform.rotation
}

// cargo test -p mesh-core-elements --release -- box_shadow_token_resolution_beats_string_clone --ignored --nocapture
#[test]
#[ignore = "release-only box-shadow token resolution microbenchmark"]
fn box_shadow_token_resolution_beats_string_clone() {
    let mut theme = mesh_core_theme::default_theme();
    theme.tokens_mut().insert(
        "shadow.panel".into(),
        TokenValue::String("2px 4px 8px 1px #112233".into()),
    );
    let resolver = StyleResolver::new(&theme);
    let variables = HashMap::new();
    let value = StyleValue::Var("--shadow-panel".into());
    let iterations = 500_000;

    let old_started = std::time::Instant::now();
    let mut old_accumulator = 0.0f32;
    for _ in 0..iterations {
        let resolved =
            resolver.resolve_value_with_variables(std::hint::black_box(&value), &variables);
        old_accumulator += std::hint::black_box(box_shadow_score(parse_box_shadow(&resolved)));
    }
    let old_time = old_started.elapsed();

    let new_started = std::time::Instant::now();
    let mut new_accumulator = 0.0f32;
    for _ in 0..iterations {
        let shadow =
            resolver.with_resolved_str(std::hint::black_box(&value), &variables, |resolved| {
                parse_box_shadow(resolved)
            });
        new_accumulator += std::hint::black_box(box_shadow_score(shadow));
    }
    let new_time = new_started.elapsed();

    eprintln!(
        "box-shadow token resolution: string clone {old_time:?}; borrowed fast path {new_time:?}; ratio {:.1}x; accumulators={old_accumulator}/{new_accumulator}",
        old_time.as_secs_f64() / new_time.as_secs_f64()
    );
    assert_eq!(old_accumulator, new_accumulator);
    assert!(new_time < old_time);
}

fn box_shadow_score(shadow: BoxShadow) -> f32 {
    shadow.offset_x
        + shadow.offset_y
        + shadow.blur_radius
        + shadow.spread_radius
        + f32::from(shadow.color.r)
        + f32::from(shadow.inset)
}

// cargo test -p mesh-core-elements --release -- flex_token_resolution_beats_string_clone --ignored --nocapture
#[test]
#[ignore = "release-only flex token resolution microbenchmark"]
fn flex_token_resolution_beats_string_clone() {
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
    let variables = HashMap::new();
    let value = StyleValue::Var("--flex-panel".into());
    let iterations = 500_000;

    let old_started = std::time::Instant::now();
    let mut old_accumulator = 0.0f32;
    for _ in 0..iterations {
        let resolved =
            resolver.resolve_value_with_variables(std::hint::black_box(&value), &variables);
        let mut style = ComputedStyle::default();
        apply_flex_value(&mut style, &resolved);
        old_accumulator += std::hint::black_box(flex_score(&style));
    }
    let old_time = old_started.elapsed();

    let new_started = std::time::Instant::now();
    let mut new_accumulator = 0.0f32;
    for _ in 0..iterations {
        let mut style = ComputedStyle::default();
        resolver.with_resolved_str(std::hint::black_box(&value), &variables, |resolved| {
            apply_flex_value(&mut style, resolved);
        });
        new_accumulator += std::hint::black_box(flex_score(&style));
    }
    let new_time = new_started.elapsed();

    eprintln!(
        "flex token resolution: string clone {old_time:?}; borrowed fast path {new_time:?}; ratio {:.1}x; accumulators={old_accumulator}/{new_accumulator}",
        old_time.as_secs_f64() / new_time.as_secs_f64()
    );
    assert_eq!(old_accumulator, new_accumulator);
    assert!(new_time < old_time);
}

fn flex_score(style: &ComputedStyle) -> f32 {
    let basis = match style.flex_basis {
        Dimension::Px(value) | Dimension::Percent(value) => value,
        Dimension::Auto => 1.0,
        Dimension::Content | Dimension::Fit => 2.0,
    };
    style.flex_grow + style.flex_shrink + basis
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

    assert_eq!(
        first_transition_mut(&mut style.transitions).easing,
        TransitionEasing::EaseInOut
    );
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

// cargo test -p mesh-core-elements --release -- animation_keyword_token_resolution_beats_string_clone --ignored --nocapture
#[test]
#[ignore = "release-only animation keyword token resolution microbenchmark"]
fn animation_keyword_token_resolution_beats_string_clone() {
    let mut theme = mesh_core_theme::default_theme();
    theme.tokens_mut().insert(
        "animation.easing".into(),
        TokenValue::String("ease-in-out".into()),
    );
    let resolver = StyleResolver::new(&theme);
    let variables = HashMap::new();
    let value = StyleValue::Var("--animation-easing".into());
    let iterations = 500_000;

    let old_started = std::time::Instant::now();
    let mut old_accumulator = 0u32;
    for _ in 0..iterations {
        let resolved =
            resolver.resolve_value_with_variables(std::hint::black_box(&value), &variables);
        let easing = parse_easing_keyword(first_comma_item(&resolved));
        old_accumulator = old_accumulator.wrapping_add(std::hint::black_box(easing_score(easing)));
    }
    let old_time = old_started.elapsed();

    let new_started = std::time::Instant::now();
    let mut new_accumulator = 0u32;
    for _ in 0..iterations {
        let easing =
            resolver.with_resolved_str(std::hint::black_box(&value), &variables, |resolved| {
                parse_easing_keyword(first_comma_item(resolved))
            });
        new_accumulator = new_accumulator.wrapping_add(std::hint::black_box(easing_score(easing)));
    }
    let new_time = new_started.elapsed();

    eprintln!(
        "animation keyword token resolution: string clone {old_time:?}; borrowed fast path {new_time:?}; ratio {:.1}x; accumulators={old_accumulator}/{new_accumulator}",
        old_time.as_secs_f64() / new_time.as_secs_f64()
    );
    assert_eq!(old_accumulator, new_accumulator);
    assert!(new_time < old_time);
}

fn easing_score(easing: TransitionEasing) -> u32 {
    match easing {
        TransitionEasing::Linear => 1,
        TransitionEasing::Ease => 2,
        TransitionEasing::EaseIn => 3,
        TransitionEasing::EaseOut => 4,
        TransitionEasing::EaseInOut => 5,
        TransitionEasing::CubicBezier(_, _, _, _) => 6,
        TransitionEasing::Steps(_, _) => 7,
    }
}

// cargo test -p mesh-core-elements --release -- overflow_theme_token_resolution_beats_string_clone --ignored --nocapture
#[test]
#[ignore = "release-only overflow token resolution microbenchmark"]
fn overflow_theme_token_resolution_beats_string_clone() {
    let mut theme = mesh_core_theme::default_theme();
    theme.tokens_mut().insert(
        "overflow.panel".into(),
        TokenValue::String("hidden auto".into()),
    );
    let resolver = StyleResolver::new(&theme);
    let variables = HashMap::new();
    let value = StyleValue::Var("--overflow-panel".into());
    let iterations = 500_000;

    let old_started = std::time::Instant::now();
    let mut old_accumulator = 0u32;
    for _ in 0..iterations {
        let resolved =
            resolver.resolve_value_with_variables(std::hint::black_box(&value), &variables);
        let (x, y) = parse_overflow_shorthand(&resolved);
        old_accumulator = old_accumulator.wrapping_add(std::hint::black_box(
            overflow_score(x).saturating_add(overflow_score(y)),
        ));
    }
    let old_time = old_started.elapsed();

    let new_started = std::time::Instant::now();
    let mut new_accumulator = 0u32;
    for _ in 0..iterations {
        let (x, y) =
            resolver.with_resolved_str(std::hint::black_box(&value), &variables, |resolved| {
                parse_overflow_shorthand(resolved)
            });
        new_accumulator = new_accumulator.wrapping_add(std::hint::black_box(
            overflow_score(x).saturating_add(overflow_score(y)),
        ));
    }
    let new_time = new_started.elapsed();

    eprintln!(
        "overflow token resolution: string clone {old_time:?}; borrowed fast path {new_time:?}; ratio {:.1}x; accumulators={old_accumulator}/{new_accumulator}",
        old_time.as_secs_f64() / new_time.as_secs_f64()
    );
    assert_eq!(old_accumulator, new_accumulator);
    assert!(new_time < old_time);
}

fn overflow_score(value: Overflow) -> u32 {
    match value {
        Overflow::Visible => 1,
        Overflow::Hidden => 2,
        Overflow::Auto => 3,
        Overflow::Scroll => 4,
    }
}

// cargo test -p mesh-core-elements --release -- dimension_theme_token_resolution_beats_string_clone --ignored --nocapture
#[test]
#[ignore = "release-only dimension token resolution microbenchmark"]
fn dimension_theme_token_resolution_beats_string_clone() {
    let mut theme = mesh_core_theme::default_theme();
    theme
        .tokens_mut()
        .insert("size.panel".into(), TokenValue::String("320px".into()));
    let resolver = StyleResolver::new(&theme);
    let variables = HashMap::new();
    let value = StyleValue::Var("--size-panel".into());
    let iterations = 500_000;

    let old_started = std::time::Instant::now();
    let mut old_accumulator = 0.0f32;
    for _ in 0..iterations {
        let resolved =
            resolver.resolve_value_with_variables(std::hint::black_box(&value), &variables);
        if let Dimension::Px(px) = parse_dimension(&resolved) {
            old_accumulator += std::hint::black_box(px);
        }
    }
    let old_time = old_started.elapsed();

    let new_started = std::time::Instant::now();
    let mut new_accumulator = 0.0f32;
    for _ in 0..iterations {
        let dimension =
            resolver.with_resolved_str(std::hint::black_box(&value), &variables, |resolved| {
                parse_dimension(resolved)
            });
        if let Dimension::Px(px) = dimension {
            new_accumulator += std::hint::black_box(px);
        }
    }
    let new_time = new_started.elapsed();

    eprintln!(
        "dimension token resolution: string clone {old_time:?}; borrowed fast path {new_time:?}; ratio {:.1}x; accumulators={old_accumulator}/{new_accumulator}",
        old_time.as_secs_f64() / new_time.as_secs_f64()
    );
    assert_eq!(old_accumulator, new_accumulator);
    assert!(new_time < old_time);
}

// cargo test -p mesh-core-elements --release -- keyword_theme_token_resolution_beats_string_clone --ignored --nocapture
#[test]
#[ignore = "release-only keyword token resolution microbenchmark"]
fn keyword_theme_token_resolution_beats_string_clone() {
    let mut theme = mesh_core_theme::default_theme();
    theme
        .tokens_mut()
        .insert("display.hidden".into(), TokenValue::String("none".into()));
    let resolver = StyleResolver::new(&theme);
    let variables = HashMap::new();
    let value = StyleValue::Var("--display-hidden".into());
    let iterations = 500_000;

    let old_started = std::time::Instant::now();
    let mut old_accumulator = 0u32;
    for _ in 0..iterations {
        let resolved =
            resolver.resolve_value_with_variables(std::hint::black_box(&value), &variables);
        let display = match resolved.as_str() {
            "none" => Display::None,
            _ => Display::Flex,
        };
        old_accumulator = old_accumulator.wrapping_add(std::hint::black_box(match display {
            Display::None => 1,
            Display::Flex => 2,
        }));
    }
    let old_time = old_started.elapsed();

    let new_started = std::time::Instant::now();
    let mut new_accumulator = 0u32;
    for _ in 0..iterations {
        let display =
            resolver.with_resolved_str(std::hint::black_box(&value), &variables, |resolved| {
                match resolved {
                    "none" => Display::None,
                    _ => Display::Flex,
                }
            });
        new_accumulator = new_accumulator.wrapping_add(std::hint::black_box(match display {
            Display::None => 1,
            Display::Flex => 2,
        }));
    }
    let new_time = new_started.elapsed();

    eprintln!(
        "keyword token resolution: string clone {old_time:?}; borrowed fast path {new_time:?}; ratio {:.1}x; accumulators={old_accumulator}/{new_accumulator}",
        old_time.as_secs_f64() / new_time.as_secs_f64()
    );
    assert_eq!(old_accumulator, new_accumulator);
    assert!(new_time < old_time);
}

// cargo test -p mesh-core-elements --release -- color_theme_token_resolution_beats_string_clone --ignored --nocapture
#[test]
#[ignore = "release-only color token resolution microbenchmark"]
fn color_theme_token_resolution_beats_string_clone() {
    let mut theme = mesh_core_theme::default_theme();
    theme
        .tokens_mut()
        .insert("color.primary".into(), TokenValue::String("#112233".into()));
    let resolver = StyleResolver::new(&theme);
    let variables = HashMap::new();
    let value = StyleValue::Var("--color-primary".into());
    let iterations = 500_000;

    let old_started = std::time::Instant::now();
    let mut old_accumulator = 0u32;
    for _ in 0..iterations {
        let resolved =
            resolver.resolve_value_with_variables(std::hint::black_box(&value), &variables);
        let color = Color::from_css(&resolved).unwrap_or(Color::TRANSPARENT);
        old_accumulator = old_accumulator.wrapping_add(std::hint::black_box(color.r as u32));
    }
    let old_time = old_started.elapsed();

    let new_started = std::time::Instant::now();
    let mut new_accumulator = 0u32;
    for _ in 0..iterations {
        let color = resolver.resolve_color_with_variables(std::hint::black_box(&value), &variables);
        new_accumulator = new_accumulator.wrapping_add(std::hint::black_box(color.r as u32));
    }
    let new_time = new_started.elapsed();

    eprintln!(
        "color token resolution: string clone {old_time:?}; borrowed fast path {new_time:?}; ratio {:.1}x; accumulators={old_accumulator}/{new_accumulator}",
        old_time.as_secs_f64() / new_time.as_secs_f64()
    );
    assert_eq!(old_accumulator, new_accumulator);
    assert!(new_time < old_time);
}
