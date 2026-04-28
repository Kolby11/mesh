pub(crate) fn json_value_to_string(value: serde_json::Value) -> String {
    match value {
        serde_json::Value::Null => String::new(),
        serde_json::Value::String(value) => value,
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
/// - `t("key")` / `t(variable)` - translation
/// - `variable` / `a.b.c` - variable lookup
pub(crate) fn eval_expr(expr: &str, store: &dyn mesh_ui::VariableStore) -> String {
    let expr = expr.trim();

    if expr.starts_with('(') && expr.ends_with(')') && balanced_parens(expr) {
        return eval_expr(&expr[1..expr.len() - 1], store);
    }

    if let Some(inner) = expr.strip_prefix('#') {
        let inner = inner.trim();
        return match store.get(inner) {
            Some(serde_json::Value::Array(arr)) => arr.len().to_string(),
            Some(serde_json::Value::String(s)) => s.len().to_string(),
            Some(serde_json::Value::Object(obj)) => obj.len().to_string(),
            _ => "0".into(),
        };
    }

    if let Some(inner) = expr.strip_prefix("not ") {
        let value = eval_expr(inner.trim(), store);
        let is_truthy = !matches!(value.as_str(), "false" | "nil" | "" | "0");
        return if is_truthy {
            "false".into()
        } else {
            "true".into()
        };
    }

    if let Some((lhs, rest)) = split_op(expr, " and ") {
        if let Some((then_val, else_val)) = split_op(rest, " or ") {
            let cond_result = eval_expr(lhs, store);
            let truthy = !matches!(cond_result.as_str(), "false" | "nil" | "" | "0");
            return if truthy {
                eval_expr(then_val, store)
            } else {
                eval_expr(else_val, store)
            };
        }
        let l = eval_expr(lhs, store);
        if matches!(l.as_str(), "false" | "nil" | "" | "0") {
            return "false".into();
        }
        let r = eval_expr(rest, store);
        return if matches!(r.as_str(), "false" | "nil" | "" | "0") {
            "false".into()
        } else {
            "true".into()
        };
    }

    if let Some((lhs, rhs)) = split_op(expr, " or ") {
        let l = eval_expr(lhs, store);
        if !matches!(l.as_str(), "false" | "nil" | "" | "0") {
            return "true".into();
        }
        let r = eval_expr(rhs, store);
        return if matches!(r.as_str(), "false" | "nil" | "" | "0") {
            "false".into()
        } else {
            "true".into()
        };
    }

    for op in &["~=", "==", ">=", "<=", ">", "<"] {
        if let Some((lhs, rhs)) = split_op(expr, op) {
            let l = eval_expr(lhs, store);
            let r = eval_expr(rhs, store);
            let result = if let (Ok(ln), Ok(rn)) = (l.parse::<f64>(), r.parse::<f64>()) {
                match *op {
                    "==" => (ln - rn).abs() < f64::EPSILON,
                    "~=" => (ln - rn).abs() >= f64::EPSILON,
                    ">=" => ln >= rn,
                    "<=" => ln <= rn,
                    ">" => ln > rn,
                    "<" => ln < rn,
                    _ => false,
                }
            } else {
                match *op {
                    "==" => l == r,
                    "~=" => l != r,
                    _ => false,
                }
            };
            return if result {
                "true".into()
            } else {
                "false".into()
            };
        }
    }

    if let Some((lhs, rhs)) = split_op(expr, " .. ") {
        let l = eval_expr(lhs, store);
        let r = eval_expr(rhs, store);
        return format!("{l}{r}");
    }

    if let Some(arg) = expr.strip_prefix("t(").and_then(|s| s.strip_suffix(')')) {
        let arg = arg.trim();
        if let Some(key) = strip_string_literal(arg) {
            return store.translate(&key).unwrap_or(key);
        }
        let resolved = eval_path(arg, store);
        return store.translate(&resolved).unwrap_or(resolved);
    }

    if let Some(s) = strip_string_literal(expr) {
        return s;
    }

    eval_path(expr, store)
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

fn eval_path(expr: &str, store: &dyn mesh_ui::VariableStore) -> String {
    if let Some(value) = store.get(expr) {
        return json_value_to_string(value);
    }

    let parts: Vec<&str> = expr.splitn(2, '.').collect();
    if parts.len() == 2 {
        if let Some(root) = store.get(parts[0]) {
            if let Some(nested) = json_path(root, parts[1]) {
                return json_value_to_string(nested);
            }
        }
    }

    expr.to_string()
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
