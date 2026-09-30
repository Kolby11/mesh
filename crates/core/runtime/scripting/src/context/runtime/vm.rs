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
    pub(super) hits: u64,
}

/// Builds one template-expression closure. It runs in an empty environment
/// and receives the component `_ENV` as an argument, so neither it nor the
/// expressions need `getfenv`/`setfenv`. Each expression chunk is compiled by
/// the host against a proxy that resolves call locals first, then records the
/// component member it read.
const TEMPLATE_EXPRESSION_FACTORY: &str = r#"
local component_env, setmetatable = ...
local no_locals = {}
return function(member_reads, compile)
  local current = no_locals
  local expression = compile(setmetatable({}, { __index = function(_, name)
    local value = current[name]
    if value ~= nil then return value end
    if member_reads[name] == nil then member_reads[name] = true end
    return component_env[name]
  end }))
  return function(locals)
    local previous = current
    current = locals or no_locals
    local value = expression()
    current = previous
    return value
  end
end
"#;

/// Install `__mesh_template_expressions` and their member-read tables into a
/// component environment, keyed by expression source.
pub(super) fn install_template_expressions(
    lua: &Lua,
    env: &Table,
    chunk_name: &str,
    expressions: &[mesh_core_expression::SharedCompiledExpression],
) -> mlua::Result<()> {
    let closures = lua.create_table()?;
    let all_member_reads = lua.create_table()?;
    if !expressions.is_empty() {
        let factory: Function = lua
            .load(TEMPLATE_EXPRESSION_FACTORY)
            .set_name("=mesh:template-expression")
            .set_environment(lua.create_table()?)
            .call((env.clone(), lua.globals().get::<Function>("setmetatable")?))?;
        for expression in expressions {
            let source = expression.source();
            let member_reads = lua.create_table()?;
            let chunk = format!("return ({source})");
            let name = chunk_name.to_string();
            let compile = lua.create_function(move |lua, expression_env: Table| {
                lua.load(chunk.as_str())
                    .set_name(&name)
                    .set_environment(expression_env)
                    .into_function()
            })?;
            let closure: Function = factory.call((member_reads.clone(), compile))?;
            closures.raw_set(source, closure)?;
            all_member_reads.raw_set(source, member_reads)?;
        }
    }
    env.raw_set("__mesh_template_expressions", closures)?;
    env.raw_set("__mesh_template_expression_member_reads", all_member_reads)?;
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
