use super::super::attrs::*;
use super::super::index::*;
use super::super::state::*;
use super::super::*;
use super::common::*;
use crate::tree::ElementState;
use mesh_core_component::style::{Declaration, Selector, StyleRule, StyleValue};
use std::collections::HashMap;

#[test]
fn state_to_rules_empty_for_unused_bit() {
    let index = StyleRuleIndex::new(&[]);
    let rules = index.rules_for_state_bit(STATE_HOVERED);
    assert!(rules.is_empty());
}

#[test]
fn state_to_rules_returns_hover_rule_for_hover_bit() {
    let rules = vec![rule_with_state("hover")];
    let index = StyleRuleIndex::new(&rules);
    let result = index.rules_for_state_bit(STATE_HOVERED);
    assert_eq!(result, &[0]);
}

#[test]
fn window_states_index_and_match_like_any_other_pseudo_state() {
    // Window states run through the same bit index as `:hover`, so a
    // fullscreen change reaches the candidate rules for a node without a
    // separate matching path.
    let rules = vec![
        rule_with_state("fullscreen"),
        rule_with_state("maximized"),
        rule_with_state("activated"),
        rule_with_state("tiled"),
        rule_with_state("windowed"),
    ];
    let index = StyleRuleIndex::new(&rules);
    assert_eq!(index.rules_for_state_bit(STATE_FULLSCREEN), &[0]);
    assert_eq!(index.rules_for_state_bit(STATE_MAXIMIZED), &[1]);
    assert_eq!(index.rules_for_state_bit(STATE_ACTIVATED), &[2]);
    assert_eq!(index.rules_for_state_bit(STATE_TILED), &[3]);
    assert_eq!(index.rules_for_state_bit(STATE_WINDOWED), &[4]);

    let fullscreen = ElementState {
        window: crate::WindowSurfaceState {
            windowed: true,
            fullscreen: true,
            activated: true,
            ..crate::WindowSurfaceState::default()
        },
        ..ElementState::default()
    };
    assert_eq!(
        active_state_mask(fullscreen),
        STATE_WINDOWED | STATE_FULLSCREEN | STATE_ACTIVATED
    );
    assert_eq!(active_state_mask(ElementState::default()), 0);
}

#[test]
fn windowed_state_is_independent_of_the_compositor_states() {
    // `:windowed` says which protocol realizes the surface; the other four
    // say what the compositor did with it. A popped-out window that is
    // merely floating matches `:windowed` and nothing else, which is what
    // lets a component draw a "dock back" control without also having to be
    // fullscreen or tiled.
    let floating_window = ElementState {
        window: crate::WindowSurfaceState {
            windowed: true,
            ..crate::WindowSurfaceState::default()
        },
        ..ElementState::default()
    };
    assert_eq!(active_state_mask(floating_window), STATE_WINDOWED);
    assert_eq!(state_name_bit("windowed"), Some(STATE_WINDOWED));

    let attrs_rules = vec![rule_with_state("windowed")];
    let index = StyleRuleIndex::new(&attrs_rules);
    assert!(index.rules_for_state_bit(STATE_FULLSCREEN).is_empty());
}

#[test]
fn state_to_rules_distinguishes_different_state_bits() {
    let rules = vec![rule_with_state("hover"), rule_with_state("focus")];
    let index = StyleRuleIndex::new(&rules);

    assert_eq!(index.rules_for_state_bit(STATE_HOVERED), &[0]);
    assert_eq!(index.rules_for_state_bit(STATE_FOCUSED), &[1]);
    assert!(index.rules_for_state_bit(STATE_ACTIVE).is_empty());
}

#[test]
fn state_to_rules_handles_compound_selector_with_state() {
    let rules = vec![rule_with_compound_state("button", "hover")];
    let index = StyleRuleIndex::new(&rules);

    assert_eq!(index.rules_for_state_bit(STATE_HOVERED), &[0]);
}

