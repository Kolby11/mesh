use super::super::ScriptError;
use super::super::lookup::map_lua_error;
use super::*;
use mesh_core_elements::VariableStore;
use mlua::{Function, LuaSerdeExt, Table, Value as LuaValue};
use serde_json::Value;
use std::collections::HashSet;
use std::sync::atomic::Ordering;

impl ScriptContext {
    pub(crate) fn benchmark_template_dependency_gate(
        &mut self,
        iterations: usize,
    ) -> (std::time::Duration, std::time::Duration, usize, usize) {
        let _ = self.ensure_initialized();
        let reads = self.lua().create_table().expect("template member reads");
        reads.set("label", true).expect("seed Lua dependency");
        self.env()
            .set("__mesh_template_member_reads_benchmark", reads)
            .expect("install Lua dependency table");
        self.template_expression_cache
            .lock()
            .unwrap()
            .template_member_reads
            .insert("label".to_string());

        let changed = std::hint::black_box("telemetry");
        let lua_started = std::time::Instant::now();
        let mut lua_hits = 0usize;
        for _ in 0..iterations {
            let reads = self
                .env()
                .get::<Table>("__mesh_template_member_reads_benchmark")
                .expect("resolve Lua dependency table");
            lua_hits += usize::from(reads.get::<bool>(changed).unwrap_or(false));
        }
        let lua_time = lua_started.elapsed();

        let rust_started = std::time::Instant::now();
        let mut rust_hits = 0usize;
        for _ in 0..iterations {
            rust_hits += usize::from(
                self.template_expression_cache
                    .lock()
                    .unwrap()
                    .template_member_reads
                    .contains(changed),
            );
        }
        let rust_time = rust_started.elapsed();
        (lua_time, rust_time, lua_hits, rust_hits)
    }

    pub fn service_payload_marker_for_test(&mut self, service: &str) -> Option<Vec<u8>> {
        let _ = self.ensure_initialized();
        self.lua()
            .globals()
            .get::<Table>("__mesh_service_payload_fingerprints")
            .ok()
            .and_then(|table| table.get::<Option<mlua::String>>(service).ok().flatten())
            .map(|marker| marker.as_bytes().to_vec())
    }

    pub(crate) fn benchmark_service_payload_marker_probes(
        &mut self,
        iterations: usize,
    ) -> (std::time::Duration, std::time::Duration, usize, usize) {
        let _ = self.ensure_initialized();
        let fingerprint = json_value_fingerprint(&serde_json::json!({
            "percent": 64,
            "muted": false,
        }));
        let pointer = 0x1234usize;
        let formatted_marker = format!("{pointer}:{fingerprint}");
        let formatted = self.lua().create_table().expect("formatted marker table");
        formatted
            .set("audio", formatted_marker.as_str())
            .expect("seed formatted marker");
        let binary = self.lua().create_table().expect("binary marker table");
        binary
            .set(
                "audio",
                self.lua()
                    .create_string(fingerprint.to_ne_bytes())
                    .expect("create binary marker"),
            )
            .expect("seed binary marker");

        let formatted_started = std::time::Instant::now();
        let mut formatted_hits = 0usize;
        for _ in 0..iterations {
            let marker = format!("{pointer}:{fingerprint}");
            let previous = formatted
                .get::<Option<String>>("audio")
                .expect("read formatted marker");
            formatted_hits +=
                std::hint::black_box(previous.as_deref() == Some(marker.as_str())) as usize;
        }
        let formatted_time = formatted_started.elapsed();

        let binary_started = std::time::Instant::now();
        let mut binary_hits = 0usize;
        let marker = fingerprint.to_ne_bytes();
        for _ in 0..iterations {
            let previous = binary
                .get::<Option<mlua::String>>("audio")
                .expect("read binary marker");
            binary_hits += std::hint::black_box(
                previous.is_some_and(|previous| previous.as_bytes().as_ref() == marker),
            ) as usize;
        }
        let binary_time = binary_started.elapsed();

        (formatted_time, binary_time, formatted_hits, binary_hits)
    }

