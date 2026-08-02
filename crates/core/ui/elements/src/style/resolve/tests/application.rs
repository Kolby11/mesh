use super::super::attrs::*;
use super::super::cache::*;
use super::super::declaration::*;
use super::super::index::*;
use super::super::matching::*;
use super::super::state::*;
use super::super::*;
use super::common::*;
use crate::lru::LruCache;
use crate::style::parse::*;
use crate::style::*;
use crate::tree::ElementState;
use mesh_core_component::style::{Declaration, Selector, StyleRule, StyleValue, prop_variable_key};
use mesh_core_theme::{Theme, TokenValue};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

// cargo test -p mesh-core-elements --release -- theme_default_direct_apply_beats_declaration_allocation --ignored --nocapture
#[test]
#[ignore = "release-only theme default application microbenchmark"]
fn theme_default_direct_apply_beats_declaration_allocation() {
    fn old_apply_theme_defaults_map_no_diagnostics(
        resolver: &StyleResolver<'_>,
        style: &mut ComputedStyle,
        defaults: &mesh_core_theme::ComponentDefaults,
        variables: &mut HashMap<String, StyleValue>,
    ) {
        for (property, value) in defaults {
            let declaration = Declaration {
                property: property.clone(),
                value: classify_theme_style_value(value),
            };
            resolver.apply_declaration_no_diagnostics(style, &declaration, variables);
        }
    }

    let mut defaults = mesh_core_theme::ComponentDefaults::new();
    defaults.insert("background-color".into(), "#112233".into());
    defaults.insert("color".into(), "#ffffff".into());
    defaults.insert("font-size".into(), "13px".into());
    defaults.insert("padding".into(), "4px 8px".into());
    defaults.insert("border-radius".into(), "6px".into());
    defaults.insert("gap".into(), "5px".into());
    defaults.insert("opacity".into(), "0.875".into());
    defaults.insert("--local-accent".into(), "#445566".into());

    let theme = mesh_core_theme::default_theme();
    let resolver = StyleResolver::new(&theme);
    let iterations = 200_000;

    let old_started = std::time::Instant::now();
    let mut old_accumulator = 0u32;
    for _ in 0..iterations {
        let mut style = ComputedStyle::default();
        let mut variables = HashMap::new();
        old_apply_theme_defaults_map_no_diagnostics(
            &resolver,
            std::hint::black_box(&mut style),
            &defaults,
            &mut variables,
        );
        old_accumulator =
            old_accumulator.wrapping_add(std::hint::black_box(style.background_color.r as u32));
    }
    let old_time = old_started.elapsed();

    let new_started = std::time::Instant::now();
    let mut new_accumulator = 0u32;
    for _ in 0..iterations {
        let mut style = ComputedStyle::default();
        let mut variables = HashMap::new();
        let mut diagnostics = None;
        resolver.apply_theme_defaults_map(
            std::hint::black_box(&mut style),
            "benchmark-card",
            &defaults,
            &mut diagnostics,
            &mut variables,
        );
        new_accumulator =
            new_accumulator.wrapping_add(std::hint::black_box(style.background_color.r as u32));
    }
    let new_time = new_started.elapsed();

    eprintln!(
        "theme defaults apply: declaration allocation {old_time:?}; direct property apply {new_time:?}; ratio {:.1}x; accumulators={old_accumulator}/{new_accumulator}",
        old_time.as_secs_f64() / new_time.as_secs_f64()
    );
    assert_eq!(old_accumulator, new_accumulator);
    assert!(new_time < old_time);
}

