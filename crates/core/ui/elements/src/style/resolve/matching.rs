use super::attrs::*;
use super::cache::*;
use super::index::*;
use crate::pseudo_state::PseudoState;
use crate::style::*;
use mesh_core_component::style::{Selector, StyleRule, StyleValue};

pub(super) fn is_strict_animation_property(property: &str) -> bool {
    matches!(
        property,
        "transition"
            | "transition-duration"
            | "transition-delay"
            | "transition-timing-function"
            | "animation"
            | "animation-duration"
            | "animation-delay"
            | "animation-timing-function"
    )
}

pub(super) fn contains_deprecated_token_reference(value: &StyleValue) -> bool {
    match value {
        StyleValue::Literal(value) => value.contains("token("),
        StyleValue::Var(_) | StyleValue::Prop(_) => false,
    }
}

pub(super) fn classify_theme_style_value(value: &str) -> StyleValue {
    let value = value.trim();
    if let Some(name) = mesh_core_theme::css::variable_reference(value) {
        StyleValue::Var(name)
    } else {
        StyleValue::Literal(value.to_string())
    }
}

/// Whether `selector` matches the node `attrs` describe. A `host::part`
/// selector matches only that part of a matching host, and every other
/// selector matches only ordinary nodes.
pub fn selector_matches_attrs(selector: &Selector, attrs: &StyleNodeAttrs) -> bool {
    match selector {
        Selector::PseudoElement(host, part) => {
            attrs.pseudo_element == Some(part.as_str()) && host_selector_matches(host, attrs)
        }
        _ => attrs.pseudo_element.is_none() && host_selector_matches(selector, attrs),
    }
}

/// Whether `selector` matches the host element itself, or for a `host::part`
/// selector, whether its host half does. Host-side dependency checks use
/// this: a part rule's outcome changes exactly when its host half's does.
pub(super) fn selector_matches_host(selector: &Selector, attrs: &StyleNodeAttrs) -> bool {
    match selector {
        Selector::PseudoElement(host, _) => host_selector_matches(host, attrs),
        _ => host_selector_matches(selector, attrs),
    }
}

fn host_selector_matches(selector: &Selector, attrs: &StyleNodeAttrs) -> bool {
    match selector {
        Selector::Universal => true,
        Selector::Tag(t) => t == attrs.tag,
        Selector::Class(c) => attrs.has_class(c),
        Selector::Id(i) => attrs.id() == Some(i.as_str()),
        Selector::State(t, pseudo) => {
            let tag_matches = t == "*" || t == attrs.tag;
            let state_matches =
                PseudoState::from_name(pseudo).is_some_and(|state| state.value(attrs.state));
            tag_matches && state_matches
        }
        Selector::Compound(parts) => parts.iter().all(|s| host_selector_matches(s, attrs)),
        // A part cannot host another part.
        Selector::PseudoElement(..) => false,
    }
}

pub(super) fn rule_matches_attrs(
    rule: &StyleRule,
    attrs: &StyleNodeAttrs,
    context: StyleContext,
) -> bool {
    selector_matches_attrs(&rule.selector, attrs)
        && rule
            .container_query
            .is_none_or(|query| query.matches(context.container_width, context.container_height))
}

/// [`rule_matches_attrs`] for a host's own dependencies: a `host::part`
/// rule counts when its host half matches.
pub(super) fn rule_matches_host(
    rule: &StyleRule,
    attrs: &StyleNodeAttrs,
    context: StyleContext,
) -> bool {
    selector_matches_host(&rule.selector, attrs)
        && rule
            .container_query
            .is_none_or(|query| query.matches(context.container_width, context.container_height))
}

pub(super) fn inherit_retained_text_style(
    style: &mut ComputedStyle,
    parent: &ParentInheritedStyle,
) {
    let explicit = style.explicit_properties;
    let mut inherited = StylePropertyMask::default();
    if !explicit.color {
        style.color = parent.color;
        inherited.color = true;
    }
    if !explicit.font_family {
        style.font_family = parent.font_family.clone();
        inherited.font_family = true;
    }
    if !explicit.font_size {
        style.font_size = parent.font_size;
        inherited.font_size = true;
    }
    if !explicit.font_weight {
        style.font_weight = parent.font_weight;
        inherited.font_weight = true;
    }
    if !explicit.line_height {
        style.line_height = parent.line_height;
        inherited.line_height = true;
    }
    style.inherited_properties = inherited;
}

pub(super) fn selector_index_key(selector: &Selector) -> Option<SelectorIndexKey<'_>> {
    match selector {
        Selector::PseudoElement(_, part) => Some(SelectorIndexKey::PseudoElement(part)),
        Selector::Tag(tag) => Some(SelectorIndexKey::Tag(tag)),
        Selector::Class(class) => Some(SelectorIndexKey::Class(class)),
        Selector::Id(id) => Some(SelectorIndexKey::Id(id)),
        Selector::State(_, state) => Some(SelectorIndexKey::State(state)),
        Selector::Compound(parts) => {
            let mut best = None;
            for part in parts {
                match part {
                    Selector::Id(id) => return Some(SelectorIndexKey::Id(id)),
                    Selector::Class(class) => best = Some(SelectorIndexKey::Class(class)),
                    Selector::Tag(tag) if best.is_none() => best = Some(SelectorIndexKey::Tag(tag)),
                    Selector::State(_, state) if best.is_none() => {
                        best = Some(SelectorIndexKey::State(state));
                    }
                    Selector::Universal => {}
                    Selector::Tag(_) | Selector::State(_, _) => {}
                    Selector::Compound(_) | Selector::PseudoElement(..) => return None,
                }
            }
            best
        }
        Selector::Universal => None,
    }
}
