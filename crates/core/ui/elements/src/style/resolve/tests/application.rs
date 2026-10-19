use super::super::index::*;
use super::super::state::*;
use super::super::*;
use super::common::*;
use crate::tree::ElementState;
use mesh_core_component::style::{Declaration, Selector, StyleRule, StyleValue};
use mesh_core_theme::{Theme, TokenValue};
use std::collections::HashMap;

#[test]
fn cached_theme_token_value_matches_theme_lookup() {
    let theme = mesh_core_theme::default_theme();
    let resolver = StyleResolver::new(&theme);

    match resolver.cached_theme_token_value("--color-primary") {
        CachedThemeTokenValue::String(value) => assert_eq!(
            Some(value.as_ref()),
            theme.token("color.primary").and_then(|value| match value {
                TokenValue::String(value) => Some(value.as_str()),
                TokenValue::Number(_) | TokenValue::Bool(_) => None,
            })
        ),
        CachedThemeTokenValue::Number(_)
        | CachedThemeTokenValue::Bool(_)
        | CachedThemeTokenValue::Error(_)
        | CachedThemeTokenValue::Missing => panic!("expected string color token"),
    }
    assert!(
        resolver
            .cached_theme_token_value("--definitely-missing")
            .is_missing()
    );
}

#[test]
fn cached_inline_styles_preserve_values_and_parse_diagnostics() {
    let theme = mesh_core_theme::default_theme();
    let resolver = StyleResolver::new(&theme);
    let rules = Vec::new();
    let index = StyleRuleIndex::new(&rules);
    let classes = Vec::new();

    for (source, expected_left) in [
        ("left: 12px; top: 8px;", 12.0),
        ("left: 42px; top: 8px;", 42.0),
        ("left: 12px; top: 8px;", 12.0),
    ] {
        let style = resolver.resolve_node_style_for_module_indexed_with_inline_style(
            &rules,
            &index,
            "box",
            &classes,
            None,
            Some(source),
            StyleContext::default(),
            ElementState::default(),
            Some("@test/inline-style-cache"),
        );
        assert_eq!(style.inset_left, Some(expected_left));
    }

    let mut invalid = crate::tree::WidgetNode::new("box");
    invalid.attributes.insert("style".into(), "left: {;".into());
    let (_, first_diagnostics) = resolver.resolve_node_style_with_diagnostics_for_node_indexed(
        &rules,
        &index,
        &mut invalid,
        StyleContext::default(),
    );
    let (_, cached_diagnostics) = resolver.resolve_node_style_with_diagnostics_for_node_indexed(
        &rules,
        &index,
        &mut invalid,
        StyleContext::default(),
    );
    assert!(!first_diagnostics.is_empty());
    assert_eq!(cached_diagnostics, first_diagnostics);
}

#[test]
fn state_to_rules_multiple_rules_for_same_bit() {
    let rules = vec![rule_with_state("hover"), rule_with_state("hover")];
    let index = StyleRuleIndex::new(&rules);

    let result = index.rules_for_state_bit(STATE_HOVERED);
    assert_eq!(result.len(), 2);
    assert!(result.contains(&0));
    assert!(result.contains(&1));
}

