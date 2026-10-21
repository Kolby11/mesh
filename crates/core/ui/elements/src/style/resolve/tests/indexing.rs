use super::super::attrs::*;
use super::super::index::*;
use super::super::matching::selector_matches_attrs;
use super::super::state::*;
use super::super::*;
use super::common::*;
use crate::tree::NodeState;
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

    let fullscreen = crate::WindowSurfaceState {
        windowed: true,
        fullscreen: true,
        activated: true,
        ..crate::WindowSurfaceState::default()
    }
    .flags();
    assert_eq!(
        active_state_mask(fullscreen),
        STATE_WINDOWED | STATE_FULLSCREEN | STATE_ACTIVATED
    );
    assert_eq!(active_state_mask(NodeState::default()), 0);
}

#[test]
fn windowed_state_is_independent_of_the_compositor_states() {
    // `:windowed` says which protocol realizes the surface; the other four
    // say what the compositor did with it. A popped-out window that is
    // merely floating matches `:windowed` and nothing else, which is what
    // lets a component draw a "dock back" control without also having to be
    // fullscreen or tiled.
    let floating_window = crate::WindowSurfaceState {
        windowed: true,
        ..crate::WindowSurfaceState::default()
    }
    .flags();
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
fn every_typed_pseudo_state_indexes_and_matches_from_the_same_table() {
    for spec in crate::pseudo_state_specs() {
        let rules = vec![rule_with_state(spec.name)];
        let index = StyleRuleIndex::new(&rules);
        assert_eq!(index.rules_for_state_bit(spec.bit), &[0], "{}", spec.name);

        let mut state = NodeState::default();
        spec.state.set_value(&mut state, true);
        let attrs = StyleNodeAttrs::new("box", &[], None, state);
        let selector = Selector::State("*".into(), spec.name.into());
        assert!(selector_matches_attrs(&selector, &attrs), "{}", spec.name);

        spec.state.set_value(&mut state, false);
        let attrs = StyleNodeAttrs::new("box", &[], None, state);
        assert!(!selector_matches_attrs(&selector, &attrs), "{}", spec.name);
    }
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
        NodeState::default(),
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
            NodeState::default(),
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
            NodeState::default(),
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
                NodeState::default(),
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
        NodeState::default(),
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
            NodeState::default(),
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
    let state = NodeState::HOVERED;
    let attrs = StyleNodeAttrs::new("button", &classes, None, state);
    let mut candidates = Vec::new();

    index.for_each_candidate_rule_index(&attrs, |idx| candidates.push(idx));

    assert_eq!(candidates, vec![0, 1, 2]);
}

#[test]
fn compound_state_dependencies_do_not_widen_candidate_buckets() {
    let rules = vec![
        StyleRule {
            selector: Selector::Compound(vec![
                Selector::Class("target".into()),
                Selector::State("*".into(), "hover".into()),
            ]),
            declarations: vec![],
            container_query: None,
        },
        StyleRule {
            selector: Selector::Compound(vec![
                Selector::Id("other".into()),
                Selector::State("*".into(), "hover".into()),
                Selector::State("*".into(), "focus".into()),
                Selector::State("*".into(), "hover".into()),
            ]),
            declarations: vec![],
            container_query: None,
        },
        rule_with_state("hover"),
    ];
    let index = StyleRuleIndex::new(&rules);
    assert_eq!(index.rules_for_state_bit(STATE_HOVERED), &[0, 1, 2]);
    assert_eq!(index.rules_for_state_bit(STATE_FOCUSED), &[1]);
    for hovered in [false, true] {
        for classes in [vec![], vec!["target".to_string()]] {
            let state = if hovered {
                NodeState::HOVERED
            } else {
                NodeState::empty()
            };
            let attrs = StyleNodeAttrs::new("box", &classes, None, state);
            let mut candidates = vec![];
            index.for_each_candidate_rule_index(&attrs, |id| candidates.push(id));
            assert!(!candidates.contains(&1));
            let actual: Vec<_> = candidates
                .into_iter()
                .filter(|id| selector_matches_attrs(&rules[*id].selector, &attrs))
                .collect();
            let expected: Vec<_> = rules
                .iter()
                .enumerate()
                .filter(|(_, rule)| selector_matches_attrs(&rule.selector, &attrs))
                .map(|(id, _)| id)
                .collect();
            assert_eq!(actual, expected);
        }
    }
}
