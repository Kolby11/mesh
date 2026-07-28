//! Interned attribute keys for the widget tree.
//!
//! Attribute *names* are template vocabulary, not data: the same handful of
//! short strings (`class`, `content`, `data-mesh-element`, `_mesh_key`, …) is
//! rebuilt for every node on every widget-tree build. Storing them as owned
//! `String`s made every node pay a `malloc` + copy + `free` per attribute.
//!
//! [`AttrKey`] keeps the same ordering and lookup behavior as `String` — it
//! borrows as `str`, so `map.get("class")` still works — while resolving
//! well-known names to `&'static str` and everything else to a shared `Arc<str>`
//! taken from a small per-thread cache.

use std::borrow::Borrow;
use std::cell::RefCell;
use std::cmp::Ordering;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::ops::Deref;
use std::sync::{Arc, OnceLock};

/// Key type for [`AttributeMap`].
///
/// Ordering, equality, and hashing all delegate to the string contents, so a
/// `BTreeMap` keyed by `AttrKey` iterates in exactly the order the equivalent
/// `BTreeMap<String, _>` would.
#[derive(Clone)]
pub enum AttrKey {
    /// A name from the known template/runtime vocabulary. Free to clone.
    Static(&'static str),
    /// Any other name, shared through the per-thread intern cache.
    Shared(Arc<str>),
}

/// A borrowed resolved attribute value that preserves the type produced by a
/// template expression.
#[derive(Clone, Copy)]
pub struct ResolvedAttributeValueRef<'a> {
    value: &'a StoredAttributeValue,
}

impl ResolvedAttributeValueRef<'_> {
    /// Match the historical string-backed boolean interpretation without
    /// formatting a typed value first.
    pub fn legacy_bool(self) -> bool {
        match self.value {
            StoredAttributeValue::String(value) => {
                matches!(value.trim(), "" | "true" | "1")
            }
            StoredAttributeValue::Typed(value) => match &value.value {
                serde_json::Value::Null => true,
                serde_json::Value::Bool(value) => *value,
                serde_json::Value::Number(value) => {
                    value.as_i64() == Some(1) || value.as_u64() == Some(1)
                }
                serde_json::Value::String(value) => {
                    matches!(value.trim(), "" | "true" | "1")
                }
                serde_json::Value::Array(_) | serde_json::Value::Object(_) => false,
            },
        }
    }

    /// Match the historical string parse used by numeric attribute consumers.
    pub fn parse_f32(self) -> Option<f32> {
        match self.value {
            StoredAttributeValue::String(value) => value.trim().parse::<f32>().ok(),
            StoredAttributeValue::Typed(value) => match &value.value {
                serde_json::Value::Number(value) => value.as_f64().map(|value| value as f32),
                serde_json::Value::String(value) => value.trim().parse::<f32>().ok(),
                serde_json::Value::Null
                | serde_json::Value::Bool(_)
                | serde_json::Value::Array(_)
                | serde_json::Value::Object(_) => None,
            },
        }
    }

    /// Materialize the legacy string representation for consumers whose public
    /// data model still requires owned text.
    pub fn to_legacy_string(self) -> String {
        match self.value {
            StoredAttributeValue::String(value) => value.clone(),
            StoredAttributeValue::Typed(value) => value_ref_to_string(&value.value),
        }
    }

    #[cfg(test)]
    fn is_string(self) -> bool {
        match self.value {
            StoredAttributeValue::String(_) => true,
            StoredAttributeValue::Typed(value) => {
                matches!(&value.value, serde_json::Value::String(_))
            }
        }
    }
}

impl fmt::Debug for ResolvedAttributeValueRef<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.value {
            StoredAttributeValue::String(value) => fmt::Debug::fmt(value, formatter),
            StoredAttributeValue::Typed(value) => fmt::Debug::fmt(&value.value, formatter),
        }
    }
}

#[derive(Clone)]
struct TypedAttributeValue {
    value: serde_json::Value,
    rendered: OnceLock<String>,
}

#[derive(Clone)]
enum StoredAttributeValue {
    String(String),
    Typed(Box<TypedAttributeValue>),
}

impl StoredAttributeValue {
    fn from_json(value: serde_json::Value) -> Self {
        match value {
            serde_json::Value::String(value) => Self::String(value),
            value => Self::Typed(Box::new(TypedAttributeValue {
                value,
                rendered: OnceLock::new(),
            })),
        }
    }

