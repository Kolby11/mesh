//! Attribute storage for the widget tree.
//!
//! Attribute *names* are template vocabulary, not data. [`AttrKey`] resolves
//! every name in the closed [`Prop`] vocabulary to its enum value and the rest
//! (free-form `data-*`, pass-through ARIA) to an `Arc<str>` from a per-thread
//! cache, while keeping `String`'s ordering. Core code addresses known
//! properties by [`Prop`]; the `*_by_name` methods are the adapter for callers
//! that only have a runtime name (bindings, selectors, inspection).

use std::borrow::Borrow;
use std::cell::RefCell;
use std::cmp::Ordering;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::ops::Deref;
use std::sync::{Arc, OnceLock};

use crate::prop::{ChangeClass, Prop};

/// Ordering, equality, and hashing delegate to the string contents, so this
/// iterates exactly as the equivalent `BTreeMap<String, _>` would.
#[derive(Clone)]
pub enum AttrKey {
    /// A name from the core vocabulary. Free to clone and compare.
    Known(Prop),
    /// Any other name, shared through the per-thread intern cache. Never a
    /// [`Prop`] name when built through [`AttrKey::new`].
    Shared(Arc<str>),
}

/// A borrowed attribute value preserving the type a template expression made.
#[derive(Clone, Copy)]
pub struct ResolvedAttributeValueRef<'a> {
    value: &'a StoredAttributeValue,
}

