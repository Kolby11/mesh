use mlua::{Lua, Value as LuaValue};
use std::collections::HashSet;
use std::path::Path;
use std::process::Command as StdCommand;

#[derive(Debug, Clone)]
struct ExecOutcome {
    success: bool,
    stdout: String,
    stderr: String,
    code: Option<i32>,
}

pub(super) fn run_exec(lua: &Lua, program: &str, args: &[String]) -> mlua::Result<LuaValue> {
    let result = StdCommand::new(program).args(args).output();
    exec_result_to_lua(lua, result)
}

pub(super) fn missing_exec_capability(
    capabilities: &HashSet<String>,
    program: &str,
) -> Option<String> {
    if capabilities.contains("exec.command") {
        return None;
    }

    let required = exec_program_capability(program);
    if capabilities.contains(&required) {
        None
    } else {
        Some(required)
    }
}

fn exec_program_capability(program: &str) -> String {
    let binary = Path::new(program)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(program);
    format!("exec.{binary}")
}

pub(super) fn exec_denied_to_lua(
    lua: &Lua,
    program: &str,
    required: &str,
) -> mlua::Result<LuaValue> {
    exec_outcome_to_lua(
        lua,
        ExecOutcome {
            success: false,
            stdout: String::new(),
            stderr: format!("denied mesh.exec(\"{program}\") without {required} or exec.command"),
            code: None,
        },
    )
}

fn exec_result_to_lua(
    lua: &Lua,
    result: std::io::Result<std::process::Output>,
) -> mlua::Result<LuaValue> {
    let outcome = match result {
        Ok(output) => ExecOutcome {
            success: output.status.success(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            code: output.status.code(),
        },
        Err(err) => {
            tracing::debug!("backend exec failed: {}", err);
            ExecOutcome {
                success: false,
                stdout: String::new(),
                stderr: err.to_string(),
                code: None,
            }
        }
    };

    exec_outcome_to_lua(lua, outcome)
}

fn exec_outcome_to_lua(lua: &Lua, outcome: ExecOutcome) -> mlua::Result<LuaValue> {
    let table = lua.create_table()?;
    table.set("success", outcome.success)?;
    table.set("stdout", outcome.stdout)?;
    table.set("stderr", outcome.stderr)?;
    table.set("code", outcome.code)?;
    Ok(LuaValue::Table(table))
}
