use mesh_core_elements::VariableStore;
use mesh_core_locale::LocaleEngine;
use serde_json::Value;
use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, Mutex};

/// Reactive state exposed to and mutated by Luau scripts.
///
/// When a script sets a variable, the state is marked dirty.
/// The UI layer checks this flag to know when to rebuild the widget tree.
pub struct ScriptState {
    pub(super) variables: HashMap<String, Arc<Value>>,
    pub(super) dirty: bool,
    // Optional proxies that forward get/set to external sources (used by the
    // host to expose imported component variables as if they lived in the
    // same namespace). The getter is invoked on reads; the setter, if
    // provided, is invoked on writes from scripts.
    proxies: HashMap<String, Proxy>,
    host_value_fingerprints: HashMap<String, u64>,
    /// Monotonically increasing counter — incremented whenever a variable
    /// actually changes. Used by callers to skip expensive re-serialization
    /// when state is provably unchanged since the last flush.
    snapshot_generation: u64,
    cached_snapshot: Mutex<Option<(u64, Value)>>,
    /// Advances on every observable mutation, including host-value writes and
    /// proxy registration changes that deliberately do not advance
    /// `snapshot_generation`. Lets callers cache full-state clones and skip
    /// re-cloning when nothing could have changed.
    mutation_generation: u64,
}

impl ScriptState {
    pub fn new() -> Self {
        Self {
            variables: HashMap::new(),
            dirty: false,
            proxies: HashMap::new(),
            host_value_fingerprints: HashMap::new(),
            snapshot_generation: 0,
            cached_snapshot: Mutex::new(None),
            mutation_generation: 0,
        }
    }

    /// Returns a value that advances whenever any variable changes. Callers
    /// can cache this and skip work when it matches the last seen value.
    pub fn snapshot_generation(&self) -> u64 {
        self.snapshot_generation
    }

    /// Returns true if any proxy is registered on this state. Proxy getters
    /// are called by external sources and can change without going through
    /// `set()`, so callers that use `snapshot_generation` for skip logic must
    /// also check this and always refresh when proxies are present.
    pub fn has_proxies(&self) -> bool {
        !self.proxies.is_empty()
    }

    /// Counter that advances on every mutation of any kind (`set`,
    /// `set_host_value`, proxy register/unregister). Cached clones of this
    /// state are valid as long as this value is unchanged: proxies are not
    /// carried by `Clone`, so live proxy reads cannot invalidate a clone.
    pub fn mutation_generation(&self) -> u64 {
        self.mutation_generation
    }

    /// Set a variable and mark state as dirty.
    pub fn set(&mut self, name: impl Into<String>, value: Value) {
        let name = name.into();
        // If a proxy is registered for this name and exposes a setter,
        // forward the write to the proxy instead of storing locally. If the
        // proxy is read-only, fall back to storing locally.
        if let Some(proxy) = self.proxies.get(&name) {
            if let Some(setter) = &proxy.setter {
                (setter)(value);
                return;
            }
        }

        if self
            .variables
            .get(&name)
            .is_some_and(|previous| reactive_values_equal(previous.as_ref(), &value))
        {
            return;
        }
        self.host_value_fingerprints.remove(&name);
        self.variables.insert(name, Arc::new(value));
        self.dirty = true;
        self.snapshot_generation = self.snapshot_generation.wrapping_add(1);
        self.mutation_generation = self.mutation_generation.wrapping_add(1);
        self.cached_snapshot
            .get_mut()
            .expect("snapshot cache poisoned")
            .take();
    }

    /// Set a host-maintained variable without requesting a component rebuild.
    ///
    /// Used for render-derived values, such as element layout metrics, that
    /// should be visible to scripts but should not themselves cause a repaint.
    pub fn set_host_value(&mut self, name: impl Into<String>, value: Value) {
        let name = name.into();
        if self
            .variables
            .get(&name)
            .is_some_and(|previous| reactive_values_equal(previous.as_ref(), &value))
        {
            return;
        }
        self.host_value_fingerprints.remove(&name);
        self.variables.insert(name, Arc::new(value));
        self.mutation_generation = self.mutation_generation.wrapping_add(1);
        self.cached_snapshot
            .get_mut()
            .expect("snapshot cache poisoned")
            .take();
    }

    /// Set a large host-maintained variable using a producer-computed
    /// fingerprint to skip the previous-value deep JSON comparison when the
    /// producer knows the snapshot is unchanged.
    pub fn set_host_value_with_fingerprint(
        &mut self,
        name: impl Into<String>,
        value: Value,
        fingerprint: u64,
    ) {
        let name = name.into();
        if self
            .host_value_fingerprints
            .get(&name)
            .is_some_and(|previous| *previous == fingerprint)
        {
            return;
        }
        self.variables.insert(name.clone(), Arc::new(value));
        self.host_value_fingerprints.insert(name, fingerprint);
        self.mutation_generation = self.mutation_generation.wrapping_add(1);
        self.cached_snapshot
            .get_mut()
            .expect("snapshot cache poisoned")
            .take();
    }

    /// Check if any variable changed since last tree build.
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Reset the dirty flag after tree rebuild.
    pub fn clear_dirty(&mut self) {
        self.dirty = false;
    }

