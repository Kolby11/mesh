use mesh_core_elements::VariableStore;
use mesh_core_locale::LocaleEngine;
use serde_json::Value;
use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, Mutex};

/// Reactive state exposed to and mutated by Luau scripts. A script write marks
/// the state dirty, which is how the UI layer knows to rebuild the widget tree.
pub struct ScriptState {
    pub(super) variables: HashMap<String, Arc<Value>>,
    pub(super) dirty: bool,
    // Forward get/set to external sources, so the host can expose imported
    // component variables as if they lived in this namespace.
    proxies: HashMap<String, Proxy>,
    host_value_fingerprints: HashMap<String, u64>,
    /// Advances whenever a variable actually changes, so callers can skip
    /// re-serialization when state is provably unchanged since the last flush.
    snapshot_generation: u64,
    cached_snapshot: Mutex<Option<(u64, Value)>>,
    /// Advances on every observable mutation, including the host-value writes
    /// and proxy registrations that deliberately leave `snapshot_generation`
    /// alone, so callers can cache full-state clones safely.
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

    /// Proxy getters can change without going through `set()`, so callers that
    /// skip work on `snapshot_generation` must always refresh when this is true.
    pub fn has_proxies(&self) -> bool {
        !self.proxies.is_empty()
    }

    /// Advances on every mutation of any kind. A cached clone stays valid while
    /// this is unchanged: `Clone` does not carry proxies, so live proxy reads
    /// cannot invalidate it.
    pub fn mutation_generation(&self) -> u64 {
        self.mutation_generation
    }

    /// Set a variable and mark state as dirty.
    pub fn set(&mut self, name: impl Into<String>, value: Value) {
        let name = name.into();
        // Forward to the proxy's setter when there is one; a read-only proxy
        // falls back to storing locally.
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

    /// Set a host-produced reactive value using its precomputed fingerprint.
    /// Unchanged values are rejected before cloning large JSON payloads or
    /// running a recursive equality comparison.
    pub fn set_with_fingerprint(&mut self, name: &str, value: &Value, fingerprint: u64) {
        if let Some(proxy) = self.proxies.get(name)
            && let Some(setter) = &proxy.setter
        {
            (setter)(value.clone());
            return;
        }
        if self
            .host_value_fingerprints
            .get(name)
            .is_some_and(|previous| *previous == fingerprint)
        {
            return;
        }

        self.variables
            .insert(name.to_owned(), Arc::new(value.clone()));
        self.host_value_fingerprints
            .insert(name.to_owned(), fingerprint);
        self.dirty = true;
        self.snapshot_generation = self.snapshot_generation.wrapping_add(1);
        self.mutation_generation = self.mutation_generation.wrapping_add(1);
        self.cached_snapshot
            .get_mut()
            .expect("snapshot cache poisoned")
            .take();
    }

    /// Install a host-produced value the owning runtime has been observed not
    /// to read, keeping it correct for a later first read without claiming the
    /// runtime changed.
    ///
    /// Service capabilities are declared per module, so every component
    /// instance of that module is handed every payload the module may read —
    /// including instances whose template and script never mention the
    /// service. Treating those writes as mutations marks the instance dirty
    /// and advances the generation its render memoization keys on, so an
    /// unrelated 1 Hz service poll re-instantiates subtrees that cannot have
    /// changed. The value is still installed; only the reactivity is withheld.
    pub fn set_unobserved_value_with_fingerprint(
        &mut self,
        name: &str,
        value: &Value,
        fingerprint: u64,
    ) {
        if let Some(proxy) = self.proxies.get(name)
            && let Some(setter) = &proxy.setter
        {
            (setter)(value.clone());
            return;
        }
        if self
            .host_value_fingerprints
            .get(name)
            .is_some_and(|previous| *previous == fingerprint)
        {
            return;
        }

        self.variables
            .insert(name.to_owned(), Arc::new(value.clone()));
        self.host_value_fingerprints
            .insert(name.to_owned(), fingerprint);
        self.cached_snapshot
            .get_mut()
            .expect("snapshot cache poisoned")
            .take();
    }

    /// Lazily construct a host-produced reactive value only when its
    /// fingerprint differs from the installed value. Returns whether the value
    /// was installed, so hosts can mirror a changed value elsewhere (into a
    /// Luau `_ENV`, say) without repeating the fingerprint comparison.
    pub fn set_with_fingerprint_lazy(
        &mut self,
        name: &str,
        fingerprint: u64,
        value: impl FnOnce() -> Value,
    ) -> bool {
        if let Some(proxy) = self.proxies.get(name)
            && let Some(setter) = &proxy.setter
        {
            (setter)(value());
            return true;
        }
        if self
            .host_value_fingerprints
            .get(name)
            .is_some_and(|previous| *previous == fingerprint)
        {
            return false;
        }

        self.variables.insert(name.to_owned(), Arc::new(value()));
        self.host_value_fingerprints
            .insert(name.to_owned(), fingerprint);
        self.dirty = true;
        self.snapshot_generation = self.snapshot_generation.wrapping_add(1);
        self.mutation_generation = self.mutation_generation.wrapping_add(1);
        self.cached_snapshot
            .get_mut()
            .expect("snapshot cache poisoned")
            .take();
        true
    }

    /// Set a host-maintained variable without requesting a rebuild — for
    /// render-derived values like layout metrics, which scripts can see but
    /// which must not themselves cause a repaint.
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
        self.set_host_shared_value_with_fingerprint(name, Arc::new(value), fingerprint);
    }

    /// Set a shared host-maintained value without cloning its JSON tree.
    pub fn set_host_shared_value_with_fingerprint(
        &mut self,
        name: impl Into<String>,
        value: Arc<Value>,
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
        self.variables.insert(name.clone(), value);
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
        // Proxies may shadow local variables.
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
