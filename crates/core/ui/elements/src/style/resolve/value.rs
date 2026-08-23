use super::StyleResolver;
use super::cache::*;
use super::declaration::*;
use super::index::*;
use crate::style::parse::*;
use crate::style::*;
use mesh_core_component::style::{StyleValue, prop_variable_key};
use mesh_core_theme::TokenValue;
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Debug, Default)]
pub struct ResolutionContext {
    stack: Vec<String>,
    cycles: Vec<Vec<String>>,
}

impl ResolutionContext {
    fn enter(&mut self, name: &str) -> bool {
        if let Some(start) = self.stack.iter().position(|entry| entry == name) {
            let mut cycle = self.stack[start..].to_vec();
            cycle.push(name.to_owned());
            if !self.cycles.iter().any(|known| known == &cycle) {
                self.cycles.push(cycle);
            }
            return false;
        }

        self.stack.push(name.to_owned());
        true
    }

    fn leave(&mut self, name: &str) {
        debug_assert_eq!(self.stack.last().map(String::as_str), Some(name));
        self.stack.pop();
    }

    fn cycle_message(&self, name: &str) -> String {
        let start = self
            .stack
            .iter()
            .position(|entry| entry == name)
            .unwrap_or(0);
        let mut cycle = self.stack[start..].to_vec();
        cycle.push(name.to_owned());
        format!(
            "cyclic CSS custom-property reference: {}",
            cycle.join(" -> ")
        )
    }

    pub(super) fn take_cycle_messages(&mut self) -> impl Iterator<Item = String> + '_ {
        self.cycles.drain(..).map(|cycle| {
            format!(
                "cyclic CSS custom-property reference: {}",
                cycle.join(" -> ")
            )
        })
    }
}

pub(super) fn split_reference(contents: &str) -> (&str, Option<&str>) {
    let mut depth = 0usize;
    for (index, character) in contents.char_indices() {
        match character {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => return (&contents[..index], Some(&contents[index + 1..])),
            _ => {}
        }
    }
    (contents, None)
}

fn function_end(value: &str, contents_start: usize) -> Option<usize> {
    let mut depth = 1usize;
    for (offset, character) in value[contents_start..].char_indices() {
        let index = contents_start + offset;
        match character {
            '(' => depth += 1,
            ')' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(index);
                }
            }
            _ => {}
        }
    }
    None
}

impl<'a> StyleResolver<'a> {
    pub(super) fn resolve_font_family_reference(&self, family: &str) -> String {
        let Some((pack_id, role)) = family.split_once('/') else {
            return family.to_owned();
        };
        if pack_id.is_empty() || role.is_empty() {
            return family.to_owned();
        }
        let token_name = format!("mesh.font.{pack_id}.{role}");
        match self.theme.resolve_token_value(&token_name) {
            Ok(Some(TokenValue::String(resolved))) => resolved,
            Ok(Some(TokenValue::Number(_))) | Ok(Some(TokenValue::Bool(_))) | Ok(None) | Err(_) => {
                family.to_owned()
            }
        }
    }

    pub fn resolve_value(&self, value: &StyleValue) -> String {
        self.resolve_value_with_variables(value, empty_variables())
    }

    pub(super) fn resolve_value_with_variables(
        &self,
        value: &StyleValue,
        variables: &HashMap<String, StyleValue>,
    ) -> String {
        self.resolve_value_with_variables_mode(value, variables, false)
    }

    pub(super) fn resolve_value_with_variables_mode(
        &self,
        value: &StyleValue,
        variables: &HashMap<String, StyleValue>,
        strict_animation_tokens: bool,
    ) -> String {
        let mut context = ResolutionContext::default();
        self.resolve_value_with_variables_mode_context(
            value,
            variables,
            strict_animation_tokens,
            &mut context,
        )
        .unwrap_or_default()
    }

