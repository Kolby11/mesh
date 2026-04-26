use crate::host_api::InterfaceProxy;
/// Script execution context — one per plugin/component instance.
use mesh_capability::CapabilitySet;
use mesh_locale::LocaleEngine;
use mesh_service::{InterfaceCatalog, InterfaceResolution};
use mesh_ui::VariableStore;
use serde_json::Value;
use std::collections::HashMap;
// (no external sync types needed here)
use std::fmt;

#[derive(Debug, Clone)]
pub struct PublishedEvent {
    pub channel: String,
    pub payload: Value,
}

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

    #[error("interface unavailable: {0}")]
    InterfaceUnavailable(String),

    #[error("unsupported interface operation: {interface}.{method}")]
    UnsupportedOperation { interface: String, method: String },
}

/// Reactive state exposed to and mutated by Luau scripts.
///
/// When a script sets a variable, the state is marked dirty.
/// The UI layer checks this flag to know when to rebuild the widget tree.
pub struct ScriptState {
    variables: HashMap<String, Value>,
    dirty: bool,
    // Optional proxies that forward get/set to external sources (used by the
    // host to expose imported component variables as if they lived in the
    // same namespace). The getter is invoked on reads; the setter, if
    // provided, is invoked on writes from scripts.
    proxies: HashMap<String, Proxy>,
}

impl ScriptState {
    pub fn new() -> Self {
        Self {
            variables: HashMap::new(),
            dirty: false,
            proxies: HashMap::new(),
        }
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

        if self.variables.get(&name) == Some(&value) {
            return;
        }
        self.variables.insert(name, value);
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
        // If a proxy exists, call its getter, otherwise read from local
        // variables.
        if let Some(proxy) = self.proxies.get(name) {
            return Some((proxy.getter)());
        }
        self.variables.get(name).cloned()
    }

    fn keys(&self) -> Vec<String> {
        // Merge local variable keys with proxy keys. Proxies may shadow
        // local variables.
        let mut keys: Vec<String> = self.variables.keys().cloned().collect();
        for k in self.proxies.keys() {
            if !keys.contains(k) {
                keys.push(k.clone());
            }
        }
        keys
    }
}

// A lightweight proxy that forwards get/set operations to host-provided
// closures.
struct Proxy {
    getter: Box<dyn Fn() -> Value + Send + 'static>,
    setter: Option<Box<dyn Fn(Value) + Send + 'static>>,
}