    fn value_ref(&self) -> ResolvedAttributeValueRef<'_> {
        ResolvedAttributeValueRef { value: self }
    }

    fn as_string(&self) -> &String {
        match self {
            Self::String(value) => value,
            Self::Typed(value) => value
                .rendered
                .get_or_init(|| value_ref_to_string(&value.value)),
        }
    }

    fn into_string(self) -> String {
        match self {
            Self::String(value) => value,
            Self::Typed(value) => value
                .rendered
                .into_inner()
                .unwrap_or_else(|| value_ref_to_string(&value.value)),
        }
    }

    fn as_string_mut(&mut self) -> &mut String {
        if matches!(self, Self::Typed(_)) {
            let previous = std::mem::replace(self, Self::String(String::new()));
            *self = Self::String(previous.into_string());
        }
        match self {
            Self::String(value) => value,
            Self::Typed(_) => unreachable!("typed attribute converted to a string above"),
        }
    }
}

impl PartialEq for StoredAttributeValue {
    fn eq(&self, other: &Self) -> bool {
        self.as_string() == other.as_string()
    }
}

impl Eq for StoredAttributeValue {}

fn value_ref_to_string(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Null => String::new(),
        serde_json::Value::String(value) => value.clone(),
        value => value.to_string(),
    }
}

/// Resolved attributes of a widget node.
///
/// A sorted `Vec` rather than a `BTreeMap`: real elements carry a handful of
/// attributes, so a B-tree spends its time allocating and walking node blocks
/// for a set that fits in one cache-friendly run. Iteration order is the same
/// key order a `BTreeMap<String, String>` would produce, so nothing downstream
/// of the widget tree observes the change. Non-string template bindings retain
/// their JSON type and lazily materialize the legacy string representation only
/// when a string-only consumer asks for it.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct AttributeMap {
    entries: Vec<(AttrKey, StoredAttributeValue)>,
}

impl AttributeMap {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            entries: Vec::with_capacity(capacity),
        }
    }

    #[inline]
    fn find(&self, key: &str) -> Result<usize, usize> {
        self.entries
            .binary_search_by(|(entry, _)| entry.as_str().cmp(key))
    }

    pub fn insert(&mut self, key: AttrKey, value: String) -> Option<String> {
        self.insert_stored(key, StoredAttributeValue::String(value))
    }

    /// Insert a value produced by template expression evaluation without
    /// stringifying booleans, numbers, null, arrays, or objects.
    pub fn insert_value(&mut self, key: AttrKey, value: serde_json::Value) -> Option<String> {
        self.insert_stored(key, StoredAttributeValue::from_json(value))
    }

    fn insert_stored(&mut self, key: AttrKey, value: StoredAttributeValue) -> Option<String> {
        // Parsed attributes and conversions from ordered maps normally arrive
        // in key order. Keep that common construction path append-only: it
        // avoids a binary search and, more importantly, avoids asking Vec to
        // shift the existing tail for every new attribute.
        if let Some((last_key, _)) = self.entries.last() {
            match last_key.as_str().cmp(key.as_str()) {
                Ordering::Less => {
                    self.entries.push((key, value));
                    return None;
                }
                Ordering::Equal => {
                    return Some(
                        std::mem::replace(
                            &mut self.entries.last_mut().expect("last entry").1,
                            value,
                        )
                        .into_string(),
                    );
                }
                Ordering::Greater => {}
            }
        } else {
            self.entries.push((key, value));
            return None;
        }

        match self.find(key.as_str()) {
            Ok(index) => Some(std::mem::replace(&mut self.entries[index].1, value).into_string()),
            Err(index) => {
                self.entries.insert(index, (key, value));
                None
            }
        }
    }

    pub fn get(&self, key: &str) -> Option<&String> {
        self.find(key)
            .ok()
            .map(|index| self.entries[index].1.as_string())
    }

    pub fn get_value(&self, key: &str) -> Option<ResolvedAttributeValueRef<'_>> {
        self.find(key)
            .ok()
            .map(|index| self.entries[index].1.value_ref())
    }

    pub fn get_mut(&mut self, key: &str) -> Option<&mut String> {
        match self.find(key) {
            Ok(index) => Some(self.entries[index].1.as_string_mut()),
            Err(_) => None,
        }
    }

    pub fn contains_key(&self, key: &str) -> bool {
        self.find(key).is_ok()
    }

    pub fn remove(&mut self, key: &str) -> Option<String> {
        match self.find(key) {
            Ok(index) => Some(self.entries.remove(index).1.into_string()),
            Err(_) => None,
        }
    }

    pub fn entry(&mut self, key: AttrKey) -> Entry<'_> {
        let (slot, occupied) = match self.find(key.as_str()) {
            Ok(index) => (index, true),
            Err(index) => (index, false),
        };
        Entry {
            map: self,
            key,
            slot,
            occupied,
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    pub fn iter(&self) -> impl Iterator<Item = (&AttrKey, &String)> {
        self.entries
            .iter()
            .map(|(key, value)| (key, value.as_string()))
    }

    pub fn iter_values(&self) -> impl Iterator<Item = (&AttrKey, ResolvedAttributeValueRef<'_>)> {
        self.entries
            .iter()
            .map(|(key, value)| (key, value.value_ref()))
    }

    pub fn keys(&self) -> impl Iterator<Item = &AttrKey> {
        self.entries.iter().map(|(key, _)| key)
    }

    pub fn values(&self) -> impl Iterator<Item = &String> {
        self.entries.iter().map(|(_, value)| value.as_string())
    }

    pub fn values_mut(&mut self) -> impl Iterator<Item = &mut String> {
        self.entries
            .iter_mut()
            .map(|(_, value)| value.as_string_mut())
    }

    pub fn retain<F: FnMut(&AttrKey, &mut String) -> bool>(&mut self, mut keep: F) {
        self.entries
            .retain_mut(|(key, value)| keep(key, value.as_string_mut()));
    }
}

