use super::super::index::*;
use super::super::*;
use mesh_core_component::style::{Declaration, StyleValue};
use mesh_core_theme::Theme;
use std::collections::HashMap;

#[test]
fn indexed_literal_operations_preserve_string_resolution_semantics() {
    let theme = Theme::new("literal-parity", "Literal parity");
    let resolver = StyleResolver::new(&theme);
    for property in lowered_css_properties() {
        for raw in [
            "0",
            "12.5px",
            "4 8 12 16",
            "40%",
            "auto",
            "fit",
            "inherit",
            "#123456",
            "italic",
            "ellipsis",
            "nowrap",
            "wrap-reverse",
            "column",
            "center",
            "space-between",
            "baseline",
            "none",
            "hidden",
            "collapse",
            "fixed",
            "multiply",
            "left top",
            "16 / 9",
            "rtl",
            "ltr",
            "scroll hidden",
            "2px -4px",
            "flex-end",
            "end",
            "start",
            "flex-start",
            "space-around",
            "right",
            "wrap",
            "row",
            "column-reverse",
            "stretch",
            "visible",
            "auto hidden",
            "1",
            "rgba(255, 0, 0, 0.5)",
            "#fff #000",
        ] {
            let value = StyleValue::Literal(raw.into());
            let declaration = Declaration {
                property: (*property).into(),
                value: value.clone(),
            };
            let indexed = IndexedDeclaration::from_declaration(&declaration);
            if !indexed.literal.is_static() {
                continue;
            }
            let mut control = ComputedStyle::default();
            resolver.apply_property_value_no_diagnostics(
                &mut control,
                property,
                &value,
                &mut HashMap::new(),
            );
            let mut typed = ComputedStyle::default();
            resolver.apply_indexed_declaration(&mut typed, &indexed, None, &mut HashMap::new());
            assert_eq!(typed, control, "property={property}, value={raw}");
        }
    }
}

#[test]
fn inherited_text_literals_do_not_mark_properties_explicit() {
    let theme = Theme::new("inherit", "Inherit");
    let resolver = StyleResolver::new(&theme);
    let mut style = ComputedStyle::default();
    for property in ["color", "font-size", "font-weight", "line-height"] {
        let declaration = Declaration {
            property: property.into(),
            value: StyleValue::Literal("inherit".into()),
        };
        let indexed = IndexedDeclaration::from_declaration(&declaration);
        assert!(!indexed.literal.is_static());
        resolver.apply_indexed_declaration(&mut style, &indexed, None, &mut HashMap::new());
    }
    assert_eq!(style, ComputedStyle::default());
    assert_eq!(style.explicit_properties, StylePropertyMask::default());
}

#[test]
fn indexed_theme_operations_respect_overrides_and_theme_revisions() {
    let mut theme = Theme::new("theme-operations", "Theme operations");
    theme.tokens_mut().insert(
        "mesh.prop.size".into(),
        mesh_core_theme::TokenValue::Number(12.0),
    );
    let declaration = Declaration {
        property: "font-size".into(),
        value: StyleValue::Var("--mesh-prop-size".into()),
    };
    let indexed = IndexedDeclaration::from_declaration(&declaration);
    let mut style = ComputedStyle::default();
    let resolver = StyleResolver::new(&theme);
    resolver.apply_indexed_declaration(&mut style, &indexed, None, &mut HashMap::new());
    assert_eq!(style.font_size, 12.0);
    assert!(style.explicit_properties.font_size);
    assert!(
        matches!(indexed.literal.theme_operation(), Some((revision, _)) if revision == theme.revision())
    );
    let mut local = HashMap::from([(
        "--mesh-prop-size".into(),
        StyleValue::Literal("24px".into()),
    )]);
    resolver.apply_indexed_declaration(&mut style, &indexed, None, &mut local);
    assert_eq!(style.font_size, 24.0);
    let props = StyleResolver::new(&theme).with_props(HashMap::from([(
        "--mesh-prop-size".into(),
        StyleValue::Literal("36px".into()),
    )]));
    props.apply_indexed_declaration(&mut style, &indexed, None, &mut HashMap::new());
    assert_eq!(style.font_size, 36.0);
    resolver.apply_indexed_declaration(&mut style, &indexed, None, &mut HashMap::new());
    assert_eq!(style.font_size, 12.0);
    theme.tokens_mut().insert(
        "mesh.prop.size".into(),
        mesh_core_theme::TokenValue::Number(48.0),
    );
    StyleResolver::new(&theme).apply_indexed_declaration(
        &mut style,
        &indexed,
        None,
        &mut HashMap::new(),
    );
    assert_eq!(style.font_size, 48.0);
    assert!(
        matches!(indexed.literal.theme_operation(), Some((revision, _)) if revision == theme.revision())
    );
}

