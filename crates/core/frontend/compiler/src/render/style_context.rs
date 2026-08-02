use mesh_core_component::PropsBlock;
use mesh_core_component::style::{StyleRule, StyleValue, prop_variable_key};
use mesh_core_elements::{StyleResolver, StyleRuleIndex, VariableStore};
use serde_json;

use super::*;
use std::collections::HashMap;

pub(crate) struct BuildStyleContext<'a, 'theme> {
    pub(super) rules: &'a [StyleRule],
    pub(super) index: BuildStyleRuleIndex<'a>,
    pub(super) resolver: &'a StyleResolver<'theme>,
    pub(super) namespace_handlers: bool,
}

pub(super) enum BuildStyleRuleIndex<'a> {
    Owned(StyleRuleIndex),
    Borrowed(&'a StyleRuleIndex),
}

impl BuildStyleRuleIndex<'_> {
    pub(super) fn as_ref(&self) -> &StyleRuleIndex {
        match self {
            Self::Owned(index) => index,
            Self::Borrowed(index) => index,
        }
    }
}

/// Owned, indexed style rules for a component embedded under a stable host.
///
/// Local component source and its host module's rules are immutable for the
/// lifetime of a compiled catalog. Preparing the combined rules once avoids
/// cloning both rule sets and rebuilding `StyleRuleIndex` on every cache miss.
#[derive(Debug)]
pub struct PreparedComponentStyleRules {
    pub(super) rules: Vec<StyleRule>,
    pub(super) index: StyleRuleIndex,
}

impl PreparedComponentStyleRules {
    pub fn new(component: &mesh_core_component::ComponentFile, host_rules: &[StyleRule]) -> Self {
        let component_rules = component
            .style
            .as_ref()
            .map(|style| style.rules.as_slice())
            .unwrap_or(&[]);
        let mut rules = Vec::with_capacity(host_rules.len() + component_rules.len());
        rules.extend_from_slice(host_rules);
        rules.extend_from_slice(component_rules);
        let index = StyleRuleIndex::new(&rules);
        Self { rules, index }
    }
}

impl<'a, 'theme> BuildStyleContext<'a, 'theme> {
    pub(crate) fn new(rules: &'a [StyleRule], resolver: &'a StyleResolver<'theme>) -> Self {
        Self {
            rules,
            index: BuildStyleRuleIndex::Owned(StyleRuleIndex::new(rules)),
            resolver,
            namespace_handlers: false,
        }
    }

    pub(crate) fn with_handler_namespacing(mut self, enabled: bool) -> Self {
        self.namespace_handlers = enabled;
        self
    }

    pub(super) fn from_prepared(
        prepared: &'a PreparedComponentStyleRules,
        resolver: &'a StyleResolver<'theme>,
    ) -> Self {
        Self {
            rules: &prepared.rules,
            index: BuildStyleRuleIndex::Borrowed(&prepared.index),
            resolver,
            namespace_handlers: false,
        }
    }
}

/// Build the per-instance CSS prop map consumed by `StyleResolver::with_props`.
///
/// Each declared prop resolves to a single value: a `props.<name>` entry in the
/// script `state` (where the shell funnels the precedence-resolved value —
/// default → user setting → instance prop → script write) overrides the declared
/// default. The map is keyed by `prop_variable_key(name)` so `prop(name)`
/// references in `<style>` resolve through the same lookup as `var(--…)`.
pub fn resolve_css_props(
    block: Option<&PropsBlock>,
    state: Option<&dyn VariableStore>,
) -> HashMap<String, StyleValue> {
    let Some(block) = block else {
        return HashMap::new();
    };
    let mut map = HashMap::with_capacity(block.props.len());
    // The shell publishes one `props` table in script state (the precedence-
    // resolved value per name); script writes round-trip back into it.
    let props_state_borrowed = state.and_then(|store| store.get_ref("props"));
    let props_state_owned = if props_state_borrowed.is_none() {
        state.and_then(|store| store.get("props"))
    } else {
        None
    };
    let props_state = props_state_borrowed.or(props_state_owned.as_ref());
    for def in &block.props {
        let value =
            props_state
                .and_then(|obj| obj.get(&def.name))
                .and_then(|value| {
                    mesh_core_component::json_to_prop_value_ref(value).and_then(|value| {
                        match mesh_core_component::prop_value_to_css(def, &value) {
                            Ok(css) => Some(css),
                            Err(err) => {
                                tracing::warn!(
                                    "invalid runtime value for prop `{}` ignored: {err}",
                                    def.name
                                );
                                None
                            }
                        }
                    })
                })
                .or_else(|| {
                    def.default.as_ref().and_then(|value| {
                        match mesh_core_component::prop_value_to_css(def, value) {
                            Ok(css) => Some(css),
                            Err(err) => {
                                tracing::warn!(
                                    "invalid default value for prop `{}` ignored: {err}",
                                    def.name
                                );
                                None
                            }
                        }
                    })
                });
        if let Some(value) = value {
            map.insert(prop_variable_key(&def.name), StyleValue::Literal(value));
        }
    }
    map
}

/// Derive a settings schema from a component's `<props>` — the third projection
/// (alongside the CSS `prop()` map and the reactive Lua `props` table). The shape
/// mirrors the manifest `inline_schema` object so a generated settings UI can
/// consume it directly. Only `expose`d props are included; returns `None` when
/// the component declares no exposable props.
pub fn props_settings_schema(block: Option<&PropsBlock>) -> Option<serde_json::Value> {
    mesh_core_component::props_settings_schema(block)
}

pub(super) struct TrackingVariableStore<'a> {
    pub(super) inner: &'a dyn VariableStore,
    pub(super) reads: std::cell::RefCell<Vec<(String, String)>>,
}

impl<'a> TrackingVariableStore<'a> {
    pub(super) fn new(inner: &'a dyn VariableStore) -> Self {
        Self {
            inner,
            reads: std::cell::RefCell::new(Vec::new()),
        }
    }
    pub(super) fn into_reads(self) -> Vec<(String, String)> {
        self.reads.into_inner()
    }

    pub(super) fn record_read(&self, name: &str) {
        let Some(dot_pos) = name.find('.') else {
            return;
        };
        let service = &name[..dot_pos];
        let field = &name[dot_pos + 1..];
        let mut reads = self.reads.borrow_mut();
        if reads.last().is_some_and(|(last_service, last_field)| {
            last_service == service && last_field == field
        }) {
            return;
        }
        if reads.iter().any(|(existing_service, existing_field)| {
            existing_service == service && existing_field == field
        }) {
            return;
        }
        reads.push((service.to_owned(), field.to_owned()));
    }
}

impl VariableStore for TrackingVariableStore<'_> {
    fn get(&self, name: &str) -> Option<serde_json::Value> {
        let result = self.inner.get(name);
        self.record_read(name);
        result
    }
    fn get_ref<'a>(&'a self, name: &str) -> Option<&'a serde_json::Value> {
        let result = self.inner.get_ref(name);
        self.record_read(name);
        result
    }
    fn keys(&self) -> Vec<String> {
        self.inner.keys()
    }
    fn translate(&self, key: &str) -> Option<String> {
        self.inner.translate(key)
    }
    fn template_locals(&self) -> serde_json::Map<String, serde_json::Value> {
        self.inner.template_locals()
    }
    fn loop_identity(&self) -> Option<&str> {
        self.inner.loop_identity()
    }
    fn record_template_service_reads(&self, reads: &[(String, String)]) {
        self.reads.borrow_mut().extend_from_slice(reads);
    }
}
