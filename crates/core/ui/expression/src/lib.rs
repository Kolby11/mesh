//! Shared compilation and evaluation for expressions embedded in `.mesh`
//! templates.
//!
//! A component is parsed once into [`CompiledExpression`] values.  The live
//! runtime registers those values as lexical Luau closures, while preview uses
//! the same Luau expression body in a host environment.  Keeping the source
//! and validation boundary shared prevents preview-only parsing from becoming
//! a second template language.

use mlua::{Lua, LuaSerdeExt, Value as LuaValue};
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::{Mutex, OnceLock};

const PARSER_STACK_BYTES: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
pub enum ExpressionCompileError {
    #[error("expression is empty")]
    Empty,
    #[error("malformed Luau expression")]
    Malformed,
    #[error(
        "expression contains unsupported non-ASCII character {character:?} at byte offset {byte_offset}"
    )]
    NonAscii { character: char, byte_offset: usize },
    #[error("expression parser panicked")]
    ParserPanicked,
    #[error("could not start expression parser: {0}")]
    ParserThread(String),
}

#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
pub enum ExpressionEvaluationError {
    #[error("Luau expression failed: {0}")]
    Lua(String),
    #[error("Luau expression returned a value that cannot be represented as JSON: {0}")]
    Json(String),
}

/// The validated, cacheable semantic unit shared by every template consumer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledExpression {
    source: String,
}

pub type SharedCompiledExpression = Arc<CompiledExpression>;

impl CompiledExpression {
    pub fn source(&self) -> &str {
        &self.source
    }
}

static EXPRESSION_CACHE: OnceLock<Mutex<HashMap<String, SharedCompiledExpression>>> =
    OnceLock::new();

/// Parse and cache one expression. Repeated references from markup, preview,
/// and runtime setup return the same shared semantic value across threads.
pub fn compile_expression(
    source: &str,
) -> Result<SharedCompiledExpression, ExpressionCompileError> {
    let source = source.trim();
    if source.is_empty() {
        return Err(ExpressionCompileError::Empty);
    }
    let non_ascii = first_non_ascii(source);

    let cache = EXPRESSION_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let mut cache = cache.lock().expect("expression cache poisoned");
    if let Some(compiled) = cache.get(source).cloned() {
        return Ok(compiled);
    }

    let candidate = format!("return ({source})");
    let parsed = std::thread::Builder::new()
        .stack_size(PARSER_STACK_BYTES)
        .spawn(move || full_moon::parse(&candidate).is_ok())
        .map_err(|error| ExpressionCompileError::ParserThread(error.to_string()))?
        .join()
        .map_err(|_| {
            non_ascii.map_or(
                ExpressionCompileError::ParserPanicked,
                |(byte_offset, character)| ExpressionCompileError::NonAscii {
                    character,
                    byte_offset,
                },
            )
        })?;
    if !parsed {
        return Err(non_ascii.map_or(
            ExpressionCompileError::Malformed,
            |(byte_offset, character)| ExpressionCompileError::NonAscii {
                character,
                byte_offset,
            },
        ));
    }

    let compiled = Arc::new(CompiledExpression {
        source: source.to_owned(),
    });
    cache.insert(source.to_owned(), Arc::clone(&compiled));
    Ok(compiled)
}

fn first_non_ascii(source: &str) -> Option<(usize, char)> {
    source
        .char_indices()
        .find(|(_, character)| !character.is_ascii())
}

/// Evaluate a compiled expression in a renderer-owned host environment.
///
/// The preview environment intentionally contains only JSON state and the
/// locale function. It still executes through Luau, so truthiness, operators,
/// indexing, concatenation, table literals, and translation calls use the same
/// language semantics as live component closures.
pub fn evaluate_preview<F>(
    expression: &CompiledExpression,
    variables: &Map<String, Value>,
    locals: &Map<String, Value>,
    translate: F,
) -> Result<Value, ExpressionEvaluationError>
where
    F: Fn(&str) -> Option<String>,
{
    let lua = Lua::new();
    let result = lua.scope(|scope| -> mlua::Result<Value> {
        let translator = scope.create_function(|_, key: String| {
            Ok(translate(&key).unwrap_or_else(|| format!("!!{key}")))
        })?;
        let environment = lua.globals();

        for (name, value) in variables {
            let value = json_to_lua(&lua, value)?;
            environment.set(name.as_str(), value)?;
        }
        for (name, value) in locals {
            let value = json_to_lua(&lua, value)?;
            environment.set(name.as_str(), value)?;
        }
        environment.set("t", translator)?;

        let value: LuaValue = lua
            .load(format!("return ({})", expression.source()))
            .set_name("mesh-template-preview")
            .eval()?;
        if matches!(value, LuaValue::Nil) {
            return Ok(Value::Null);
        }
        lua.from_value(value)
    });
    result.map_err(|error| ExpressionEvaluationError::Lua(error.to_string()))
}