impl Clone for ScriptState {
    fn clone(&self) -> Self {
        Self {
            variables: self.variables.clone(),
            dirty: self.dirty,
            proxies: HashMap::new(), // proxies are host-registered and not cloned
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

impl ScriptState {
    /// Register or replace a proxy for a variable name.
    pub fn register_proxy(
        &mut self,
        name: impl Into<String>,
        getter: Box<dyn Fn() -> Value + Send + 'static>,
        setter: Option<Box<dyn Fn(Value) + Send + 'static>>,
    ) {
        let name = name.into();
        self.proxies.insert(name, Proxy { getter, setter });
    }

    /// Remove a previously-registered proxy.
    pub fn unregister_proxy(&mut self, name: &str) {
        self.proxies.remove(name);
    }

    /// Check if a proxy exists for the given name.
    pub fn has_proxy(&self, name: &str) -> bool {
        self.proxies.contains_key(name)
    }
}

/// A script execution context for one component instance.
///
/// Owns the Luau VM, reactive state, and capability set.
/// When `mlua` is integrated, this will hold the `mlua::Lua` instance.
/// A reactive binding from a service field to a local script variable.
/// Declared in scripts as: `local x = mesh.service.bind("service.field")`
#[derive(Debug, Clone)]
struct ServiceBinding {
    var_name: String,
    service: String,
    field: String,
}

#[derive(Debug)]
pub struct ScriptContext {
    pub plugin_id: String,
    pub capabilities: CapabilitySet,
    pub state: ScriptState,
    handlers: HashMap<String, ScriptFunction>,
    interface_catalog: InterfaceCatalog,
    interface_bindings: HashMap<String, InterfaceResolution>,
    service_bindings: Vec<ServiceBinding>,
    published_events: Vec<PublishedEvent>,
}

#[derive(Debug, Clone)]
struct ScriptFunction {
    body: Vec<String>,
}

impl ScriptContext {
    /// Create a new script context for a plugin.
    pub fn new(
        plugin_id: impl Into<String>,
        capabilities: CapabilitySet,
    ) -> Result<Self, ScriptError> {
        Ok(Self {
            plugin_id: plugin_id.into(),
            capabilities,
            state: ScriptState::new(),
            handlers: HashMap::new(),
            interface_catalog: InterfaceCatalog::default(),
            interface_bindings: HashMap::new(),
            service_bindings: Vec::new(),
            published_events: Vec::new(),
        })
    }

    pub fn set_interface_catalog(&mut self, catalog: InterfaceCatalog) {
        self.interface_catalog = catalog;
    }

    /// Load a script source. Parses and registers functions, seeds state from
    /// top-level `local name = value` declarations, and registers service bindings.
    pub fn load_script(&mut self, source: &str) -> Result<(), ScriptError> {
        self.handlers = parse_functions(source);
        self.interface_bindings.clear();
        self.service_bindings = parse_service_bindings(source);
        for (name, value) in parse_top_level_locals(source) {
            self.state.variables.entry(name).or_insert(value);
        }
        tracing::info!("loaded script for plugin {}", self.plugin_id);
        Ok(())
    }

    /// Copy service payload fields into bound local state variables.
    ///
    /// Called by core after each service update, before `on_<service>_update()`.
    /// Enables scripts to read `muted`, `percent`, etc. as plain local variables
    /// rather than accessing `state["audio"]["muted"]` via dot notation.
    pub fn apply_service_bindings(&mut self, service: &str, payload: &Value) {
        let updates: Vec<(String, Value)> = self
            .service_bindings
            .iter()
            .filter(|b| b.service == service)
            .filter_map(|b| payload.get(&b.field).map(|v| (b.var_name.clone(), v.clone())))
            .collect();
        for (var, val) in updates {
            self.state.set(var, val);
        }
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

    pub fn drain_published_events(&mut self) -> Vec<PublishedEvent> {
        std::mem::take(&mut self.published_events)
    }

    fn execute_function(&mut self, name: &str) -> Result<(), ScriptError> {
        let Some(function) = self.handlers.get(name).cloned() else {
            return Err(ScriptError::HandlerNotFound(name.to_string()));
        };
        self.execute_lines(&function.body)
    }

    /// Execute a slice of Luau lines, handling if/then/else/end blocks.
    fn execute_lines(&mut self, lines: &[String]) -> Result<(), ScriptError> {
        let mut i = 0;
        while i < lines.len() {
            let trimmed = lines[i].trim().to_string();
            if trimmed.is_empty() || trimmed.starts_with("--") {
                i += 1;
                continue;
            }

            // if ... then ... [else ...] end
            if let Some(cond_expr) = trimmed
                .strip_prefix("if ")
                .and_then(|s| s.strip_suffix(" then"))
            {
                let (then_end, else_idx, end_idx) = find_if_block_bounds(lines, i + 1);
                let condition = self.eval_condition(cond_expr.trim());
                if condition {
                    let branch = lines[i + 1..then_end].to_vec();
                    self.execute_lines(&branch)?;
                } else if let Some(else_i) = else_idx {
                    let branch = lines[else_i + 1..end_idx].to_vec();
                    self.execute_lines(&branch)?;
                }
                i = end_idx + 1;
                continue;
            }

            self.execute_statement(&trimmed)?;
            i += 1;
        }
        Ok(())
    }

    /// Evaluate a Luau boolean condition against current state.
    ///
    /// Supports: `or`, `and`, `not`, `==`, `~=`, `<`, `<=`, `>`, `>=`,
    /// and plain variable truthy checks.
    fn eval_condition(&self, expr: &str) -> bool {
        let expr = expr.trim();

        // `or` — lowest precedence (split left-to-right)
        if let Some(pos) = expr.find(" or ") {
            return self.eval_condition(&expr[..pos]) || self.eval_condition(&expr[pos + 4..]);
        }

        // `and`
        if let Some(pos) = expr.find(" and ") {
            return self.eval_condition(&expr[..pos]) && self.eval_condition(&expr[pos + 5..]);
        }

        // `not x`
        if let Some(inner) = expr.strip_prefix("not ") {
            return !self.eval_condition(inner.trim());
        }

        // comparisons — check multi-char ops before single-char to avoid prefix matches
        for (op, op_len) in &[("==", 2usize), ("~=", 2), ("<=", 2), (">=", 2), ("<", 1), (">", 1)] {
            if let Some(pos) = expr.find(op) {
                let lhs = expr[..pos].trim();
                let rhs = expr[pos + op_len..].trim();
                if !lhs.is_empty() && !rhs.is_empty() {
                    return self.eval_comparison(lhs, op, rhs);
                }
            }
        }

        // plain truthy check
        let val = self
            .state
            .variables
            .get(expr)
            .cloned()
            .unwrap_or(Value::Bool(false));
        is_luau_truthy(&val)
    }

    fn eval_comparison(&self, lhs: &str, op: &str, rhs: &str) -> bool {
        let lval = self.state.variables.get(lhs).cloned().unwrap_or(Value::Null);

        if let Some(ln) = value_as_f64(&lval) {
            if let Ok(rn) = rhs.parse::<f64>() {
                return match op {
                    "==" => ln == rn,
                    "~=" => ln != rn,
                    "<" => ln < rn,
                    "<=" => ln <= rn,
                    ">" => ln > rn,
                    ">=" => ln >= rn,
                    _ => false,
                };
            }
        }

        let lstr = value_as_string(&lval);
        let rstr = rhs.trim_matches('"').trim_matches('\'');
        match op {
            "==" => lstr == rstr,
            "~=" => lstr != rstr,
            _ => false,
        }
    }

    fn execute_statement(&mut self, line: &str) -> Result<(), ScriptError> {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("--") {
            return Ok(());
        }

        if let Some((binding, interface, version)) = parse_interface_binding(trimmed) {
            let canonical = InterfaceProxy::canonical_name(&interface);
            if canonical.starts_with("mesh.")
                && !InterfaceProxy::can_read(&self.capabilities, &canonical)
            {
                return Err(ScriptError::CapabilityDenied(canonical));
            }

            let resolution = self
                .interface_catalog
                .resolve(&canonical, version.as_deref());
            if resolution.contract.is_none() || resolution.provider.is_none() {
                return Err(ScriptError::InterfaceUnavailable(format!(
                    "{}{}",
                    canonical,
                    version
                        .as_deref()
                        .map(|value| format!(" ({value})"))
                        .unwrap_or_default()
                )));
            }

            tracing::debug!(
                "{} bound interface alias '{}' -> {}{}",
                self.plugin_id,
                binding,
                canonical,
                version
                    .as_deref()
                    .map(|value| format!(" {value}"))
                    .unwrap_or_default()
            );
            self.interface_bindings.insert(binding, resolution);
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
                let payload = args
                    .get(1)
                    .and_then(|value| parse_event_payload(value))
                    .unwrap_or(Value::Null);
                tracing::info!("{} published event {}", self.plugin_id, channel);
                self.published_events
                    .push(PublishedEvent { channel, payload });
            }
            return Ok(());
        }

        if let Some((binding, method)) = parse_bound_method_call(trimmed, &self.interface_bindings)
        {
            let resolution = self
                .interface_bindings
                .get(binding)
                .expect("bound interface must exist");
            let contract = resolution
                .contract
                .as_ref()
                .ok_or_else(|| ScriptError::InterfaceUnavailable(resolution.requested.clone()))?;
            if !contract
                .methods
                .iter()
                .any(|candidate| candidate.name == method)
            {
                return Err(ScriptError::UnsupportedOperation {
                    interface: contract.interface.clone(),
                    method: method.to_string(),
                });
            }
            tracing::debug!(
                "{} invoked {} on interface alias '{}'",
                self.plugin_id,
                method,
                binding
            );
            return Ok(());
        }

        // Simple variable assignment: `name = expr` or `local name = expr`
        let decl = trimmed.strip_prefix("local ").unwrap_or(trimmed);
        if let Some((lhs, rhs)) = decl.split_once('=') {
            let lhs = lhs.trim();
            let rhs = rhs.trim();
            if is_simple_identifier(lhs) {
                // not expr
                if let Some(inner) = rhs.strip_prefix("not ") {
                    let current = self
                        .state
                        .variables
                        .get(inner.trim())
                        .cloned()
                        .unwrap_or(Value::Bool(false));
                    self.state.set(lhs, Value::Bool(!is_luau_truthy(&current)));
                    return Ok(());
                }
                // literal value
                if let Some(value) = parse_literal_value(rhs) {
                    self.state.set(lhs, value);
                    return Ok(());
                }
                // copy from another variable
                if is_simple_identifier(rhs) {
                    if let Some(value) = self.state.variables.get(rhs).cloned() {
                        self.state.set(lhs, value);
                        return Ok(());
                    }
                }
            }
        }

        Ok(())
    }
}

fn is_luau_truthy(value: &Value) -> bool {
    !matches!(value, Value::Bool(false) | Value::Null)
}

fn value_as_f64(v: &Value) -> Option<f64> {
    v.as_f64()
}

fn value_as_string(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Bool(b) => b.to_string(),
        Value::Null => "nil".to_string(),
        _ => v.to_string(),
    }
}

fn is_simple_identifier(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_alphanumeric() || c == '_')
}

/// Parse top-level `local name = literal` declarations (outside any function).
fn parse_top_level_locals(source: &str) -> Vec<(String, Value)> {
    let mut locals = Vec::new();
    let mut depth = 0usize;

    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("function ") {
            depth += 1;
            continue;
        }
        if trimmed == "end" {
            depth = depth.saturating_sub(1);
            continue;
        }
        if depth > 0 {
            continue;
        }
        // Top-level local declaration
        if let Some(rest) = trimmed.strip_prefix("local ") {
            if let Some((name, value_str)) = rest.split_once('=') {
                let name = name.trim();
                let value_str = value_str.trim();
                // Skip interface/service bindings — handled separately
                if value_str.contains("mesh.interfaces.get")
                    || value_str.contains("mesh.services.get")
                    || value_str.contains("mesh.service.bind")
                {
                    continue;
                }
                if is_simple_identifier(name) {
                    if let Some(value) = parse_literal_value(value_str) {
                        locals.push((name.to_string(), value));
                    }
                }
            }
        }
    }

