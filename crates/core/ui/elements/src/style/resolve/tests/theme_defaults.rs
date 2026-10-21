use super::super::*;
use crate::tree::NodeState;
use mesh_core_component::style::{Declaration, Selector, StyleRule, StyleValue, prop_variable_key};
use std::collections::HashMap;
use std::rc::Rc;

// cargo test -p mesh-core-elements --release -- shared_theme_defaults_beat_hashed_deep_clone --ignored --nocapture
#[test]
#[ignore = "release-only theme-default lookup microbenchmark"]
fn shared_theme_defaults_beat_hashed_deep_clone() {
    use std::time::Instant;

    const MODULE: &str = "@mesh/benchmark";
    // The tag mix a surface actually walks: a few containers repeated far
    // more often than the leaves between them.
    const TAGS: &[&str] = &[
        "row", "text", "row", "text", "column", "text", "button", "row", "icon", "text",
    ];
    const ITERATIONS: usize = 400_000;

    let mut theme = mesh_core_theme::default_theme();
    for (index, tag) in ["row", "column", "box", "button", "text", "icon"]
        .into_iter()
        .enumerate()
    {
        theme.defaults_mut().components.insert(
            tag.into(),
            [
                ("font-size".into(), format!("{}px", 10 + index)),
                ("padding".into(), format!("{}px", index)),
            ]
            .into_iter()
            .collect(),
        );
    }
    let resolver = StyleResolver::new(&theme);

    // The previous shape: defaults stored by value behind two hashed
    // lookups, deep-cloned for every node.
    let mut owned: HashMap<String, HashMap<String, (ComputedStyle, HashMap<String, StyleValue>)>> =
        HashMap::new();
    for tag in TAGS {
        let shared = resolver.cached_theme_component_defaults_no_diagnostics(tag, Some(MODULE));
        owned.entry(MODULE.to_string()).or_default().insert(
            (*tag).to_string(),
            (shared.style.clone(), shared.variables.clone()),
        );
    }

    // Parity: both paths must hand the caller the same starting style.
    for tag in TAGS {
        let hashed = &owned[MODULE][*tag].0;
        let shared = resolver.cached_theme_component_defaults_no_diagnostics(tag, Some(MODULE));
        assert_eq!(hashed.font_size, shared.style.font_size);
        assert_eq!(hashed.padding, shared.style.padding);
    }

    let hashed_started = Instant::now();
    let mut hashed_total = 0f32;
    for index in 0..ITERATIONS {
        let tag = std::hint::black_box(TAGS[index % TAGS.len()]);
        let (style, variables) = owned
            .get(MODULE)
            .and_then(|tags| tags.get(tag))
            .cloned()
            .expect("seeded above");
        hashed_total += style.font_size + variables.len() as f32;
    }
    let hashed = hashed_started.elapsed();

    let shared_started = Instant::now();
    let mut shared_total = 0f32;
    for index in 0..ITERATIONS {
        let tag = std::hint::black_box(TAGS[index % TAGS.len()]);
        let defaults = resolver.cached_theme_component_defaults_no_diagnostics(tag, Some(MODULE));
        let style = defaults.style.clone();
        shared_total += style.font_size + defaults.variables.len() as f32;
    }
    let shared = shared_started.elapsed();

    assert_eq!(hashed_total, shared_total);
    eprintln!(
        "theme defaults over {ITERATIONS} node resolutions: hashed lookup + deep clone {hashed:?}, front cache + shared defaults {shared:?}, ratio {:.2}x",
        hashed.as_secs_f64() / shared.as_secs_f64()
    );
    println!(
        "MESH_PERF metric=shared_theme_defaults_speedup value={:.6}",
        hashed.as_secs_f64() / shared.as_secs_f64()
    );
}