// cargo test -p mesh-core-elements --release -- cached_theme_default_prototype_beats_reapplying_string_map --ignored --nocapture
#[test]
#[ignore = "release-only theme default prototype microbenchmark"]
fn cached_theme_default_prototype_beats_reapplying_string_map() {
    let mut theme = mesh_core_theme::default_theme();
    theme.defaults_mut().components.insert(
        "benchmark-card".into(),
        [
            ("background-color".into(), "#112233".into()),
            ("color".into(), "#ffffff".into()),
            ("font-size".into(), "13px".into()),
            ("padding".into(), "4px 8px".into()),
            ("border-radius".into(), "6px".into()),
            ("gap".into(), "5px".into()),
            ("opacity".into(), "0.875".into()),
            ("--local-accent".into(), "#445566".into()),
        ]
        .into_iter()
        .collect(),
    );
    let resolver = StyleResolver::new(&theme);
    let attrs = StyleNodeAttrs {
        tag: "benchmark-card",
        ..StyleNodeAttrs::default()
    };
    let rules = Vec::new();
    let index = StyleRuleIndex::new(&rules);
    let context = StyleContext::default();
    let iterations = 200_000;

    let uncached_started = std::time::Instant::now();
    let mut uncached_accumulator = 0u32;
    for _ in 0..iterations {
        let mut style = ComputedStyle::default();
        let mut variables = HashMap::new();
        resolver.apply_theme_component_defaults(
            std::hint::black_box(&mut style),
            "benchmark-card",
            None,
            None,
            &mut variables,
        );
        uncached_accumulator = uncached_accumulator
            .wrapping_add(std::hint::black_box(style.background_color.r as u32));
    }
    let uncached_time = uncached_started.elapsed();

    // Populate the prototype outside the timed cache-hit loop.
    let _ = resolver
        .resolve_node_style_with_attrs_indexed_no_diagnostics(&rules, &index, &attrs, context);
    let cached_started = std::time::Instant::now();
    let mut cached_accumulator = 0u32;
    for _ in 0..iterations {
        let style = resolver.resolve_node_style_with_attrs_indexed_no_diagnostics(
            std::hint::black_box(&rules),
            std::hint::black_box(&index),
            std::hint::black_box(&attrs),
            context,
        );
        cached_accumulator =
            cached_accumulator.wrapping_add(std::hint::black_box(style.background_color.r as u32));
    }
    let cached_time = cached_started.elapsed();

    eprintln!(
        "theme default prototype: reapply strings {uncached_time:?}; cached {cached_time:?}; ratio {:.1}x; accumulators={uncached_accumulator}/{cached_accumulator}",
        uncached_time.as_secs_f64() / cached_time.as_secs_f64()
    );
    assert_eq!(uncached_accumulator, cached_accumulator);
    assert!(cached_time < uncached_time);
}

// cargo test -p mesh-core-elements --release -- cached_diagnostic_theme_default_prototype_beats_reapplying_string_map --ignored --nocapture
#[test]
#[ignore = "release-only diagnostic theme default prototype microbenchmark"]
fn cached_diagnostic_theme_default_prototype_beats_reapplying_string_map() {
    fn old_resolve_diagnostic_theme_defaults(
        resolver: &StyleResolver<'_>,
        tag: &str,
    ) -> (ComputedStyle, Vec<StyleDiagnostic>) {
        let mut style = ComputedStyle::default();
        let mut diagnostics = Vec::new();
        let mut variables = HashMap::new();
        resolver.apply_theme_component_defaults(
            &mut style,
            tag,
            None,
            Some(&mut diagnostics),
            &mut variables,
        );
        (style, diagnostics)
    }

    let mut theme = mesh_core_theme::default_theme();
    theme.defaults_mut().components.insert(
        "benchmark-card".into(),
        [
            ("background-color".into(), "#112233".into()),
            ("color".into(), "#ffffff".into()),
            ("font-size".into(), "13px".into()),
            ("padding".into(), "4px 8px".into()),
            ("border-radius".into(), "6px".into()),
            ("gap".into(), "5px".into()),
            ("opacity".into(), "0.875".into()),
            ("grid-template-columns".into(), "1fr 1fr".into()),
            ("--local-accent".into(), "#445566".into()),
        ]
        .into_iter()
        .collect(),
    );
    let resolver = StyleResolver::new(&theme);
    let attrs = StyleNodeAttrs {
        tag: "benchmark-card",
        ..StyleNodeAttrs::default()
    };
    let rules = Vec::new();
    let index = StyleRuleIndex::new(&rules);
    let context = StyleContext::default();
    let iterations = 200_000;

    let uncached_started = std::time::Instant::now();
    let mut uncached_accumulator = 0u32;
    for _ in 0..iterations {
        let (style, diagnostics) =
            old_resolve_diagnostic_theme_defaults(&resolver, "benchmark-card");
        uncached_accumulator = uncached_accumulator.wrapping_add(std::hint::black_box(
            style.background_color.r as u32 + diagnostics.len() as u32,
        ));
    }
    let uncached_time = uncached_started.elapsed();

    let _ = resolver.resolve_node_style_with_attrs_indexed(&rules, &index, &attrs, context);
    let cached_started = std::time::Instant::now();
    let mut cached_accumulator = 0u32;
    for _ in 0..iterations {
        let (style, diagnostics) = resolver.resolve_node_style_with_attrs_indexed(
            std::hint::black_box(&rules),
            std::hint::black_box(&index),
            std::hint::black_box(&attrs),
            context,
        );
        cached_accumulator = cached_accumulator.wrapping_add(std::hint::black_box(
            style.background_color.r as u32 + diagnostics.len() as u32,
        ));
    }
    let cached_time = cached_started.elapsed();

    eprintln!(
        "diagnostic theme default prototype: reapply strings {uncached_time:?}; cached {cached_time:?}; ratio {:.1}x; accumulators={uncached_accumulator}/{cached_accumulator}",
        uncached_time.as_secs_f64() / cached_time.as_secs_f64()
    );
    assert_eq!(uncached_accumulator, cached_accumulator);
    assert!(cached_time < uncached_time);
}

