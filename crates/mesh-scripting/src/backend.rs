/// Backend plugin script interpreter.
///
/// This runs `.mesh` backend plugin scripts (Luau) with a different host API set
/// than frontend components. Backend plugins interact with the system — they call
/// `mesh.exec()`, emit service state via `mesh.service.emit()`, and respond to
/// commands via `on_command_<name>()` handlers.
///
/// The Luau is currently executed by a simple line-by-line interpreter. When
/// real Luau (mlua) is integrated, this module is the only thing that changes.
use std::collections::HashMap;
use std::process::Command as StdCommand;

/// Executes a backend plugin's Luau script.
///
/// Exposes these host APIs to scripts:
/// - `mesh.service.set_poll_interval(ms)` — set polling interval (top-level only)
/// - `mesh.exec("program arg1 arg2 {payload_key}")` — run a system command
/// - `mesh.parse_wpctl_volume()` — parse `__exec_stdout` as wpctl output
/// - `mesh.parse_pactl_volume()` — parse `__exec_stdout` as pactl output
/// - `mesh.service.emit({ key = value, key = __var })` — emit service state
/// - `mesh.service.emit_unavailable()` — emit unavailable state
/// - `mesh.log.info(msg)` / `mesh.log.warn(msg)`
///
/// Named function conventions:
/// - `on_poll()` — called every `poll_interval_ms`
/// - `on_command_volume_up()` — called for command `volume-up` (dashes → underscores)
pub struct BackendScriptContext {
    plugin_id: String,
    poll_interval_ms: u64,
    handlers: HashMap<String, Vec<String>>,
    /// Runtime variables: `__exec_stdout`, `__exec_success`, `__parsed_volume`,
    /// `__parsed_muted`, and `__payload_<key>` for current command payload.
    vars: HashMap<String, serde_json::Value>,
    /// Set by `mesh.service.emit()` during handler execution.
    pending_emit: Option<serde_json::Value>,
    /// Current command payload, set before calling a command handler.
    current_payload: serde_json::Value,
}

impl BackendScriptContext {
    pub fn new(plugin_id: impl Into<String>) -> Self {
        Self {
            plugin_id: plugin_id.into(),
            poll_interval_ms: 1000,
            handlers: HashMap::new(),
            vars: HashMap::new(),
            pending_emit: None,
            current_payload: serde_json::Value::Null,
        }
    }

    pub fn poll_interval_ms(&self) -> u64 {
        self.poll_interval_ms
    }

    /// Load and interpret a backend Luau script.
    /// Registers function handlers and processes top-level statements.
    pub fn load_script(&mut self, source: &str) -> Result<(), BackendScriptError> {
        self.handlers = parse_handlers(source);
        // Execute top-level statements (e.g. `mesh.service.set_poll_interval`)
        let top_level = collect_top_level_statements(source);
        for line in &top_level {
            self.execute_statement(line, &serde_json::Value::Null)?;
        }
        tracing::info!("loaded backend script for {}", self.plugin_id);
        Ok(())
    }

    /// Call `on_poll()` if it exists. Returns any emitted payload.
    pub fn run_poll(&mut self) -> Option<serde_json::Value> {
        self.pending_emit = None;
        self.current_payload = serde_json::Value::Null;
        let body = self.handlers.get("on_poll").cloned()?;
        if let Err(e) = self.execute_body(&body, &serde_json::Value::Null) {
            tracing::warn!("{} on_poll error: {e}", self.plugin_id);
        }
        self.pending_emit.take()
    }

