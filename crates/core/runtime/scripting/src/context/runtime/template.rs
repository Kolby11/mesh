use super::super::ScriptError;
use super::super::lookup::{lua_err, map_lua_error};
use super::*;
use mlua::{LuaSerdeExt, Table, Value as LuaValue};
use serde_json::Value;

impl ScriptContext {
    pub fn evaluate_template_expression(
        &self,
        expression: &str,
        locals: &serde_json::Map<String, Value>,
    ) -> Result<(Value, Vec<(String, String)>), ScriptError> {
        let empty_locals = locals.is_empty();
        if empty_locals && !self.changed_public_members.is_empty() {
            let mut cache = self.template_expression_cache.lock().unwrap();
            let cached = cache
                .member_reads
                .get(expression)
                .zip(cache.values.get(expression))
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
        let closure: mlua::Function = closures.get(expression).map_err(map_lua_error)?;
        let locals = self.lua().to_value(locals).map_err(lua_err)?;
        let previous_reads = {
            let mut tracked = self.tracked_service_fields.lock().unwrap();
            std::mem::take(&mut *tracked)
        };
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
        let value: Value = self.lua().from_value(evaluated?).map_err(lua_err)?;
        let reads: Vec<_> = expression_reads
            .into_iter()
            .flat_map(|(service, fields)| {
                fields
                    .into_iter()
                    .map(move |field| (service.clone(), field))
            })
            .collect();
        if empty_locals {
            let member_reads = self
                .env()
                .get::<Table>("__mesh_template_expression_member_reads")
                .ok()
                .and_then(|all_reads| all_reads.get::<Table>(expression).ok())
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
                let cacheable = reads.is_empty()
                    && member_reads
                        .iter()
                        .all(|name| self.user_global_key_set.contains(name));
                let mut cache = self.template_expression_cache.lock().unwrap();
                cache
                    .template_member_reads
                    .extend(member_reads.iter().cloned());
                cache
                    .member_reads
                    .insert(expression.to_string(), member_reads);
                if cacheable {
                    cache.values.insert(expression.to_string(), value.clone());
                } else {
                    cache.values.remove(expression);
                }
            }
        }
        Ok((value, reads))
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
        self.changed_public_members
            .iter()
            .any(|name| cache.template_member_reads.contains(name))
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
