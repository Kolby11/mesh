/// Script execution context — one per plugin/component instance.
use mesh_capability::CapabilitySet;
use mesh_ui::VariableStore;
use serde_json::Value;
use std::collections::HashMap;

/// Errors from the scripting runtime.
#[derive(Debug, thiserror::Error)]
pub enum ScriptError {
    #[error("Luau error: {0}")]
    LuaError(String),

    #[error("script init failed: {0}")]
    InitFailed(String),

    #[error("handler not found: {0}")]
    HandlerNotFound(String),

    #[error("capability denied: {0}")]
    CapabilityDenied(String),

    #[error("script execution timed out")]
    Timeout,
}

/// Reactive state exposed to and mutated by Luau scripts.
///
/// When a script sets a variable, the state is marked dirty.
/// The UI layer checks this flag to know when to rebuild the widget tree.
#[derive(Debug, Clone)]
pub struct ScriptState {
    variables: HashMap<String, Value>,
    dirty: bool,
}

impl ScriptState {
    pub fn new() -> Self {
        Self {
            variables: HashMap::new(),
            dirty: false,
        }
    }

    /// Set a variable and mark state as dirty.
    pub fn set(&mut self, name: impl Into<String>, value: Value) {
        self.variables.insert(name.into(), value);
        self.dirty = true;
    }

    /// Check if any variable changed since last tree build.
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Reset the dirty flag after tree rebuild.
    pub fn clear_dirty(&mut self) {
        self.dirty = false;
    }
}

impl Default for ScriptState {
    fn default() -> Self {
        Self::new()
    }
}

impl VariableStore for ScriptState {
    fn get(&self, name: &str) -> Option<Value> {
        self.variables.get(name).cloned()
    }

    fn keys(&self) -> Vec<String> {
        self.variables.keys().cloned().collect()
    }
}

/// A script execution context for one component instance.
///
/// Owns the Luau VM, reactive state, and capability set.
/// When `mlua` is integrated, this will hold the `mlua::Lua` instance.
#[derive(Debug)]
pub struct ScriptContext {
    pub plugin_id: String,
    pub capabilities: CapabilitySet,
    pub state: ScriptState,
    handlers: HashMap<String, String>,
}

impl ScriptContext {
    /// Create a new script context for a plugin.
    pub fn new(plugin_id: impl Into<String>, capabilities: CapabilitySet) -> Result<Self, ScriptError> {
        Ok(Self {
            plugin_id: plugin_id.into(),
            capabilities,
            state: ScriptState::new(),
            handlers: HashMap::new(),
        })
    }

    /// Load a script source. Parses and registers functions.
    ///
    /// Stub: extracts function names from the source text.
    /// Real implementation will execute in the Luau VM.
    pub fn load_script(&mut self, source: &str) -> Result<(), ScriptError> {
        // Stub: scan for `function name()` declarations.
        for line in source.lines() {
            let trimmed = line.trim();
            if let Some(rest) = trimmed.strip_prefix("function ") {
                if let Some(name) = rest.split('(').next() {
                    let name = name.trim();
                    if !name.is_empty() {
                        self.handlers.insert(name.to_string(), name.to_string());
                        tracing::debug!("registered handler: {name}");
                    }
                }
            }
        }
        tracing::info!("loaded script for plugin {}", self.plugin_id);
        Ok(())
    }

    /// Call the script's `init()` function if it exists.
    pub fn call_init(&mut self) -> Result<(), ScriptError> {
        if self.handlers.contains_key("init") {
            tracing::debug!("calling init() for {}", self.plugin_id);
            // Stub: would execute init() in the Luau VM.
        }
        Ok(())
    }

    /// Call a named event handler.
    pub fn call_handler(&mut self, name: &str, _args: &[Value]) -> Result<(), ScriptError> {
        if !self.handlers.contains_key(name) {
            return Err(ScriptError::HandlerNotFound(name.to_string()));
        }
        tracing::debug!("calling handler {name}() for {}", self.plugin_id);
        // Stub: would execute the handler in the Luau VM.
        Ok(())
    }

    /// Check if a handler exists.
    pub fn has_handler(&self, name: &str) -> bool {
        self.handlers.contains_key(name)
    }

    /// Get a reference to the current state for tree building.
    pub fn state(&self) -> &ScriptState {
        &self.state
    }

    /// Get a mutable reference to state.
    pub fn state_mut(&mut self) -> &mut ScriptState {
        &mut self.state
    }
}