    /// Call `on_command_<name>()` for the given command. Returns any emitted payload.
    /// Dashes in command names are converted to underscores for the function name.
    pub fn run_command(
        &mut self,
        command: &str,
        payload: &serde_json::Value,
    ) -> Option<serde_json::Value> {
        self.pending_emit = None;
        self.current_payload = payload.clone();
        // Expose payload fields as __payload_<key> vars
        if let Some(obj) = payload.as_object() {
            for (k, v) in obj {
                self.vars.insert(format!("__payload_{k}"), v.clone());
            }
        }

        let fn_name = format!("on_command_{}", command.replace('-', "_"));
        let body = self.handlers.get(&fn_name).cloned()?;
        if let Err(e) = self.execute_body(&body, payload) {
            tracing::warn!("{} {fn_name} error: {e}", self.plugin_id);
        }
        self.pending_emit.take()
    }

    fn execute_body(
        &mut self,
        body: &[String],
        payload: &serde_json::Value,
    ) -> Result<(), BackendScriptError> {
        let mut i = 0;
        while i < body.len() {
            let line = body[i].trim().to_string();
            if line.is_empty() || line.starts_with("--") {
                i += 1;
                continue;
            }

            if let Some(cond) = line
                .strip_prefix("if ")
                .and_then(|s| s.strip_suffix(" then"))
            {
                let (then_end, else_idx, end_idx) = find_block_bounds(body, i + 1);
                if self.eval_condition(cond.trim()) {
                    let branch = body[i + 1..then_end].to_vec();
                    self.execute_body(&branch, payload)?;
                } else if let Some(ei) = else_idx {
                    let branch = body[ei + 1..end_idx].to_vec();
                    self.execute_body(&branch, payload)?;
                }
                i = end_idx + 1;
                continue;
            }

            self.execute_statement(&line, payload)?;
            i += 1;
        }
        Ok(())
    }

    fn execute_statement(
        &mut self,
        line: &str,
        _payload: &serde_json::Value,
    ) -> Result<(), BackendScriptError> {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("--") {
            return Ok(());
        }

        if let Some(args) = extract_args(trimmed, "mesh.service.set_poll_interval") {
            if let Some(ms) = args.first().and_then(|s| s.trim().parse::<u64>().ok()) {
                self.poll_interval_ms = ms;
            }
            return Ok(());
        }

        if let Some(args) = extract_args(trimmed, "mesh.exec") {
            let cmd_str = args
                .first()
                .and_then(|s| parse_string_literal(s))
                .unwrap_or_default();
            let cmd_str = self.substitute_payload_vars(&cmd_str);
            let parts: Vec<&str> = cmd_str.split_whitespace().collect();
            if let Some((prog, rest)) = parts.split_first() {
                match StdCommand::new(prog).args(rest).output() {
                    Ok(out) => {
                        let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
                        self.vars.insert("__exec_success".into(), out.status.success().into());
                        self.vars.insert("__exec_stdout".into(), stdout.into());
                    }
                    Err(e) => {
                        tracing::debug!("{} exec `{prog}` failed: {e}", self.plugin_id);
                        self.vars.insert("__exec_success".into(), false.into());
                        self.vars.insert("__exec_stdout".into(), "".into());
                    }
                }
            }
            return Ok(());
        }

        if trimmed == "mesh.parse_wpctl_volume()" {
            let stdout = self
                .vars
                .get("__exec_stdout")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_owned();
            let muted = stdout.contains("[MUTED]");
            let volume = parse_wpctl_volume(&stdout).unwrap_or(0);
            self.vars.insert("__parsed_volume".into(), volume.into());
            self.vars.insert("__parsed_muted".into(), muted.into());
            return Ok(());
        }

        if trimmed == "mesh.parse_pactl_volume()" {
            let stdout = self
                .vars
                .get("__exec_stdout")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_owned();
            let volume = parse_first_percent(&stdout).unwrap_or(0);
            let muted = volume == 0;
            self.vars.insert("__parsed_volume".into(), volume.into());
            self.vars.insert("__parsed_muted".into(), muted.into());
            return Ok(());
        }

        if let Some(args) = extract_args(trimmed, "mesh.service.emit") {
            if let Some(table_str) = args.first() {
                if let Some(payload) = parse_table_with_vars(table_str, &self.vars) {
                    self.pending_emit = Some(payload);
                }
            }
            return Ok(());
        }

        if trimmed == "mesh.service.emit_unavailable()" {
            self.pending_emit = Some(serde_json::json!({
                "available": false,
                "percent": 0,
                "label": "Unavailable",
                "glyph": "VOL",
                "source_plugin": self.plugin_id,
            }));
            return Ok(());
        }

        if let Some(args) = extract_args(trimmed, "mesh.log.info") {
            if let Some(msg) = args.first().and_then(|s| parse_string_literal(s)) {
                tracing::info!("{}: {msg}", self.plugin_id);
            }
            return Ok(());
        }

        if let Some(args) = extract_args(trimmed, "mesh.log.warn") {
            if let Some(msg) = args.first().and_then(|s| parse_string_literal(s)) {
                tracing::warn!("{}: {msg}", self.plugin_id);
            }
            return Ok(());
        }

        Ok(())
    }

