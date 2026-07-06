use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

pub(crate) fn json_value_to_string(value: serde_json::Value) -> String {
    match value {
        serde_json::Value::Null => String::new(),
        serde_json::Value::String(value) => value,
        other => other.to_string(),
    }
}

fn json_value_ref_to_string(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Null => String::new(),
        serde_json::Value::String(value) => value.clone(),
        other => other.to_string(),
    }
}

/// Evaluate a template expression against the current variable store.
///
/// Supports a subset of Luau expression syntax:
/// - `"string literal"` / `'string literal'`
/// - `not x` - boolean negation
/// - `cond and a or b` - ternary (Lua idiom)
/// - `x == y`, `x ~= y`, `x > y`, `x >= y`, `x < y`, `x <= y` - comparisons
/// - `x .. y` - string concatenation
/// - `t(expr)` - translation where expr is any expression (literal, variable, concat, etc.)
/// - `variable` / `a.b.c` - variable lookup
///
/// Expressions are static after module compilation, so the parsed form is
/// memoized per expression string; only evaluation runs per frame.
pub(crate) fn eval_expr(expr: &str, store: &dyn mesh_core_elements::VariableStore) -> String {
    let compiled = compiled_expr(expr);
    eval_compiled(&compiled, store).into_string()
}

pub(crate) fn uses_translation(expr: &str) -> bool {
    compiled_expr(expr).uses_translation()
}

/// Upper bound on memoized expressions. Template expressions are a fixed set
/// per loaded module, so this is only reached through repeated hot reloads
/// with changing sources; clearing then is cheap and self-corrects.
const EXPR_CACHE_CAPACITY: usize = 4096;

thread_local! {
    static EXPR_CACHE: RefCell<HashMap<String, Rc<CompiledExpr>>> =
        RefCell::new(HashMap::new());
}