/// Vacant/occupied slot returned by [`AttributeMap::entry`].
pub struct Entry<'a> {
    map: &'a mut AttributeMap,
    key: AttrKey,
    slot: usize,
    occupied: bool,
}

impl<'a> Entry<'a> {
    pub fn or_insert(self, default: String) -> &'a mut String {
        self.or_insert_with(|| default)
    }

    pub fn or_default(self) -> &'a mut String {
        self.or_insert_with(String::new)
    }

    pub fn or_insert_with<F: FnOnce() -> String>(self, default: F) -> &'a mut String {
        if !self.occupied {
            self.map.entries.insert(
                self.slot,
                (self.key, StoredAttributeValue::String(default())),
            );
        }
        self.map.entries[self.slot].1.as_string_mut()
    }
}

impl fmt::Debug for AttributeMap {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_map().entries(self.iter()).finish()
    }
}

impl<'a> IntoIterator for &'a AttributeMap {
    type Item = (&'a AttrKey, &'a String);
    type IntoIter = AttributeIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        AttributeIter {
            inner: self.entries.iter(),
        }
    }
}

impl IntoIterator for AttributeMap {
    type Item = (AttrKey, String);
    type IntoIter = AttributeIntoIter;

    fn into_iter(self) -> Self::IntoIter {
        AttributeIntoIter {
            inner: self.entries.into_iter(),
        }
    }
}

#[derive(Clone)]
pub struct AttributeIter<'a> {
    inner: std::slice::Iter<'a, (AttrKey, StoredAttributeValue)>,
}

impl<'a> Iterator for AttributeIter<'a> {
    type Item = (&'a AttrKey, &'a String);

    fn next(&mut self) -> Option<Self::Item> {
        self.inner
            .next()
            .map(|(key, value)| (key, value.as_string()))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl ExactSizeIterator for AttributeIter<'_> {}

impl DoubleEndedIterator for AttributeIter<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.inner
            .next_back()
            .map(|(key, value)| (key, value.as_string()))
    }
}

impl std::iter::FusedIterator for AttributeIter<'_> {}

pub struct AttributeIntoIter {
    inner: std::vec::IntoIter<(AttrKey, StoredAttributeValue)>,
}

impl Iterator for AttributeIntoIter {
    type Item = (AttrKey, String);

    fn next(&mut self) -> Option<Self::Item> {
        self.inner
            .next()
            .map(|(key, value)| (key, value.into_string()))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl ExactSizeIterator for AttributeIntoIter {}

impl DoubleEndedIterator for AttributeIntoIter {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.inner
            .next_back()
            .map(|(key, value)| (key, value.into_string()))
    }
}

impl std::iter::FusedIterator for AttributeIntoIter {}

impl FromIterator<(AttrKey, String)> for AttributeMap {
    fn from_iter<T: IntoIterator<Item = (AttrKey, String)>>(iter: T) -> Self {
        let mut map = AttributeMap::new();
        map.extend(iter);
        map
    }
}

impl Extend<(AttrKey, String)> for AttributeMap {
    fn extend<T: IntoIterator<Item = (AttrKey, String)>>(&mut self, iter: T) {
        for (key, value) in iter {
            self.insert(key, value);
        }
    }
}

impl<const N: usize> From<[(AttrKey, String); N]> for AttributeMap {
    fn from(entries: [(AttrKey, String); N]) -> Self {
        entries.into_iter().collect()
    }
}

impl serde::Serialize for AttributeMap {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(Some(self.len()))?;
        for (key, value) in self {
            map.serialize_entry(key.as_str(), value)?;
        }
        map.end()
    }
}

impl<'de> serde::Deserialize<'de> for AttributeMap {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;

        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = AttributeMap;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a map of attribute names to values")
            }

            fn visit_map<M: serde::de::MapAccess<'de>>(
                self,
                mut access: M,
            ) -> Result<AttributeMap, M::Error> {
                let mut map = AttributeMap::with_capacity(access.size_hint().unwrap_or(0));
                while let Some((key, value)) = access.next_entry::<String, String>()? {
                    map.insert(AttrKey::new(&key), value);
                }
                Ok(map)
            }
        }

        deserializer.deserialize_map(Visitor)
    }
}

