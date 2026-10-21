use super::super::index::*;
use super::super::*;
use mesh_core_component::style::{Declaration, StyleValue};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

#[test]
fn indexed_theme_defaults_reuse_lowered_declarations_per_revision() {
    let mut defaults = mesh_core_theme::ComponentDefaults::new();
    defaults.insert("font-size".into(), "var(--spacing-md)".into());
    defaults.insert("--local-accent".into(), "#445566".into());

    let first = indexed_theme_defaults(u64::MAX - 1, &defaults);
    let second = indexed_theme_defaults(u64::MAX - 1, &defaults);
    assert!(Arc::ptr_eq(&first, &second));
    // `ComponentDefaults` preserves insertion order, so match on the declaration
    // rather than its position: the custom property stays custom and the token
    // reference stays unresolved until the node resolves it.
    let custom = first
        .iter()
        .find(|declaration| {
            matches!(&declaration.property, IndexedProperty::Custom(name) if name == "--local-accent")
        })
        .expect("custom property is indexed as custom");
    assert!(matches!(&custom.value, StyleValue::Literal(value) if value == "#445566"));
    let font_size = first
        .iter()
        .find(|declaration| {
            matches!(&declaration.property, IndexedProperty::Lowered { name, .. } if name == "font-size")
        })
        .expect("font-size is lowered");
    assert!(matches!(&font_size.value, StyleValue::Var(_)));

    let changed_revision = indexed_theme_defaults(u64::MAX, &defaults);
    assert!(!Arc::ptr_eq(&first, &changed_revision));
}

#[test]
fn bounded_style_caches_evict_one_cold_entry_without_flushing_hot_entries() {
    INLINE_STYLE_CACHE.with(|cache| cache.borrow_mut().clear());
    let hot_inline = cached_inline_style("left: 7px;");
    for index in 0..MAX_INLINE_STYLE_CACHE_ENTRIES - 1 {
        cached_inline_style(&format!("left: {}px;", index + 100));
    }
    let refreshed_inline = cached_inline_style("left: 7px;");
    cached_inline_style("left: 99999px;");
    let retained_inline = cached_inline_style("left: 7px;");
    match (hot_inline, refreshed_inline, retained_inline) {
        (
            CachedInlineStyle::Declarations(first),
            CachedInlineStyle::Declarations(refreshed),
            CachedInlineStyle::Declarations(retained),
        ) => {
            assert!(Arc::ptr_eq(&first, &refreshed));
            assert!(Arc::ptr_eq(&first, &retained));
        }
        _ => panic!("valid inline declarations must remain cached"),
    }
    INLINE_STYLE_CACHE.with(|cache| {
        let cache = cache.borrow();
        assert_eq!(cache.len(), MAX_INLINE_STYLE_CACHE_ENTRIES);
        assert!(!cache.contains_key("left: 100px;"));
    });

    SHARED_THEME_DEFAULT_CACHE.with(|cache| cache.borrow_mut().clear());
    let props = HashMap::new();
    let hot_defaults = Rc::new(ThemeComponentDefaults::default());
    remember_shared_theme_defaults(1, 0, &props, "hot", None, &hot_defaults);
    for index in 0..MAX_SHARED_THEME_DEFAULTS_PER_REVISION - 1 {
        remember_shared_theme_defaults(
            1,
            0,
            &props,
            &format!("cold-{index}"),
            None,
            &Rc::new(ThemeComponentDefaults::default()),
        );
    }
    assert!(Rc::ptr_eq(
        &shared_theme_defaults(1, 0, &props, "hot", None).unwrap(),
        &hot_defaults
    ));
    remember_shared_theme_defaults(
        1,
        0,
        &props,
        "new",
        None,
        &Rc::new(ThemeComponentDefaults::default()),
    );
    assert!(Rc::ptr_eq(
        &shared_theme_defaults(1, 0, &props, "hot", None).unwrap(),
        &hot_defaults
    ));
    assert!(shared_theme_defaults(1, 0, &props, "cold-0", None).is_none());

    for revision in 2..=MAX_SHARED_THEME_REVISIONS as u64 {
        remember_shared_theme_defaults(
            revision,
            0,
            &props,
            "revision",
            None,
            &Rc::new(ThemeComponentDefaults::default()),
        );
    }
    let _ = shared_theme_defaults(1, 0, &props, "hot", None);
    remember_shared_theme_defaults(
        MAX_SHARED_THEME_REVISIONS as u64 + 1,
        0,
        &props,
        "revision",
        None,
        &Rc::new(ThemeComponentDefaults::default()),
    );
    SHARED_THEME_DEFAULT_CACHE.with(|cache| {
        let cache = cache.borrow();
        assert_eq!(cache.len(), MAX_SHARED_THEME_REVISIONS);
        assert!(cache.contains_key(&1));
        assert!(!cache.contains_key(&2));
    });

    THEME_DEFAULT_DECLARATION_CACHE.with(|cache| cache.borrow_mut().clear());
    let hot_declarations = Box::new(mesh_core_theme::ComponentDefaults::new());
    let hot_lowered = indexed_theme_defaults(10_001, &hot_declarations);
    let cold_declarations = (0..MAX_THEME_DEFAULT_DECLARATION_CACHE_ENTRIES - 1)
        .map(|_| Box::new(mesh_core_theme::ComponentDefaults::new()))
        .collect::<Vec<_>>();
    for declarations in &cold_declarations {
        indexed_theme_defaults(10_001, declarations);
    }
    let refreshed_lowered = indexed_theme_defaults(10_001, &hot_declarations);
    assert!(Arc::ptr_eq(&hot_lowered, &refreshed_lowered));
    let newcomer = Box::new(mesh_core_theme::ComponentDefaults::new());
    indexed_theme_defaults(10_001, &newcomer);
    let retained_lowered = indexed_theme_defaults(10_001, &hot_declarations);
    assert!(Arc::ptr_eq(&hot_lowered, &retained_lowered));
    THEME_DEFAULT_DECLARATION_CACHE.with(|cache| {
        assert_eq!(
            cache.borrow().len(),
            MAX_THEME_DEFAULT_DECLARATION_CACHE_ENTRIES
        );
    });
}

