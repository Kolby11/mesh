use super::super::element_ref::ElementMetricsStore;
use crate::policy::RuntimePolicy;
use crate::pool;
use mesh_core_service::InterfaceResolution;
use mlua::Lua;
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

pub(super) fn component_source_with_compiled_template_expressions(
    source: &str,
    expressions: &[mesh_core_expression::SharedCompiledExpression],
) -> String {
    let mut combined = String::with_capacity(source.len() + expressions.len() * 192);
    combined.push_str(source);
    combined.push_str(
        "\nlocal __mesh_component_env = getfenv(1)\nlocal __mesh_setfenv = setfenv\nlocal __mesh_setmetatable = setmetatable\n__mesh_template_expressions = {}\n__mesh_template_expression_member_reads = {}\n",
    );
    for expression in expressions {
        let source = expression.source();
        let key = serde_json::to_string(source).expect("template expression string");
        combined.push_str("__mesh_template_expressions[");
        combined.push_str(&key);
        combined.push_str("] = (function()\n");
        combined.push_str("  local __mesh_expression_member_reads = {}\n");
        combined.push_str("  __mesh_template_expression_member_reads[");
        combined.push_str(&key);
        combined.push_str("] = __mesh_expression_member_reads\n");
        combined.push_str("  return function(__mesh_locals)\n");
        combined.push_str("  __mesh_locals = __mesh_locals or {}\n");
        combined.push_str("  local __mesh_expression_env = __mesh_setmetatable({}, { __index = function(_, name)\n");
        combined.push_str("    local value = __mesh_locals[name]\n");
        combined.push_str("    if value ~= nil then return value end\n");
        combined.push_str("    if __mesh_expression_member_reads[name] == nil then __mesh_expression_member_reads[name] = true end\n");
        combined.push_str("    return __mesh_component_env[name]\n");
        combined.push_str("  end })\n");
        combined.push_str("  __mesh_setfenv(1, __mesh_expression_env)\n");
        combined.push_str("  return (");
        combined.push_str(source);
        combined.push_str(")\n  end\nend)()\n");
    }
    combined
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