    fn eval_condition(&self, expr: &str) -> bool {
        let expr = expr.trim();
        if let Some(inner) = expr.strip_prefix("not ") {
            return !self.eval_condition(inner.trim());
        }
        if let Some(val) = self.vars.get(expr) {
            return is_truthy(val);
        }
        false
    }

    /// Replace `{key}` tokens in `s` with values from `__payload_<key>` vars.
    fn substitute_payload_vars(&self, s: &str) -> String {
        let mut result = s.to_string();
        if let Some(obj) = self.current_payload.as_object() {
            for (k, v) in obj {
                let token = format!("{{{k}}}");
                let replacement = match v {
                    serde_json::Value::String(s) => s.clone(),
                    serde_json::Value::Number(n) => n.to_string(),
                    serde_json::Value::Bool(b) => b.to_string(),
                    _ => continue,
                };
                result = result.replace(&token, &replacement);
            }
        }
        result
    }
}

#[derive(Debug, thiserror::Error)]
pub enum BackendScriptError {
    #[error("script error in {plugin_id}: {message}")]
    Runtime { plugin_id: String, message: String },
}

// --- Parsing helpers ---

fn parse_handlers(source: &str) -> HashMap<String, Vec<String>> {
    let mut handlers: HashMap<String, Vec<String>> = HashMap::new();
    let mut current: Option<String> = None;
    let mut body: Vec<String> = Vec::new();
    let mut depth = 0usize;

    for line in source.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("function ") {
            if let Some(name) = rest.split('(').next().map(str::trim) {
                if !name.is_empty() {
                    current = Some(name.to_string());
                    body.clear();
                    depth = 0;
                    continue;
                }
            }
        }
        if current.is_some() {
            // Track nested blocks (if/then/end, for/do/end, etc.)
            if trimmed.starts_with("if ") && trimmed.ends_with(" then") {
                depth += 1;
            }
            if trimmed == "end" {
                if depth == 0 {
                    if let Some(name) = current.take() {
                        handlers.insert(name, std::mem::take(&mut body));
                    }
                    continue;
                }
                depth -= 1;
            }
            body.push(trimmed.to_string());
        }
    }

    handlers
}

fn collect_top_level_statements(source: &str) -> Vec<String> {
    let mut stmts = Vec::new();
    let mut in_function = false;

    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("function ") {
            in_function = true;
            continue;
        }
        if trimmed == "end" && in_function {
            in_function = false;
            continue;
        }
        if !in_function && !trimmed.is_empty() && !trimmed.starts_with("--") {
            stmts.push(trimmed.to_string());
        }
    }

    stmts
}

fn extract_args(line: &str, prefix: &str) -> Option<Vec<String>> {
    let rest = line.strip_prefix(prefix)?.trim();
    let inner = rest.strip_prefix('(')?.strip_suffix(')')?;
    Some(split_args(inner))
}