    pub(super) fn resolve_value_with_variables_mode_context(
        &self,
        value: &StyleValue,
        variables: &HashMap<String, StyleValue>,
        strict_animation_tokens: bool,
        context: &mut ResolutionContext,
    ) -> Result<String, String> {
        match value {
            StyleValue::Literal(s) => self.resolve_embedded_references_cached_context(
                s,
                variables,
                strict_animation_tokens,
                context,
            ),
            StyleValue::Var(name) => {
                self.resolve_var_reference(name, variables, strict_animation_tokens, context)
            }
            StyleValue::Prop(name) => {
                self.resolve_prop_reference(name, variables, strict_animation_tokens, context)
            }
        }
    }

    fn resolve_var_reference(
        &self,
        contents: &str,
        variables: &HashMap<String, StyleValue>,
        strict_animation_tokens: bool,
        context: &mut ResolutionContext,
    ) -> Result<String, String> {
        let (name, fallback) = split_reference(contents);
        self.resolve_named_reference(name, fallback, variables, strict_animation_tokens, context)
    }

    fn resolve_prop_reference(
        &self,
        contents: &str,
        variables: &HashMap<String, StyleValue>,
        strict_animation_tokens: bool,
        context: &mut ResolutionContext,
    ) -> Result<String, String> {
        let (name, fallback) = split_reference(contents);
        let name = name.trim();
        if name.is_empty() {
            return Err("empty component prop reference".to_string());
        }

        let key = prop_variable_key(name);
        let Some(value) = self.lookup_variable(variables, &key) else {
            return fallback
                .map(|fallback| {
                    self.resolve_value_with_variables_mode_context(
                        &StyleValue::Literal(fallback.trim().to_owned()),
                        variables,
                        strict_animation_tokens,
                        context,
                    )
                })
                .unwrap_or_else(|| Ok(String::new()));
        };

        if !context.enter(&key) {
            return self.resolve_fallback_or_error(
                fallback,
                context.cycle_message(&key),
                variables,
                strict_animation_tokens,
                context,
            );
        }
        let result = self.resolve_value_with_variables_mode_context(
            value,
            variables,
            strict_animation_tokens,
            context,
        );
        context.leave(&key);
        match result {
            Ok(value) => Ok(value),
            Err(error) => self.resolve_fallback_or_error(
                fallback,
                error,
                variables,
                strict_animation_tokens,
                context,
            ),
        }
    }

