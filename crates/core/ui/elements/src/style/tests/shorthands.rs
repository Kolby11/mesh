use super::super::parse::parse_transition_properties;
use super::super::*;
use super::common::*;
use crate::tree::ElementState;
use mesh_core_component::{
    parser::parse_component,
    style::{Declaration, Selector, StyleRule, StyleValue, prop_variable_key},
};

#[test]
fn padding_inline_and_block_tokens_resolve_to_computed_edges() {
    use mesh_core_component::parser::parse_component;

    let source = r#"
<style>
.panel {
padding-inline: var(--spacing-lg);
padding-block: var(--spacing-sm);
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
    assert_eq!(&*style.font_family, "Inter");
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
fn css_variable_resolves_theme_variable_before_computed_style() {
    let theme = mesh_core_theme::default_theme();
    let resolver = StyleResolver::new(&theme);
    let rules = vec![StyleRule {
        selector: Selector::Class("panel".to_string()),
        declarations: vec![
            mesh_core_component::style::Declaration {
                property: "--surface".to_string(),
                value: StyleValue::Var("--color-primary".to_string()),
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
fn css_variable_local_value_wins_over_theme_variable() {
    let theme = mesh_core_theme::default_theme();
    let resolver = StyleResolver::new(&theme);
    let rules = vec![StyleRule {
        selector: Selector::Class("panel".to_string()),
        declarations: vec![
            mesh_core_component::style::Declaration {
                property: "--color-primary".to_string(),
                value: StyleValue::Literal("#ffffff".to_string()),
            },
            mesh_core_component::style::Declaration {
                property: "background".to_string(),
                value: StyleValue::Var("--color-primary".to_string()),
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
