use super::super::attrs::*;
use super::super::cache::*;
use super::super::declaration::*;
use super::super::index::*;
use super::super::matching::*;
use super::super::*;
use crate::tree::ElementState;
use mesh_core_component::style::{Declaration, Selector, StyleRule, StyleValue, prop_variable_key};
use mesh_core_theme::Theme;
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
        ElementState::default(),
    );
    let big = resolver.resolve_node_style(
        &rules,
        "card",
        &["big".to_string()],
        None,
        Default::default(),
        ElementState::default(),
    );
    let plain_again = resolver.resolve_node_style(
        &rules,
        "card",
        &[],
        None,
        Default::default(),
        ElementState::default(),
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

// cargo test -p mesh-core-elements --release -- borrowed_style_props_beat_per_resolver_clone --ignored --nocapture
#[test]
#[ignore = "release-only borrowed style-prop resolver benchmark"]
fn borrowed_style_props_beat_per_resolver_clone() {
    fn run(
        theme: &Theme,
        props: &HashMap<String, StyleValue>,
        value: &StyleValue,
        borrow: bool,
        iterations: usize,
    ) -> std::time::Duration {
        let started = std::time::Instant::now();
        let mut checksum = 0usize;
        for _ in 0..iterations {
            let resolver = if borrow {
                StyleResolver::new(std::hint::black_box(theme))
                    .with_borrowed_props(std::hint::black_box(props))
            } else {
                StyleResolver::new(std::hint::black_box(theme))
                    .with_props(std::hint::black_box(props.clone()))
            };
            checksum = checksum.wrapping_add(std::hint::black_box(
                resolver.resolve_value(std::hint::black_box(value)).len(),
            ));
        }
        assert_eq!(checksum, iterations * 4);
        started.elapsed()
    }

    const ITERATIONS: usize = 50_000;
    let theme = mesh_core_theme::default_theme();
    let props = (0..32)
        .map(|index| {
            (
                prop_variable_key(&format!("setting_{index}")),
                StyleValue::Literal(format!("{index}px")),
            )
        })
        .collect::<HashMap<_, _>>();
    let value = StyleValue::Prop("setting_31".to_string());
    let mut owned_samples = Vec::new();
    let mut borrowed_samples = Vec::new();
    for sample in 0..5 {
        let (owned, borrowed) = if sample % 2 == 0 {
            (
                run(&theme, &props, &value, false, ITERATIONS),
                run(&theme, &props, &value, true, ITERATIONS),
            )
        } else {
            let borrowed = run(&theme, &props, &value, true, ITERATIONS);
            let owned = run(&theme, &props, &value, false, ITERATIONS);
            (owned, borrowed)
        };
        owned_samples.push(owned);
        borrowed_samples.push(borrowed);
    }
    owned_samples.sort_unstable();
    borrowed_samples.sort_unstable();
    let owned_median = owned_samples[owned_samples.len() / 2];
    let borrowed_median = borrowed_samples[borrowed_samples.len() / 2];
    let speedup = owned_median.as_secs_f64() / borrowed_median.as_secs_f64();

    eprintln!(
        "style resolver setup + prop lookup over {ITERATIONS} targeted restyles with 32 props: owned clone {:?}..{:?} (median {owned_median:?}); borrowed {:?}..{:?} (median {borrowed_median:?}); ratio {speedup:.2}x",
        owned_samples[0],
        owned_samples[owned_samples.len() - 1],
        borrowed_samples[0],
        borrowed_samples[borrowed_samples.len() - 1],
    );
    eprintln!("MESH_PERF metric=borrowed_style_props_speedup value={speedup:.3}");
    assert!(
        borrowed_median * 2 < owned_median,
        "borrowing stable style props should at least halve resolver setup and lookup time"
    );
}

// cargo test -p mesh-core-elements --release -- shared_theme_revision_cache_beats_per_resolver_lowering --ignored --nocapture
#[test]
#[ignore = "release-only cross-resolver theme-default benchmark"]
fn shared_theme_revision_cache_beats_per_resolver_lowering() {
    use std::time::Instant;

    const ITERATIONS: usize = 100_000;
    let mut theme =
        mesh_core_theme::Theme::new("revision-cache-benchmark", "Revision cache benchmark");
    theme.defaults_mut().components.insert(
        "base".into(),
        [
            ("color".into(), "#f4f4f4".into()),
            ("font-family".into(), "Inter".into()),
            ("font-size".into(), "13px".into()),
            ("line-height".into(), "1.4".into()),
        ]
        .into_iter()
        .collect(),
    );
    theme.defaults_mut().components.insert(
        "card".into(),
        [
            ("background".into(), "#202124".into()),
            ("border-color".into(), "#45474d".into()),
            ("border-radius".into(), "9px".into()),
            ("border-width".into(), "1px".into()),
            ("padding".into(), "8px 12px".into()),
            ("background-color".into(), "prop(blur_background)".into()),
        ]
        .into_iter()
        .collect(),
    );
    let props = HashMap::from([(
        prop_variable_key("blur_background"),
        StyleValue::Literal("#202124".into()),
    )]);

    let uncached_started = Instant::now();
    let mut uncached_checksum = 0f32;
    for _ in 0..ITERATIONS {
        let resolver = StyleResolver::new(std::hint::black_box(&theme))
            .with_props(std::hint::black_box(props.clone()));
        let mut style = ComputedStyle::default();
        let mut variables = HashMap::new();
        resolver.apply_theme_component_defaults(&mut style, "card", None, None, &mut variables);
        uncached_checksum += std::hint::black_box(
            style.font_size
                + style.border_width.left
                + style.padding.left
                + style.background_color.r as f32,
        );
    }
    let uncached = uncached_started.elapsed();

    let _ = StyleResolver::new(&theme)
        .with_props(props.clone())
        .cached_theme_component_defaults_no_diagnostics("card", None);
    let cached_started = Instant::now();
    let mut cached_checksum = 0f32;
    for _ in 0..ITERATIONS {
        let resolver = StyleResolver::new(std::hint::black_box(&theme))
            .with_props(std::hint::black_box(props.clone()));
        let defaults = resolver.cached_theme_component_defaults_no_diagnostics("card", None);
        let style = defaults.style.clone();
        cached_checksum += std::hint::black_box(
            style.font_size
                + style.border_width.left
                + style.padding.left
                + style.background_color.r as f32,
        );
    }
    let cached = cached_started.elapsed();

    assert_eq!(uncached_checksum, cached_checksum);
    eprintln!(
        "theme defaults across {ITERATIONS} fresh resolvers: per-resolver lowering {uncached:?}, revision cache {cached:?}, ratio {:.2}x",
        uncached.as_secs_f64() / cached.as_secs_f64()
    );
    println!(
        "MESH_PERF metric=shared_theme_revision_cache_speedup value={:.6}",
        uncached.as_secs_f64() / cached.as_secs_f64()
    );
    assert!(cached < uncached);
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

// cargo test -p mesh-core-elements --release -- indexed_diagnostic_declarations_skip_static_reclassification --ignored --nocapture
#[test]
#[ignore = "release-only indexed diagnostic declaration microbenchmark"]
fn indexed_diagnostic_declarations_skip_static_reclassification() {
    let theme = mesh_core_theme::default_theme();
    let resolver = StyleResolver::new(&theme);
    let declarations = vec![
        Declaration {
            property: "--accent".to_string(),
            value: StyleValue::Literal("#112233".to_string()),
        },
        Declaration {
            property: "background-color".to_string(),
            value: StyleValue::Var("--accent".to_string()),
        },
        Declaration {
            property: "grid-template-columns".to_string(),
            value: StyleValue::Literal("1fr 1fr".to_string()),
        },
        Declaration {
            property: "unknown-property".to_string(),
            value: StyleValue::Literal("ignored".to_string()),
        },
        Declaration {
            property: "transition-duration".to_string(),
            value: StyleValue::Var("--animation-missing-duration".to_string()),
        },
    ];
    let rules = vec![StyleRule {
        selector: Selector::Class("panel".to_string()),
        declarations,
        container_query: None,
    }];
    let index = StyleRuleIndex::new(&rules);
    let classes = vec!["panel".to_string()];
    let attrs = StyleNodeAttrs::new("box", &classes, None, ElementState::default());
    let iterations = 200_000;

    let old_started = std::time::Instant::now();
    let mut old_count = 0usize;
    for _ in 0..iterations {
        let mut style = ComputedStyle::default();
        let mut diagnostics = Vec::new();
        let mut variables = HashMap::new();
        index.for_each_candidate_rule(&rules, &attrs, |rule| {
            if rule_matches_attrs(rule, &attrs, StyleContext::default()) {
                for decl in &rule.declarations {
                    resolver.apply_declaration_with_diagnostics(
                        std::hint::black_box(&mut style),
                        decl,
                        Some(selector_to_diagnostic_string(&rule.selector)),
                        &mut diagnostics,
                        &mut variables,
                    );
                }
            }
        });
        old_count = old_count.wrapping_add(std::hint::black_box(diagnostics.len()));
    }
    let old_time = old_started.elapsed();

    let new_started = std::time::Instant::now();
    let mut new_count = 0usize;
    for _ in 0..iterations {
        let mut style = ComputedStyle::default();
        let mut diagnostics = Vec::new();
        let mut variables = HashMap::new();
        index.for_each_candidate_rule_index(&attrs, |rule_idx| {
            let rule = &rules[rule_idx];
            if rule_matches_attrs(rule, &attrs, StyleContext::default()) {
                let selector = selector_to_diagnostic_string(&rule.selector);
                for decl in index.no_diagnostics_declarations(rule_idx) {
                    resolver.apply_indexed_declaration(
                        std::hint::black_box(&mut style),
                        decl,
                        Some((&selector, &mut diagnostics)),
                        &mut variables,
                    );
                }
            }
        });
        new_count = new_count.wrapping_add(std::hint::black_box(diagnostics.len()));
    }
    let new_time = new_started.elapsed();

    eprintln!(
        "diagnostic declaration apply: static reclassification {old_time:?}; indexed metadata {new_time:?}; ratio {:.1}x; counts={old_count}/{new_count}",
        old_time.as_secs_f64() / new_time.as_secs_f64()
    );
    assert_eq!(old_count, new_count);
    assert!(new_time < old_time);
}

// cargo test -p mesh-core-elements --release -- indexed_selector_diagnostics_skip_per_node_formatting --ignored --nocapture
#[test]
#[ignore = "release-only indexed selector diagnostic string microbenchmark"]
fn indexed_selector_diagnostics_skip_per_node_formatting() {
    let selector = Selector::Compound(vec![
        Selector::Tag("button".to_string()),
        Selector::Class("primary".to_string()),
        Selector::State("button".to_string(), "hover".to_string()),
    ]);
    let rules = vec![StyleRule {
        selector,
        declarations: vec![Declaration {
            property: "grid-template-columns".to_string(),
            value: StyleValue::Literal("1fr 1fr".to_string()),
        }],
        container_query: None,
    }];
    let index = StyleRuleIndex::new(&rules);
    let iterations = 500_000;

    let old_started = std::time::Instant::now();
    let mut old_total = 0usize;
    for _ in 0..iterations {
        old_total = old_total.wrapping_add(
            selector_to_diagnostic_string(std::hint::black_box(&rules[0].selector)).len(),
        );
    }
    let old_time = old_started.elapsed();

    let new_started = std::time::Instant::now();
    let mut new_total = 0usize;
    for _ in 0..iterations {
        new_total =
            new_total.wrapping_add(index.selector_diagnostic(std::hint::black_box(0)).len());
    }
    let new_time = new_started.elapsed();

    eprintln!(
        "selector diagnostics: per-node formatting {old_time:?}; indexed string {new_time:?}; ratio {:.1}x; totals={old_total}/{new_total}",
        old_time.as_secs_f64() / new_time.as_secs_f64()
    );
    assert_eq!(old_total, new_total);
    assert!(new_time < old_time);
}

// cargo test -p mesh-core-elements --release -- indexed_diagnostics_beat_per_node_index_rebuild --ignored --nocapture
#[test]
#[ignore = "release-only indexed diagnostics microbenchmark"]
fn indexed_diagnostics_beat_per_node_index_rebuild() {
    let theme = mesh_core_theme::default_theme();
    let resolver = StyleResolver::new(&theme);
    let mut rules = Vec::new();
    for index in 0..80 {
        rules.push(StyleRule {
            selector: Selector::Class(format!("panel-{index}")),
            declarations: vec![
                Declaration {
                    property: "color".to_string(),
                    value: StyleValue::Literal("#112233".to_string()),
                },
                Declaration {
                    property: "grid-template-columns".to_string(),
                    value: StyleValue::Literal("1fr 1fr".to_string()),
                },
            ],
            container_query: None,
        });
    }
    let index = StyleRuleIndex::new(&rules);
    let classes = vec!["panel-79".to_string()];
    let iterations = 20_000usize;

    let old_started = std::time::Instant::now();
    let mut old_count = 0usize;
    for _ in 0..iterations {
        let (_style, diagnostics) = resolver.resolve_node_style_with_diagnostics_for_module(
            std::hint::black_box(&rules),
            "box",
            std::hint::black_box(&classes),
            None,
            StyleContext::default(),
            ElementState::default(),
            Some("@test/module"),
        );
        old_count = old_count.wrapping_add(diagnostics.len());
    }
    let old_time = old_started.elapsed();

    let new_started = std::time::Instant::now();
    let mut new_count = 0usize;
    for _ in 0..iterations {
        let (_style, diagnostics) = resolver
            .resolve_node_style_with_diagnostics_for_module_indexed(
                std::hint::black_box(&rules),
                std::hint::black_box(&index),
                "box",
                std::hint::black_box(&classes),
                None,
                StyleContext::default(),
                ElementState::default(),
                Some("@test/module"),
            );
        new_count = new_count.wrapping_add(diagnostics.len());
    }
    let new_time = new_started.elapsed();

    eprintln!(
        "style diagnostics: per-node index rebuild {old_time:?}; cached index {new_time:?}; ratio {:.1}x; counts={old_count}/{new_count}",
        old_time.as_secs_f64() / new_time.as_secs_f64()
    );
    assert_eq!(old_count, new_count);
    assert!(new_time < old_time);
}