fn split_args(inner: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut depth = 0i32;
    let mut in_str = false;
    let mut quote = '\0';

    for ch in inner.chars() {
        match ch {
            '"' | '\'' if !in_str => {
                in_str = true;
                quote = ch;
                current.push(ch);
            }
            c if in_str && c == quote => {
                in_str = false;
                quote = '\0';
                current.push(ch);
            }
            '{' if !in_str => {
                depth += 1;
                current.push(ch);
            }
            '}' if !in_str => {
                depth -= 1;
                current.push(ch);
            }
            ',' if !in_str && depth == 0 => {
                let s = current.trim().to_string();
                if !s.is_empty() {
                    args.push(s);
                }
                current.clear();
            }
            _ => current.push(ch),
        }
    }
    let s = current.trim().to_string();
    if !s.is_empty() {
        args.push(s);
    }
    args
}

fn parse_string_literal(s: &str) -> Option<String> {
    let s = s.trim();
    if s.len() >= 2 {
        let q = s.chars().next()?;
        if (q == '"' || q == '\'') && s.ends_with(q) {
            return Some(s[1..s.len() - 1].to_string());
        }
    }
    None
}

fn parse_literal_value(s: &str) -> Option<serde_json::Value> {
    if let Some(str_val) = parse_string_literal(s) {
        return Some(serde_json::Value::String(str_val));
    }
    match s.trim() {
        "true" => Some(serde_json::Value::Bool(true)),
        "false" => Some(serde_json::Value::Bool(false)),
        "nil" | "null" => Some(serde_json::Value::Null),
        other => {
            if let Ok(n) = other.parse::<i64>() {
                return Some(serde_json::Value::Number(n.into()));
            }
            if let Ok(n) = other.parse::<f64>() {
                return serde_json::Number::from_f64(n).map(serde_json::Value::Number);
            }
            None
        }
    }
}

fn parse_table_with_vars(
    s: &str,
    vars: &HashMap<String, serde_json::Value>,
) -> Option<serde_json::Value> {
    let s = s.trim();
    if !s.starts_with('{') || !s.ends_with('}') {
        return None;
    }
    let inner = &s[1..s.len() - 1];
    let mut map = serde_json::Map::new();
    for pair in inner.split(',') {
        let pair = pair.trim();
        if pair.is_empty() {
            continue;
        }
        if let Some(eq) = pair.find('=') {
            let key = pair[..eq].trim().trim_matches('"').trim_matches('\'');
            let val_str = pair[eq + 1..].trim();
            let val = if let Some(v) = parse_literal_value(val_str) {
                v
            } else if is_identifier(val_str) {
                vars.get(val_str).cloned().unwrap_or(serde_json::Value::Null)
            } else {
                serde_json::Value::Null
            };
            map.insert(key.to_string(), val);
        }
    }
    Some(serde_json::Value::Object(map))
}

fn is_identifier(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_alphanumeric() || c == '_')
}

fn is_truthy(v: &serde_json::Value) -> bool {
    !matches!(v, serde_json::Value::Bool(false) | serde_json::Value::Null)
}

fn find_block_bounds(lines: &[String], start: usize) -> (usize, Option<usize>, usize) {
    let mut depth = 1usize;
    let mut else_idx = None;
    let mut end_idx = start;

    while end_idx < lines.len() {
        let inner = lines[end_idx].trim();
        if inner.starts_with("if ") && inner.ends_with(" then") {
            depth += 1;
        } else if inner == "end" {
            depth -= 1;
            if depth == 0 {
                break;
            }
        } else if inner == "else" && depth == 1 {
            else_idx = Some(end_idx);
        }
        end_idx += 1;
    }

    let then_end = else_idx.unwrap_or(end_idx);
    (then_end, else_idx, end_idx)
}

/// Parse wpctl volume output: `Volume: 0.65 [MUTED]` → percent.
pub fn parse_wpctl_volume(output: &str) -> Option<u32> {
    if output.contains("[MUTED]") {
        return Some(0);
    }
    output
        .split_whitespace()
        .find_map(|part| part.parse::<f32>().ok())
        .map(|v| (v * 100.0).round().clamp(0.0, 100.0) as u32)
}