    /// Register or replace a proxy for a variable name.
    pub fn register_proxy(
        &mut self,
        name: impl Into<String>,
        getter: Box<dyn Fn() -> Value + Send + Sync + 'static>,
        setter: Option<Box<dyn Fn(Value) + Send + Sync + 'static>>,
    ) {
        let name = name.into();
        self.proxies.insert(name, Proxy { getter, setter });
        self.snapshot_generation = self.snapshot_generation.wrapping_add(1);
        self.mutation_generation = self.mutation_generation.wrapping_add(1);
        self.cached_snapshot
            .get_mut()
            .expect("snapshot cache poisoned")
            .take();
    }

    /// Remove a previously-registered proxy.
    pub fn unregister_proxy(&mut self, name: &str) {
        if self.proxies.remove(name).is_some() {
            self.snapshot_generation = self.snapshot_generation.wrapping_add(1);
            self.mutation_generation = self.mutation_generation.wrapping_add(1);
            self.cached_snapshot
                .get_mut()
                .expect("snapshot cache poisoned")
                .take();
        }
    }

    /// Check if a proxy exists for the given name.
    pub fn has_proxy(&self, name: &str) -> bool {
        self.proxies.contains_key(name)
    }

    #[cfg(test)]
    pub(super) fn value_arc_ptr(&self, name: &str) -> Option<*const Value> {
        self.variables.get(name).map(|value| Arc::as_ptr(value))
    }

    /// Return a JSON object snapshot of all visible state variables.
    pub fn snapshot(&self) -> Value {
        let snapshot = self.variable_snapshot();
        if self.proxies.is_empty() {
            return snapshot;
        }

        let mut object = match snapshot {
            Value::Object(object) => object,
            other => return other,
        };
        for (key, proxy) in &self.proxies {
            object.insert(key.clone(), (proxy.getter)());
        }
        Value::Object(object)
    }

    fn variable_snapshot(&self) -> Value {
        let generation = self.snapshot_generation;
        let mut cached = self
            .cached_snapshot
            .lock()
            .expect("snapshot cache poisoned");
        if let Some((cached_generation, cached_snapshot)) = cached.as_ref()
            && *cached_generation == generation
        {
            return cached_snapshot.clone();
        }

        let snapshot = Value::Object(
            self.variables
                .iter()
                .map(|(key, value)| (key.clone(), value.as_ref().clone()))
                .collect(),
        );
        *cached = Some((generation, snapshot.clone()));
        snapshot
    }
}

fn reactive_values_equal(previous: &Value, next: &Value) -> bool {
    previous == next
}

impl Default for ScriptState {
    fn default() -> Self {
        Self::new()
    }
}

impl VariableStore for ScriptState {
    fn get(&self, name: &str) -> Option<Value> {
        // If a proxy exists, call its getter, otherwise read from local
        // variables.
        if let Some(proxy) = self.proxies.get(name) {
            return Some((proxy.getter)());
        }
        self.variables.get(name).map(|value| value.as_ref().clone())
    }

    fn get_ref<'a>(&'a self, name: &str) -> Option<&'a Value> {
        if self.proxies.contains_key(name) {
            return None;
        }
        self.variables.get(name).map(Arc::as_ref)
    }

    fn keys(&self) -> Vec<String> {
        // Merge local variable keys with proxy keys. Proxies may shadow
        // local variables.
        let mut keys: Vec<String> = self.variables.keys().cloned().collect();
        for key in self.proxies.keys() {
            if !self.variables.contains_key(key) {
                keys.push(key.clone());
            }
        }
        keys
    }
}

// A lightweight proxy that forwards get/set operations to host-provided
// closures.
struct Proxy {
    getter: Box<dyn Fn() -> Value + Send + Sync + 'static>,
    setter: Option<Box<dyn Fn(Value) + Send + Sync + 'static>>,
}

impl Clone for ScriptState {
    fn clone(&self) -> Self {
        Self {
            variables: self.variables.clone(),
            dirty: self.dirty,
            proxies: HashMap::new(), // proxies are host-registered and not cloned
            host_value_fingerprints: self.host_value_fingerprints.clone(),
            snapshot_generation: self.snapshot_generation,
            cached_snapshot: Mutex::new(None),
            mutation_generation: self.mutation_generation,
        }
    }
}

impl fmt::Debug for ScriptState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ScriptState")
            .field("variables", &self.variables)
            .field("dirty", &self.dirty)
            .field("proxies_count", &self.proxies.len())
            .finish()
    }
}

/// A `VariableStore` that combines script state with locale engine access.
///
/// Pass this to `build_preview_tree_with_state` so that template expressions
/// like `{t("greeting")}` resolve through the active locale engine.
pub struct LocaleBoundState<'a> {
    state: &'a ScriptState,
    locale: &'a LocaleEngine,
}

impl<'a> LocaleBoundState<'a> {
    pub fn new(state: &'a ScriptState, locale: &'a LocaleEngine) -> Self {
        Self { state, locale }
    }
}

impl<'a> VariableStore for LocaleBoundState<'a> {
    fn get(&self, name: &str) -> Option<Value> {
        self.state.get(name)
    }

    fn get_ref<'b>(&'b self, name: &str) -> Option<&'b Value> {
        self.state.get_ref(name)
    }

    fn keys(&self) -> Vec<String> {
        self.state.keys()
    }

    fn translate(&self, key: &str) -> Option<String> {
        self.locale.translate(key).map(str::to_string)
    }
}