    locals
}

/// Parse top-level `local x = mesh.service.bind("service.field")` declarations.
/// These create reactive bindings: when the named service emits, `x` is updated
/// from the specified field before `on_<service>_update()` is called.
fn parse_service_bindings(source: &str) -> Vec<ServiceBinding> {
    let mut bindings = Vec::new();
    let mut depth = 0usize;

    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("function ") {
            depth += 1;
            continue;
        }
        if trimmed == "end" {
            depth = depth.saturating_sub(1);
            continue;
        }
        if depth > 0 {
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("local ") {
            if let Some((var_name, expr)) = rest.split_once('=') {
                let var_name = var_name.trim();
                let expr = expr.trim();
                if let Some(args) = extract_call_args(expr, "mesh.service.bind") {
                    if let Some(path) = args.first().and_then(|a| parse_string_literal(a)) {
                        if let Some((service, field)) = path.split_once('.') {
                            if is_simple_identifier(var_name) {
                                bindings.push(ServiceBinding {
                                    var_name: var_name.to_string(),
                                    service: service.to_string(),
                                    field: field.to_string(),
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    bindings
}

/// Find the bounds of an if/then/[else]/end block starting from `start`.
/// Returns (then_end, else_idx, end_idx).
fn find_if_block_bounds(lines: &[String], start: usize) -> (usize, Option<usize>, usize) {
    let mut depth = 1usize;
    let mut else_idx = None;
    let mut end_idx = start;

    while end_idx < lines.len() {
        let inner = lines[end_idx].trim();
        if inner.starts_with("if ") && inner.ends_with(" then") {
            depth += 1;
        } else if inner == "end" {
            depth -= 1;
            if depth == 0 {
                break;
            }
        } else if inner == "else" && depth == 1 {
            else_idx = Some(end_idx);
        }
        end_idx += 1;
    }

    let then_end = else_idx.unwrap_or(end_idx);
    (then_end, else_idx, end_idx)
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

fn parse_interface_binding(line: &str) -> Option<(String, String, Option<String>)> {
    let trimmed = line.trim();
    let assignment = trimmed.strip_prefix("local ")?;
    let (binding, expr) = assignment.split_once('=')?;
    let binding = binding.trim().to_string();
    let expr = expr.trim();

    let (args, _kind) = if let Some(args) = extract_call_args(expr, "mesh.interfaces.get") {
        (args, "interface")
    } else if let Some(args) = extract_call_args(expr, "mesh.services.get") {
        (args, "service")
    } else {
        return None;
    };

    let interface = parse_string_literal(args.first()?)?;
    let version = args.get(1).and_then(|value| parse_string_literal(value));
    Some((binding, interface, version))
}

fn parse_bound_method_call<'a>(
    line: &'a str,
    bindings: &HashMap<String, InterfaceResolution>,
) -> Option<(&'a str, &'a str)> {
    let trimmed = line.trim();
    let call_target = trimmed.split('(').next()?.trim();

    if let Some((binding, method)) = call_target.split_once(':') {
        if bindings.contains_key(binding.trim()) {
            return Some((binding.trim(), method.trim()));
        }
    }

    if let Some((binding, method)) = call_target.split_once('.') {
        if bindings.contains_key(binding.trim()) {
            return Some((binding.trim(), method.trim()));
        }
    }

    None
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

fn parse_event_payload(value: &str) -> Option<Value> {
    let trimmed = value.trim();
    if trimmed == "{}" {
        return Some(Value::Object(serde_json::Map::new()));
    }
    if trimmed == "[]" {
        return Some(Value::Array(Vec::new()));
    }
    // Handle Luau table literal: { key = value, key2 = value2 }
    if trimmed.starts_with('{') && trimmed.ends_with('}') {
        let inner = &trimmed[1..trimmed.len() - 1];
        let mut map = serde_json::Map::new();
        for pair in inner.split(',') {
            let pair = pair.trim();
            if pair.is_empty() {
                continue;
            }
            if let Some(eq) = pair.find('=') {
                let key = pair[..eq].trim().trim_matches('"').trim_matches('\'');
                let val_str = pair[eq + 1..].trim();
                if let Some(val) = parse_literal_value(val_str) {
                    map.insert(key.to_string(), val);
                }
            }
        }
        if !map.is_empty() {
            return Some(Value::Object(map));
        }
    }
    parse_literal_value(trimmed)
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

    fn keys(&self) -> Vec<String> {
        self.state.keys()
    }

    fn translate(&self, key: &str) -> Option<String> {
        self.locale.translate(key).map(str::to_string)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mesh_capability::{Capability, CapabilitySet};
    use mesh_service::{
        ContractCapabilities, InterfaceCatalog, InterfaceContract, InterfaceMethod,
        InterfaceProvider, parse_contract_version,
    };
    use std::path::PathBuf;

    fn audio_catalog() -> InterfaceCatalog {
        let mut catalog = InterfaceCatalog::default();
        catalog.register_contract(InterfaceContract {
            interface: "mesh.audio".into(),
            version: parse_contract_version("1.0").unwrap(),
            file_path: PathBuf::from("<test>"),
            methods: vec![
                InterfaceMethod {
                    name: "default_output".into(),
                    args: Vec::new(),
                    returns: Some("Device?".into()),
                },
                InterfaceMethod {
                    name: "set_volume".into(),
                    args: Vec::new(),
                    returns: Some("Result".into()),
                },
            ],
            events: Vec::new(),
            types: HashMap::new(),
            capabilities: ContractCapabilities::default(),
        });
        catalog.register_provider(InterfaceProvider {
            interface: "mesh.audio".into(),
            version: Some("1.0".into()),
            provider_plugin: "@mesh/pipewire-audio".into(),
            backend_name: "PipeWire".into(),
            priority: 100,
        });
        catalog
    }

    #[test]
    fn binds_interface_aliases_from_new_api() {
        let mut caps = CapabilitySet::new();
        caps.grant(Capability::new("service.audio.read"));
        let mut ctx = ScriptContext::new("@mesh/test", caps).unwrap();
        ctx.set_interface_catalog(audio_catalog());
        ctx.load_script(
            r#"
function init()
    local audio = mesh.interfaces.get("mesh.audio", ">=1.0")
end
"#,
        )
        .unwrap();
        ctx.call_init().unwrap();

        assert_eq!(
            ctx.interface_bindings
                .get("audio")
                .map(|resolution| resolution.requested.as_str()),
            Some("mesh.audio")
        );
    }

    #[test]
    fn binds_interface_aliases_from_legacy_service_api() {
        let mut caps = CapabilitySet::new();
        caps.grant(Capability::new("service.audio.read"));

        let mut ctx = ScriptContext::new("@mesh/test", caps).unwrap();
        ctx.set_interface_catalog(audio_catalog());
        ctx.load_script(
            r#"
function init()
    local audio = mesh.services.get("audio")
end
"#,
        )
        .unwrap();
        ctx.call_init().unwrap();

        assert_eq!(
            ctx.interface_bindings
                .get("audio")
                .map(|resolution| resolution.requested.as_str()),
            Some("mesh.audio")
        );
    }

    #[test]
    fn rejects_missing_interface_contract() {
        let mut caps = CapabilitySet::new();
        caps.grant(Capability::new("service.audio.read"));
        let mut ctx = ScriptContext::new("@mesh/test", caps).unwrap();
        ctx.load_script(
            r#"
function init()
    local audio = mesh.interfaces.get("mesh.audio", ">=1.0")
end
"#,
        )
        .unwrap();

        let err = ctx.call_init().unwrap_err();
        assert!(matches!(err, ScriptError::InterfaceUnavailable(_)));
    }

    #[test]
    fn rejects_unsupported_interface_method() {
        let mut caps = CapabilitySet::new();
        caps.grant(Capability::new("service.audio.read"));
        let mut ctx = ScriptContext::new("@mesh/test", caps).unwrap();
        ctx.set_interface_catalog(audio_catalog());
        ctx.load_script(
            r#"
function init()
    local audio = mesh.interfaces.get("mesh.audio", ">=1.0")
    audio:mute_all()
end
"#,
        )
        .unwrap();

        let err = ctx.call_init().unwrap_err();
        assert!(matches!(
            err,
            ScriptError::UnsupportedOperation { interface, method }
            if interface == "mesh.audio" && method == "mute_all"
        ));
    }

    #[test]
    fn top_level_locals_seed_state() {
        let caps = CapabilitySet::new();
        let mut ctx = ScriptContext::new("@test/local", caps).unwrap();
        ctx.load_script(
            r#"
local volumeHidden = true
local count = 0

function toggle()
    volumeHidden = not volumeHidden
end
"#,
        )
        .unwrap();

        // Initial state from top-level locals
        assert_eq!(ctx.state.get("volumeHidden"), Some(Value::Bool(true)));
        assert_eq!(ctx.state.get("count"), Some(Value::Number(0.into())));

        // Toggle negates the boolean
        ctx.call_handler("toggle", &[]).unwrap();
        assert_eq!(ctx.state.get("volumeHidden"), Some(Value::Bool(false)));

        ctx.call_handler("toggle", &[]).unwrap();
        assert_eq!(ctx.state.get("volumeHidden"), Some(Value::Bool(true)));
    }

    #[test]
    fn if_then_end_executes_conditionally() {
        let caps = CapabilitySet::new();
        let mut ctx = ScriptContext::new("@test/if", caps).unwrap();
        ctx.load_script(
            r#"
local a = true
local b = false

function run()
    a = not a
    if not a then
        b = true
    end
end
"#,
        )
        .unwrap();

        ctx.call_handler("run", &[]).unwrap();
        assert_eq!(ctx.state.get("a"), Some(Value::Bool(false)));
        assert_eq!(ctx.state.get("b"), Some(Value::Bool(true)));

        ctx.call_handler("run", &[]).unwrap();
        assert_eq!(ctx.state.get("a"), Some(Value::Bool(true)));
        // b stays true — the if branch doesn't reset it
        assert_eq!(ctx.state.get("b"), Some(Value::Bool(true)));
    }

    #[test]
    fn service_bindings_update_local_vars_and_trigger_handler() {
        let caps = CapabilitySet::new();
        let mut ctx = ScriptContext::new("@test/audio-widget", caps).unwrap();
        ctx.load_script(
            r#"
local icon_name = "audio-volume-muted"
local muted = mesh.service.bind("audio.muted")
local percent = mesh.service.bind("audio.percent")

function on_audio_update()
    if muted then
        icon_name = "audio-volume-muted"
    else
        if percent < 34 then
            icon_name = "audio-volume-low"
        else
            if percent < 67 then
                icon_name = "audio-volume-medium"
            else
                icon_name = "audio-volume-high"
            end
        end
    end
end
"#,
        )
        .unwrap();

        // Initial default
        assert_eq!(
            ctx.state.get("icon_name"),
            Some(Value::String("audio-volume-muted".into()))
        );

        // Simulate 65% volume, not muted
        let payload = serde_json::json!({ "percent": 65, "muted": false });
        ctx.apply_service_bindings("audio", &payload);
        ctx.call_handler("on_audio_update", &[]).unwrap();
        assert_eq!(
            ctx.state.get("icon_name"),
            Some(Value::String("audio-volume-medium".into()))
        );

        // High volume
        let payload = serde_json::json!({ "percent": 90, "muted": false });
        ctx.apply_service_bindings("audio", &payload);
        ctx.call_handler("on_audio_update", &[]).unwrap();
        assert_eq!(
            ctx.state.get("icon_name"),
            Some(Value::String("audio-volume-high".into()))
        );

        // Muted
        let payload = serde_json::json!({ "percent": 90, "muted": true });
        ctx.apply_service_bindings("audio", &payload);
        ctx.call_handler("on_audio_update", &[]).unwrap();
        assert_eq!(
            ctx.state.get("icon_name"),
            Some(Value::String("audio-volume-muted".into()))
        );
    }
}