    fn resolve_named_reference(
        &self,
        name: &str,
        fallback: Option<&str>,
        variables: &HashMap<String, StyleValue>,
        strict_animation_tokens: bool,
        context: &mut ResolutionContext,
    ) -> Result<String, String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("empty CSS variable reference".to_string());
        }

        let result = if let Some(value) = self.lookup_variable(variables, name) {
            if !context.enter(name) {
                Err(context.cycle_message(name))
            } else {
                let result = self.resolve_value_with_variables_mode_context(
                    value,
                    variables,
                    strict_animation_tokens,
                    context,
                );
                context.leave(name);
                result
            }
        } else if fallback.is_some()
            && matches!(
                self.cached_theme_token_value(name),
                CachedThemeTokenValue::Missing | CachedThemeTokenValue::Error(_)
            )
        {
            Err(format!("unresolved CSS variable reference '{name}'"))
        } else {
            self.resolve_theme_reference(name, strict_animation_tokens)
        };

        match result {
            Ok(value) => Ok(value),
            Err(error) => self.resolve_fallback_or_error(
                fallback,
                error,
                variables,
                strict_animation_tokens,
                context,
            ),
        }
    }

    fn resolve_fallback_or_error(
        &self,
        fallback: Option<&str>,
        error: String,
        variables: &HashMap<String, StyleValue>,
        strict_animation_tokens: bool,
        context: &mut ResolutionContext,
    ) -> Result<String, String> {
        fallback
            .map(|fallback| {
                self.resolve_value_with_variables_mode_context(
                    &StyleValue::Literal(fallback.trim().to_owned()),
                    variables,
                    strict_animation_tokens,
                    context,
                )
            })
            .unwrap_or(Err(error))
    }

    pub(super) fn with_resolved_str<R>(
        &self,
        value: &StyleValue,
        variables: &HashMap<String, StyleValue>,
        read: impl FnOnce(&str) -> R,
    ) -> R {
        if let Some(resolved) = self.resolve_simple_str_with_variables(value, variables, 0) {
            return read(resolved);
        }
        let resolved = self.resolve_value_with_variables(value, variables);
        read(&resolved)
    }

    pub(super) fn with_resolved_str_context<R>(
        &self,
        value: &StyleValue,
        variables: &HashMap<String, StyleValue>,
        context: &mut ResolutionContext,
        read: impl FnOnce(&str) -> R,
    ) -> R {
        let resolved = match self
            .resolve_value_with_variables_mode_context(value, variables, false, context)
        {
            Ok(resolved) => resolved,
            Err(_) => String::new(),
        };
        read(&resolved)
    }

    pub(super) fn resolve_simple_str_with_variables<'b>(
        &'b self,
        value: &'b StyleValue,
        variables: &'b HashMap<String, StyleValue>,
        depth: u8,
    ) -> Option<&'b str> {
        if depth > 16 {
            return None;
        }
        match value {
            StyleValue::Literal(value) => {
                if references_style_function(value) {
                    None
                } else {
                    Some(value.as_str())
                }
            }
            StyleValue::Var(name) => {
                if let Some(value) = self.lookup_variable(variables, name) {
                    return self.resolve_simple_str_with_variables(
                        value,
                        variables,
                        depth.saturating_add(1),
                    );
                }
                let token_name = self.cached_theme_token_name(name);
                match self.theme.token(&token_name) {
                    Some(TokenValue::String(value)) => Some(value.as_str()),
                    Some(TokenValue::Number(_)) | Some(TokenValue::Bool(_)) | None => None,
                }
            }
            StyleValue::Prop(name) => self
                .lookup_variable(variables, &prop_variable_key(name))
                .and_then(|value| {
                    self.resolve_simple_str_with_variables(
                        value,
                        variables,
                        depth.saturating_add(1),
                    )
                }),
        }
    }

    pub(super) fn resolve_theme_reference(
        &self,
        name: &str,
        strict_animation_tokens: bool,
    ) -> Result<String, String> {
        match self.cached_theme_token_value(name) {
            CachedThemeTokenValue::String(s) => Ok(s.to_string()),
            CachedThemeTokenValue::Number(n) => Ok(format!("{n}")),
            CachedThemeTokenValue::Bool(b) => Ok(format!("{b}")),
            CachedThemeTokenValue::Error(error) => Err(error.to_string()),
            CachedThemeTokenValue::Missing => {
                let token_name = self.cached_theme_token_name(name);
                if strict_animation_tokens && token_name.starts_with("animation.") {
                    return Err(token_name.to_string());
                }
                tracing::warn!("unresolved theme token: {token_name}");
                Ok(String::new())
            }
        }
    }

    pub(super) fn resolve_embedded_references_cached(
        &self,
        value: &str,
        variables: &HashMap<String, StyleValue>,
        strict_animation_tokens: bool,
    ) -> Result<String, String> {
        let mut context = ResolutionContext::default();
        self.resolve_embedded_references_cached_context(
            value,
            variables,
            strict_animation_tokens,
            &mut context,
        )
    }

    pub(super) fn resolve_embedded_references_cached_context(
        &self,
        value: &str,
        variables: &HashMap<String, StyleValue>,
        strict_animation_tokens: bool,
        context: &mut ResolutionContext,
    ) -> Result<String, String> {
        let mut output = String::with_capacity(value.len());
        let mut rest = value;

        loop {
            let var_pos = rest.find("var(");
            let prop_pos = rest.find("prop(");
            let Some((start, is_prop)) = (match (var_pos, prop_pos) {
                (Some(v), Some(p)) if p < v => Some((p, true)),
                (Some(v), _) => Some((v, false)),
                (None, Some(p)) => Some((p, true)),
                (None, None) => None,
            }) else {
                break;
            };

            output.push_str(&rest[..start]);
            let prefix_len = if is_prop { "prop(".len() } else { "var(".len() };
            let reference_start = start + prefix_len;
            let Some(end) = function_end(rest, reference_start) else {
                output.push_str(&rest[start..]);
                return Ok(output);
            };

            let contents = &rest[reference_start..end];
            let resolved = if is_prop {
                self.resolve_prop_reference(contents, variables, strict_animation_tokens, context)?
            } else {
                self.resolve_var_reference(contents, variables, strict_animation_tokens, context)?
            };
            output.push_str(&resolved);
            rest = &rest[end + 1..];
        }

        output.push_str(rest);
        Ok(output)
    }

    pub(super) fn validate_animation_value_with_variables_context(
        &self,
        value: &StyleValue,
        variables: &HashMap<String, StyleValue>,
        context: &mut ResolutionContext,
    ) -> Result<(), String> {
        self.resolve_value_with_variables_mode_context(value, variables, true, context)
            .map(|_| ())
    }

    pub(super) fn resolve_color_with_variables(
        &self,
        value: &StyleValue,
        variables: &HashMap<String, StyleValue>,
    ) -> Color {
        let mut context = ResolutionContext::default();
        self.resolve_color_with_variables_context(value, variables, &mut context)
    }

    pub(super) fn resolve_color_with_variables_context(
        &self,
        value: &StyleValue,
        variables: &HashMap<String, StyleValue>,
        context: &mut ResolutionContext,
    ) -> Color {
        self.resolve_color_with_variables_inner(value, variables, 0, context)
            .unwrap_or_else(|| {
                match self
                    .resolve_value_with_variables_mode_context(value, variables, false, context)
                {
                    Ok(resolved) => Color::from_css(&resolved).unwrap_or(Color::TRANSPARENT),
                    Err(_) => Color::TRANSPARENT,
                }
            })
    }

    pub(super) fn resolve_color_with_variables_inner(
        &self,
        value: &StyleValue,
        variables: &HashMap<String, StyleValue>,
        depth: u8,
        context: &mut ResolutionContext,
    ) -> Option<Color> {
        if depth > 16 {
            return None;
        }
        match value {
            StyleValue::Literal(value) => {
                if references_style_function(value) {
                    None
                } else {
                    Some(Color::from_css(value).unwrap_or(Color::TRANSPARENT))
                }
            }
            StyleValue::Var(name) if name.contains(',') => {
                let resolved = self
                    .resolve_value_with_variables_mode_context(value, variables, false, context)
                    .ok()?;
                Some(Color::from_css(&resolved).unwrap_or(Color::TRANSPARENT))
            }
            StyleValue::Var(name) => {
                if let Some(value) = self.lookup_variable(variables, name) {
                    if !context.enter(name) {
                        return None;
                    }
                    let result = self.resolve_color_with_variables_inner(
                        value,
                        variables,
                        depth.saturating_add(1),
                        context,
                    );
                    context.leave(name);
                    return result;
                }
                match self.cached_theme_token_value(name) {
                    CachedThemeTokenValue::String(value) => {
                        Some(Color::from_css(&value).unwrap_or(Color::TRANSPARENT))
                    }
                    CachedThemeTokenValue::Number(_) | CachedThemeTokenValue::Bool(_) => {
                        Some(Color::TRANSPARENT)
                    }
                    CachedThemeTokenValue::Error(_) => None,
                    CachedThemeTokenValue::Missing => None,
                }
            }
            StyleValue::Prop(name) => {
                let key = prop_variable_key(name);
                let value = self.lookup_variable(variables, &key)?;
                if !context.enter(&key) {
                    return None;
                }
                let result = self.resolve_color_with_variables_inner(
                    value,
                    variables,
                    depth.saturating_add(1),
                    context,
                );
                context.leave(&key);
                result
            }
        }
    }

    pub(super) fn resolve_number_with_variables(
        &self,
        value: &StyleValue,
        variables: &HashMap<String, StyleValue>,
    ) -> f32 {
        let mut context = ResolutionContext::default();
        self.resolve_number_with_variables_context(value, variables, &mut context)
    }

    pub(super) fn resolve_number_with_variables_context(
        &self,
        value: &StyleValue,
        variables: &HashMap<String, StyleValue>,
        context: &mut ResolutionContext,
    ) -> f32 {
        self.resolve_number_with_variables_inner(value, variables, 0, context)
            .unwrap_or_else(|| {
                match self
                    .resolve_value_with_variables_mode_context(value, variables, false, context)
                {
                    Ok(resolved) => parse_px(&resolved),
                    Err(_) => 0.0,
                }
            })
    }

    pub(super) fn resolve_number_with_variables_inner(
        &self,
        value: &StyleValue,
        variables: &HashMap<String, StyleValue>,
        depth: u8,
        context: &mut ResolutionContext,
    ) -> Option<f32> {
        if depth > 16 {
            return None;
        }
        match value {
            StyleValue::Literal(value) => {
                if references_style_function(value) {
                    None
                } else {
                    Some(parse_px(value))
                }
            }
            StyleValue::Var(name) if name.contains(',') => {
                let resolved = self
                    .resolve_value_with_variables_mode_context(value, variables, false, context)
                    .ok()?;
                Some(parse_px(&resolved))
            }
            StyleValue::Var(name) => {
                if let Some(value) = self.lookup_variable(variables, name) {
                    if !context.enter(name) {
                        return None;
                    }
                    let result = self.resolve_number_with_variables_inner(
                        value,
                        variables,
                        depth.saturating_add(1),
                        context,
                    );
                    context.leave(name);
                    return result;
                }
                match self.cached_theme_token_value(name) {
                    CachedThemeTokenValue::Number(value) => Some(value as f32),
                    CachedThemeTokenValue::String(value) => Some(parse_px(&value)),
                    CachedThemeTokenValue::Bool(_) => Some(0.0),
                    CachedThemeTokenValue::Error(_) => None,
                    CachedThemeTokenValue::Missing => None,
                }
            }
            StyleValue::Prop(name) => {
                let key = prop_variable_key(name);
                let value = self.lookup_variable(variables, &key)?;
                if !context.enter(&key) {
                    return None;
                }
                let result = self.resolve_number_with_variables_inner(
                    value,
                    variables,
                    depth.saturating_add(1),
                    context,
                );
                context.leave(&key);
                result
            }
        }
    }

    pub(super) fn lookup_variable<'b>(
        &'b self,
        variables: &'b HashMap<String, StyleValue>,
        name: &str,
    ) -> Option<&'b StyleValue> {
        variables.get(name).or_else(|| self.props.get(name))
    }

    pub(super) fn cached_theme_token_name(&self, reference: &str) -> Arc<str> {
        if let Some(name) = self.theme_reference_cache.borrow().get(reference) {
            return Arc::clone(name);
        }
        let name = Arc::<str>::from(theme_reference_to_token_name(reference));
        self.theme_reference_cache
            .borrow_mut()
            .insert(reference.to_owned(), Arc::clone(&name));
        name
    }

    pub(super) fn cached_theme_token_value(&self, reference: &str) -> CachedThemeTokenValue {
        let reference = reference.trim();
        if let Some(value) = self.theme_value_cache.borrow().get(reference) {
            return value.clone();
        }
        let token_name = self.cached_theme_token_name(reference);
        let value =
            CachedThemeTokenValue::from_resolution(self.theme.resolve_token_value(&token_name));
        self.theme_value_cache
            .borrow_mut()
            .insert(reference.to_owned(), value.clone());
        value
    }

    pub(super) fn cached_theme_token_error(&self, reference: &str) -> Option<String> {
        match self.cached_theme_token_value(reference) {
            CachedThemeTokenValue::Error(error) => Some(error.to_string()),
            CachedThemeTokenValue::Missing
            | CachedThemeTokenValue::String(_)
            | CachedThemeTokenValue::Number(_)
            | CachedThemeTokenValue::Bool(_) => None,
        }
    }
}
