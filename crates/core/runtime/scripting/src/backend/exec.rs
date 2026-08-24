use crate::policy::ResourceBudget;
use mlua::{Lua, Value as LuaValue};
use std::collections::HashSet;
use std::io::Read;
use std::path::Path;
use std::process::{Command as StdCommand, Stdio};
use std::thread;
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
struct ExecOutcome {
    success: bool,
    stdout: String,
    stderr: String,
    code: Option<i32>,
}

pub(super) fn run_exec(
    lua: &Lua,
    program: &str,
    args: &[String],
    resources: &ResourceBudget,
) -> mlua::Result<LuaValue> {
    resources
        .acquire_child()
        .map_err(|error| mlua::Error::external(error.to_string()))?;
    let result = run_bounded_command(
        program,
        args,
        resources.output_limit() as usize,
        resources.child_process_timeout(),
    );
    resources.release_child();
    let outcome = match result {
        Ok(outcome) => outcome,
        Err(error) => {
            tracing::debug!("backend exec failed: {}", error);
            ExecOutcome {
                success: false,
                stdout: String::new(),
                stderr: error.to_string(),
                code: None,
            }
        }
    };
    let output_size = outcome.stdout.len().saturating_add(outcome.stderr.len());
    resources
        .reserve_output(output_size)
        .map_err(|error| mlua::Error::external(error.to_string()))?;
    exec_outcome_to_lua(lua, outcome)
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
    resources: &ResourceBudget,
) -> mlua::Result<LuaValue> {
    let outcome = ExecOutcome {
        success: false,
        stdout: String::new(),
        stderr: format!("denied mesh.exec(\"{program}\") without {required} or exec.command"),
        code: None,
    };
    resources
        .reserve_output(outcome.stderr.len())
        .map_err(|error| mlua::Error::external(error.to_string()))?;
    exec_outcome_to_lua(lua, outcome)
}

fn run_bounded_command(
    program: &str,
    args: &[String],
    output_limit: usize,
    timeout: Duration,
) -> std::io::Result<ExecOutcome> {
    let mut child = StdCommand::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| std::io::Error::other("stdout was not piped"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| std::io::Error::other("stderr was not piped"))?;
    let stdout_reader = thread::spawn(move || read_limited(stdout, output_limit));
    let stderr_reader = thread::spawn(move || read_limited(stderr, output_limit));

    let started = Instant::now();
    let mut timed_out = false;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if started.elapsed() >= timeout {
            timed_out = true;
            let _ = child.kill();
            break child.wait()?;
        }
        thread::sleep(Duration::from_millis(1));
    };
    let (stdout, stdout_truncated) = stdout_reader
        .join()
        .map_err(|_| std::io::Error::other("stdout reader panicked"))?;
    let (stderr, stderr_truncated) = stderr_reader
        .join()
        .map_err(|_| std::io::Error::other("stderr reader panicked"))?;
    let mut stdout = stdout;
    let mut stderr = stderr;
    let mut truncated = stdout_truncated || stderr_truncated;
    if stdout.len().saturating_add(stderr.len()) > output_limit {
        truncated = true;
        if stdout.len() >= output_limit {
            stdout.truncate(output_limit);
            stderr.clear();
        } else {
            stderr.truncate(output_limit - stdout.len());
        }
    }
    if timed_out {
        stderr.extend_from_slice(b"process timed out");
    } else if truncated {
        if !stderr.is_empty() {
            stderr.extend_from_slice(b"; ");
        }
        stderr.extend_from_slice(b"process output exceeded budget");
    }
    if stdout.len().saturating_add(stderr.len()) > output_limit {
        if stdout.len() >= output_limit {
            stdout.truncate(output_limit);
            stderr.clear();
        } else {
            stderr.truncate(output_limit - stdout.len());
        }
    }
    Ok(ExecOutcome {
        success: !timed_out && !truncated && status.success(),
        stdout: String::from_utf8_lossy(&stdout).into_owned(),
        stderr: String::from_utf8_lossy(&stderr).into_owned(),
        code: if timed_out { None } else { status.code() },
    })
}

fn read_limited(mut reader: impl Read, limit: usize) -> (Vec<u8>, bool) {
    let mut output = Vec::new();
    let mut buffer = [0_u8; 8 * 1024];
    let mut truncated = false;
    loop {
        match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => {
                let remaining = limit.saturating_sub(output.len());
                if read > remaining {
                    output.extend_from_slice(&buffer[..remaining]);
                    truncated = true;
                    continue;
                }
                output.extend_from_slice(&buffer[..read]);
            }
            Err(_) => break,
        }
    }
    (output, truncated)
}

fn exec_outcome_to_lua(lua: &Lua, outcome: ExecOutcome) -> mlua::Result<LuaValue> {
    let table = lua.create_table()?;
    table.set("success", outcome.success)?;
    table.set("stdout", outcome.stdout)?;
    table.set("stderr", outcome.stderr)?;
    table.set("code", outcome.code)?;
    Ok(LuaValue::Table(table))
}
