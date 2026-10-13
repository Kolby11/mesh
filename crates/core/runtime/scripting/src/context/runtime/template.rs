use super::super::ScriptError;
use super::super::lookup::{lua_err, map_lua_error};
use super::*;
use mesh_core_expression::CompiledExpression;
use mlua::{LuaSerdeExt, Table, Value as LuaValue};
use serde_json::Value;

impl ScriptContext {
    pub fn evaluate_template_expression(
        &self,
        expression: &str,
        locals: &serde_json::Map<String, Value>,
    ) -> Result<(Value, Vec<(String, String)>), ScriptError> {
        let expression = mesh_core_expression::compile_expression(expression)
            .map_err(|error| ScriptError::LuaError(error.to_string()))?;
        self.evaluate_compiled_template_expression(&expression, locals)
    }

    pub fn evaluate_compiled_template_expression(
        &self,
        expression: &CompiledExpression,
        locals: &serde_json::Map<String, Value>,
    ) -> Result<(Value, Vec<(String, String)>), ScriptError> {
        let expression_source = expression.source();
        let empty_locals = locals.is_empty();
        if empty_locals && !self.changed_public_members.is_empty() {
            let mut cache = self.template_expression_cache.lock().unwrap();
            let cached = cache
                .member_reads
                .get(expression_source)
                .zip(cache.values.get(expression_source))
                .filter(|(member_reads, _)| {
                    !member_reads.iter().any(|name| {
                        self.changed_public_members
                            .iter()
                            .any(|changed| changed == name)
                    })
                })
                .map(|(_, value)| value.clone());
            if let Some(value) = cached {
                cache.hits = cache.hits.saturating_add(1);
                return Ok((value, Vec::new()));
            }
        }
        let closures: Table = self
            .env()
            .get("__mesh_template_expressions")
            .map_err(map_lua_error)?;
        let closure: mlua::Function = closures.get(expression_source).map_err(map_lua_error)?;
        let locals = self.lua().to_value(locals).map_err(lua_err)?;
        let previous_reads = {
            let mut tracked = self.tracked_service_fields.lock().unwrap();
            std::mem::take(&mut *tracked)
        };
        let _budget = self.realm_policy.begin_callback();
        let evaluated = closure.call::<LuaValue>(locals).map_err(map_lua_error);
        let expression_reads = {
            let mut tracked = self.tracked_service_fields.lock().unwrap();
            let expression_reads = std::mem::take(&mut *tracked);
            for (service, fields) in previous_reads {
                tracked.entry(service).or_default().extend(fields);
            }
            for (service, fields) in &expression_reads {
                tracked
                    .entry(service.clone())
                    .or_default()
                    .extend(fields.iter().cloned());
            }
            expression_reads
        };
        let value: Result<Value, ScriptError> =
            evaluated.and_then(|value| self.lua().from_value(value).map_err(lua_err));
        let reads: Vec<_> = expression_reads
            .into_iter()
            .flat_map(|(service, fields)| {
                fields
                    .into_iter()
                    .map(move |field| (service.clone(), field))
            })
            .collect();
        {
            let mut cache = self.template_expression_cache.lock().unwrap();
            let dependencies = cache
                .service_reads
                .entry(expression_source.to_owned())
                .or_default();
            dependencies.extend(reads.iter().cloned());
        }
        {
            let member_reads = self
                .env()
                .get::<Table>("__mesh_template_expression_member_reads")
                .ok()
                .and_then(|all_reads| all_reads.get::<Table>(expression_source).ok())
                .map(|expression_reads| {
                    let mut names: Vec<_> = expression_reads
                        .pairs::<String, bool>()
                        .filter_map(|entry| {
                            entry.ok().and_then(|(name, read)| read.then_some(name))
                        })
                        .collect();
                    names.sort_unstable();
                    names
                });
            if let Some(member_reads) = member_reads {
                let calls_component_function = expression.calls_function();
                let cacheable = empty_locals
                    && reads.is_empty()
                    && !calls_component_function
                    && member_reads
                        .iter()
                        .all(|name| self.user_global_key_set.contains(name));
                let mut cache = self.template_expression_cache.lock().unwrap();
                if calls_component_function {
                    cache
                        .opaque_expressions
                        .insert(expression_source.to_string());
                }
                cache
                    .template_member_reads
                    .extend(member_reads.iter().cloned());
                cache
                    .member_reads
                    .insert(expression_source.to_string(), member_reads);
                if cacheable && let Ok(value) = &value {
                    cache
                        .values
                        .insert(expression_source.to_string(), value.clone());
                } else {
                    cache.values.remove(expression_source);
                }
            }
        }
        Ok((value?, reads))
    }