/// The comparison-keyed front cache sits ahead of the hashed maps, so it
/// must key on the *pair* and survive eviction. A wrong hit here silently
/// paints one element's defaults onto another.
#[test]
fn recent_theme_defaults_key_on_tag_and_module_together() {
    let mut theme = mesh_core_theme::default_theme();
    theme.defaults_mut().components.insert(
        "card".into(),
        [("font-size".into(), "11px".into())].into_iter().collect(),
    );
    theme.defaults_mut().components.insert(
        "panel".into(),
        [("font-size".into(), "22px".into())].into_iter().collect(),
    );
    let mut module = mesh_core_theme::ThemeModule::default();
    module.defaults.components.insert(
        "card".into(),
        [("font-size".into(), "33px".into())].into_iter().collect(),
    );
    theme.modules_mut().insert("@mesh/demo".into(), module);

    let resolver = StyleResolver::new(&theme);
    let font_size = |tag: &str, module_id: Option<&str>| {
        resolver
            .cached_theme_component_defaults_no_diagnostics(tag, module_id)
            .style
            .font_size
    };

    assert_eq!(font_size("card", None), 11.0);
    assert_eq!(font_size("panel", None), 22.0);
    assert_eq!(
        font_size("card", Some("@mesh/demo")),
        33.0,
        "a module override must not be served for the module-less key"
    );

    // Re-request in a different order: every answer must still be its own.
    assert_eq!(font_size("panel", None), 22.0);
    assert_eq!(font_size("card", Some("@mesh/demo")), 33.0);
    assert_eq!(font_size("card", None), 11.0);
    assert_eq!(font_size("panel", Some("@mesh/demo")), 22.0);
}

#[test]
fn recent_theme_defaults_stay_correct_past_eviction() {
    let mut theme = mesh_core_theme::default_theme();
    let tags: Vec<String> = (0..THEME_DEFAULT_RECENT_CAPACITY * 3)
        .map(|index| format!("probe-{index}"))
        .collect();
    for (index, tag) in tags.iter().enumerate() {
        theme.defaults_mut().components.insert(
            tag.clone(),
            [("font-size".into(), format!("{}px", index + 8))]
                .into_iter()
                .collect(),
        );
    }

    let resolver = StyleResolver::new(&theme);
    let expected: Vec<f32> = tags
        .iter()
        .map(|tag| {
            resolver
                .cached_theme_component_defaults_no_diagnostics(tag, None)
                .style
                .font_size
        })
        .collect();

    assert!(
        resolver.theme_default_recent.borrow().len() <= THEME_DEFAULT_RECENT_CAPACITY,
        "the front cache must stay bounded"
    );
    for (index, tag) in tags.iter().enumerate() {
        assert_eq!(
            resolver
                .cached_theme_component_defaults_no_diagnostics(tag, None)
                .style
                .font_size,
            expected[index],
            "{tag} resolved differently after eviction"
        );
        assert_eq!(expected[index], (index + 8) as f32);
    }
}

/// Sharing the defaults by `Rc` must not let one node's resolution leak
/// into the next: every node still starts from an untouched copy.
#[test]
fn shared_theme_defaults_are_not_mutated_by_node_resolution() {
    let mut theme = mesh_core_theme::default_theme();
    theme.defaults_mut().components.insert(
        "card".into(),
        [("font-size".into(), "13px".into())].into_iter().collect(),
    );
    let resolver = StyleResolver::new(&theme);
    let rules = vec![StyleRule {
        selector: Selector::Class("big".into()),
        declarations: vec![Declaration {
            property: "font-size".into(),
            value: StyleValue::Literal("31px".into()),
        }],
        container_query: None,
    }];

    let plain = resolver.resolve_node_style(
        &rules,
        "card",
        &[],
        None,
        Default::default(),
        NodeState::default(),
    );
    let big = resolver.resolve_node_style(
        &rules,
        "card",
        &["big".to_string()],
        None,
        Default::default(),
        NodeState::default(),
    );
    let plain_again = resolver.resolve_node_style(
        &rules,
        "card",
        &[],
        None,
        Default::default(),
        NodeState::default(),
    );

    assert_eq!(plain.font_size, 13.0);
    assert_eq!(big.font_size, 31.0);
    assert_eq!(
        plain_again.font_size, 13.0,
        "the shared defaults were mutated by an earlier node"
    );
}

#[test]
fn prop_less_theme_defaults_are_shared_across_resolver_instances() {
    let mut theme = mesh_core_theme::Theme::new("shared-defaults", "Shared defaults");
    theme.defaults_mut().components.insert(
        "card".into(),
        [("font-size".into(), "17px".into())].into_iter().collect(),
    );

    let first_resolver = StyleResolver::new(&theme);
    let first = first_resolver.cached_theme_component_defaults_no_diagnostics("card", None);
    let second_resolver = StyleResolver::new(&theme);
    let second = second_resolver.cached_theme_component_defaults_no_diagnostics("card", None);

    assert_eq!(second.style.font_size, 17.0);
    assert!(
        Rc::ptr_eq(&first, &second),
        "a stable theme revision should reuse its lowered defaults"
    );
}

