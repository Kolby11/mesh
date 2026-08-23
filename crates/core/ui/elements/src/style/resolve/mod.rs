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

use cache::*;

/// Resolves style values against a theme's design tokens.
pub struct StyleResolver<'a> {
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
}

#[cfg(test)]
mod tests;
