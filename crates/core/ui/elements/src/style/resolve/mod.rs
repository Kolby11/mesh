use super::*;
use mesh_core_component::style::StyleValue;
use mesh_core_theme::Theme;
use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

mod attrs;
mod cache;
mod declaration;
mod index;
mod literal;
mod matching;
mod node;
mod state;
mod subtree;
mod value;

pub use attrs::StyleNodeAttrs;
pub(super) use declaration::lowered_css_properties;
pub use index::{StyleRuleAttribution, StyleRuleAttributionEntry, StyleRuleIndex};
pub use matching::selector_matches_attrs;
pub(super) use value::ResolutionContext;

pub(crate) use cache::FinalizedStyle;
use cache::*;

/// Resolves style values against a theme's design tokens.
pub struct StyleResolver<'a> {
    reuse_finalized: bool,
    incremental_matching: bool,
    theme_rule_indexes: RefCell<
        HashMap<Option<String>, (Vec<mesh_core_component::style::StyleRule>, StyleRuleIndex)>,
    >,
    pub(in crate::style::resolve) theme: &'a Theme,
    /// Per-instance resolved component-prop values, keyed by `prop_variable_key`
    /// (`--mesh-prop-<name>`). Consulted as a read-only fallback after the
    /// per-node custom-variable scratch. Empty without a `<props>` block.
    pub(in crate::style::resolve) props: Cow<'a, HashMap<String, StyleValue>>,
    pub(in crate::style::resolve) props_fingerprint: u64,
    pub(in crate::style::resolve) module_variable_cache:
        RefCell<HashMap<String, Vec<(String, StyleValue)>>>,
    /// Comparison-keyed front cache for the shared theme-default cache.
    ///
    /// Every node resolves the defaults for its own `(module_id, tag)`, and a
    /// tree walks long runs of the same pair. The map behind this answers in
    /// two SipHash computations of short strings per node; this answers most
    /// nodes in one or two string comparisons instead.
    pub(in crate::style::resolve) theme_default_recent: RefCell<Vec<RecentThemeDefaults>>,
    pub(in crate::style::resolve) theme_default_diagnostic_cache:
        RefCell<HashMap<String, ThemeDefaultDiagnosticPrototype>>,
    module_theme_default_diagnostic_cache:
        RefCell<HashMap<String, HashMap<String, ThemeDefaultDiagnosticPrototype>>>,
    pub(in crate::style::resolve) theme_reference_cache: RefCell<HashMap<String, Arc<str>>>,
    pub(in crate::style::resolve) theme_value_cache:
        RefCell<HashMap<String, CachedThemeTokenValue>>,
}

impl<'a> StyleResolver<'a> {
    pub fn new(theme: &'a Theme) -> Self {
        Self {
            theme,
            reuse_finalized: false,
            incremental_matching: false,
            theme_rule_indexes: RefCell::new(HashMap::new()),
            props: Cow::Owned(HashMap::new()),
            props_fingerprint: style_props_fingerprint(&HashMap::new()),
            module_variable_cache: RefCell::new(HashMap::new()),
            theme_default_recent: RefCell::new(Vec::new()),
            theme_default_diagnostic_cache: RefCell::new(HashMap::new()),
            module_theme_default_diagnostic_cache: RefCell::new(HashMap::new()),
            theme_reference_cache: RefCell::new(HashMap::new()),
            theme_value_cache: RefCell::new(HashMap::new()),
        }
    }

    fn theme_dependencies_preserve_matches(
        &self,
        module_id: Option<&str>,
        old: &StyleNodeAttrs<'_>,
        new: &StyleNodeAttrs<'_>,
        context: StyleContext,
    ) -> bool {
        if old.state_mask == new.state_mask && old.classes.iter().eq(new.classes.iter()) {
            return true;
        }
        let module_rules = module_id
            .and_then(|id| self.theme.modules().get(id))
            .map(|module| module.rules.as_slice())
            .unwrap_or(&[]);
        if self.theme.style_rules().is_empty() && module_rules.is_empty() {
            return true;
        }
        let mut indexes = self.theme_rule_indexes.borrow_mut();
        let (rules, index) = indexes
            .entry(module_id.map(str::to_owned))
            .or_insert_with(|| {
                let rules = self
                    .theme
                    .style_rules()
                    .iter()
                    .chain(module_rules)
                    .map(|rule| mesh_core_component::style::StyleRule {
                        selector: rule.selector.clone(),
                        declarations: Vec::new(),
                        container_query: None,
                    })
                    .collect::<Vec<_>>();
                let index = StyleRuleIndex::new(&rules);
                (rules, index)
            });
        index.changed_dependencies_preserve_matches(rules, old, new, context)
    }

    /// Retain selector inputs so targeted class/state updates can consult
    /// reverse dependencies before reapplying the cascade.
    pub fn with_incremental_matching(mut self) -> Self {
        self.incremental_matching = true;
        self
    }

    /// Reuse styles held by validated, pre-animation component memo clones.
    /// Callers must validate the annotated tree bottom-up first. Retained trees
    /// with sampled animation values must use the ordinary restyle path.
    pub fn with_finalized_memo_styles(mut self, enabled: bool) -> Self {
        self.reuse_finalized = enabled;
        self
    }

    /// Attach per-instance component-prop values. `props` is keyed by
    /// `prop_variable_key(name)` and holds the resolved value for each prop.
    pub fn with_props(mut self, props: HashMap<String, StyleValue>) -> Self {
        self.props_fingerprint = style_props_fingerprint(&props);
        self.props = Cow::Owned(props);
        self
    }

    /// Borrow per-instance component props when their owner outlives this
    /// resolver. Shell restyles already retain the map for the full cascade,
    /// so borrowing avoids cloning every key and value on each frame.
    pub fn with_borrowed_props(mut self, props: &'a HashMap<String, StyleValue>) -> Self {
        self.props_fingerprint = style_props_fingerprint(props);
        self.props = Cow::Borrowed(props);
        self
    }

    /// Fingerprint of the attached component props; equal fingerprints
    /// resolve `prop()` references identically.
    pub fn props_fingerprint(&self) -> u64 {
        self.props_fingerprint
    }
}

#[cfg(test)]
mod tests;