fn json_to_lua(lua: &Lua, value: &Value) -> mlua::Result<LuaValue> {
    match value {
        Value::Null => Ok(LuaValue::Nil),
        Value::Bool(value) => Ok(LuaValue::Boolean(*value)),
        Value::Number(number) => {
            if let Some(value) = number.as_i64() {
                Ok(LuaValue::Integer(value))
            } else if let Some(value) = number.as_u64() {
                i64::try_from(value).map(LuaValue::Integer).or_else(|_| {
                    number.as_f64().map(LuaValue::Number).ok_or_else(|| {
                        mlua::Error::external("JSON integer does not fit in Luau number")
                    })
                })
            } else {
                number
                    .as_f64()
                    .map(LuaValue::Number)
                    .ok_or_else(|| mlua::Error::external("invalid JSON number"))
            }
        }
        Value::String(value) => Ok(LuaValue::String(lua.create_string(value)?)),
        Value::Array(values) => {
            let table = lua.create_table_with_capacity(values.len(), 0)?;
            for (index, value) in values.iter().enumerate() {
                table.raw_set(index + 1, json_to_lua(lua, value)?)?;
            }
            Ok(LuaValue::Table(table))
        }
        Value::Object(values) => {
            let table = lua.create_table_with_capacity(0, values.len())?;
            for (name, value) in values {
                table.raw_set(name.as_str(), json_to_lua(lua, value)?)?;
            }
            Ok(LuaValue::Table(table))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compiles_once_and_preserves_luau_semantics() {
        let first = compile_expression("0 or 'fallback'").unwrap();
        let second = compile_expression("0 or 'fallback'").unwrap();
        assert!(Arc::ptr_eq(&first, &second));
        assert_eq!(first.source(), "0 or 'fallback'");
    }

    #[test]
    fn preview_uses_luau_truthiness_and_translation_host() {
        let expression = compile_expression("t((enabled and 'nav.open') or fallback)").unwrap();
        let variables = Map::from_iter([
            ("enabled".into(), Value::Bool(true)),
            ("fallback".into(), Value::String("missing".into())),
        ]);
        let value = evaluate_preview(&expression, &variables, &Map::new(), |key| {
            (key == "nav.open").then(|| "Open".into())
        })
        .unwrap();
        assert_eq!(value, Value::String("Open".into()));
    }

    #[test]
    fn preview_treats_missing_and_null_values_as_luau_nil() {
        let expression = compile_expression("nothing or fallback").unwrap();
        let variables = Map::from_iter([
            ("nothing".into(), Value::Null),
            ("fallback".into(), Value::String("fallback".into())),
        ]);
        assert_eq!(
            evaluate_preview(&expression, &variables, &Map::new(), |_| None).unwrap(),
            Value::String("fallback".into())
        );
    }

    #[test]
    fn malformed_expression_is_rejected() {
        assert_eq!(
            compile_expression("value +").unwrap_err(),
            ExpressionCompileError::Malformed
        );
    }

    #[test]
    fn non_ascii_expression_syntax_is_diagnosed_without_panicking() {
        for source in [
            "é + 1",
            "value .. é",
            "é == value",
            "é ~= value",
            "é < value",
            "é <= value",
            "é > value",
            "é >= value",
            "é and value",
            "é or value",
            "not é",
            "#é",
        ] {
            let byte_offset = source
                .char_indices()
                .find(|(_, character)| !character.is_ascii())
                .map(|(byte_offset, _)| byte_offset)
                .expect("test expression contains non-ASCII input");
            let result = std::panic::catch_unwind(|| compile_expression(source));
            let result = result.expect("non-ASCII expression scanning must not panic");
            assert_eq!(
                result,
                Err(ExpressionCompileError::NonAscii {
                    character: 'é',
                    byte_offset,
                }),
                "unexpected result for {source:?}"
            );
        }
    }

    #[test]
    fn unicode_string_literals_remain_valid_luau_expressions() {
        let expression = compile_expression("'café'").unwrap();
        assert_eq!(
            evaluate_preview(&expression, &Map::new(), &Map::new(), |_| None).unwrap(),
            Value::String("café".into())
        );
    }
}
