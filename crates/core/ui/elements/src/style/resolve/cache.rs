use super::index::*;
use crate::lru::LruCache;
use crate::style::*;
use mesh_core_component::style::StyleValue;
use mesh_core_theme::{ThemeTokenError, TokenValue};
use std::cell::RefCell;
use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::rc::Rc;
use std::sync::{Arc, OnceLock};

pub(super) fn empty_variables() -> &'static HashMap<String, StyleValue> {
    pub(super) static EMPTY: OnceLock<HashMap<String, StyleValue>> = OnceLock::new();
    EMPTY.get_or_init(HashMap::new)
}

/// Where declaration application writes custom properties and reads them
/// back for `var()` resolution.
pub(super) trait VariableSink {
    fn variables(&self) -> &HashMap<String, StyleValue>;
    fn insert_variable(&mut self, name: &str, value: &StyleValue);
}

impl VariableSink for HashMap<String, StyleValue> {
    fn variables(&self) -> &HashMap<String, StyleValue> {
        self
    }

    fn insert_variable(&mut self, name: &str, value: &StyleValue) {
        self.insert(name.to_owned(), value.clone());
    }
}

/// A node's custom properties during its cascade: the inherited shared map
/// until a declaration changes an entry, then a private copy. Most nodes
/// declare no variables and finish with their parent's map unchanged.
pub(super) struct VariableScope {
    base: CustomProperties,
    owned: Option<HashMap<String, StyleValue>>,
}

impl VariableScope {
    pub(super) fn new(base: CustomProperties) -> Self {
        Self { base, owned: None }
    }

    pub(super) fn finish(self) -> CustomProperties {
        match self.owned {
            Some(owned) => CustomProperties::new(owned),
            None => self.base,
        }
    }
}

impl VariableSink for VariableScope {
    fn variables(&self) -> &HashMap<String, StyleValue> {
        self.owned.as_ref().unwrap_or(&self.base)
    }

    fn insert_variable(&mut self, name: &str, value: &StyleValue) {
        if self.variables().get(name) == Some(value) {
            return;
        }
        self.owned
            .get_or_insert_with(|| (*self.base).clone())
            .insert(name.to_owned(), value.clone());
    }
}

thread_local! {
    pub(super) static CANDIDATE_RULE_SCRATCH: RefCell<Vec<usize>> = const { RefCell::new(Vec::new()) };
    pub(super) static MATCHED_RULE_SCRATCH: RefCell<Vec<usize>> = const { RefCell::new(Vec::new()) };
    pub(super) static INLINE_STYLE_CACHE: RefCell<LruCache<Arc<str>, CachedInlineStyle>> =
        RefCell::new(LruCache::new(MAX_INLINE_STYLE_CACHE_ENTRIES));
    pub(super) static SHARED_THEME_VALUE_CACHE: RefCell<LruCache<u64, LruCache<Arc<str>, CachedThemeTokenValue>>> =
        RefCell::new(LruCache::new(MAX_SHARED_THEME_REVISIONS));
    pub(super) static SHARED_THEME_DEFAULT_CACHE: RefCell<LruCache<u64, SharedThemeDefaultCache>> =
        RefCell::new(LruCache::new(MAX_SHARED_THEME_REVISIONS));
    pub(super) static THEME_DEFAULT_DECLARATION_CACHE: RefCell<LruCache<(u64, usize), CachedThemeDefaultDeclarations>> =
        RefCell::new(LruCache::new(MAX_THEME_DEFAULT_DECLARATION_CACHE_ENTRIES));
}

pub(super) const MAX_INLINE_STYLE_CACHE_ENTRIES: usize = 256;
pub(super) const MAX_SHARED_THEME_REVISIONS: usize = 16;
pub(super) const MAX_SHARED_THEME_VALUES: usize = 512;
pub(super) const MAX_SHARED_THEME_DEFAULTS_PER_REVISION: usize = 256;
pub(super) const MAX_THEME_DEFAULT_DECLARATION_CACHE_ENTRIES: usize = 512;

