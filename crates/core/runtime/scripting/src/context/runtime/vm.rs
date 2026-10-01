use super::super::element_ref::ElementMetricsStore;
use crate::policy::RuntimePolicy;
use crate::pool;
use mesh_core_service::InterfaceResolution;
use mlua::{Function, Lua, Table};
use serde_json::Value;
use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex};

#[derive(Debug, Default)]
pub(super) struct SharedInterfaceBindings {
    pub(super) bindings: HashMap<String, InterfaceResolution>,
    pub(super) generation: u64,
}

#[derive(Debug, Default)]
pub(super) struct TemplateExpressionCache {
    pub(super) template_member_reads: HashSet<String>,
    pub(super) member_reads: HashMap<String, Vec<String>>,
    pub(super) values: HashMap<String, Value>,
    /// Expressions that call a component function. Their reads happen inside
    /// the function, so any public change may affect them.
    pub(super) opaque_expressions: HashSet<String>,
    pub(super) hits: u64,
}

/// Append each template expression to the component source as a closure, so
/// it resolves the component's lexical locals. The host later binds each
/// closure's environment; see [`install_template_expressions`].
pub(super) fn component_source_with_compiled_template_expressions(
    source: &str,
    expressions: &[mesh_core_expression::SharedCompiledExpression],
) -> String {
    let mut combined = String::with_capacity(source.len() + expressions.len() * 64);
    combined.push_str(source);
    combined.push_str("\n__mesh_template_expressions = {}\n");
    for expression in expressions {
        let source = expression.source();
        let key = serde_json::to_string(source).expect("template expression string");
        combined.push_str("__mesh_template_expressions[");
        combined.push_str(&key);
        combined.push_str("] = function() return (");
        combined.push_str(source);
        combined.push_str(") end\n");
    }
    combined
}

/// Binds template-expression closures. It runs in an empty environment and
/// receives the component `_ENV` as an argument, so neither it nor the
/// expressions need `getfenv`/`setfenv`. Each expression's environment
/// resolves call locals first, then records the component member it read.
const TEMPLATE_EXPRESSION_FACTORY: &str = r#"
local component_env, setmetatable = ...
local no_locals = {}
return function(member_reads)
  local current = no_locals
  local env = setmetatable({}, { __index = function(_, name)
    local value = current[name]
    if value ~= nil then return value end
    if member_reads[name] == nil then member_reads[name] = true end
    return component_env[name]
  end })
  return env, function(expression)
    return function(locals)
      local previous = current
      current = locals or no_locals
      local value = expression()
      current = previous
      return value
    end
  end
end
"#;

/// Rebind the closures the component source installed in
/// `__mesh_template_expressions` to their per-expression environments, and
/// install `__mesh_template_expression_member_reads`, keyed by source.
pub(super) fn install_template_expressions(
    lua: &Lua,
    env: &Table,
    expressions: &[mesh_core_expression::SharedCompiledExpression],
) -> mlua::Result<()> {
    let all_member_reads = lua.create_table()?;
    env.raw_set("__mesh_template_expression_member_reads", all_member_reads.clone())?;
    if expressions.is_empty() {
        return Ok(());
    }
    let closures: Table = env.raw_get("__mesh_template_expressions")?;
    let factory: Function = lua
        .load(TEMPLATE_EXPRESSION_FACTORY)
        .set_name("=mesh:template-expression")
        .set_environment(lua.create_table()?)
        .call((env.clone(), lua.globals().get::<Function>("setmetatable")?))?;
    for expression in expressions {
        let source = expression.source();
        let closure: Function = closures.raw_get(source)?;
        let member_reads = lua.create_table()?;
        let (expression_env, bind): (Table, Function) = factory.call(member_reads.clone())?;
        closure.set_environment(expression_env)?;
        closures.raw_set(source, bind.call::<Function>(closure)?)?;
        all_member_reads.raw_set(source, member_reads)?;
    }
    Ok(())
}

/// Backing VM for a [`ScriptContext`].
///
/// Cheap handle to a thread-owned Luau realm. Per-context `_ENV` tables are the
/// isolation boundary; all contexts initialized on one thread share the VM and
/// its standard-library heap.
#[derive(Debug)]
pub(super) struct ScriptVm(pub(super) Lua);

impl ScriptVm {
    pub(super) fn lua(&self) -> &Lua {
        &self.0
    }
}

/// An opaque handle to the current thread's shared frontend Lua realm.
///
/// Every frontend surface created on the thread receives a clone of the same
/// sandboxed VM. Component `_ENV` tables keep globals, host channels, and
/// subscriptions isolated; sharing the realm enables live `bind:this` calls
/// without per-surface standard-library allocation.
#[derive(Clone, Debug)]
pub struct SurfaceVm {
    pub(super) lua: Lua,
    pub(super) policy: RuntimePolicy,
    pub(super) element_metrics: Arc<Mutex<ElementMetricsStore>>,
}

impl SurfaceVm {
    /// Clone the current thread's sandboxed realm for a frontend surface.
    pub fn new() -> Self {
        Self {
            lua: pool::thread_vm(),
            policy: pool::thread_policy(),
            element_metrics: Arc::new(Mutex::new(ElementMetricsStore::default())),
        }
    }

    pub(crate) fn handle(&self) -> Lua {
        self.lua.clone()
    }

    pub(crate) fn policy(&self) -> RuntimePolicy {
        self.policy.clone()
    }
}

impl Default for SurfaceVm {
    fn default() -> Self {
        Self::new()
    }
}

pub(super) fn json_value_fingerprint(value: &Value) -> u64 {
    let mut hasher = DefaultHasher::new();
    hash_json_value(value, &mut hasher);
    hasher.finish()
}

fn hash_json_value(value: &Value, hasher: &mut DefaultHasher) {
    match value {
        Value::Null => 0u8.hash(hasher),
        Value::Bool(value) => {
            1u8.hash(hasher);
            value.hash(hasher);
        }
        Value::Number(value) => {
            2u8.hash(hasher);
            if let Some(value) = value.as_i64() {
                0u8.hash(hasher);
                value.hash(hasher);
            } else if let Some(value) = value.as_u64() {
                1u8.hash(hasher);
                value.hash(hasher);
            } else if let Some(value) = value.as_f64() {
                2u8.hash(hasher);
                value.to_bits().hash(hasher);
            } else {
                3u8.hash(hasher);
                value.to_string().hash(hasher);
            }
        }
        Value::String(value) => {
            3u8.hash(hasher);
            value.hash(hasher);
        }
        Value::Array(values) => {
            4u8.hash(hasher);
            values.len().hash(hasher);
            for value in values {
                hash_json_value(value, hasher);
            }
        }
        Value::Object(map) => {
            5u8.hash(hasher);
            map.len().hash(hasher);
            for (key, value) in map {
                key.hash(hasher);
                hash_json_value(value, hasher);
            }
        }
    }
}