#[test]
fn indexed_theme_operations_keep_diagnostics_and_numeric_cast_semantics() {
    let mut theme = Theme::new("theme-parity", "Theme parity");
    let halfway = 1.000_000_059_604_644_8;
    theme.tokens_mut().extend([
        (
            "precise".into(),
            mesh_core_theme::TokenValue::Number(halfway),
        ),
        (
            "cycle.a".into(),
            mesh_core_theme::TokenValue::String("var(--cycle-b)".into()),
        ),
        (
            "cycle.b".into(),
            mesh_core_theme::TokenValue::String("var(--cycle-a)".into()),
        ),
        (
            "edges".into(),
            mesh_core_theme::TokenValue::String("4px 8px".into()),
        ),
    ]);
    let resolver = StyleResolver::new(&theme);
    for (property, reference) in [
        ("font-size", "--precise"),
        ("font-weight", "--precise"),
        ("font-size", "--missing"),
        ("font-size", "--missing, 4px"),
        ("font-size", "--cycle-a"),
        ("padding", "--edges"),
    ] {
        let declaration = Declaration {
            property: property.into(),
            value: StyleValue::Var(reference.into()),
        };
        let indexed = IndexedDeclaration::from_declaration(&declaration);
        let mut control = ComputedStyle::default();
        let diagnostics = resolver.apply_declarations_with_diagnostics(
            &mut control,
            std::slice::from_ref(&declaration),
            Some(".test"),
        );
        for _ in 0..2 {
            let mut typed = ComputedStyle::default();
            let mut actual_diagnostics = Vec::new();
            resolver.apply_indexed_declaration(
                &mut typed,
                &indexed,
                Some((".test", &mut actual_diagnostics)),
                &mut HashMap::new(),
            );
            assert_eq!(typed, control, "{property}: {reference}");
            assert_eq!(actual_diagnostics, diagnostics, "{property}: {reference}");
        }
    }
}

#[test]
fn shared_indexes_keep_theme_assignments_separate_from_node_overrides() {
    use mesh_core_component::style::{Selector, StyleRule};
    use std::sync::Arc;
    let mut theme = Theme::new("shared-index", "Shared index");
    theme.tokens_mut().insert(
        "spacing.base".into(),
        mesh_core_theme::TokenValue::Number(12.0),
    );
    let rules = [StyleRule {
        selector: Selector::Universal,
        declarations: vec![Declaration {
            property: "font-size".into(),
            value: StyleValue::Var("--spacing-base".into()),
        }],
        container_query: None,
    }];
    let index = Arc::new(StyleRuleIndex::new(&rules));
    std::thread::scope(|scope| {
        for override_value in [None, Some(24.0)] {
            let theme = &theme;
            let index = &index;
            scope.spawn(move || {
                let resolver = StyleResolver::new(theme);
                let declaration = &index.no_diagnostics_declarations(0)[0];
                let mut variables = HashMap::new();
                if let Some(value) = override_value {
                    variables.insert(
                        "--spacing-base".into(),
                        StyleValue::Literal(format!("{value}px")),
                    );
                }
                let mut style = ComputedStyle::default();
                for _ in 0..500 {
                    resolver.apply_indexed_declaration(
                        &mut style,
                        declaration,
                        None,
                        &mut variables,
                    );
                    assert_eq!(style.font_size, override_value.unwrap_or(12.0));
                }
            });
        }
    });
}

// cargo test -p mesh-core-elements --release --lib -- typed_keyword_declaration_profile --ignored --nocapture
#[test]
#[ignore = "release-only typed keyword declaration profile"]
fn typed_keyword_declaration_profile() {
    let theme = Theme::new("keyword-profile", "Keyword profile");
    let resolver = StyleResolver::new(&theme);
    let declarations = [
        ("flex-shrink", "0"),
        ("flex-grow", "1"),
        ("align-items", "center"),
        ("justify-content", "space-between"),
        ("text-align", "center"),
        ("border-color", "#334455"),
        ("padding-inline", "12px"),
        ("padding-block", "8px"),
        ("flex-direction", "column"),
        ("overflow-y", "auto"),
        ("display", "flex"),
    ]
    .map(|(property, value)| Declaration {
        property: property.into(),
        value: StyleValue::Literal(value.into()),
    });
    let indexed = declarations
        .iter()
        .map(IndexedDeclaration::from_declaration)
        .collect::<Vec<_>>();
    assert!(indexed.iter().all(|decl| decl.literal.is_static()));
    let rounds = 100_000;
    let mut variables = HashMap::new();
    let mut style = ComputedStyle::default();
    let started = std::time::Instant::now();
    for _ in 0..rounds {
        for declaration in &declarations {
            resolver.apply_property_value_no_diagnostics(
                &mut style,
                &declaration.property,
                &declaration.value,
                &mut variables,
            );
        }
    }
    let string_time = started.elapsed();
    std::hint::black_box(&style);
    let mut typed = ComputedStyle::default();
    let started = std::time::Instant::now();
    for _ in 0..rounds {
        for declaration in &indexed {
            resolver.apply_indexed_declaration(&mut typed, declaration, None, &mut variables);
        }
    }
    let typed_time = started.elapsed();
    std::hint::black_box(&typed);
    let speedup = string_time.as_secs_f64() / typed_time.as_secs_f64();
    eprintln!(
        "{} keyword declarations: string path {string_time:?}; typed {typed_time:?}; ratio {speedup:.2}x",
        rounds * declarations.len()
    );
    println!("MESH_PERF metric=typed_keyword_declaration_speedup value={speedup:.6}");
    assert!(speedup >= 1.2, "typed keyword declarations should be at least 1.2x");
}