impl ResolvedAttributeValueRef<'_> {
    /// Matches the string-backed interpretation without formatting first.
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

    /// Matches the string parse numeric attribute consumers use.
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

    /// For consumers whose data model still requires owned text.
    pub fn to_legacy_string(self) -> String {
        match self.value {
            StoredAttributeValue::String(value) => value.clone(),
            StoredAttributeValue::Typed(value) => value_ref_to_string(&value.value),
        }
    }

    /// Return the resolved value while preserving its JSON scalar/structured type.
    pub fn to_json_value(self) -> serde_json::Value {
        match self.value {
            StoredAttributeValue::String(value) => serde_json::Value::String(value.clone()),
            StoredAttributeValue::Typed(value) => value.value.clone(),
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
/// A sorted `Vec`, not a `BTreeMap`: real elements carry a handful of
/// attributes, a set that fits in one cache-friendly run rather than a tree of
/// separately allocated node blocks. Iteration order is unchanged. Non-string
/// bindings keep their JSON type and materialize text only on demand.
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

    /// Real elements carry a handful of attributes, where comparing one-byte
    /// discriminants beats a binary search over string comparisons.
    #[inline]
    fn find_prop(&self, prop: Prop) -> Option<usize> {
        if self.entries.len() <= 8 {
            self.entries
                .iter()
                .position(|(key, _)| key.prop() == Some(prop))
        } else {
            self.find(prop.name()).ok()
        }
    }

    pub fn insert(&mut self, key: impl Into<AttrKey>, value: String) -> Option<String> {
        self.insert_stored(key.into(), StoredAttributeValue::String(value))
    }

    /// Insert without stringifying non-string JSON values.
    pub fn insert_value(
        &mut self,
        key: impl Into<AttrKey>,
        value: serde_json::Value,
    ) -> Option<String> {
        self.insert_stored(key.into(), StoredAttributeValue::from_json(value))
    }

    /// Write `value` and report the stages the write reaches: empty when the
    /// value is unchanged, the prop's [`ChangeClass`] otherwise, and
    /// [`ChangeClass::FREE_FORM`] for a name outside the vocabulary.
    pub fn set(&mut self, key: impl Into<AttrKey>, value: String) -> ChangeClass {
        self.set_stored(key.into(), StoredAttributeValue::String(value))
    }

    /// [`AttributeMap::set`] for a value that keeps its JSON type.
    pub fn set_value(&mut self, key: impl Into<AttrKey>, value: serde_json::Value) -> ChangeClass {
        self.set_stored(key.into(), StoredAttributeValue::from_json(value))
    }

    /// Remove `key`, reporting its change class when it was present.
    pub fn unset(&mut self, key: impl Into<AttrKey>) -> ChangeClass {
        let key = key.into();
        match self.find(key.as_str()) {
            Ok(index) => {
                self.entries.remove(index);
                key.change_class()
            }
            Err(_) => ChangeClass::empty(),
        }
    }

    fn set_stored(&mut self, key: AttrKey, value: StoredAttributeValue) -> ChangeClass {
        match self.find(key.as_str()) {
            Ok(index) if self.entries[index].1 == value => ChangeClass::empty(),
            Ok(index) => {
                self.entries[index].1 = value;
                key.change_class()
            }
            Err(index) => {
                let class = key.change_class();
                self.entries.insert(index, (key, value));
                class
            }
        }
    }

    fn insert_stored(&mut self, key: AttrKey, value: StoredAttributeValue) -> Option<String> {
        // Attributes normally arrive in key order; keeping that path
        // append-only avoids a binary search and a tail shift per insert.
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

    pub fn get(&self, prop: Prop) -> Option<&String> {
        self.find_prop(prop)
            .map(|index| self.entries[index].1.as_string())
    }

    pub fn get_value(&self, prop: Prop) -> Option<ResolvedAttributeValueRef<'_>> {
        self.find_prop(prop)
            .map(|index| self.entries[index].1.value_ref())
    }

    pub fn get_mut(&mut self, prop: Prop) -> Option<&mut String> {
        self.find_prop(prop)
            .map(|index| self.entries[index].1.as_string_mut())
    }

    pub fn contains_key(&self, prop: Prop) -> bool {
        self.find_prop(prop).is_some()
    }

    pub fn remove(&mut self, prop: Prop) -> Option<String> {
        self.find_prop(prop)
            .map(|index| self.entries.remove(index).1.into_string())
    }

    /// The value as text, without allocating for string-backed values.
    pub fn text(&self, prop: Prop) -> Option<&str> {
        self.get(prop).map(String::as_str)
    }

    /// A boolean attribute: present and empty, `true` or `1`.
    pub fn flag(&self, prop: Prop) -> bool {
        self.get_value(prop)
            .is_some_and(ResolvedAttributeValueRef::legacy_bool)
    }

    pub fn number(&self, prop: Prop) -> Option<f32> {
        self.get_value(prop)
            .and_then(ResolvedAttributeValueRef::parse_f32)
    }

    /// Lookup for a runtime name; prefer [`AttributeMap::get`] for a [`Prop`].
    pub fn get_by_name(&self, name: &str) -> Option<&String> {
        match Prop::from_name(name) {
            Some(prop) => self.get(prop),
            None => self
                .find(name)
                .ok()
                .map(|index| self.entries[index].1.as_string()),
        }
    }

    pub fn get_value_by_name(&self, name: &str) -> Option<ResolvedAttributeValueRef<'_>> {
        match Prop::from_name(name) {
            Some(prop) => self.get_value(prop),
            None => self
                .find(name)
                .ok()
                .map(|index| self.entries[index].1.value_ref()),
        }
    }

    pub fn contains_name(&self, name: &str) -> bool {
        self.find(name).is_ok()
    }

    pub fn remove_by_name(&mut self, name: &str) -> Option<String> {
        match self.find(name) {
            Ok(index) => Some(self.entries.remove(index).1.into_string()),
            Err(_) => None,
        }
    }

    pub fn entry(&mut self, key: impl Into<AttrKey>) -> Entry<'_> {
        let key = key.into();
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
    /// Allocation-free for known vocabulary.
    pub fn new(name: &str) -> Self {
        match Prop::from_name(name) {
            Some(prop) => AttrKey::Known(prop),
            None => AttrKey::Shared(intern(name)),
        }
    }

    #[inline]
    pub fn as_str(&self) -> &str {
        match self {
            AttrKey::Known(prop) => prop.name(),
            AttrKey::Shared(name) => name,
        }
    }

    #[inline]
    pub fn prop(&self) -> Option<Prop> {
        match self {
            AttrKey::Known(prop) => Some(*prop),
            AttrKey::Shared(_) => None,
        }
    }

    pub fn change_class(&self) -> ChangeClass {
        self.prop()
            .map_or(ChangeClass::FREE_FORM, Prop::change_class)
    }
}

