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
    if value.starts_with("var(") && value.ends_with(')') {
        StyleValue::Var(value[4..value.len() - 1].trim().to_string())
    } else {
        StyleValue::Literal(value.to_string())
    }
}

pub fn selector_matches_attrs(selector: &Selector, attrs: &StyleNodeAttrs) -> bool {
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
        Selector::Compound(parts) => parts.iter().all(|s| selector_matches_attrs(s, attrs)),
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
                    Selector::Compound(_) => return None,
                }
            }
            best
        }
        Selector::Universal => None,
    }
}