/// The CSS-inherited fields from a parent node. Used instead of cloning
/// the full `ComputedStyle` (~60 fields) when passing parent context into
/// recursive restyle calls.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct ParentInheritedStyle {
    pub(super) custom_properties: CustomProperties,
    pub(super) color: Color,
    pub(super) font_family: Arc<str>,
    pub(super) font_size: f32,
    pub(super) font_weight: u16,
    pub(super) line_height: f32,
}

impl From<&ComputedStyle> for ParentInheritedStyle {
    fn from(s: &ComputedStyle) -> Self {
        Self {
            custom_properties: s.custom_properties.clone(),
            color: s.color,
            font_family: s.font_family.clone(),
            font_size: s.font_size,
            font_weight: s.font_weight,
            line_height: s.line_height,
        }
    }
}

/// Lowered theme defaults for one `(module_id, tag)` pair.
///
/// Shared rather than cloned per node: only `style` is needed by value (it is
/// the mutable base a node's own rules are applied to), and `variables` is read
/// through, so the per-node cost is one refcount bump plus one `ComputedStyle`
/// clone instead of a deep copy of both.
#[derive(Debug, Default)]
pub(super) struct ThemeComponentDefaults {
    pub(super) style: ComputedStyle,
    pub(super) variables: HashMap<String, StyleValue>,
}

pub(super) struct SharedThemeDefaultCache {
    pub(super) entries: LruCache<SharedThemeDefaultKey, Vec<SharedThemePropDefaults>>,
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub(super) struct SharedThemeDefaultKey {
    pub(super) props_fingerprint: u64,
    pub(super) module_id_hash: u64,
    pub(super) tag_hash: u64,
}

pub(super) struct SharedThemePropDefaults {
    pub(super) props: HashMap<String, StyleValue>,
    pub(super) module_id: Option<String>,
    pub(super) tag: String,
    pub(super) defaults: Rc<ThemeComponentDefaults>,
}

impl Default for SharedThemeDefaultCache {
    fn default() -> Self {
        Self {
            entries: LruCache::new(MAX_SHARED_THEME_DEFAULTS_PER_REVISION),
        }
    }
}

pub(super) fn style_props_fingerprint(props: &HashMap<String, StyleValue>) -> u64 {
    let mut fingerprint = props.len() as u64;
    for (name, value) in props {
        let mut entry = DefaultHasher::new();
        name.hash(&mut entry);
        value.hash(&mut entry);
        fingerprint ^= entry.finish().rotate_left(17);
    }
    fingerprint
}

pub(super) fn shared_theme_key(
    props_fingerprint: u64,
    tag: &str,
    module_id: Option<&str>,
) -> SharedThemeDefaultKey {
    pub(super) fn hash(value: Option<&str>) -> u64 {
        let mut hasher = DefaultHasher::new();
        value.hash(&mut hasher);
        hasher.finish()
    }
    SharedThemeDefaultKey {
        props_fingerprint,
        module_id_hash: hash(module_id),
        tag_hash: hash(Some(tag)),
    }
}

pub(super) fn shared_theme_defaults(
    revision: u64,
    props_fingerprint: u64,
    props: &HashMap<String, StyleValue>,
    tag: &str,
    module_id: Option<&str>,
) -> Option<Rc<ThemeComponentDefaults>> {
    SHARED_THEME_DEFAULT_CACHE.with(|cache| {
        let key = shared_theme_key(props_fingerprint, tag, module_id);
        let mut cache = cache.borrow_mut();
        let revision_cache = cache.get_mut(&revision)?;
        revision_cache
            .entries
            .get(&key)?
            .iter()
            .find(|entry| {
                entry.props == *props && entry.module_id.as_deref() == module_id && entry.tag == tag
            })
            .map(|entry| Rc::clone(&entry.defaults))
    })
}

pub(super) fn remember_shared_theme_defaults(
    revision: u64,
    props_fingerprint: u64,
    props: &HashMap<String, StyleValue>,
    tag: &str,
    module_id: Option<&str>,
    defaults: &Rc<ThemeComponentDefaults>,
) {
    SHARED_THEME_DEFAULT_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if !cache.contains_key(&revision) {
            cache.insert(revision, SharedThemeDefaultCache::default());
        }
        let revision_cache = cache
            .get_mut(&revision)
            .expect("revision cache inserted above");
        let key = shared_theme_key(props_fingerprint, tag, module_id);
        if let Some(entries) = revision_cache.entries.get_mut(&key) {
            if let Some(entry) = entries.iter_mut().find(|entry| {
                entry.props == *props && entry.module_id.as_deref() == module_id && entry.tag == tag
            }) {
                entry.defaults = Rc::clone(defaults);
            } else {
                entries.push(SharedThemePropDefaults {
                    props: props.clone(),
                    module_id: module_id.map(str::to_owned),
                    tag: tag.to_owned(),
                    defaults: Rc::clone(defaults),
                });
            }
        } else {
            revision_cache.entries.insert(
                key,
                vec![SharedThemePropDefaults {
                    props: props.clone(),
                    module_id: module_id.map(str::to_owned),
                    tag: tag.to_owned(),
                    defaults: Rc::clone(defaults),
                }],
            );
        }
    });
}

