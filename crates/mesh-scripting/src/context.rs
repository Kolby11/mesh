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
    handlers: HashMap<String, ScriptFunction>,
}

#[derive(Debug, Clone)]
struct ScriptFunction {
    body: Vec<String>,
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
        self.handlers = parse_functions(source);
        tracing::info!("loaded script for plugin {}", self.plugin_id);
        Ok(())
    }

    /// Call the script's `init()` function if it exists.
    pub fn call_init(&mut self) -> Result<(), ScriptError> {
        if self.handlers.contains_key("init") {
            tracing::debug!("calling init() for {}", self.plugin_id);
            self.execute_function("init")?;
        }
        Ok(())
    }

    /// Call a named event handler.
    pub fn call_handler(&mut self, name: &str, _args: &[Value]) -> Result<(), ScriptError> {
        if !self.handlers.contains_key(name) {
            return Err(ScriptError::HandlerNotFound(name.to_string()));
        }
        tracing::debug!("calling handler {name}() for {}", self.plugin_id);
        self.execute_function(name)
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

    fn execute_function(&mut self, name: &str) -> Result<(), ScriptError> {
        let Some(function) = self.handlers.get(name).cloned() else {
            return Err(ScriptError::HandlerNotFound(name.to_string()));
        };

        for line in function.body {
            self.execute_statement(&line)?;
        }

        Ok(())
    }

    fn execute_statement(&mut self, line: &str) -> Result<(), ScriptError> {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("--") {
            return Ok(());
        }

        if let Some(args) = extract_call_args(trimmed, "mesh.state.set") {
            if args.len() != 2 {
                return Err(ScriptError::LuaError(format!(
                    "mesh.state.set expects 2 arguments in {}",
                    self.plugin_id
                )));
            }

            let key = parse_string_literal(&args[0]).ok_or_else(|| {
                ScriptError::LuaError(format!("invalid mesh.state.set key: {}", args[0]))
            })?;
            let value = parse_literal_value(&args[1]).ok_or_else(|| {
                ScriptError::LuaError(format!("invalid mesh.state.set value: {}", args[1]))
            })?;
            self.state.set(key, value);
            return Ok(());
        }

        if let Some(args) = extract_call_args(trimmed, "mesh.log.info") {
            if let Some(message) = args.first().and_then(|value| parse_string_literal(value)) {
                tracing::info!("{}: {}", self.plugin_id, message);
            }
            return Ok(());
        }

        if let Some(args) = extract_call_args(trimmed, "mesh.log.warn") {
            if let Some(message) = args.first().and_then(|value| parse_string_literal(value)) {
                tracing::warn!("{}: {}", self.plugin_id, message);
            }
            return Ok(());
        }

        if trimmed == "mesh.ui.request_redraw()" {
            self.state.dirty = true;
            return Ok(());
        }

        if let Some(args) = extract_call_args(trimmed, "mesh.events.publish") {
            if let Some(channel) = args.first().and_then(|value| parse_string_literal(value)) {
                tracing::info!("{} published event {}", self.plugin_id, channel);
            }
            return Ok(());
        }

        Ok(())
    }
}

fn parse_functions(source: &str) -> HashMap<String, ScriptFunction> {
    let mut functions = HashMap::new();
    let mut current_name: Option<String> = None;
    let mut current_body = Vec::new();

    for line in source.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("function ") {
            if let Some(name) = rest.split('(').next() {
                let name = name.trim();
                if !name.is_empty() {
                    current_name = Some(name.to_string());
                    current_body.clear();
                }
            }
            continue;
        }

        if trimmed == "end" {
            if let Some(name) = current_name.take() {
                tracing::debug!("registered handler: {name}");
                functions.insert(
                    name.clone(),
                    ScriptFunction {
                        body: std::mem::take(&mut current_body),
                    },
                );
            }
            continue;
        }

        if current_name.is_some() {
            current_body.push(trimmed.to_string());
        }
    }

    functions
}

fn extract_call_args(line: &str, prefix: &str) -> Option<Vec<String>> {
    let call = line.strip_prefix(prefix)?;
    let call = call.trim();
    let inner = call.strip_prefix('(')?.strip_suffix(')')?;
    Some(split_call_args(inner))
}

fn split_call_args(inner: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut in_string = false;
    let mut quote = '\0';

    for ch in inner.chars() {
        match ch {
            '"' | '\'' => {
                current.push(ch);
                if in_string && ch == quote {
                    in_string = false;
                    quote = '\0';
                } else if !in_string {
                    in_string = true;
                    quote = ch;
                }
            }
            ',' if !in_string => {
                if !current.trim().is_empty() {
                    args.push(current.trim().to_string());
                }
                current.clear();
            }
            _ => current.push(ch),
        }
    }

    if !current.trim().is_empty() {
        args.push(current.trim().to_string());
    }

    args
}

fn parse_string_literal(value: &str) -> Option<String> {
    let value = value.trim();
    if value.len() < 2 {
        return None;
    }

    let quote = value.chars().next()?;
    if (quote == '"' || quote == '\'') && value.ends_with(quote) {
        return Some(value[1..value.len() - 1].to_string());
    }

    None
}

fn parse_literal_value(value: &str) -> Option<Value> {
    if let Some(string) = parse_string_literal(value) {
        return Some(Value::String(string));
    }

    match value.trim() {
        "true" => Some(Value::Bool(true)),
        "false" => Some(Value::Bool(false)),
        "null" | "nil" => Some(Value::Null),
        other => {
            if let Ok(number) = other.parse::<i64>() {
                return Some(Value::Number(number.into()));
            }
            if let Ok(number) = other.parse::<f64>() {
                return serde_json::Number::from_f64(number).map(Value::Number);
            }
            None
        }
    }
}