// cargo test -p mesh-core-elements --release -- layered_prop_lookup_beats_per_node_prop_cloning --ignored --nocapture
#[test]
#[ignore = "release-only layered prop lookup microbenchmark"]
fn layered_prop_lookup_beats_per_node_prop_cloning() {
    let theme = mesh_core_theme::default_theme();
    let props = (0..32)
        .map(|index| {
            (
                prop_variable_key(&format!("prop_{index}")),
                StyleValue::Literal(format!("{index}px")),
            )
        })
        .collect::<HashMap<_, _>>();
    let resolver = StyleResolver::new(&theme).with_props(props.clone());
    let attrs = StyleNodeAttrs {
        tag: "box",
        ..StyleNodeAttrs::default()
    };
    let rules = Vec::new();
    let index = StyleRuleIndex::new(&rules);
    let context = StyleContext::default();
    let _ = resolver
        .resolve_node_style_with_attrs_indexed_no_diagnostics(&rules, &index, &attrs, context);
    let iterations = 200_000usize;

    let cloned_started = std::time::Instant::now();
    let mut cloned_total = 0usize;
    for _ in 0..iterations {
        let mut variables = HashMap::new();
        for (key, value) in &props {
            variables.insert(key.clone(), value.clone());
        }
        cloned_total = cloned_total.wrapping_add(std::hint::black_box(variables.len()));
        let style = resolver
            .resolve_node_style_with_attrs_indexed_no_diagnostics(&rules, &index, &attrs, context);
        cloned_total = cloned_total.wrapping_add(std::hint::black_box(style.opacity as usize));
    }
    let cloned_time = cloned_started.elapsed();

    let layered_started = std::time::Instant::now();
    let mut layered_total = 0usize;
    for _ in 0..iterations {
        let style = resolver.resolve_node_style_with_attrs_indexed_no_diagnostics(
            std::hint::black_box(&rules),
            std::hint::black_box(&index),
            std::hint::black_box(&attrs),
            context,
        );
        layered_total = layered_total.wrapping_add(std::hint::black_box(style.opacity as usize));
    }
    let layered_time = layered_started.elapsed();

    eprintln!(
        "per-node prop seed: cloned {cloned_time:?}; layered {layered_time:?}; ratio {:.1}x; totals={cloned_total}/{layered_total}",
        cloned_time.as_secs_f64() / layered_time.as_secs_f64()
    );
    assert!(cloned_total > layered_total);
    assert!(layered_time < cloned_time);
}