#[test]
fn typed_static_declarations_match_string_lowering() {
    let theme = mesh_core_theme::default_theme();
    let resolver = StyleResolver::new(&theme);
    let declarations = [
        ("background-color", "#112233"),
        ("color", "rgba(255, 255, 255, 0.75)"),
        ("font-size", "15px"),
        ("font-weight", "600"),
        ("line-height", "1.4"),
        ("padding", "4px 8px 12px 16px"),
        ("margin", "3px 6px"),
        ("border-width", "2px 1px"),
        ("border-radius", "2px 4px 6px 8px"),
        ("width", "75%"),
        ("height", "fit-content"),
        ("flex-basis", "24px"),
    ];
    let mut old_style = ComputedStyle::default();
    let mut typed_style = ComputedStyle::default();
    let mut old_variables = HashMap::new();
    let mut typed_variables = HashMap::new();

    for (property, value) in declarations {
        let declaration = Declaration {
            property: property.into(),
            value: StyleValue::Literal(value.into()),
        };
        resolver.apply_declaration_no_diagnostics(&mut old_style, &declaration, &mut old_variables);
        let indexed = IndexedDeclaration::from_declaration(&declaration);
        assert!(indexed.literal.is_static(), "{property} should lower once");
        resolver.apply_indexed_declaration(&mut typed_style, &indexed, None, &mut typed_variables);
    }

    assert_eq!(typed_style, old_style);
    assert_eq!(typed_variables, old_variables);
}

#[test]
fn references_style_function_matches_the_contains_pair_it_replaced() {
    for value in [
        "",
        "(",
        "var",
        "prop",
        "var(",
        "prop(",
        "var(--x)",
        "prop(size)",
        "8px",
        "rgba(0, 0, 0, 0.5)",
        "calc(var(--gap) * 2)",
        "linear-gradient(prop(a), var(b))",
        "avar(",
        "aprop(",
        "novar",
        "wrap(",
        "p(",
        "ar(",
        "(var",
        "translate(4px) var(--y)",
        "☃var(--snow)",
    ] {
        assert_eq!(
            references_style_function(value),
            value.contains("var(") || value.contains("prop("),
            "mismatch for {value:?}"
        );
    }
}