#[test]
fn node_indexed_diagnostics_match_allocating_class_path() {
    let theme = mesh_core_theme::default_theme();
    let resolver = StyleResolver::new(&theme);
    let rules = vec![StyleRule {
        selector: Selector::Class("primary".into()),
        declarations: vec![Declaration {
            property: "unsupported-proof-property".into(),
            value: StyleValue::Literal("proof".into()),
        }],
        container_query: None,
    }];
    let index = StyleRuleIndex::new(&rules);
    let context = StyleContext::default();
    let mut node = crate::tree::WidgetNode::new("button");
    node.attributes
        .insert("class".into(), "primary compact".into());

    let classes = vec!["primary".to_string(), "compact".to_string()];
    let allocating = resolver.resolve_node_style_with_diagnostics_for_module_indexed(
        &rules,
        &index,
        "button",
        &classes,
        None,
        context,
        ElementState::default(),
        None,
    );
    let cached = resolver
        .resolve_node_style_with_diagnostics_for_node_indexed(&rules, &index, &mut node, context);

    assert_eq!(cached.0.background_color, allocating.0.background_color);
    assert_eq!(cached.1, allocating.1);
    assert!(!cached.1.is_empty());
}

#[test]
fn indexed_static_diagnostic_prototypes_match_uncached_resolution() {
    let theme = mesh_core_theme::default_theme();
    let resolver = StyleResolver::new(&theme);
    let rules = vec![StyleRule {
        selector: Selector::Class("proof".into()),
        declarations: vec![
            Declaration {
                property: "border-style".into(),
                value: StyleValue::Literal("solid".into()),
            },
            Declaration {
                property: "grid-template-columns".into(),
                value: StyleValue::Literal("1fr 1fr".into()),
            },
            Declaration {
                property: "unsupported-proof-property".into(),
                value: StyleValue::Literal("proof".into()),
            },
            Declaration {
                property: "color".into(),
                value: StyleValue::Literal("token(color.primary)".into()),
            },
        ],
        container_query: None,
    }];
    let index = StyleRuleIndex::new(&rules);
    let classes = vec!["proof".to_string()];

    let uncached = resolver
        .resolve_node_style_with_diagnostics(
            &rules,
            "box",
            &classes,
            None,
            StyleContext::default(),
            ElementState::default(),
        )
        .1;
    let indexed = resolver
        .resolve_node_style_with_diagnostics_for_module_indexed(
            &rules,
            &index,
            "box",
            &classes,
            None,
            StyleContext::default(),
            ElementState::default(),
            None,
        )
        .1;

    assert_eq!(indexed, uncached);
    assert_eq!(indexed.len(), 4);
}

#[test]
fn rebuilding_rule_index_invalidates_static_diagnostic_prototypes() {
    fn diagnostics_for(property: &str) -> Vec<StyleDiagnostic> {
        let theme = mesh_core_theme::default_theme();
        let resolver = StyleResolver::new(&theme);
        let rules = vec![StyleRule {
            selector: Selector::Class("proof".into()),
            declarations: vec![Declaration {
                property: property.into(),
                value: StyleValue::Literal("proof".into()),
            }],
            container_query: None,
        }];
        let index = StyleRuleIndex::new(&rules);
        resolver
            .resolve_node_style_with_diagnostics_for_module_indexed(
                &rules,
                &index,
                "box",
                &["proof".into()],
                None,
                StyleContext::default(),
                ElementState::default(),
                None,
            )
            .1
    }

    let unsupported = diagnostics_for("unsupported-proof-property");
    let diagnostic_only = diagnostics_for("border-style");

    assert_eq!(unsupported[0].property, "unsupported-proof-property");
    assert_eq!(diagnostic_only[0].property, "border-style");
    assert_ne!(unsupported[0].message, diagnostic_only[0].message);
}