// cargo test -p mesh-core-elements --release -- cached_theme_reference_beats_recanonicalizing --ignored --nocapture
#[test]
#[ignore = "release-only theme reference canonicalization microbenchmark"]
fn cached_theme_reference_beats_recanonicalizing() {
    let theme = mesh_core_theme::default_theme();
    let resolver = StyleResolver::new(&theme);
    let reference = "--color-primary";
    let iterations = 1_000_000usize;

    let canonicalized_started = std::time::Instant::now();
    let mut canonicalized_total = 0usize;
    for _ in 0..iterations {
        let name = theme_reference_to_token_name(std::hint::black_box(reference));
        canonicalized_total = canonicalized_total.wrapping_add(name.len());
    }
    let canonicalized_time = canonicalized_started.elapsed();

    let _ = resolver.cached_theme_token_name(reference);
    let cached_started = std::time::Instant::now();
    let mut cached_total = 0usize;
    for _ in 0..iterations {
        let name = resolver.cached_theme_token_name(std::hint::black_box(reference));
        cached_total = cached_total.wrapping_add(name.len());
    }
    let cached_time = cached_started.elapsed();

    eprintln!(
        "theme reference mapping: canonicalized {canonicalized_time:?}; cached {cached_time:?}; ratio {:.1}x; totals={canonicalized_total}/{cached_total}",
        canonicalized_time.as_secs_f64() / cached_time.as_secs_f64()
    );
    assert_eq!(canonicalized_total, cached_total);
    assert!(cached_time < canonicalized_time);
}

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
        | CachedThemeTokenValue::Missing => panic!("expected string color token"),
    }
    assert!(
        resolver
            .cached_theme_token_value("--definitely-missing")
            .is_missing()
    );
}

// cargo test -p mesh-core-elements --release -- cached_theme_token_value_beats_cached_name_theme_lookup --ignored --nocapture
#[test]
#[ignore = "release-only theme token value lookup microbenchmark"]
fn cached_theme_token_value_beats_cached_name_theme_lookup() {
    let theme = mesh_core_theme::default_theme();
    let resolver = StyleResolver::new(&theme);
    let reference = "--color-primary";
    let iterations = 1_000_000usize;

    let _ = resolver.cached_theme_token_name(reference);
    let old_started = std::time::Instant::now();
    let mut old_total = 0usize;
    for _ in 0..iterations {
        let name = resolver.cached_theme_token_name(std::hint::black_box(reference));
        if let Some(TokenValue::String(value)) = theme.token(&name) {
            old_total = old_total.wrapping_add(std::hint::black_box(value.len()));
        }
    }
    let old_time = old_started.elapsed();

    let _ = resolver.cached_theme_token_value(reference);
    let new_started = std::time::Instant::now();
    let mut new_total = 0usize;
    for _ in 0..iterations {
        if let CachedThemeTokenValue::String(value) =
            resolver.cached_theme_token_value(std::hint::black_box(reference))
        {
            new_total = new_total.wrapping_add(std::hint::black_box(value.len()));
        }
    }
    let new_time = new_started.elapsed();

    eprintln!(
        "theme token value lookup: cached-name+theme {old_time:?}; cached-value {new_time:?}; ratio {:.1}x; totals={old_total}/{new_total}",
        old_time.as_secs_f64() / new_time.as_secs_f64()
    );
    assert_eq!(old_total, new_total);
    assert!(new_time < old_time);
}