#[test]
fn custom_properties_inherit_through_initial_style_resolution() {
    let theme = mesh_core_theme::default_theme();
    let rules = vec![
        StyleRule {
            selector: Selector::Class("parent".into()),
            declarations: vec![Declaration {
                property: "--accent".into(),
                value: StyleValue::Literal("#123456".into()),
            }],
            container_query: None,
        },
        StyleRule {
            selector: Selector::Tag("text".into()),
            declarations: vec![Declaration {
                property: "color".into(),
                value: StyleValue::Var("--accent".into()),
            }],
            container_query: None,
        },
    ];
    let index = StyleRuleIndex::new(&rules);
    let resolver = StyleResolver::new(&theme);
    let parent_classes = vec!["parent".to_string()];
    let parent = resolver.resolve_node_style_for_module_indexed_with_parent_style(
        &rules,
        &index,
        "box",
        &parent_classes,
        None,
        StyleContext::default(),
        ElementState::default(),
        None,
        None,
    );
    let child = resolver.resolve_node_style_for_module_indexed_with_parent_style(
        &rules,
        &index,
        "text",
        &[],
        None,
        StyleContext::default(),
        ElementState::default(),
        None,
        Some(&parent),
    );

    assert_eq!(
        child.custom_properties.get("--accent"),
        Some(&StyleValue::Literal("#123456".into()))
    );
    assert_eq!(child.color, Color::from_css("#123456").unwrap());
}

#[test]
fn typed_theme_values_are_shared_across_resolvers_and_detach_after_mutation() {
    let mut theme = Theme::new("typed-values", "Typed values");
    theme.tokens_mut().extend([
        ("color.primary".into(), TokenValue::String("#123456".into())),
        ("spacing.base".into(), TokenValue::String("12px".into())),
    ]);
    let first = StyleResolver::new(&theme);
    let CachedThemeTokenValue::String(first_value) =
        first.cached_theme_token_value("--color-primary")
    else {
        panic!("expected string");
    };
    let second = StyleResolver::new(&theme);
    let CachedThemeTokenValue::String(second_value) =
        second.cached_theme_token_value("--color-primary")
    else {
        panic!("expected string");
    };
    assert_eq!(first_value.as_ptr(), second_value.as_ptr());
    assert_eq!(second_value.color, Color::from_css("#123456").unwrap());
    assert_eq!(
        second.resolve_number_with_variables(
            &StyleValue::Var("--spacing-base".into()),
            &HashMap::new()
        ),
        12.0
    );
    drop(first);
    drop(second);
    theme
        .tokens_mut()
        .insert("color.primary".into(), TokenValue::String("#abcdef".into()));
    let next = StyleResolver::new(&theme);
    assert_eq!(
        next.resolve_color_with_variables(
            &StyleValue::Var("--color-primary".into()),
            &HashMap::new()
        ),
        Color::from_css("#abcdef").unwrap()
    );
    let variables = HashMap::from([
        (
            "--color-primary".into(),
            StyleValue::Literal("#fedcba".into()),
        ),
        ("--spacing-base".into(), StyleValue::Literal("24px".into())),
    ]);
    assert_eq!(
        next.resolve_color_with_variables(&StyleValue::Var("--color-primary".into()), &variables),
        Color::from_css("#fedcba").unwrap()
    );
    assert_eq!(
        next.resolve_number_with_variables(&StyleValue::Var("--spacing-base".into()), &variables),
        24.0
    );
}

#[test]
fn shared_typed_theme_value_cache_is_bounded() {
    let theme = Theme::new("bounded-values", "Bounded values");
    let resolver = StyleResolver::new(&theme);
    for index in 0..MAX_SHARED_THEME_VALUES + 100 {
        assert!(
            resolver
                .cached_theme_token_value(&format!("--missing-{index}"))
                .is_missing()
        );
    }
    SHARED_THEME_VALUE_CACHE.with(|cache| {
        assert_eq!(
            cache.borrow_mut().get(&theme.revision()).unwrap().len(),
            MAX_SHARED_THEME_VALUES
        );
    });
    for index in 0..MAX_SHARED_THEME_REVISIONS + 5 {
        let theme = Theme::new(format!("revision-{index}"), "Revision");
        StyleResolver::new(&theme).cached_theme_token_value("--absent");
    }
    SHARED_THEME_VALUE_CACHE
        .with(|cache| assert_eq!(cache.borrow().len(), MAX_SHARED_THEME_REVISIONS));
    assert!(
        StyleResolver::new(&theme)
            .cached_theme_token_value("--absent")
            .is_missing()
    );
}