impl From<Prop> for AttrKey {
    #[inline]
    fn from(prop: Prop) -> Self {
        AttrKey::Known(prop)
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
        match (self, other) {
            (AttrKey::Known(left), AttrKey::Known(right)) => left == right,
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

/// Per-thread cache for names outside the static vocabulary. Names come from
/// template source, never user data, so the working set is tiny; the cap only
/// guards against a pathological module, and eviction costs an allocation.
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

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
        assert_eq!(AttrKey::new(&names[0]).as_str(), names[0].as_str());
    }

    #[test]
    fn ordering_matches_string_ordering() {
        let names = [
            "class",
            "id",
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

    /// Must be observationally identical to the `BTreeMap` it replaced.
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
                2 => assert_eq!(map.remove_by_name(name), reference.remove(name)),
                _ => {
                    assert_eq!(map.get_by_name(name), reference.get(name));
                    assert_eq!(map.contains_name(name), reference.contains_key(name));
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
        assert_eq!(map.get(Prop::Class).map(String::as_str), Some("panel"));

        map.entry(AttrKey::new("content"))
            .or_insert_with(|| "hello".to_string());
        assert_eq!(map.get(Prop::Content).map(String::as_str), Some("hello"));

        map.entry(AttrKey::new("aria-label")).or_default();
        assert_eq!(map.get(Prop::AriaLabel).map(String::as_str), Some(""));

        let keys: Vec<&str> = map.keys().map(AttrKey::as_str).collect();
        assert_eq!(keys, ["aria-label", "class", "content"]);

        map.entry(AttrKey::new("class")).or_default().push_str("-x");
        assert_eq!(map.get(Prop::Class).map(String::as_str), Some("panel-x"));
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

        assert!(map.get_value(Prop::Disabled).unwrap().legacy_bool());
        assert_eq!(map.get_value(Prop::Min).unwrap().parse_f32(), Some(1.5));
        assert_eq!(
            map.get_value(Prop::Value).unwrap().to_legacy_string(),
            String::new()
        );
        assert_eq!(
            map.get_value_by_name("zzz-custom")
                .unwrap()
                .to_legacy_string(),
            r#"{"enabled":true}"#
        );
        assert_eq!(
            map.get_value_by_name("data-items")
                .unwrap()
                .to_legacy_string(),
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

        assert_eq!(map.get(Prop::Disabled).map(String::as_str), Some("true"));
        let disabled = &map.entries[map.find("disabled").unwrap()].1;
        let StoredAttributeValue::Typed(disabled) = disabled else {
            panic!("immutable string reads should preserve the typed value");
        };
        assert_eq!(disabled.rendered.get().map(String::as_str), Some("true"));

        map.insert_value(AttrKey::new("label"), serde_json::json!("Ready"));
        assert!(map.get_value(Prop::Label).unwrap().is_string());
        assert_eq!(map.get(Prop::Label).map(String::as_str), Some("Ready"));

        map.get_mut(Prop::Min).unwrap().push('0');
        assert!(map.get_value(Prop::Min).unwrap().is_string());
        assert_eq!(map.get(Prop::Min).map(String::as_str), Some("1.50"));
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

    #[test]
    fn set_reports_the_change_class_only_for_real_changes() {
        let mut map = AttributeMap::new();
        assert_eq!(
            map.set(Prop::Content, "a".into()),
            Prop::Content.change_class()
        );
        assert_eq!(map.set(Prop::Content, "a".into()), ChangeClass::empty());
        assert_eq!(map.set(Prop::Class, "x".into()), ChangeClass::STYLE);
        assert_eq!(
            map.set_value(Prop::Checked, serde_json::json!(true)),
            Prop::Checked.change_class()
        );
        assert_eq!(
            map.set(Prop::Checked, "true".into()),
            ChangeClass::empty(),
            "equal text forms are the same value"
        );
        assert_eq!(map.set("data-custom", "1".into()), ChangeClass::FREE_FORM);
        assert_eq!(map.unset(Prop::Class), ChangeClass::STYLE);
        assert_eq!(map.unset(Prop::Class), ChangeClass::empty());
        assert!(map.flag(Prop::Checked));
        assert_eq!(map.text(Prop::Content), Some("a"));
    }

    #[test]
    fn prop_and_name_lookups_agree_past_the_linear_scan_threshold() {
        let mut map = AttributeMap::new();
        for (index, prop) in Prop::ALL.iter().take(20).enumerate() {
            map.insert(*prop, index.to_string());
        }
        map.insert("data-custom", "free".into());
        for prop in Prop::ALL.iter().take(20) {
            assert_eq!(map.get(*prop), map.get_by_name(prop.name()));
            assert!(matches!(AttrKey::new(prop.name()), AttrKey::Known(known) if known == *prop));
        }
        assert_eq!(
            map.get_by_name("data-custom").map(String::as_str),
            Some("free")
        );
        assert_eq!(map.number(Prop::ALL[3]), Some(3.0));
    }

    #[test]
    fn borrowed_str_lookup_works() {
        let mut map: AttributeMap = AttributeMap::new();
        map.insert(AttrKey::new("class"), "panel".to_string());
        map.insert(AttrKey::new("data-custom"), "value".to_string());
        assert_eq!(map.get(Prop::Class).map(String::as_str), Some("panel"));
        assert_eq!(
            map.get_by_name("data-custom").map(String::as_str),
            Some("value")
        );
        assert!(map.contains_key(Prop::Class));
        assert_eq!(map.remove(Prop::Class).as_deref(), Some("panel"));
        assert!(!map.contains_key(Prop::Class));
    }
}