#[test]
fn mutating_theme_style_data_invalidates_shared_defaults() {
    let mut theme = mesh_core_theme::Theme::new("mutable-defaults", "Mutable defaults");
    theme.defaults_mut().components.insert(
        "card".into(),
        [("font-size".into(), "11px".into())].into_iter().collect(),
    );
    let first =
        StyleResolver::new(&theme).cached_theme_component_defaults_no_diagnostics("card", None);

    theme.defaults_mut().components.insert(
        "card".into(),
        [("font-size".into(), "23px".into())].into_iter().collect(),
    );
    let second =
        StyleResolver::new(&theme).cached_theme_component_defaults_no_diagnostics("card", None);

    assert_eq!(first.style.font_size, 11.0);
    assert_eq!(second.style.font_size, 23.0);
    assert!(!Rc::ptr_eq(&first, &second));
}

#[test]
fn prop_bearing_resolvers_share_only_matching_values() {
    let mut theme = mesh_core_theme::Theme::new("prop-defaults", "Prop defaults");
    theme.defaults_mut().components.insert(
        "card".into(),
        [("font-size".into(), "prop(size)".into())]
            .into_iter()
            .collect(),
    );
    let resolver_with = |size: &str| {
        StyleResolver::new(&theme).with_props(HashMap::from([(
            prop_variable_key("size"),
            StyleValue::Literal(size.into()),
        )]))
    };

    let small = resolver_with("12px").cached_theme_component_defaults_no_diagnostics("card", None);
    let small_again =
        resolver_with("12px").cached_theme_component_defaults_no_diagnostics("card", None);
    let large = resolver_with("28px").cached_theme_component_defaults_no_diagnostics("card", None);

    assert_eq!(small.style.font_size, 12.0);
    assert_eq!(large.style.font_size, 28.0);
    assert!(Rc::ptr_eq(&small, &small_again));
    assert!(!Rc::ptr_eq(&small, &large));
}

#[test]
fn borrowed_props_match_owned_resolver_semantics() {
    let theme = mesh_core_theme::default_theme();
    let prop_name = prop_variable_key("panel_width");
    let props = HashMap::from([(prop_name.clone(), StyleValue::Literal("320px".to_string()))]);
    let value = StyleValue::Prop("panel_width".to_string());

    let owned = StyleResolver::new(&theme).with_props(props.clone());
    let borrowed = StyleResolver::new(&theme).with_borrowed_props(&props);

    assert_eq!(borrowed.props_fingerprint, owned.props_fingerprint);
    assert_eq!(borrowed.resolve_value(&value), owned.resolve_value(&value));
    assert!(matches!(borrowed.props, Cow::Borrowed(_)));
    assert!(matches!(owned.props, Cow::Owned(_)));
}

#[test]
fn cached_diagnostic_theme_defaults_match_replayed_defaults() {
    let mut theme = mesh_core_theme::default_theme();
    theme.defaults_mut().components.insert(
        "benchmark-card".into(),
        [
            ("background-color".into(), "#112233".into()),
            ("color".into(), "#ffffff".into()),
            ("font-size".into(), "13px".into()),
            ("grid-template-columns".into(), "1fr 1fr".into()),
            ("--local-accent".into(), "#445566".into()),
        ]
        .into_iter()
        .collect(),
    );
    let resolver = StyleResolver::new(&theme);

    let mut replayed_style = ComputedStyle::default();
    let mut replayed_diagnostics = Vec::new();
    let mut replayed_variables = HashMap::new();
    resolver.apply_theme_component_defaults(
        &mut replayed_style,
        "benchmark-card",
        None,
        Some(&mut replayed_diagnostics),
        &mut replayed_variables,
    );

    let (cached_style, cached_variables, cached_diagnostics) =
        resolver.cached_theme_component_defaults_with_diagnostics("benchmark-card", None);

    assert_eq!(
        cached_style.background_color,
        replayed_style.background_color
    );
    assert_eq!(cached_style.color, replayed_style.color);
    assert!((cached_style.font_size - replayed_style.font_size).abs() < f32::EPSILON);
    assert_eq!(cached_variables.len(), replayed_variables.len());
    assert!(matches!(
        cached_variables.get("--local-accent"),
        Some(StyleValue::Literal(value)) if value == "#445566"
    ));
    assert_eq!(cached_diagnostics, replayed_diagnostics);
}
