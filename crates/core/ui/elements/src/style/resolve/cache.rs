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

// Reusable scratch HashMap for CSS custom-property variable resolution.
// Cleared at the start of each resolve call to avoid per-node allocations
// while retaining allocated capacity across calls on the same thread.
thread_local! {
    pub(super) static VARIABLE_SCRATCH: RefCell<HashMap<String, StyleValue>> =
        RefCell::new(HashMap::new());
    pub(super) static CANDIDATE_RULE_SCRATCH: RefCell<Vec<usize>> = const { RefCell::new(Vec::new()) };
    pub(super) static INLINE_STYLE_CACHE: RefCell<LruCache<Arc<str>, CachedInlineStyle>> =
        RefCell::new(LruCache::new(MAX_INLINE_STYLE_CACHE_ENTRIES));
    pub(super) static SHARED_THEME_DEFAULT_CACHE: RefCell<LruCache<u64, SharedThemeDefaultCache>> =
        RefCell::new(LruCache::new(MAX_SHARED_THEME_REVISIONS));
    pub(super) static THEME_DEFAULT_DECLARATION_CACHE: RefCell<LruCache<(u64, usize), CachedThemeDefaultDeclarations>> =
        RefCell::new(LruCache::new(MAX_THEME_DEFAULT_DECLARATION_CACHE_ENTRIES));
}

pub(super) const MAX_INLINE_STYLE_CACHE_ENTRIES: usize = 256;
pub(super) const MAX_SHARED_THEME_REVISIONS: usize = 16;
pub(super) const MAX_SHARED_THEME_DEFAULTS_PER_REVISION: usize = 256;
pub(super) const MAX_THEME_DEFAULT_DECLARATION_CACHE_ENTRIES: usize = 512;

/// The CSS-inherited fields from a parent node. Used instead of cloning
/// the full `ComputedStyle` (~60 fields) when passing parent context into
/// recursive restyle calls.
#[derive(PartialEq)]
pub(super) struct ParentInheritedStyle {
    pub(super) custom_properties: HashMap<String, StyleValue>,
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
pub(super) enum CachedThemeTokenValue {
    Missing,
    Error(Arc<str>),
    String(Arc<str>),
    Number(f64),
    Bool(bool),
}

impl CachedThemeTokenValue {
    pub(super) fn from_resolution(value: Result<Option<TokenValue>, ThemeTokenError>) -> Self {
        match value {
            Ok(Some(TokenValue::String(value))) => Self::String(Arc::from(value)),
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