    pub(crate) fn benchmark_service_payload_table_access(
        &mut self,
        iterations: usize,
    ) -> (std::time::Duration, std::time::Duration, usize, usize) {
        let _ = self.ensure_initialized();
        let marker = 42u64.to_ne_bytes();
        let table = self.lua().create_table().expect("marker table");
        table
            .set(
                "audio",
                self.lua()
                    .create_string(marker)
                    .expect("create marker string"),
            )
            .expect("seed marker");
        self.lua()
            .globals()
            .set("__mesh_service_payload_fingerprints", table.clone())
            .expect("install marker table");
        self.cached_service_payload_fingerprints = Some(table);

        let globals = self.lua().globals();
        let global_started = std::time::Instant::now();
        let mut global_hits = 0usize;
        for _ in 0..iterations {
            let table = globals
                .get::<Table>("__mesh_service_payload_fingerprints")
                .expect("resolve global marker table");
            let previous = table
                .get::<Option<mlua::String>>("audio")
                .expect("read global marker");
            global_hits += std::hint::black_box(
                previous.is_some_and(|previous| previous.as_bytes().as_ref() == marker),
            ) as usize;
        }
        let global_time = global_started.elapsed();

        let cached_started = std::time::Instant::now();
        let mut cached_hits = 0usize;
        for _ in 0..iterations {
            let table = self
                .cached_service_payload_fingerprints
                .as_ref()
                .expect("cached marker table");
            let previous = table
                .get::<Option<mlua::String>>("audio")
                .expect("read cached marker");
            cached_hits += std::hint::black_box(
                previous.is_some_and(|previous| previous.as_bytes().as_ref() == marker),
            ) as usize;
        }
        let cached_time = cached_started.elapsed();

        (global_time, cached_time, global_hits, cached_hits)
    }

    pub(crate) fn clear_refs_proxy_cache_for_benchmark(&mut self) {
        let _ = self.ensure_initialized();
        let Ok(refs) = self.env().get::<Table>("refs") else {
            return;
        };
        let keys = refs
            .clone()
            .pairs::<String, LuaValue>()
            .filter_map(|result| result.ok().map(|(key, _)| key))
            .collect::<Vec<_>>();
        for key in keys {
            let _ = refs.raw_set(key, LuaValue::Nil);
        }
    }

    pub(crate) fn apply_element_metrics_eager_for_benchmark(&mut self, metrics: &Value) {
        let _ = self.ensure_initialized();
        if let Ok(lua_value) = self.lua().to_value(metrics) {
            let _ = self
                .env()
                .set("__mesh_element_metrics_benchmark", lua_value);
        }
    }

    pub(crate) fn has_user_global_key_for_test(&self, key: &str) -> bool {
        self.user_global_key_set.contains(key)
    }

    pub(crate) fn clear_cached_self_table_for_benchmark(&mut self) {
        self.cached_self_table = None;
    }

    pub(crate) fn legacy_module_state_mirror_for_benchmark(&self) -> usize {
        let snapshot = self.state.snapshot();
        let count = snapshot.as_object().map_or(0, |values| values.len());
        let module_table = self
            .env()
            .get::<Table>("module")
            .expect("module table installed");
        let lua_value = self
            .lua()
            .to_value(&snapshot)
            .expect("snapshot converts to Lua");
        module_table
            .set("state", lua_value)
            .expect("legacy state mirror writes");
        count
    }

    pub(crate) fn pending_side_channels_for_test(&self) -> bool {
        self.pending_side_channels.load(Ordering::Acquire)
    }

    pub(crate) fn sync_side_channels_for_benchmark(&mut self) {
        self.sync_side_channels();
    }