impl AttrKey {
    /// Intern `name`, avoiding an allocation for known vocabulary.
    pub fn new(name: &str) -> Self {
        match well_known(name) {
            Some(name) => AttrKey::Static(name),
            None => AttrKey::Shared(intern(name)),
        }
    }

    #[inline]
    pub fn as_str(&self) -> &str {
        match self {
            AttrKey::Static(name) => name,
            AttrKey::Shared(name) => name,
        }
    }

    /// True when the name resolved to the allocation-free static vocabulary.
    pub fn is_static(&self) -> bool {
        matches!(self, AttrKey::Static(_))
    }
}

impl Deref for AttrKey {
    type Target = str;

    #[inline]
    fn deref(&self) -> &str {
        self.as_str()
    }
}

impl Borrow<str> for AttrKey {
    #[inline]
    fn borrow(&self) -> &str {
        self.as_str()
    }
}

impl AsRef<str> for AttrKey {
    #[inline]
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl PartialEq for AttrKey {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        // Two keys built by `AttrKey::new` from the same name share one
        // vocabulary literal, so the common case answers on the pointer. The
        // byte comparison still runs as a fallback because `Static` is a public
        // variant anyone can build from their own `&'static str`.
        match (self, other) {
            (AttrKey::Static(left), AttrKey::Static(right)) => {
                std::ptr::eq(*left, *right) || left == right
            }
            _ => self.as_str() == other.as_str(),
        }
    }
}

impl Eq for AttrKey {}

impl PartialEq<str> for AttrKey {
    #[inline]
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}

impl PartialEq<&str> for AttrKey {
    #[inline]
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

impl PartialEq<AttrKey> for str {
    #[inline]
    fn eq(&self, other: &AttrKey) -> bool {
        self == other.as_str()
    }
}

impl PartialEq<AttrKey> for &str {
    #[inline]
    fn eq(&self, other: &AttrKey) -> bool {
        *self == other.as_str()
    }
}

impl PartialEq<String> for AttrKey {
    #[inline]
    fn eq(&self, other: &String) -> bool {
        self.as_str() == other.as_str()
    }
}

impl Ord for AttrKey {
    #[inline]
    fn cmp(&self, other: &Self) -> Ordering {
        self.as_str().cmp(other.as_str())
    }
}

impl PartialOrd for AttrKey {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Hash for AttrKey {
    #[inline]
    fn hash<H: Hasher>(&self, hasher: &mut H) {
        self.as_str().hash(hasher);
    }
}

impl fmt::Debug for AttrKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self.as_str(), formatter)
    }
}

impl fmt::Display for AttrKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl From<&str> for AttrKey {
    #[inline]
    fn from(name: &str) -> Self {
        AttrKey::new(name)
    }
}

impl From<&String> for AttrKey {
    #[inline]
    fn from(name: &String) -> Self {
        AttrKey::new(name.as_str())
    }
}

impl From<String> for AttrKey {
    #[inline]
    fn from(name: String) -> Self {
        AttrKey::new(name.as_str())
    }
}

impl From<&AttrKey> for AttrKey {
    #[inline]
    fn from(key: &AttrKey) -> Self {
        key.clone()
    }
}

impl From<AttrKey> for String {
    fn from(key: AttrKey) -> Self {
        key.as_str().to_string()
    }
}

impl serde::Serialize for AttrKey {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> serde::Deserialize<'de> for AttrKey {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let name = <std::borrow::Cow<'de, str> as serde::Deserialize>::deserialize(deserializer)?;
        Ok(AttrKey::new(&name))
    }
}

/// Per-thread cache for names outside the static vocabulary.
///
/// Attribute names come from template source, never from user data, so the
/// working set is tiny and bounded in practice. The cache is capped anyway so a
/// pathological module cannot grow it without limit; eviction only costs the
/// next lookup an allocation, never correctness.
const INTERN_CAPACITY: usize = 64;

thread_local! {
    static INTERNED: RefCell<Vec<Arc<str>>> = const { RefCell::new(Vec::new()) };
}

fn intern(name: &str) -> Arc<str> {
    INTERNED.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some(position) = cache.iter().position(|entry| &**entry == name) {
            // Most-recently-used: keep hot names at the front of the scan.
            let entry = cache.remove(position);
            cache.insert(0, entry.clone());
            return entry;
        }
        let entry: Arc<str> = Arc::from(name);
        if cache.len() == INTERN_CAPACITY {
            cache.pop();
        }
        cache.insert(0, entry.clone());
        entry
    })
}