fn compiled_expr(expr: &str) -> Rc<CompiledExpr> {
    EXPR_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some(compiled) = cache.get(expr) {
            return Rc::clone(compiled);
        }
        let compiled = Rc::new(parse_expr(expr));
        if cache.len() >= EXPR_CACHE_CAPACITY {
            cache.clear();
        }
        cache.insert(expr.to_string(), Rc::clone(&compiled));
        compiled
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CompareOp {
    NotEq,
    Eq,
    Ge,
    Le,
    Gt,
    Lt,
}

#[derive(Debug)]
enum CompiledExpr {
    /// `#name` — length of an array/string/object variable.
    Length(String),
    Not(Rc<CompiledExpr>),
    /// `cond and a or b` Lua ternary idiom.
    Ternary {
        cond: Rc<CompiledExpr>,
        then_val: Rc<CompiledExpr>,
        else_val: Rc<CompiledExpr>,
    },
    And(Rc<CompiledExpr>, Rc<CompiledExpr>),
    Or(Rc<CompiledExpr>, Rc<CompiledExpr>),
    Compare {
        op: CompareOp,
        lhs: Rc<CompiledExpr>,
        rhs: Rc<CompiledExpr>,
    },
    Concat(Rc<CompiledExpr>, Rc<CompiledExpr>),
    /// `t(expr)` — evaluate the inner expression, then translate the result.
    TranslateExpr(Rc<CompiledExpr>),
    Literal(String),
    /// Bare variable or dotted path lookup.
    Path(String),
}

impl CompiledExpr {
    fn uses_translation(&self) -> bool {
        match self {
            Self::TranslateExpr(_) => true,
            Self::Not(inner) => inner.uses_translation(),
            Self::Ternary {
                cond,
                then_val,
                else_val,
            } => {
                cond.uses_translation()
                    || then_val.uses_translation()
                    || else_val.uses_translation()
            }
            Self::And(lhs, rhs)
            | Self::Or(lhs, rhs)
            | Self::Concat(lhs, rhs)
            | Self::Compare { lhs, rhs, .. } => lhs.uses_translation() || rhs.uses_translation(),
            Self::Literal(_) | Self::Path(_) | Self::Length(_) => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
enum ExprValue {
    Bool(bool),
    Number {
        value: f64,
        source: Option<NumberSource>,
    },
    String(String),
}

#[derive(Debug, Clone, PartialEq)]
enum NumberSource {
    Json(serde_json::Number),
    Literal(String),
}

impl NumberSource {
    fn into_string(self) -> String {
        match self {
            Self::Json(value) => value.to_string(),
            Self::Literal(value) => value,
        }
    }

    fn as_string(&self) -> String {
        match self {
            Self::Json(value) => value.to_string(),
            Self::Literal(value) => value.clone(),
        }
    }
}

impl ExprValue {
    fn into_string(self) -> String {
        match self {
            Self::Bool(value) => {
                if value {
                    "true".into()
                } else {
                    "false".into()
                }
            }
            Self::Number { value, source } => {
                source.map_or_else(|| format_number(value), NumberSource::into_string)
            }
            Self::String(value) => value,
        }
    }

    fn as_string(&self) -> String {
        match self {
            Self::Bool(value) => {
                if *value {
                    "true".into()
                } else {
                    "false".into()
                }
            }
            Self::Number { value, source } => source
                .as_ref()
                .map(NumberSource::as_string)
                .unwrap_or_else(|| format_number(*value)),
            Self::String(value) => value.clone(),
        }
    }

    fn is_truthy(&self) -> bool {
        match self {
            Self::Bool(value) => *value,
            Self::Number { value, .. } => *value != 0.0,
            Self::String(value) => !matches!(value.as_str(), "false" | "nil" | "" | "0"),
        }
    }

    fn numeric_value(&self) -> Option<f64> {
        match self {
            Self::Number { value, .. } => Some(*value),
            Self::String(value) => value.parse::<f64>().ok(),
            Self::Bool(_) => None,
        }
    }
}

fn parse_expr(expr: &str) -> CompiledExpr {
    let expr = expr.trim();

    if expr.starts_with('(') && expr.ends_with(')') && balanced_parens(expr) {
        return parse_expr(&expr[1..expr.len() - 1]);
    }

    if let Some(inner) = expr.strip_prefix('#') {
        return CompiledExpr::Length(inner.trim().to_string());
    }

    if let Some(inner) = expr.strip_prefix("not ") {
        return CompiledExpr::Not(Rc::new(parse_expr(inner.trim())));
    }

    if let Some((lhs, rest)) = split_op(expr, " and ") {
        if let Some((then_val, else_val)) = split_op(rest, " or ") {
            return CompiledExpr::Ternary {
                cond: Rc::new(parse_expr(lhs)),
                then_val: Rc::new(parse_expr(then_val)),
                else_val: Rc::new(parse_expr(else_val)),
            };
        }
        return CompiledExpr::And(Rc::new(parse_expr(lhs)), Rc::new(parse_expr(rest)));
    }

    if let Some((lhs, rhs)) = split_op(expr, " or ") {
        return CompiledExpr::Or(Rc::new(parse_expr(lhs)), Rc::new(parse_expr(rhs)));
    }

    for (token, op) in [
        ("~=", CompareOp::NotEq),
        ("==", CompareOp::Eq),
        (">=", CompareOp::Ge),
        ("<=", CompareOp::Le),
        (">", CompareOp::Gt),
        ("<", CompareOp::Lt),
    ] {
        if let Some((lhs, rhs)) = split_op(expr, token) {
            return CompiledExpr::Compare {
                op,
                lhs: Rc::new(parse_expr(lhs)),
                rhs: Rc::new(parse_expr(rhs)),
            };
        }
    }

    if let Some((lhs, rhs)) = split_op(expr, " .. ") {
        return CompiledExpr::Concat(Rc::new(parse_expr(lhs)), Rc::new(parse_expr(rhs)));
    }

    if let Some(arg) = expr.strip_prefix("t(").and_then(|s| s.strip_suffix(')')) {
        return CompiledExpr::TranslateExpr(Rc::new(parse_expr(arg.trim())));
    }

    if let Some(s) = strip_string_literal(expr) {
        return CompiledExpr::Literal(s);
    }

    CompiledExpr::Path(expr.to_string())
}

fn eval_compiled(expr: &CompiledExpr, store: &dyn mesh_core_elements::VariableStore) -> ExprValue {
    match expr {
        CompiledExpr::Length(name) => {
            if let Some(value) = store.get_ref(name) {
                return ExprValue::Number {
                    value: json_value_len(value) as f64,
                    source: None,
                };
            }
            match store.get(name) {
                Some(value) => ExprValue::Number {
                    value: json_value_len(&value) as f64,
                    source: None,
                },
                _ => ExprValue::Number {
                    value: 0.0,
                    source: None,
                },
            }
        }
        CompiledExpr::Not(inner) => ExprValue::Bool(!eval_compiled(inner, store).is_truthy()),
        CompiledExpr::Ternary {
            cond,
            then_val,
            else_val,
        } => {
            if eval_compiled(cond, store).is_truthy() {
                eval_compiled(then_val, store)
            } else {
                eval_compiled(else_val, store)
            }
        }
        CompiledExpr::And(lhs, rhs) => {
            if !eval_compiled(lhs, store).is_truthy() {
                return ExprValue::Bool(false);
            }
            ExprValue::Bool(eval_compiled(rhs, store).is_truthy())
        }
        CompiledExpr::Or(lhs, rhs) => {
            if eval_compiled(lhs, store).is_truthy() {
                return ExprValue::Bool(true);
            }
            ExprValue::Bool(eval_compiled(rhs, store).is_truthy())
        }
        CompiledExpr::Compare { op, lhs, rhs } => {
            let l = eval_compiled(lhs, store);
            let r = eval_compiled(rhs, store);
            let result = if let (Some(ln), Some(rn)) = (l.numeric_value(), r.numeric_value()) {
                match op {
                    CompareOp::Eq => (ln - rn).abs() < f64::EPSILON,
                    CompareOp::NotEq => (ln - rn).abs() >= f64::EPSILON,
                    CompareOp::Ge => ln >= rn,
                    CompareOp::Le => ln <= rn,
                    CompareOp::Gt => ln > rn,
                    CompareOp::Lt => ln < rn,
                }
            } else {
                let l = l.as_string();
                let r = r.as_string();
                match op {
                    CompareOp::Eq => l == r,
                    CompareOp::NotEq => l != r,
                    _ => false,
                }
            };
            ExprValue::Bool(result)
        }
        CompiledExpr::Concat(lhs, rhs) => {
            let l = eval_compiled(lhs, store).into_string();
            let r = eval_compiled(rhs, store).into_string();
            ExprValue::String(format!("{l}{r}"))
        }
        CompiledExpr::TranslateExpr(inner) => {
            let resolved = eval_compiled(inner, store).into_string();
            ExprValue::String(store.translate(&resolved).unwrap_or(resolved))
        }
        CompiledExpr::Literal(s) => ExprValue::String(s.clone()),
        CompiledExpr::Path(path) => eval_path(path, store),
    }
}

fn split_op<'a>(expr: &'a str, op: &str) -> Option<(&'a str, &'a str)> {
    let bytes = expr.as_bytes();
    let mut depth = 0i32;
    let mut in_string = false;
    let mut quote = b'"';
    let mut i = 0usize;

    while i < bytes.len() {
        let b = bytes[i];
        if in_string {
            if b == quote && (i == 0 || bytes[i - 1] != b'\\') {
                in_string = false;
            }
            i += 1;
            continue;
        }
        if b == b'"' || b == b'\'' {
            in_string = true;
            quote = b;
            i += 1;
            continue;
        }
        if b == b'(' {
            depth += 1;
            i += 1;
            continue;
        }
        if b == b')' {
            depth -= 1;
            i += 1;
            continue;
        }
        if depth == 0 && expr[i..].starts_with(op) {
            return Some((&expr[..i], &expr[i + op.len()..]));
        }
        i += 1;
    }
    None
}

fn balanced_parens(expr: &str) -> bool {
    let mut depth = 0i32;
    for (i, b) in expr.bytes().enumerate() {
        if b == b'(' {
            depth += 1;
        } else if b == b')' {
            depth -= 1;
            if depth == 0 && i < expr.len() - 1 {
                return false;
            }
        }
    }
    depth == 0
}

fn eval_path(expr: &str, store: &dyn mesh_core_elements::VariableStore) -> ExprValue {
    if let Some(value) = store.get_ref(expr) {
        return json_value_ref_to_expr_value(value);
    }

    let parts: Vec<&str> = expr.splitn(2, '.').collect();
    if parts.len() == 2 {
        if let Some(root) = store.get_ref(parts[0]) {
            if let Some(nested) = json_path_ref(root, parts[1]) {
                return json_value_ref_to_expr_value(nested);
            }
        }
    }

    if let Some(value) = store.get(expr) {
        return json_value_to_expr_value(value);
    }

    if parts.len() == 2 {
        if let Some(root) = store.get(parts[0]) {
            if let Some(nested) = json_path(root, parts[1]) {
                return json_value_to_expr_value(nested);
            }
        }
    }

    if let Ok(value) = expr.parse::<f64>() {
        return ExprValue::Number {
            value,
            source: Some(NumberSource::Literal(expr.to_string())),
        };
    }

    ExprValue::String(expr.to_string())
}

fn json_value_to_expr_value(value: serde_json::Value) -> ExprValue {
    match value {
        serde_json::Value::Bool(value) => ExprValue::Bool(value),
        serde_json::Value::Number(value) => {
            if let Some(number) = value.as_f64() {
                ExprValue::Number {
                    value: number,
                    source: Some(NumberSource::Json(value)),
                }
            } else {
                ExprValue::String(value.to_string())
            }
        }
        other => ExprValue::String(json_value_to_string(other)),
    }
}

fn json_value_ref_to_expr_value(value: &serde_json::Value) -> ExprValue {
    match value {
        serde_json::Value::Bool(value) => ExprValue::Bool(*value),
        serde_json::Value::Number(value) => value
            .as_f64()
            .map(|number| ExprValue::Number {
                value: number,
                source: Some(NumberSource::Json(value.clone())),
            })
            .unwrap_or_else(|| ExprValue::String(value.to_string())),
        other => ExprValue::String(json_value_ref_to_string(other)),
    }
}

fn format_number(value: f64) -> String {
    if value.fract() == 0.0 && value >= i64::MIN as f64 && value <= i64::MAX as f64 {
        (value as i64).to_string()
    } else {
        value.to_string()
    }
}

fn json_value_len(value: &serde_json::Value) -> usize {
    match value {
        serde_json::Value::Array(arr) => arr.len(),
        serde_json::Value::String(s) => s.len(),
        serde_json::Value::Object(obj) => obj.len(),
        _ => 0,
    }
}

fn json_path_ref<'a>(
    mut value: &'a serde_json::Value,
    path: &str,
) -> Option<&'a serde_json::Value> {
    for key in path.split('.') {
        value = value.get(key)?;
    }
    Some(value)
}

fn json_path(mut value: serde_json::Value, path: &str) -> Option<serde_json::Value> {
    for key in path.split('.') {
        value = value.get(key)?.clone();
    }
    Some(value)
}

fn strip_string_literal(s: &str) -> Option<String> {
    let s = s.trim();
    if s.len() >= 2 {
        let q = s.chars().next()?;
        if (q == '"' || q == '\'') && s.ends_with(q) {
            return Some(s[1..s.len() - 1].to_string());
        }
    }
    None
}