// cargo test -p mesh-core-elements --release -- cached_node_classes_beat_diagnostic_resplit --ignored --nocapture
#[test]
#[ignore = "release-only diagnostic class-token cache microbenchmark"]
fn cached_node_classes_beat_diagnostic_resplit() {
    let theme = mesh_core_theme::default_theme();
    let resolver = StyleResolver::new(&theme);
    let rules = vec![StyleRule {
        selector: Selector::Class("selected".into()),
        declarations: vec![Declaration {
            property: "background-color".into(),
            value: StyleValue::Literal("#336699".into()),
        }],
        container_query: None,
    }];
    let index = StyleRuleIndex::new(&rules);
    let context = StyleContext::default();
    let mut node = crate::tree::WidgetNode::new("button");
    node.attributes.insert(
        "class".into(),
        "button selected interactive compact elevated".into(),
    );
    node.refresh_class_tokens_cache();
    let iterations = 200_000;

    let old_started = std::time::Instant::now();
    let mut old_accumulator = 0_u32;
    for _ in 0..iterations {
        let classes = node
            .attributes
            .get("class")
            .unwrap()
            .split_whitespace()
            .map(str::to_owned)
            .collect::<Vec<_>>();
        let (style, diagnostics) = resolver.resolve_node_style_with_diagnostics_for_module_indexed(
            &rules,
            &index,
            std::hint::black_box("button"),
            std::hint::black_box(&classes),
            None,
            context,
            ElementState::default(),
            None,
        );
        old_accumulator = old_accumulator.wrapping_add(
            std::hint::black_box(style.background_color.b as u32)
                .wrapping_add(diagnostics.len() as u32),
        );
    }
    let old_time = old_started.elapsed();

    let new_started = std::time::Instant::now();
    let mut new_accumulator = 0_u32;
    for _ in 0..iterations {
        let (style, diagnostics) = resolver.resolve_node_style_with_diagnostics_for_node_indexed(
            &rules,
            &index,
            std::hint::black_box(&mut node),
            context,
        );
        new_accumulator = new_accumulator.wrapping_add(
            std::hint::black_box(style.background_color.b as u32)
                .wrapping_add(diagnostics.len() as u32),
        );
    }
    let new_time = new_started.elapsed();

    eprintln!(
        "diagnostic class tokens: resplit {old_time:?}; cached node {new_time:?}; ratio {:.2}x; accumulators={old_accumulator}/{new_accumulator}",
        old_time.as_secs_f64() / new_time.as_secs_f64()
    );
    assert_eq!(old_accumulator, new_accumulator);
    assert!(new_time < old_time);
}

#[test]
fn indexed_declarations_match_uncached_no_diagnostics_application() {
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
            property: "padding".to_string(),
            value: StyleValue::Literal("2px 4px".to_string()),
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
        selector: Selector::Tag("button".to_string()),
        declarations: declarations.clone(),
        container_query: None,
    }];
    let index = StyleRuleIndex::new(&rules);

    let mut uncached = ComputedStyle::default();
    let mut uncached_variables = HashMap::new();
    for declaration in &declarations {
        resolver.apply_declaration_no_diagnostics(
            &mut uncached,
            declaration,
            &mut uncached_variables,
        );
    }

    let mut indexed = ComputedStyle::default();
    let mut indexed_variables = HashMap::new();
    for declaration in index.no_diagnostics_declarations(0) {
        resolver.apply_indexed_declaration(&mut indexed, declaration, None, &mut indexed_variables);
    }

    assert_eq!(indexed.background_color, uncached.background_color);
    assert_eq!(indexed.padding, uncached.padding);
    assert_eq!(indexed.transitions, uncached.transitions);
    assert_eq!(
        resolver.resolve_value_with_variables(
            &StyleValue::Var("--accent".to_string()),
            &indexed_variables,
        ),
        resolver.resolve_value_with_variables(
            &StyleValue::Var("--accent".to_string()),
            &uncached_variables,
        )
    );
}

#[test]
fn indexed_diagnostics_match_uncached_diagnostics() {
    let theme = mesh_core_theme::default_theme();
    let resolver = StyleResolver::new(&theme);
    let rules = vec![
        StyleRule {
            selector: Selector::Class("panel".to_string()),
            declarations: vec![Declaration {
                property: "grid-template-columns".to_string(),
                value: StyleValue::Literal("1fr 1fr".to_string()),
            }],
            container_query: None,
        },
        StyleRule {
            selector: Selector::Tag("box".to_string()),
            declarations: vec![
                Declaration {
                    property: "color".to_string(),
                    value: StyleValue::Literal("#112233".to_string()),
                },
                Declaration {
                    property: "unknown-property".to_string(),
                    value: StyleValue::Literal("ignored".to_string()),
                },
                Declaration {
                    property: "background-color".to_string(),
                    value: StyleValue::Var("--missing-color".to_string()),
                },
            ],
            container_query: None,
        },
    ];
    let index = StyleRuleIndex::new(&rules);
    let classes = vec!["panel".to_string()];

    let (_uncached_style, uncached) = resolver.resolve_node_style_with_diagnostics_for_module(
        &rules,
        "box",
        &classes,
        None,
        StyleContext::default(),
        ElementState::default(),
        Some("@test/module"),
    );
    let (_indexed_style, indexed) = resolver
        .resolve_node_style_with_diagnostics_for_module_indexed(
            &rules,
            &index,
            "box",
            &classes,
            None,
            StyleContext::default(),
            ElementState::default(),
            Some("@test/module"),
        );

    assert_eq!(indexed, uncached);
}

