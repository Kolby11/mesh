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
        match value {
            StyleValue::Literal(s) => self
                .resolve_embedded_references_cached(s, variables, strict_animation_tokens)
                .unwrap_or_default(),
            StyleValue::Var(name) => variables
                .get(name)
                .map(|value| {
                    self.resolve_value_with_variables_mode(
                        value,
                        variables,
                        strict_animation_tokens,
                    )
                })
                .unwrap_or_else(|| {
                    self.resolve_theme_reference(name, strict_animation_tokens)
                        .unwrap_or_default()
                }),
            StyleValue::Prop(name) => self
                .lookup_variable(variables, &prop_variable_key(name))
                .map(|value| {
                    self.resolve_value_with_variables_mode(
                        value,
                        variables,
                        strict_animation_tokens,
                    )
                })
                .unwrap_or_default(),
        }
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
            let Some(end) = rest[reference_start..].find(')') else {
                output.push_str(&rest[start..]);
                return Ok(output);
            };

            let name = rest[reference_start..reference_start + end].trim();
            if is_prop {
                let prop_key = prop_variable_key(name);
                if let Some(value) = self.lookup_variable(variables, &prop_key) {
                    let resolved = self.style_value_to_string_cached(
                        value,
                        variables,
                        strict_animation_tokens,
                    )?;
                    output.push_str(&self.resolve_embedded_references_cached(
                        &resolved,
                        variables,
                        strict_animation_tokens,
                    )?);
                }
            } else if let Some(value) = self.lookup_variable(variables, name) {
                let resolved =
                    self.style_value_to_string_cached(value, variables, strict_animation_tokens)?;
                output.push_str(&self.resolve_embedded_references_cached(
                    &resolved,
                    variables,
                    strict_animation_tokens,
                )?);
            } else {
                match self.cached_theme_token_value(name) {
                    CachedThemeTokenValue::String(s) => output.push_str(&s),
                    CachedThemeTokenValue::Number(n) => output.push_str(&format!("{n}")),
                    CachedThemeTokenValue::Bool(b) => output.push_str(&format!("{b}")),
                    CachedThemeTokenValue::Error(error) => {
                        if strict_animation_tokens {
                            return Err(error.to_string());
                        }
                        tracing::warn!("unresolved theme token dependency: {error}");
                    }
                    CachedThemeTokenValue::Missing => {
                        let token_name = self.cached_theme_token_name(name);
                        if strict_animation_tokens && token_name.starts_with("animation.") {
                            return Err(token_name.to_string());
                        }
                        tracing::warn!("unresolved theme token: {token_name}");
                    }
                }
            }
            rest = &rest[reference_start + end + 1..];
        }

        output.push_str(rest);
        Ok(output)
    }

    pub(super) fn style_value_to_string_cached(
        &self,
        value: &StyleValue,
        variables: &HashMap<String, StyleValue>,
        strict_animation_tokens: bool,
    ) -> Result<String, String> {
        match value {
            StyleValue::Literal(value) => {
                self.resolve_embedded_references_cached(value, variables, strict_animation_tokens)
            }
            StyleValue::Prop(name) => {
                if let Some(value) = self.lookup_variable(variables, &prop_variable_key(name)) {
                    return self.style_value_to_string_cached(
                        value,
                        variables,
                        strict_animation_tokens,
                    );
                }
                Ok(String::new())
            }
            StyleValue::Var(name) => {
                if let Some(value) = self.lookup_variable(variables, name) {
                    return self.style_value_to_string_cached(
                        value,
                        variables,
                        strict_animation_tokens,
                    );
                }
                self.resolve_theme_reference(name, strict_animation_tokens)
            }
        }
    }

    pub(super) fn find_unresolved_animation_token_cached(&self, value: &str) -> Option<String> {
        let mut rest = value;

        loop {
            let var_start = rest.find("var(");
            let Some(start) = var_start else {
                break;
            };

            let reference_start = start + "var(".len();
            let end = rest[reference_start..].find(')')?;
            let reference = rest[reference_start..reference_start + end].trim();
            let token_name = self.cached_theme_token_name(reference);
            let token_value = self.cached_theme_token_value(reference);
            if token_name.starts_with("animation.") && token_value.is_missing() {
                return Some(token_name.to_string());
            }
            rest = &rest[reference_start + end + 1..];
        }

        None
    }

    pub(super) fn validate_animation_value_with_variables(
        &self,
        value: &StyleValue,
        variables: &HashMap<String, StyleValue>,
    ) -> Result<(), String> {
        match value {
            StyleValue::Literal(value) => {
                if let Some(name) = self.find_unresolved_animation_token_cached(value) {
                    return Err(name);
                }
                Ok(())
            }
            StyleValue::Var(name) => variables
                .get(name)
                .map(|value| self.validate_animation_value_with_variables(value, variables))
                .unwrap_or_else(|| {
                    let token_name = self.cached_theme_token_name(name);
                    match self.cached_theme_token_value(name) {
                        CachedThemeTokenValue::Error(error) => Err(error.to_string()),
                        value if token_name.starts_with("animation.") && value.is_missing() => {
                            Err(token_name.to_string())
                        }
                        _ => Ok(()),
                    }
                }),
            StyleValue::Prop(name) => self
                .lookup_variable(variables, &prop_variable_key(name))
                .map(|value| self.validate_animation_value_with_variables(value, variables))
                .unwrap_or(Ok(())),
        }
    }

    pub(super) fn resolve_color_with_variables(
        &self,
        value: &StyleValue,
        variables: &HashMap<String, StyleValue>,
    ) -> Color {
        self.resolve_color_with_variables_inner(value, variables, 0)
            .unwrap_or_else(|| {
                let resolved = self.resolve_value_with_variables(value, variables);
                Color::from_css(&resolved).unwrap_or(Color::TRANSPARENT)
            })
    }

    pub(super) fn resolve_color_with_variables_inner(
        &self,
        value: &StyleValue,
        variables: &HashMap<String, StyleValue>,
        depth: u8,
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
            StyleValue::Var(name) => {
                if let Some(value) = self.lookup_variable(variables, name) {
                    return self.resolve_color_with_variables_inner(
                        value,
                        variables,
                        depth.saturating_add(1),
                    );
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
            StyleValue::Prop(name) => self
                .lookup_variable(variables, &prop_variable_key(name))
                .and_then(|value| {
                    self.resolve_color_with_variables_inner(
                        value,
                        variables,
                        depth.saturating_add(1),
                    )
                }),
        }
    }

    pub(super) fn resolve_number_with_variables(
        &self,
        value: &StyleValue,
        variables: &HashMap<String, StyleValue>,
    ) -> f32 {
        self.resolve_number_with_variables_inner(value, variables, 0)
            .unwrap_or_else(|| parse_px(&self.resolve_value_with_variables(value, variables)))
    }

    pub(super) fn resolve_number_with_variables_inner(
        &self,
        value: &StyleValue,
        variables: &HashMap<String, StyleValue>,
        depth: u8,
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
            StyleValue::Var(name) => {
                if let Some(value) = self.lookup_variable(variables, name) {
                    return self.resolve_number_with_variables_inner(
                        value,
                        variables,
                        depth.saturating_add(1),
                    );
                }
                match self.cached_theme_token_value(name) {
                    CachedThemeTokenValue::Number(value) => Some(value as f32),
                    CachedThemeTokenValue::String(value) => Some(parse_px(&value)),
                    CachedThemeTokenValue::Bool(_) => Some(0.0),
                    CachedThemeTokenValue::Error(_) => None,
                    CachedThemeTokenValue::Missing => None,
                }
            }
            StyleValue::Prop(name) => self
                .lookup_variable(variables, &prop_variable_key(name))
                .and_then(|value| {
                    self.resolve_number_with_variables_inner(
                        value,
                        variables,
                        depth.saturating_add(1),
                    )
                }),
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