// cargo test -p mesh-core-elements --release -- cached_embedded_theme_references_beat_recanonicalizing --ignored --nocapture
#[test]
#[ignore = "release-only embedded theme reference microbenchmark"]
fn cached_embedded_theme_references_beat_recanonicalizing() {
    fn old_resolve_embedded_references(value: &str, theme: &Theme) -> String {
        let mut output = String::new();
        let mut rest = value;
        loop {
            let Some(start) = rest.find("var(") else {
                break;
            };
            output.push_str(&rest[..start]);
            let reference_start = start + "var(".len();
            let Some(end) = rest[reference_start..].find(')') else {
                output.push_str(&rest[start..]);
                return output;
            };
            let name =
                theme_reference_to_token_name(rest[reference_start..reference_start + end].trim());
            if let Some(TokenValue::String(value)) = theme.token(&name) {
                output.push_str(value);
            }
            rest = &rest[reference_start + end + 1..];
        }
        output.push_str(rest);
        output
    }

    let theme = mesh_core_theme::default_theme();
    let resolver = StyleResolver::new(&theme);
    let value =
        "linear-gradient(var(--color-primary), var(--color-secondary), var(--color-primary))";
    let variables = HashMap::new();
    let iterations = 300_000usize;

    let old_started = std::time::Instant::now();
    let mut old_total = 0usize;
    for _ in 0..iterations {
        let resolved = old_resolve_embedded_references(
            std::hint::black_box(value),
            std::hint::black_box(&theme),
        );
        old_total = old_total.wrapping_add(std::hint::black_box(resolved.len()));
    }
    let old_time = old_started.elapsed();

    let _ = resolver.resolve_embedded_references_cached(value, &variables, false);
    let new_started = std::time::Instant::now();
    let mut new_total = 0usize;
    for _ in 0..iterations {
        let resolved = resolver
            .resolve_embedded_references_cached(
                std::hint::black_box(value),
                std::hint::black_box(&variables),
                false,
            )
            .expect("embedded references should resolve");
        new_total = new_total.wrapping_add(std::hint::black_box(resolved.len()));
    }
    let new_time = new_started.elapsed();

    eprintln!(
        "embedded theme references: recanonicalized {old_time:?}; cached {new_time:?}; ratio {:.1}x; totals={old_total}/{new_total}",
        old_time.as_secs_f64() / new_time.as_secs_f64()
    );
    assert_eq!(old_total, new_total);
    assert!(new_time < old_time);
}

// cargo test -p mesh-core-elements --release -- indexed_declaration_application_beats_uncached_validation --ignored --nocapture
#[test]
#[ignore = "release-only indexed declaration application microbenchmark"]
fn indexed_declaration_application_beats_uncached_validation() {
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
            property: "color".to_string(),
            value: StyleValue::Literal("#ffffff".to_string()),
        },
        Declaration {
            property: "font-size".to_string(),
            value: StyleValue::Literal("13px".to_string()),
        },
        Declaration {
            property: "padding".to_string(),
            value: StyleValue::Literal("4px 8px".to_string()),
        },
        Declaration {
            property: "border-radius".to_string(),
            value: StyleValue::Literal("6px".to_string()),
        },
        Declaration {
            property: "gap".to_string(),
            value: StyleValue::Literal("5px".to_string()),
        },
        Declaration {
            property: "opacity".to_string(),
            value: StyleValue::Literal("0.875".to_string()),
        },
        Declaration {
            property: "unknown-property".to_string(),
            value: StyleValue::Literal("ignored".to_string()),
        },
    ];
    let rules = vec![StyleRule {
        selector: Selector::Tag("button".to_string()),
        declarations: declarations.clone(),
        container_query: None,
    }];
    let index = StyleRuleIndex::new(&rules);
    let indexed_declarations = index.no_diagnostics_declarations(0);
    let iterations = 200_000;

    let old_started = std::time::Instant::now();
    let mut old_accumulator = 0u32;
    for _ in 0..iterations {
        let mut style = ComputedStyle::default();
        let mut variables = HashMap::new();
        for declaration in &declarations {
            resolver.apply_declaration_no_diagnostics(
                std::hint::black_box(&mut style),
                declaration,
                &mut variables,
            );
        }
        old_accumulator =
            old_accumulator.wrapping_add(std::hint::black_box(style.background_color.r as u32));
    }
    let old_time = old_started.elapsed();

    let new_started = std::time::Instant::now();
    let mut new_accumulator = 0u32;
    for _ in 0..iterations {
        let mut style = ComputedStyle::default();
        let mut variables = HashMap::new();
        for declaration in indexed_declarations {
            resolver.apply_indexed_declaration(
                std::hint::black_box(&mut style),
                declaration,
                None,
                &mut variables,
            );
        }
        new_accumulator =
            new_accumulator.wrapping_add(std::hint::black_box(style.background_color.r as u32));
    }
    let new_time = new_started.elapsed();

    eprintln!(
        "declaration apply: uncached validation {old_time:?}; indexed metadata {new_time:?}; ratio {:.1}x; accumulators={old_accumulator}/{new_accumulator}",
        old_time.as_secs_f64() / new_time.as_secs_f64()
    );
    assert_eq!(old_accumulator, new_accumulator);
    assert!(new_time < old_time);
}