#[test]
fn candidate_rule_fast_path_preserves_source_order_and_multi_bucket_deduplication() {
    let rules = vec![
        rule_with_class("primary"),
        rule_with_compound_state("button", "hover"),
        rule_with_tag("button"),
    ];
    let index = StyleRuleIndex::new(&rules);
    let classes = vec!["primary".to_string()];
    let mut state = ElementState::default();
    state.hovered = true;
    let attrs = StyleNodeAttrs::new("button", &classes, None, state);
    let mut candidates = Vec::new();

    index.for_each_candidate_rule_index(&attrs, |idx| candidates.push(idx));

    assert_eq!(candidates, vec![0, 1, 2]);
}

// cargo test -p mesh-core-elements --release -- single_bucket_candidate_fast_path_beats_sort_dedup --ignored --nocapture
#[test]
#[ignore = "release-only style candidate filtering microbenchmark"]
fn single_bucket_candidate_fast_path_beats_sort_dedup() {
    fn copy_sort_dedup(
        index: &StyleRuleIndex,
        attrs: &StyleNodeAttrs,
        ids: &mut Vec<usize>,
        mut visit: impl FnMut(usize),
    ) {
        ids.clear();
        ids.extend_from_slice(&index.fallback);
        if let Some(tag) = index.tag.get(attrs.tag) {
            ids.extend_from_slice(tag);
        }
        for class in attrs.classes.iter() {
            if let Some(class_ids) = index.class.get(class) {
                ids.extend_from_slice(class_ids);
            }
        }
        if let Some(id) = attrs.id()
            && let Some(id_ids) = index.id.get(id)
        {
            ids.extend_from_slice(id_ids);
        }
        for (state_bit, state_ids) in &index.state {
            if attrs.state_mask & *state_bit != 0 {
                ids.extend_from_slice(state_ids);
            }
        }
        ids.sort_unstable();
        ids.dedup();
        for &idx in ids.iter() {
            visit(idx);
        }
    }

    let rules = (0..64)
        .map(|idx| rule_with_class(&format!("class-{idx}")))
        .collect::<Vec<_>>();
    let index = StyleRuleIndex::new(&rules);
    let classes = vec!["class-63".to_string()];
    let attrs = StyleNodeAttrs::new("box", &classes, None, ElementState::default());
    let iterations = 2_000_000usize;

    let copied_started = std::time::Instant::now();
    let mut copied_checksum = 0usize;
    let mut copied_scratch = Vec::new();
    for _ in 0..iterations {
        copy_sort_dedup(
            std::hint::black_box(&index),
            std::hint::black_box(&attrs),
            std::hint::black_box(&mut copied_scratch),
            |idx| copied_checksum = copied_checksum.wrapping_add(idx),
        );
    }
    let copied = copied_started.elapsed();

    let direct_started = std::time::Instant::now();
    let mut direct_checksum = 0usize;
    for _ in 0..iterations {
        index.for_each_candidate_rule_index(std::hint::black_box(&attrs), |idx| {
            direct_checksum = direct_checksum.wrapping_add(idx);
        });
    }
    let direct = direct_started.elapsed();

    eprintln!(
        "single-bucket candidate filtering over {iterations} nodes: copy-sort-dedup {copied:?}, direct bucket {direct:?}, ratio {:.2}x",
        copied.as_secs_f64() / direct.as_secs_f64()
    );
    println!(
        "MESH_PERF metric=style_single_bucket_speedup value={:.6}",
        copied.as_secs_f64() / direct.as_secs_f64()
    );
    assert_eq!(copied_checksum, direct_checksum);
    assert!(direct < copied);
}
