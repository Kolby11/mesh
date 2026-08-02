use super::super::parse::parse_transition_properties;
use super::super::*;
use crate::tree::ElementState;
use mesh_core_component::{
    parser::parse_component,
    style::{Declaration, Selector, StyleRule, StyleValue, prop_variable_key},
};

pub(super) fn parse_fixture_style(source: &str) -> Vec<StyleRule> {
    parse_component(source)
        .expect("fixture parses")
        .style
        .expect("fixture has style")
        .rules
}

pub(super) fn selector_has_class(selector: &Selector, class: &str) -> bool {
    match selector {
        Selector::Class(name) => name == class,
        Selector::Compound(parts) => parts.iter().any(|part| selector_has_class(part, class)),
        Selector::Tag(_) | Selector::Id(_) | Selector::State(_, _) | Selector::Universal => false,
    }
}

pub(super) fn resolve_class(
    resolver: &StyleResolver<'_>,
    rules: &[StyleRule],
    class: &str,
) -> (ComputedStyle, Vec<StyleDiagnostic>) {
    resolver.resolve_node_style_with_diagnostics(
        rules,
        "box",
        &[class.to_string()],
        None,
        StyleContext::default(),
        ElementState::default(),
    )
}