pub(super) struct RecentThemeDefaults {
    pub(super) module_id: Option<String>,
    pub(super) tag: String,
    pub(super) defaults: Rc<ThemeComponentDefaults>,
}

/// Entries kept in the comparison-keyed front cache. Small enough that a linear
/// scan of short-string compares beats hashing, large enough to cover the
/// handful of tags a surface actually repeats.
pub(super) const THEME_DEFAULT_RECENT_CAPACITY: usize = 8;

pub(super) type ThemeDefaultDiagnosticPrototype = (
    ComputedStyle,
    HashMap<String, StyleValue>,
    Vec<StyleDiagnostic>,
);

#[derive(Debug, Clone)]
pub(super) struct CachedThemeString {
    raw: Arc<str>,
    pub(super) color: Color,
    pub(super) number: f32,
}

impl std::ops::Deref for CachedThemeString {
    type Target = str;

    fn deref(&self) -> &str {
        &self.raw
    }
}

impl AsRef<str> for CachedThemeString {
    fn as_ref(&self) -> &str {
        &self.raw
    }
}

#[derive(Debug, Clone)]
pub(super) enum CachedThemeTokenValue {
    Missing,
    Error(Arc<str>),
    String(CachedThemeString),
    Number(f64),
    Bool(bool),
}

impl CachedThemeTokenValue {
    pub(super) fn from_resolution(value: Result<Option<TokenValue>, ThemeTokenError>) -> Self {
        match value {
            Ok(Some(TokenValue::String(value))) => Self::String(CachedThemeString {
                color: Color::from_css(&value).unwrap_or(Color::TRANSPARENT),
                number: crate::style::parse::parse_px(&value),
                raw: Arc::from(value),
            }),
            Ok(Some(TokenValue::Number(value))) => Self::Number(value),
            Ok(Some(TokenValue::Bool(value))) => Self::Bool(value),
            Ok(None) => Self::Missing,
            Err(error) => Self::Error(Arc::from(error.to_string())),
        }
    }

    pub(super) fn is_missing(&self) -> bool {
        matches!(self, Self::Missing | Self::Error(_))
    }
}

/// Inputs to a style already held by a finalized memo node. No style value is
/// copied on a hit; inheritance and the cascade revision are checked separately.
#[derive(Debug, Clone)]
pub(crate) struct FinalizedStyle {
    tag: String,
    classes: Vec<String>,
    id: Option<String>,
    inline_style: Option<String>,
    parts: Option<String>,
    hidden: Option<String>,
    promoted: (bool, bool),
    module_id: Option<String>,
    state: crate::tree::ElementState,
    parent: Option<ParentInheritedStyle>,
    theme_revision: u64,
    index_revision: u64,
    props: HashMap<String, StyleValue>,
    container: (f32, f32),
}