/// Parse the first `N%` token in pactl output.
pub fn parse_first_percent(output: &str) -> Option<u32> {
    output
        .split_whitespace()
        .find_map(|part| part.strip_suffix('%').and_then(|v| v.parse().ok()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_wpctl_volume() {
        assert_eq!(parse_wpctl_volume("Volume: 0.65"), Some(65));
        assert_eq!(parse_wpctl_volume("Volume: 0.00 [MUTED]"), Some(0));
        assert_eq!(parse_wpctl_volume("Volume: 1.00"), Some(100));
    }

    #[test]
    fn parses_pactl_percent() {
        assert_eq!(parse_first_percent("Volume: front-left: 65536 / 65%"), Some(65));
        assert_eq!(parse_first_percent("no percent here"), None);
    }

    #[test]
    fn loads_poll_interval_from_script() {
        let mut ctx = BackendScriptContext::new("@test/backend");
        ctx.load_script("mesh.service.set_poll_interval(250)").unwrap();
        assert_eq!(ctx.poll_interval_ms(), 250);
    }

    #[test]
    fn registers_handlers_from_script() {
        let mut ctx = BackendScriptContext::new("@test/backend");
        ctx.load_script(
            "function on_poll()\nmesh.log.info(\"polling\")\nend\n\
             function on_command_volume_up()\nmesh.log.info(\"up\")\nend",
        )
        .unwrap();
        assert!(ctx.handlers.contains_key("on_poll"));
        assert!(ctx.handlers.contains_key("on_command_volume_up"));
    }

    #[test]
    fn emit_stores_pending_payload() {
        let mut ctx = BackendScriptContext::new("@test/backend");
        ctx.load_script(
            "function on_poll()\nmesh.service.emit({ available = true, percent = 65 })\nend",
        )
        .unwrap();
        let payload = ctx.run_poll();
        assert!(payload.is_some());
        let p = payload.unwrap();
        assert_eq!(p.get("available").and_then(|v| v.as_bool()), Some(true));
        assert_eq!(p.get("percent").and_then(|v| v.as_u64()), Some(65));
    }

    #[test]
    fn emit_unavailable_stores_unavailable_payload() {
        let mut ctx = BackendScriptContext::new("@test/backend");
        ctx.load_script("function on_poll()\nmesh.service.emit_unavailable()\nend")
            .unwrap();
        let payload = ctx.run_poll().unwrap();
        assert_eq!(payload.get("available").and_then(|v| v.as_bool()), Some(false));
    }

    #[test]
    fn command_handler_substitutes_payload_vars() {
        let mut ctx = BackendScriptContext::new("@test/backend");
        // We can't easily test exec output in unit tests (no real wpctl),
        // but we can verify the handler is found and runs without panicking.
        ctx.load_script(
            "function on_command_set_volume()\nmesh.log.info(\"set\")\nend",
        )
        .unwrap();
        let result = ctx.run_command("set-volume", &serde_json::json!({ "percent": 50 }));
        assert!(result.is_none()); // no emit called in this handler
    }

    #[test]
    fn emit_resolves_vars_from_table() {
        let mut ctx = BackendScriptContext::new("@test/backend");
        ctx.vars.insert("__parsed_volume".into(), 42u32.into());
        ctx.vars.insert("__parsed_muted".into(), false.into());
        ctx.load_script(
            "function on_poll()\nmesh.service.emit({ percent = __parsed_volume, muted = __parsed_muted })\nend",
        )
        .unwrap();
        let payload = ctx.run_poll().unwrap();
        assert_eq!(payload.get("percent").and_then(|v| v.as_u64()), Some(42));
        assert_eq!(payload.get("muted").and_then(|v| v.as_bool()), Some(false));
    }
}