    /// Whether dirty public script members can affect an evaluated template
    /// expression. An empty dirty-name set is treated conservatively because
    /// explicit redraw requests and side channels dirty state without a name.
    pub fn dirty_state_affects_template(&self) -> bool {
        if !self.state.is_dirty() {
            return false;
        }
        if !self.template_dependencies_ready {
            return true;
        }
        if self.changed_public_members.is_empty() {
            return true;
        }
        let cache = self.template_expression_cache.lock().unwrap();
        !cache.opaque_expressions.is_empty()
            || self
                .changed_public_members
                .iter()
                .any(|name| cache.template_member_reads.contains(name))
    }

    /// The template expressions that read any public member written since the
    /// template last consumed this instance's state, together with those
    /// members. Function calls conservatively read every changed member because
    /// their captured dependencies may be hidden inside a lexical closure.
    /// `None` before complete evaluation or when a write carried no member name.
    pub fn template_expressions_reading_changes(&self) -> Option<(Vec<String>, Vec<String>)> {
        if !self.template_dependencies_ready || self.changed_public_members.is_empty() {
            return None;
        }
        let cache = self.template_expression_cache.lock().unwrap();
        let readers = cache
            .member_reads
            .iter()
            .filter(|(source, reads)| {
                cache.opaque_expressions.contains(*source)
                    || reads
                        .iter()
                        .any(|read| self.changed_public_members.contains(read))
            })
            .map(|(source, _)| source.clone())
            .collect();
        Some((readers, self.changed_public_members.clone()))
    }

    /// Mark the pending public-member writes as applied without a template
    /// rebuild, after the caller re-evaluated every expression that reads them.
    pub fn consume_template_changes(&mut self) {
        self.changed_public_members.clear();
        self.state.clear_dirty();
    }

    /// Evaluated expressions that consume changed service fields, including
    /// reads made inside lexical helper functions and keyed loop iterations.
    pub fn template_expressions_reading_service_fields(
        &self,
        fields: &[(String, String)],
    ) -> Option<Vec<String>> {
        if !self.template_dependencies_ready {
            return None;
        }
        let cache = self.template_expression_cache.lock().unwrap();
        Some(
            cache
                .service_reads
                .iter()
                .filter(|(source, reads)| {
                    fields.iter().any(|field| reads.contains(field))
                        || cache.member_reads.get(*source).is_some_and(|members| {
                            members.iter().any(|member| {
                                member == "last_service_update"
                                    || fields.iter().any(|(service, _)| member == service)
                            })
                        })
                })
                .map(|(source, _)| source.clone())
                .collect(),
        )
    }

    /// Whether this runtime has completed a template evaluation, so its
    /// recorded read sets describe what the component actually consumes. Before
    /// that, an empty read set means "not yet known", not "reads nothing".
    pub fn template_dependencies_ready(&self) -> bool {
        self.template_dependencies_ready
    }

    /// Whether a write to the public member `name` can reach this component.
    ///
    /// Answers `true` until the first template evaluation completes, because
    /// an empty read set is not yet evidence of anything.
    pub fn observes_state_member(&self, name: &str) -> bool {
        if !self.template_dependencies_ready {
            return true;
        }
        self.template_expression_cache
            .lock()
            .unwrap()
            .template_member_reads
            .contains(name)
    }

    /// Mark the dependency set as authoritative after a complete component
    /// template evaluation, including the valid empty-set case.
    pub fn mark_template_dependencies_ready(&mut self) {
        self.template_dependencies_ready = true;
        self.changed_public_members.clear();
    }

    /// Drop memoized pure public-member expression values. Normal script and
    /// source lifecycle paths do this automatically; the explicit hook is
    /// useful when an owning runtime broadens an invalidation for reasons not
    /// represented in public script state.
    pub fn clear_template_expression_cache(&self) {
        *self.template_expression_cache.lock().unwrap() = TemplateExpressionCache::default();
    }

    #[cfg(test)]
    pub(in crate::context) fn template_expression_cache_contains(&self, expression: &str) -> bool {
        self.template_expression_cache
            .lock()
            .unwrap()
            .values
            .contains_key(expression)
    }

    #[cfg(test)]
    pub(in crate::context) fn template_expression_cache_hits(&self) -> u64 {
        self.template_expression_cache.lock().unwrap().hits
    }
}