impl FinalizedStyle {
    pub(crate) fn matches_node(&self, node: &crate::tree::WidgetNode) -> bool {
        self.hidden.as_deref() == node.attributes.get("hidden").map(String::as_str)
            && self.promoted == (node.is_promoted_popover(), node.is_promoted_window())
            && self.tag == node.tag
            && self.module_id.as_deref() == node.module_id()
            && self.id.as_deref() == node.attributes.get("id").map(String::as_str)
            && self.inline_style.as_deref() == node.attributes.get("style").map(String::as_str)
            && self.parts.as_deref() == node.attributes.get("part").map(String::as_str)
            && self.state == node.state
            && self.classes.iter().map(String::as_str).eq(node
                .attributes
                .get("class")
                .into_iter()
                .flat_map(|s| s.split_whitespace()))
    }

    pub(super) fn new(
        node: &crate::tree::WidgetNode,
        parent: Option<&ParentInheritedStyle>,
        resolver: &super::StyleResolver<'_>,
        index: &StyleRuleIndex,
        context: StyleContext,
    ) -> Self {
        Self {
            tag: node.tag.clone(),
            classes: node
                .attributes
                .get("class")
                .map(|s| s.split_whitespace().map(str::to_owned).collect())
                .unwrap_or_default(),
            id: node.attributes.get("id").cloned(),
            inline_style: node.attributes.get("style").cloned(),
            parts: node.attributes.get("part").cloned(),
            hidden: node.attributes.get("hidden").cloned(),
            promoted: (node.is_promoted_popover(), node.is_promoted_window()),
            module_id: node.module_id().map(str::to_owned),
            state: node.state,
            parent: parent.cloned(),
            theme_revision: resolver.theme.revision(),
            index_revision: index.revision,
            props: resolver.props.as_ref().clone(),
            container: (context.container_width, context.container_height),
        }
    }

    pub(super) fn same_environment(
        &self,
        parent: Option<&ParentInheritedStyle>,
        resolver: &super::StyleResolver<'_>,
        index: &StyleRuleIndex,
        context: StyleContext,
    ) -> bool {
        self.theme_revision == resolver.theme.revision()
            && self.index_revision == index.revision
            && self.container == (context.container_width, context.container_height)
            && self.props == *resolver.props
            && self.parent.as_ref() == parent
    }

    pub(super) fn same_matching_rules(
        &self,
        node: &mut crate::tree::WidgetNode,
        rules: &[mesh_core_component::style::StyleRule],
        index: &StyleRuleIndex,
        context: StyleContext,
        resolver: &super::StyleResolver<'_>,
    ) -> bool {
        if self.hidden.as_deref() != node.attributes.get("hidden").map(String::as_str)
            || self.promoted != (node.is_promoted_popover(), node.is_promoted_window())
        {
            return false;
        }
        let attrs = StyleNodeAttrs::from_node(node);
        if self.tag != attrs.tag
            || self.id.as_deref() != attrs.id
            || self.inline_style.as_deref() != attrs.inline_style
            || self.parts.as_deref() != attrs.parts
            || self.module_id.as_deref() != attrs.module_id
        {
            return false;
        }
        let mut old = StyleNodeAttrs::new(&self.tag, &self.classes, self.id.as_deref(), self.state);
        old.module_id = self.module_id.as_deref();
        old.parts = self.parts.as_deref();
        index.changed_dependencies_preserve_matches(rules, &old, &attrs, context)
            && resolver.theme_dependencies_preserve_matches(
                self.module_id.as_deref(),
                &old,
                &attrs,
                context,
            )
    }
}