    pub(crate) fn old_sync_side_channels_for_benchmark(&mut self) {
        {
            let mut published = self.shared_published_events.lock().unwrap();
            if !published.is_empty() {
                self.published_events.extend(published.drain(..));
            }
        }
        {
            let mut diagnostics = self.shared_diagnostics.lock().unwrap();
            if !diagnostics.is_empty() {
                self.diagnostics.extend(diagnostics.drain(..));
            }
        }
        {
            let mut element_actions = self.shared_element_actions.lock().unwrap();
            if !element_actions.is_empty() {
                self.element_actions.extend(element_actions.drain(..));
            }
        }
        let changed_storage_keys = {
            let mut changed = self.changed_storage_keys.lock().unwrap();
            changed.drain().collect::<HashSet<_>>()
        };
        if !changed_storage_keys.is_empty() {
            let tracked_storage_keys = self.tracked_storage_keys.lock().unwrap();
            if changed_storage_keys
                .iter()
                .any(|key| tracked_storage_keys.contains(key))
            {
                self.state.dirty = true;
            }
        }
        let shared_interface_bindings = self.shared_interface_bindings.lock().unwrap();
        if self.interface_bindings_generation != shared_interface_bindings.generation {
            self.interface_bindings = shared_interface_bindings.bindings.clone();
            self.interface_bindings_generation = shared_interface_bindings.generation;
        }
    }

    pub(crate) fn call_lua_function_without_sync_for_test(
        &mut self,
        name: &str,
    ) -> Result<(), ScriptError> {
        self.ensure_initialized()?;
        let handler = self
            .env()
            .get::<Function>(name)
            .map_err(|_| ScriptError::HandlerNotFound(name.to_string()))?;
        handler.call::<()>(()).map_err(map_lua_error)
    }

    pub(crate) fn old_sync_state_from_lua_scan_for_benchmark(&mut self) {
        for key in &self.user_global_keys {
            if let Ok(lua_value) = self.env().get::<LuaValue>(key.as_str())
                && !matches!(lua_value, LuaValue::Nil | LuaValue::Function(_))
                && let Ok(value) = self.lua().from_value::<Value>(lua_value)
            {
                self.state.set(key.clone(), value);
            }
        }
        let known = self
            .user_global_keys
            .iter()
            .cloned()
            .collect::<HashSet<_>>();
        let new_user_globals: Vec<(String, LuaValue)> = self
            .env()
            .pairs::<String, LuaValue>()
            .filter_map(|result| result.ok())
            .filter(|(key, value)| {
                !known.contains(key)
                    && !key.starts_with("__")
                    && !self.builtin_globals.contains(key)
                    && !matches!(value, LuaValue::Function(_))
            })
            .collect();
        for (name, lua_value) in new_user_globals {
            if let Ok(value) = self.lua().from_value::<Value>(lua_value) {
                self.state.set(name.clone(), value);
                self.user_global_key_set.insert(name.clone());
                self.user_global_keys.push(name);
            }
        }
    }

    pub(crate) fn sync_known_globals_with_scalar_gate_for_benchmark(&mut self) {
        for key in &self.user_global_keys {
            if let Ok(lua_value) = self.env().get::<LuaValue>(key.as_str()) {
                if matches!(lua_value, LuaValue::Nil | LuaValue::Function(_))
                    || self
                        .state
                        .get_ref(key)
                        .is_some_and(|current| lua_scalar_matches_json(&lua_value, current))
                {
                    continue;
                }
                if let Ok(value) = self.lua().from_value::<Value>(lua_value) {
                    self.state.set(key.clone(), value);
                }
            }
        }
    }

    pub(crate) fn old_global_redraw_flag_sync_for_benchmark(&mut self) {
        if self
            .env()
            .get::<bool>("__mesh_request_redraw")
            .unwrap_or(false)
        {
            self.state.dirty = true;
            let _ = self.env().set("__mesh_request_redraw", false);
        }
    }

    pub(crate) fn pending_redraw_for_benchmark(&self) -> bool {
        self.pending_redraw.swap(false, Ordering::AcqRel)
    }

    pub(crate) fn old_empty_assigned_globals_drain_for_benchmark(&mut self) -> usize {
        let mut assigned = self.assigned_global_keys.lock().unwrap();
        assigned.drain().count()
    }

    pub(crate) fn pending_assigned_globals_for_benchmark(&self) -> bool {
        self.pending_assigned_global_keys
            .swap(false, Ordering::AcqRel)
    }
}