// cargo test -p mesh-core-elements --release -- cached_module_theme_variables_beat_reformatting --ignored --nocapture
#[test]
#[ignore = "release-only module theme variable cache microbenchmark"]
fn cached_module_theme_variables_beat_reformatting() {
    let mut theme = mesh_core_theme::default_theme();
    let module = theme.modules_mut().entry("benchmark".into()).or_default();
    for index in 0..32 {
        module.tokens.insert(
            format!("palette.group{index}.accent"),
            TokenValue::String(format!("#{index:06x}")),
        );
    }
    let resolver = StyleResolver::new(&theme);
    let mut warm = HashMap::new();
    resolver.seed_module_theme_variables("benchmark", &mut warm);
    let iterations = 100_000;

    let old_started = std::time::Instant::now();
    let mut old_total = 0_usize;
    let mut old_variables = HashMap::new();
    for _ in 0..iterations {
        old_variables.clear();
        for (name, value) in &theme.modules()["benchmark"].tokens {
            old_variables.insert(
                format!("--{}", name.replace('.', "-")),
                StyleValue::Literal(match value {
                    TokenValue::String(value) => value.clone(),
                    TokenValue::Number(value) => format!("{value}"),
                    TokenValue::Bool(value) => format!("{value}"),
                }),
            );
        }
        old_total = old_total.saturating_add(std::hint::black_box(old_variables.len()));
    }
    let old_time = old_started.elapsed();

    let new_started = std::time::Instant::now();
    let mut new_total = 0_usize;
    let mut new_variables = HashMap::new();
    for _ in 0..iterations {
        new_variables.clear();
        resolver.seed_module_theme_variables("benchmark", &mut new_variables);
        new_total = new_total.saturating_add(std::hint::black_box(new_variables.len()));
    }
    let new_time = new_started.elapsed();

    eprintln!(
        "module theme variables: reformat {old_time:?}; cached {new_time:?}; ratio {:.1}x; totals={old_total}/{new_total}",
        old_time.as_secs_f64() / new_time.as_secs_f64()
    );
    assert_eq!(old_total, new_total);
    assert!(new_time < old_time);
}

// cargo test -p mesh-core-elements --release -- inline_style_resolution_benchmark --ignored --nocapture
#[test]
#[ignore = "release-only inline-style resolution benchmark"]
fn inline_style_resolution_benchmark() {
    let theme = mesh_core_theme::default_theme();
    let resolver = StyleResolver::new(&theme);
    let rules = Vec::new();
    let index = StyleRuleIndex::new(&rules);
    let classes = Vec::new();
    let inline_style = "left: 38px; top: 32px; width: 36px; height: 36px; background: transparent;";
    let iterations = 50_000_u32;

    let started = std::time::Instant::now();
    let mut checksum = 0.0_f32;
    for _ in 0..iterations {
        let style = resolver.resolve_node_style_for_module_indexed_with_inline_style(
            &rules,
            &index,
            "button",
            &classes,
            None,
            Some(std::hint::black_box(inline_style)),
            StyleContext::default(),
            ElementState::default(),
            Some("@benchmark/inline-style"),
        );
        checksum += std::hint::black_box(style.inset_left.unwrap_or_default());
    }
    let elapsed = started.elapsed();
    let ns_per_resolution = elapsed.as_nanos() as f64 / f64::from(iterations);

    eprintln!(
        "MESH_PERF metric=inline_style_resolution_ns_per_node value={ns_per_resolution:.3} total={elapsed:?} checksum={checksum}"
    );
    assert_eq!(checksum, 38.0 * iterations as f32);
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