/// Known attribute vocabulary.
///
/// Every name declared by an element contract must appear here — the
/// `well_known_covers_every_contract_attribute` test fails otherwise — plus the
/// runtime-internal `_mesh_*` keys and the annotations the shell writes.
///
/// The `match` lowers to a length switch over short literals rather than a
/// scan, which is why the table can grow without costing lookups.
fn well_known(name: &str) -> Option<&'static str> {
    Some(match name {
        // Common contract attributes.
        "align" => "align",
        "alt" => "alt",
        "anchor" => "anchor",
        "anchor-ref" => "anchor-ref",
        "aria-checked" => "aria-checked",
        "aria-description" => "aria-description",
        "aria-disabled" => "aria-disabled",
        "aria-expanded" => "aria-expanded",
        "aria-haspopup" => "aria-haspopup",
        "aria-hidden" => "aria-hidden",
        "aria-label" => "aria-label",
        "aria-role" => "aria-role",
        "busy" => "busy",
        "checked" => "checked",
        "class" => "class",
        "column" => "column",
        "column-span" => "column-span",
        "columns" => "columns",
        "command" => "command",
        "content" => "content",
        "data-benchmark" => "data-benchmark",
        "data-index" => "data-index",
        "data-mesh-element" => "data-mesh-element",
        "data-tooltip-disabled" => "data-tooltip-disabled",
        "default" => "default",
        "destructive" => "destructive",
        "disabled" => "disabled",
        "expanded" => "expanded",
        "for" => "for",
        "gap" => "gap",
        "gravity" => "gravity",
        "height" => "height",
        "hidden" => "hidden",
        "href" => "href",
        "id" => "id",
        "indeterminate" => "indeterminate",
        "invalid" => "invalid",
        "justify" => "justify",
        "key" => "key",
        "keybind" => "keybind",
        "label" => "label",
        "lang" => "lang",
        "layer" => "layer",
        "masked" => "masked",
        "max" => "max",
        "max-height" => "max-height",
        "max-width" => "max-width",
        "min" => "min",
        "min-height" => "min-height",
        "min-width" => "min-width",
        "multiline" => "multiline",
        "name" => "name",
        "offset-x" => "offset-x",
        "offset-y" => "offset-y",
        "open" => "open",
        "orient" => "orient",
        "overflow" => "overflow",
        "overflow-x" => "overflow-x",
        "overflow-y" => "overflow-y",
        "placeholder" => "placeholder",
        "pressed" => "pressed",
        "readonly" => "readonly",
        "ref" => "ref",
        "required" => "required",
        "role" => "role",
        "row" => "row",
        "row-span" => "row-span",
        "rows" => "rows",
        "scroll-x" => "scroll-x",
        "scroll-y" => "scroll-y",
        "selectable" => "selectable",
        "selected" => "selected",
        "size" => "size",
        "spacing" => "spacing",
        "src" => "src",
        "state" => "state",
        "step" => "step",
        "style" => "style",
        "title" => "title",
        "tooltip" => "tooltip",
        "tooltip-anchor" => "tooltip-anchor",
        "tooltip-for" => "tooltip-for",
        "type" => "type",
        "value" => "value",
        "variant" => "variant",
        "width" => "width",
        // Runtime-internal annotations.
        "_mesh_bind_this" => "_mesh_bind_this",
        "_mesh_content_height" => "_mesh_content_height",
        "_mesh_content_width" => "_mesh_content_width",
        "_mesh_error_placeholder" => "_mesh_error_placeholder",
        "_mesh_focused" => "_mesh_focused",
        "_mesh_key" => "_mesh_key",
        "_mesh_module_id" => "_mesh_module_id",
        "_mesh_promoted_popover" => "_mesh_promoted_popover",
        "_mesh_scroll_max_x" => "_mesh_scroll_max_x",
        "_mesh_scroll_max_y" => "_mesh_scroll_max_y",
        "_mesh_scroll_x" => "_mesh_scroll_x",
        "_mesh_scroll_y" => "_mesh_scroll_y",
        "_mesh_selection_anchor_x" => "_mesh_selection_anchor_x",
        "_mesh_selection_anchor_y" => "_mesh_selection_anchor_y",
        "_mesh_selection_background" => "_mesh_selection_background",
        "_mesh_selection_focus_x" => "_mesh_selection_focus_x",
        "_mesh_selection_focus_y" => "_mesh_selection_focus_y",
        "_mesh_selection_foreground" => "_mesh_selection_foreground",
        "_mesh_selection_text_x" => "_mesh_selection_text_x",
        "_mesh_selection_text_y" => "_mesh_selection_text_y",
        "_mesh_slot_source" => "_mesh_slot_source",
        "_mesh_source_file" => "_mesh_source_file",
        "_mesh_surface_entering" => "_mesh_surface_entering",
        "_mesh_surface_exiting" => "_mesh_surface_exiting",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::element::ELEMENT_CONTRACT_DEFS;
    use std::collections::BTreeMap;

    #[test]
    fn well_known_covers_every_contract_attribute() {
        let mut missing: Vec<&str> = Vec::new();
        for def in ELEMENT_CONTRACT_DEFS {
            for attribute in def.attributes {
                if !AttrKey::new(attribute.name).is_static() {
                    missing.push(attribute.name);
                }
            }
        }
        missing.sort_unstable();
        missing.dedup();
        assert!(
            missing.is_empty(),
            "element contract attributes missing from the interned vocabulary: {missing:?}"
        );
    }

    #[test]
    fn well_known_returns_the_matching_name() {
        for def in ELEMENT_CONTRACT_DEFS {
            for attribute in def.attributes {
                assert_eq!(AttrKey::new(attribute.name).as_str(), attribute.name);
            }
        }
    }

    #[test]
    fn unknown_names_share_one_allocation() {
        let first = AttrKey::new("data-not-in-the-vocabulary");
        let second = AttrKey::new("data-not-in-the-vocabulary");
        assert_eq!(first, second);
        assert_eq!(first.as_str(), "data-not-in-the-vocabulary");
        match (&first, &second) {
            (AttrKey::Shared(left), AttrKey::Shared(right)) => {
                assert!(Arc::ptr_eq(left, right), "repeat interning must share");
            }
            other => panic!("expected shared keys, got {other:?}"),
        }
    }

    #[test]
    fn intern_cache_stays_bounded_and_correct_after_eviction() {
        let names: Vec<String> = (0..INTERN_CAPACITY * 3)
            .map(|index| format!("data-generated-{index}"))
            .collect();
        for name in &names {
            assert_eq!(AttrKey::new(name).as_str(), name.as_str());
        }
        INTERNED.with(|cache| assert!(cache.borrow().len() <= INTERN_CAPACITY));
        // A name evicted by later inserts still resolves to the same contents.
        assert_eq!(AttrKey::new(&names[0]).as_str(), names[0].as_str());
    }

    #[test]
    fn ordering_matches_string_ordering() {
        let names = [
            "class",
            "id",
            "_mesh_key",
            "data-mesh-element",
            "aria-label",
            "zzz-unknown",
            "content",
        ];
        let interned: BTreeMap<AttrKey, u32> = names
            .iter()
            .enumerate()
            .map(|(index, name)| (AttrKey::new(name), index as u32))
            .collect();
        let owned: BTreeMap<String, u32> = names
            .iter()
            .enumerate()
            .map(|(index, name)| (name.to_string(), index as u32))
            .collect();
        let interned_order: Vec<(&str, u32)> = interned
            .iter()
            .map(|(key, value)| (key.as_str(), *value))
            .collect();
        let owned_order: Vec<(&str, u32)> = owned
            .iter()
            .map(|(key, value)| (key.as_str(), *value))
            .collect();
        assert_eq!(interned_order, owned_order);
    }

    /// The sorted-`Vec` map must be observationally identical to the
    /// `BTreeMap<String, String>` it replaced, including iteration order and
    /// the value returned by a replacing `insert`.
    #[test]
    fn matches_btreemap_semantics_under_random_operations() {
        let mut state = 0x2545_f491_4f6c_dd1du64;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        let names = [
            "class",
            "id",
            "style",
            "content",
            "_mesh_key",
            "data-mesh-element",
            "aria-label",
            "value",
            "zzz-custom-one",
            "zzz-custom-two",
        ];

        let mut map = AttributeMap::new();
        let mut reference: BTreeMap<String, String> = BTreeMap::new();
        for step in 0..5_000u64 {
            let name = names[(next() % names.len() as u64) as usize];
            match next() % 4 {
                0 | 1 => {
                    let value = format!("v{step}");
                    assert_eq!(
                        map.insert(AttrKey::new(name), value.clone()),
                        reference.insert(name.to_string(), value)
                    );
                }
                2 => assert_eq!(map.remove(name), reference.remove(name)),
                _ => {
                    assert_eq!(map.get(name), reference.get(name));
                    assert_eq!(map.contains_key(name), reference.contains_key(name));
                }
            }
            assert_eq!(map.len(), reference.len());
            assert_eq!(map.is_empty(), reference.is_empty());
        }

        let ordered: Vec<(&str, &str)> = map
            .iter()
            .map(|(key, value)| (key.as_str(), value.as_str()))
            .collect();
        let reference_ordered: Vec<(&str, &str)> = reference
            .iter()
            .map(|(key, value)| (key.as_str(), value.as_str()))
            .collect();
        assert_eq!(ordered, reference_ordered);
    }

    #[test]
    fn entry_inserts_only_when_vacant() {
        let mut map = AttributeMap::new();
        map.insert(AttrKey::new("class"), "panel".to_string());

        map.entry(AttrKey::new("class")).or_insert("other".into());
        assert_eq!(map.get("class").map(String::as_str), Some("panel"));

        map.entry(AttrKey::new("content"))
            .or_insert_with(|| "hello".to_string());
        assert_eq!(map.get("content").map(String::as_str), Some("hello"));

        map.entry(AttrKey::new("aria-label")).or_default();
        assert_eq!(map.get("aria-label").map(String::as_str), Some(""));

        // Insertion happened at the sorted slot, not appended.
        let keys: Vec<&str> = map.keys().map(AttrKey::as_str).collect();
        assert_eq!(keys, ["aria-label", "class", "content"]);

        map.entry(AttrKey::new("class")).or_default().push_str("-x");
        assert_eq!(map.get("class").map(String::as_str), Some("panel-x"));
    }

    #[test]
    fn serde_round_trips_through_a_json_map() {
        let mut map = AttributeMap::new();
        map.insert(AttrKey::new("class"), "panel".to_string());
        map.insert(AttrKey::new("zzz-custom"), "value".to_string());
        let json = serde_json::to_string(&map).unwrap();
        assert_eq!(json, r#"{"class":"panel","zzz-custom":"value"}"#);
        let restored: AttributeMap = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, map);
    }

    #[test]
    fn expression_values_remain_typed_until_a_string_consumer_reads_them() {
        let mut map = AttributeMap::new();
        map.insert_value(AttrKey::new("disabled"), serde_json::json!(true));
        map.insert_value(AttrKey::new("min"), serde_json::json!(1.5));
        map.insert_value(AttrKey::new("value"), serde_json::Value::Null);
        map.insert_value(
            AttrKey::new("zzz-custom"),
            serde_json::json!({"enabled": true}),
        );
        map.insert_value(AttrKey::new("data-items"), serde_json::json!([1, 2]));

        assert!(map.get_value("disabled").unwrap().legacy_bool());
        assert_eq!(map.get_value("min").unwrap().parse_f32(), Some(1.5));
        assert_eq!(
            map.get_value("value").unwrap().to_legacy_string(),
            String::new()
        );
        assert_eq!(
            map.get_value("zzz-custom").unwrap().to_legacy_string(),
            r#"{"enabled":true}"#
        );
        assert_eq!(
            map.get_value("data-items").unwrap().to_legacy_string(),
            "[1,2]"
        );

        for (_, value) in &map.entries {
            let StoredAttributeValue::Typed(value) = value else {
                panic!("non-string expression values should retain their type");
            };
            assert!(
                value.rendered.get().is_none(),
                "typed reads must not materialize the compatibility string"
            );
        }

        assert_eq!(map.get("disabled").map(String::as_str), Some("true"));
        let disabled = &map.entries[map.find("disabled").unwrap()].1;
        let StoredAttributeValue::Typed(disabled) = disabled else {
            panic!("immutable string reads should preserve the typed value");
        };
        assert_eq!(disabled.rendered.get().map(String::as_str), Some("true"));

        map.insert_value(AttrKey::new("label"), serde_json::json!("Ready"));
        assert!(map.get_value("label").unwrap().is_string());
        assert_eq!(map.get("label").map(String::as_str), Some("Ready"));

        map.get_mut("min").unwrap().push('0');
        assert!(map.get_value("min").unwrap().is_string());
        assert_eq!(map.get("min").map(String::as_str), Some("1.50"));
    }

    #[test]
    fn typed_and_legacy_attribute_maps_compare_and_serialize_identically() {
        let mut typed = AttributeMap::new();
        typed.insert_value(AttrKey::new("checked"), serde_json::json!(false));
        typed.insert_value(AttrKey::new("max"), serde_json::json!(42));
        typed.insert_value(AttrKey::new("value"), serde_json::Value::Null);

        let legacy = AttributeMap::from([
            (AttrKey::new("checked"), "false".to_owned()),
            (AttrKey::new("max"), "42".to_owned()),
            (AttrKey::new("value"), String::new()),
        ]);

        assert_eq!(typed, legacy);
        assert_eq!(
            serde_json::to_string(&typed).unwrap(),
            r#"{"checked":"false","max":"42","value":""}"#
        );
        assert_eq!(
            typed.into_iter().collect::<Vec<_>>(),
            legacy.into_iter().collect::<Vec<_>>()
        );
    }

    /// Representative per-node attribute sets from the shipped modules: a
    /// handful of short, well-known names built once per node per tree build.
    const BENCH_NODES: &[&[(&str, &str)]] = &[
        &[("data-mesh-element", "row")],
        &[("data-mesh-element", "column"), ("class", "entry")],
        &[
            ("data-mesh-element", "button"),
            ("class", "entry-action primary"),
            ("role", "button"),
        ],
        &[
            ("data-mesh-element", "text"),
            ("class", "entry-title"),
            ("content", "Entry 17"),
        ],
        &[
            ("data-mesh-element", "input"),
            ("class", "field"),
            ("type", "text"),
            ("value", "hello"),
            ("placeholder", "Search"),
            ("_mesh_key", "root/2/1"),
        ],
    ];

    // cargo test -p mesh-core-elements --release -- interned_attribute_map_beats_owned_btreemap --ignored --nocapture
    #[test]
    #[ignore = "release-only attribute-map construction microbenchmark"]
    fn interned_attribute_map_beats_owned_btreemap() {
        use std::time::Instant;

        // A tree's worth of maps is built and held before any of it is
        // released, which is what makes the key allocations cost more than a
        // tcache round-trip. Building and dropping one map at a time measures
        // the allocator's fast path, not the widget-tree build.
        const NODES_PER_TREE: usize = 456;
        const TREES: usize = 900;

        // The previous representation: an owned `String` key per attribute per
        // node, stored in a B-tree.
        fn owned_build(source: &[(&str, &str)]) -> BTreeMap<String, String> {
            let mut map = BTreeMap::new();
            for (name, value) in source {
                map.insert(name.to_string(), value.to_string());
            }
            map
        }

        fn interned_build(source: &[(&str, &str)]) -> AttributeMap {
            let mut map = AttributeMap::with_capacity(source.len());
            for (name, value) in source {
                map.insert(AttrKey::new(name), value.to_string());
            }
            map
        }

        // Parity first: the two representations must agree key for key, in the
        // same order, before either timing means anything.
        for source in BENCH_NODES {
            let owned = owned_build(source);
            let interned = interned_build(source);
            let owned_pairs: Vec<(&str, &str)> = owned
                .iter()
                .map(|(key, value)| (key.as_str(), value.as_str()))
                .collect();
            let interned_pairs: Vec<(&str, &str)> = interned
                .iter()
                .map(|(key, value)| (key.as_str(), value.as_str()))
                .collect();
            assert_eq!(owned_pairs, interned_pairs);
        }

        let mut owned_total = 0usize;
        let mut owned_tree: Vec<BTreeMap<String, String>> = Vec::with_capacity(NODES_PER_TREE);
        let owned_started = Instant::now();
        for _ in 0..TREES {
            owned_tree.clear();
            for index in 0..NODES_PER_TREE {
                let source = std::hint::black_box(BENCH_NODES[index % BENCH_NODES.len()]);
                owned_tree.push(owned_build(source));
            }
            owned_total += owned_tree
                .iter()
                .map(|map| map.len() + map.get("class").map_or(0, String::len))
                .sum::<usize>();
        }
        let owned = owned_started.elapsed();

        let mut interned_total = 0usize;
        let mut interned_tree: Vec<AttributeMap> = Vec::with_capacity(NODES_PER_TREE);
        let interned_started = Instant::now();
        for _ in 0..TREES {
            interned_tree.clear();
            for index in 0..NODES_PER_TREE {
                let source = std::hint::black_box(BENCH_NODES[index % BENCH_NODES.len()]);
                interned_tree.push(interned_build(source));
            }
            interned_total += interned_tree
                .iter()
                .map(|map| map.len() + map.get("class").map_or(0, String::len))
                .sum::<usize>();
        }
        let interned = interned_started.elapsed();

        assert_eq!(owned_total, interned_total);
        eprintln!(
            "attribute maps for {TREES} trees of {NODES_PER_TREE} nodes: owned String keys in a BTreeMap {owned:?}, interned keys in a sorted vec {interned:?}, ratio {:.2}x",
            owned.as_secs_f64() / interned.as_secs_f64()
        );
        println!(
            "MESH_PERF metric=interned_attribute_map_speedup value={:.6}",
            owned.as_secs_f64() / interned.as_secs_f64()
        );
    }

    /// `Static` is a public variant, so equality must not rest on every static
    /// key coming from the vocabulary table.
    #[test]
    fn independently_constructed_static_keys_compare_by_contents() {
        let interned = AttrKey::new("class");
        let hand_built = AttrKey::Static(String::from("class").leak());
        assert_eq!(interned, hand_built);
        assert_eq!(interned.cmp(&hand_built), Ordering::Equal);

        let mut map = AttributeMap::new();
        map.insert(hand_built, "panel".to_string());
        assert_eq!(map.get("class").map(String::as_str), Some("panel"));
        assert_eq!(
            map.insert(interned, "other".to_string()).as_deref(),
            Some("panel"),
            "the two keys must address the same slot"
        );
        assert_eq!(map.len(), 1);
    }

    #[test]
    fn borrowed_str_lookup_works() {
        let mut map: AttributeMap = AttributeMap::new();
        map.insert(AttrKey::new("class"), "panel".to_string());
        map.insert(AttrKey::new("data-custom"), "value".to_string());
        assert_eq!(map.get("class").map(String::as_str), Some("panel"));
        assert_eq!(map.get("data-custom").map(String::as_str), Some("value"));
        assert!(map.contains_key("class"));
        assert_eq!(map.remove("class").as_deref(), Some("panel"));
        assert!(!map.contains_key("class"));
    }
}
