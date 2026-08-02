use super::super::attrs::*;
use super::super::cache::*;
use super::super::declaration::*;
use super::super::index::*;
use super::super::matching::*;
use super::super::state::*;
use super::super::*;
use crate::lru::LruCache;
use crate::style::parse::*;
use crate::style::*;
use crate::tree::ElementState;
use mesh_core_component::style::{Declaration, Selector, StyleRule, StyleValue, prop_variable_key};
use mesh_core_theme::{Theme, TokenValue};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

pub(super) fn rule_with_state(state_selector: &str) -> StyleRule {
    StyleRule {
        selector: Selector::State("".to_string(), state_selector.to_string()),
        declarations: vec![Declaration {
            property: "color".to_string(),
            value: StyleValue::Literal("red".to_string()),
        }],
        container_query: None,
    }
}

pub(super) fn rule_with_class(class: &str) -> StyleRule {
    StyleRule {
        selector: Selector::Class(class.to_string()),
        declarations: Vec::new(),
        container_query: None,
    }
}

pub(super) fn rule_with_tag(tag: &str) -> StyleRule {
    StyleRule {
        selector: Selector::Tag(tag.to_string()),
        declarations: Vec::new(),
        container_query: None,
    }
}

pub(super) fn rule_with_compound_state(tag: &str, state: &str) -> StyleRule {
    StyleRule {
        selector: Selector::Compound(vec![
            Selector::Tag(tag.to_string()),
            Selector::State(tag.to_string(), state.to_string()),
        ]),
        declarations: vec![Declaration {
            property: "color".to_string(),
            value: StyleValue::Literal("red".to_string()),
        }],
        container_query: None,
    }
}
