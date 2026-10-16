use super::super::index::*;
use super::super::literal::*;
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
        ] {
            let value = StyleValue::Literal(raw.into());
            let declaration = Declaration {
                property: (*property).into(),
                value: value.clone(),
            };
            let indexed = IndexedDeclaration::from_declaration(&declaration);
            if indexed.literal.get().is_none() {
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
        assert!(indexed.literal.get().is_none());
        resolver.apply_indexed_declaration(&mut style, &indexed, None, &mut HashMap::new());
    }
    assert_eq!(style, ComputedStyle::default());
    assert_eq!(style.explicit_properties, StylePropertyMask::default());
}

#[test]
#[ignore = "release performance gate"]
fn typed_literal_operations_beat_property_name_dispatch() {
    use std::hint::black_box;
    use std::time::Instant;
    fn string_dispatch(style: &mut ComputedStyle, property: &str, value: f32) {
        match property {
            "font-size" => style.font_size = value,
            "line-height" => style.line_height = value,
            "gap" => style.gap = value,
            "opacity" => style.opacity = value,
            "padding-top" => style.padding.top = value,
            "margin-right" => style.margin.right = value,
            "border-left-width" => style.border_width.left = value,
            "letter-spacing" => style.letter_spacing = value,
            _ => unreachable!(),
        }
    }
    let properties = [
        "font-size",
        "line-height",
        "gap",
        "opacity",
        "padding-top",
        "margin-right",
        "border-left-width",
        "letter-spacing",
    ];
    let operations = properties.map(|property| {
        typed_literal_value(property, &StyleValue::Literal("12.5px".into())).unwrap()
    });
    let iterations = 200_000;
    let mut control = ComputedStyle::default();
    let start = Instant::now();
    for _ in 0..iterations {
        for property in &properties {
            string_dispatch(
                black_box(&mut control),
                black_box(property),
                black_box(12.5),
            );
        }
    }
    let strings = start.elapsed();
    let mut typed = ComputedStyle::default();
    let start = Instant::now();
    for _ in 0..iterations {
        for operation in &operations {
            apply_typed_literal(black_box(&mut typed), black_box(*operation));
        }
    }
    let operations_time = start.elapsed();
    assert_eq!(typed, control);
    let speedup = strings.as_secs_f64() / operations_time.as_secs_f64();
    println!(
        "literal dispatch: 8 numeric operations x {iterations}: names={strings:?} operations={operations_time:?}"
    );
    println!("MESH_PERF metric=typed_literal_operation_dispatch_speedup value={speedup:.3}");
    assert!(
        speedup >= 1.05,
        "typed operation speedup {speedup:.3} below 1.05x"
    );
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
        matches!(indexed.literal.get(), Some(CachedLiteral::Theme { revision, .. }) if revision == theme.revision())
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
        matches!(indexed.literal.get(), Some(CachedLiteral::Theme { revision, .. }) if revision == theme.revision())
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
#[ignore = "release performance gate"]
fn cached_theme_operations_beat_relowering() {
    use std::hint::black_box;
    use std::time::Instant;
    let mut theme = Theme::new("operation-workload", "Operation workload");
    theme.tokens_mut().extend([
        (
            "color.primary".into(),
            mesh_core_theme::TokenValue::String("rgba(42, 88, 144, 0.75)".into()),
        ),
        (
            "spacing.box".into(),
            mesh_core_theme::TokenValue::String("4px 8px 12px 16px".into()),
        ),
        (
            "radius.box".into(),
            mesh_core_theme::TokenValue::String("2px 4px 6px 8px".into()),
        ),
        (
            "size.box".into(),
            mesh_core_theme::TokenValue::String("75%".into()),
        ),
    ]);
    let declarations = [
        ("background-color", "--color-primary"),
        ("padding", "--spacing-box"),
        ("border-radius", "--radius-box"),
        ("width", "--size-box"),
    ]
    .map(|(property, reference)| {
        IndexedDeclaration::from_declaration(&Declaration {
            property: property.into(),
            value: StyleValue::Var(reference.into()),
        })
    });
    let resolver = StyleResolver::new(&theme);
    let iterations = 50_000;
    let mut variables = HashMap::new();
    let mut control = ComputedStyle::default();
    let start = Instant::now();
    for _ in 0..iterations {
        for declaration in &declarations {
            declaration.literal.clear_theme();
            resolver.apply_indexed_declaration(
                black_box(&mut control),
                black_box(declaration),
                None,
                &mut variables,
            );
        }
    }
    let lowering = start.elapsed();
    let mut cached = ComputedStyle::default();
    let start = Instant::now();
    for _ in 0..iterations {
        for declaration in &declarations {
            resolver.apply_indexed_declaration(
                black_box(&mut cached),
                black_box(declaration),
                None,
                &mut variables,
            );
        }
    }
    let reuse = start.elapsed();
    assert_eq!(cached, control);
    let speedup = lowering.as_secs_f64() / reuse.as_secs_f64();
    println!(
        "theme operations: 4 declarations x {iterations}: re-lowering={lowering:?} cached={reuse:?}"
    );
    println!("MESH_PERF metric=cached_theme_operation_speedup value={speedup:.3}");
    assert!(
        speedup >= 1.2,
        "theme operation speedup {speedup:.3} below 1.2x"
    );
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
