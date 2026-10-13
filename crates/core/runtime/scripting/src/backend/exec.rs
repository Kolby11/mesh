use crate::policy::{ResourceBudget, ResourceLimit};
use mlua::{Lua, Value as LuaValue};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command as StdCommand, Stdio};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

#[cfg(unix)]
use std::os::unix::process::CommandExt;

const CANCEL_REAP_GRACE: Duration = Duration::from_millis(250);

/// Locale for every exec child unless the call asks for another one.
/// Providers parse tool output, which must not follow the user's locale:
/// `upower -i` under `LC_ALL=sk_SK` prints `63,14 Wh`.
pub(super) const CHILD_LOCALE: (&str, &str) = ("LC_ALL", "C.UTF-8");

/// Grants the `locale` option of `mesh.exec` / `mesh.exec_stream`, for tools
/// whose output is shown to the user rather than parsed.
pub(super) const EXEC_LOCALE_CAPABILITY: &str = "exec.locale";

/// The locale an exec child runs in.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) enum ExecLocale {
    /// `LC_ALL=C.UTF-8`, so output parses the same on every host.
    #[default]
    Neutral,
    /// The shell's own environment, untouched.
    Inherit,
    /// `LC_ALL` set to a validated POSIX locale name such as `sk_SK.UTF-8`.
    Named(String),
}

impl ExecLocale {
    /// Parse the script-facing option: `"inherit"`, `"C.UTF-8"`, or a locale
    /// name of letters, digits and `_ . @ -`, at most 64 bytes.
    pub(super) fn parse(value: &str) -> Result<Self, String> {
        match value {
            "inherit" => Ok(Self::Inherit),
            "C.UTF-8" => Ok(Self::Neutral),
            name if !name.is_empty()
                && name.len() <= 64
                && name
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"_.@-".contains(&byte)) =>
            {
                Ok(Self::Named(name.to_string()))
            }
            other => Err(format!("invalid exec locale '{other}'")),
        }
    }

    /// The `LC_ALL` value the child gets, or `None` to leave the shell's.
    pub(super) fn lc_all(&self) -> Option<&str> {
        match self {
            Self::Neutral => Some(CHILD_LOCALE.1),
            Self::Inherit => None,
            Self::Named(name) => Some(name),
        }
    }
}
const EXEC_ARGV_PREFIX: &str = "exec.argv:";

#[derive(Debug, Clone, PartialEq, Eq)]
enum ArgumentPattern {
    Exact(String),
    Glob(String),
    Any,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ExecutableRule {
    canonical_path: PathBuf,
    arguments: Option<Vec<ArgumentPattern>>,
}

/// The executable policy attached to one backend generation.
///
/// Executable grants are deliberately not inferred from a file name. A grant
/// has the form `exec.argv:<program>:<json-array>` where `<program>` is
/// resolved once to its canonical path and each argument is matched exactly or
/// by an explicit `*` glob. An in-argument `*` matches only a restricted
/// character set, and interpreter grants whose wildcards reach code are
/// refused. A bare `*` argument matches one value, while a JSON
/// `*` in place of the array opts into any argument list. `exec.command` remains
/// the explicit, high-risk unrestricted override.
#[derive(Debug, Clone, Default)]
pub(super) struct ExecutableCapabilityPolicy {
    allow_all: bool,
    rules: Vec<ExecutableRule>,
}

impl ExecutableCapabilityPolicy {
    pub(super) fn new(capabilities: &HashSet<String>) -> Self {
        let mut policy = Self {
            allow_all: capabilities.contains("exec.command"),
            rules: Vec::new(),
        };
        for capability in capabilities {
            if let Some(rule) = parse_executable_rule(capability) {
                policy.rules.push(rule);
            }
        }
        policy
    }

    pub(super) fn allows(&self, program: &str, args: &[String]) -> bool {
        self.allow_all || self.find_rule(program, args).is_some()
    }

    pub(super) fn canonical_launch_program(
        &self,
        program: &str,
        args: &[String],
    ) -> Option<String> {
        self.find_rule(program, args)
            .map(|rule| rule.canonical_path.to_string_lossy().into_owned())
    }

    fn missing_capability(&self, program: &str, args: &[String]) -> Option<String> {
        (!self.allows(program, args)).then(|| required_exec_capability(program, args))
    }

    fn find_rule(&self, program: &str, args: &[String]) -> Option<&ExecutableRule> {
        let canonical_path = canonical_program_path(program)?;
        self.rules.iter().find(|rule| {
            rule.canonical_path == canonical_path
                && rule
                    .arguments
                    .as_ref()
                    .is_none_or(|patterns| arguments_match(patterns, args))
        })
    }
}

fn parse_executable_rule(capability: &str) -> Option<ExecutableRule> {
    let (program, arguments) = mesh_core_capability::parse_exec_argv_capability(capability)?;
    if mesh_core_capability::exec_argv_grant_error(&program, arguments.as_deref()).is_some() {
        return None;
    }
    let arguments = arguments.map(|arguments| {
        arguments
            .into_iter()
            .map(|argument| {
                if argument == "*" {
                    ArgumentPattern::Any
                } else if argument.contains('*') {
                    ArgumentPattern::Glob(argument)
                } else {
                    ArgumentPattern::Exact(argument)
                }
            })
            .collect()
    });
    Some(ExecutableRule {
        canonical_path: canonical_program_path(&program)?,
        arguments,
    })
}

fn arguments_match(patterns: &[ArgumentPattern], args: &[String]) -> bool {
    patterns.len() == args.len()
        && patterns
            .iter()
            .zip(args)
            .all(|(pattern, argument)| match pattern {
                ArgumentPattern::Exact(expected) => expected == argument,
                ArgumentPattern::Glob(pattern) => glob_match(pattern, argument),
                ArgumentPattern::Any => true,
            })
}

/// Match an in-argument glob. Each `*` matches only characters that cannot
/// end the literal around it; see `exec_argv_glob_char_allowed`.
fn glob_match(pattern: &str, value: &str) -> bool {
    let Some((prefix, rest)) = pattern.split_once('*') else {
        return pattern == value;
    };
    let Some(value) = value.strip_prefix(prefix) else {
        return false;
    };
    let mut end = 0;
    loop {
        if glob_match(rest, &value[end..]) {
            return true;
        }
        let Some(next) = value[end..].chars().next() else {
            return false;
        };
        if !mesh_core_capability::exec_argv_glob_char_allowed(next) {
            return false;
        }
        end += next.len_utf8();
    }
}

fn canonical_program_path(program: &str) -> Option<PathBuf> {
    if program.is_empty() || program.contains('\0') {
        return None;
    }
    let candidate = if Path::new(program).components().count() > 1 {
        PathBuf::from(program)
    } else {
        let path = std::env::var_os("PATH")?;
        std::env::split_paths(&path)
            .map(|directory| directory.join(program))
            .find(|candidate| {
                fs::metadata(candidate)
                    .is_ok_and(|metadata| metadata.is_file() && is_executable(&metadata))
            })?
    };
    let canonical = fs::canonicalize(candidate).ok()?;
    let metadata = fs::metadata(&canonical).ok()?;
    if !metadata.is_file() || !is_executable(&metadata) {
        return None;
    }
    Some(canonical)
}

#[cfg(unix)]
fn is_executable(metadata: &fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;

    metadata.permissions().mode() & 0o111 != 0
}

#[cfg(not(unix))]
fn is_executable(_metadata: &fs::Metadata) -> bool {
    true
}

fn required_exec_capability(program: &str, args: &[String]) -> String {
    let canonical_program = canonical_program_path(program)
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_else(|| program.to_string());
    let arguments = serde_json::to_string(args).unwrap_or_else(|_| "[]".to_string());
    format!("{EXEC_ARGV_PREFIX}{canonical_program}:{arguments}")
}

#[derive(Debug, Clone)]
struct ExecOutcome {
    success: bool,
    stdout: String,
    stderr: String,
    code: Option<i32>,
}

#[derive(Debug)]
struct ExecJob {
    cancel: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
}

#[derive(Debug, Default)]
struct ExecServiceState {
    next_job: u64,
    shutting_down: bool,
    jobs: HashMap<u64, ExecJob>,
}

/// Owns the worker jobs for one backend generation.
///
/// Luau host callbacks are synchronous, so the compatibility-shaped
/// `mesh.exec` function still waits for its result. The process itself, its
/// pipe readers, and its cancellation/reaping live on this worker instead of
/// on the Tokio backend task. Waiting is bounded by the same deadline as the
/// child, and dropping a backend cancels and joins every remaining job.
#[derive(Debug, Clone)]
pub(super) struct ExecService {
    state: Arc<Mutex<ExecServiceState>>,
    resources: ResourceBudget,
}

#[derive(Debug)]
enum ExecRunError {
    Resource(ResourceLimit),
    Internal(String),
}

impl ExecService {
    pub(super) fn new(resources: ResourceBudget) -> Self {
        Self {
            state: Arc::new(Mutex::new(ExecServiceState::default())),
            resources,
        }
    }

    #[cfg(test)]
    fn run(
        &self,
        program: &str,
        args: &[String],
        output_limit: usize,
        timeout: Duration,
    ) -> Result<ExecOutcome, ExecRunError> {
        let argv0 = Path::new(program)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(program);
        self.run_with_argv0(
            program,
            argv0,
            args,
            &ExecLocale::Neutral,
            output_limit,
            timeout,
        )
    }

    fn run_with_argv0(
        &self,
        program: &str,
        argv0: &str,
        args: &[String],
        locale: &ExecLocale,
        output_limit: usize,
        timeout: Duration,
    ) -> Result<ExecOutcome, ExecRunError> {
        self.resources
            .acquire_child()
            .map_err(ExecRunError::Resource)?;

        let cancel = Arc::new(AtomicBool::new(false));
        let (result_tx, result_rx) = mpsc::channel();
        let worker_cancel = Arc::clone(&cancel);
        let worker_resources = self.resources.clone();
        let program = program.to_string();
        let argv0 = argv0.to_string();
        let args = args.to_vec();
        let locale = locale.clone();

        let mut state = self.state.lock().unwrap();
        if state.shutting_down {
            self.resources.release_child();
            return Err(ExecRunError::Internal(
                "backend exec service is shutting down".to_string(),
            ));
        }
        state.next_job = state.next_job.wrapping_add(1).max(1);
        let job_id = state.next_job;
        let join = match thread::Builder::new()
            .name(format!("mesh-exec-{job_id}"))
            .spawn(move || {
                let _child_budget = ChildBudgetGuard {
                    resources: worker_resources,
                };
                let result = run_bounded_command(
                    &program,
                    &argv0,
                    &args,
                    &locale,
                    output_limit,
                    timeout,
                    worker_cancel,
                );
                let _ = result_tx.send(result);
            }) {
            Ok(join) => join,
            Err(error) => {
                self.resources.release_child();
                return Err(ExecRunError::Internal(format!(
                    "failed to start backend exec worker: {error}"
                )));
            }
        };
        state.jobs.insert(
            job_id,
            ExecJob {
                cancel: Arc::clone(&cancel),
                join: Some(join),
            },
        );
        drop(state);

        match result_rx.recv_timeout(timeout) {
            Ok(result) => self.finish_job(job_id, result),
            Err(mpsc::RecvTimeoutError::Timeout) => {
                cancel.store(true, Ordering::Release);
                match result_rx.recv_timeout(CANCEL_REAP_GRACE) {
                    Ok(result) => self.finish_job(job_id, result).map(mark_deadline_outcome),
                    Err(mpsc::RecvTimeoutError::Timeout) => Ok(ExecOutcome {
                        success: false,
                        stdout: String::new(),
                        stderr: "process deadline exceeded".to_string(),
                        code: None,
                    }),
                    Err(mpsc::RecvTimeoutError::Disconnected) => Err(ExecRunError::Internal(
                        "backend exec worker disconnected after deadline".to_string(),
                    )),
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => Err(ExecRunError::Internal(
                "backend exec worker disconnected".to_string(),
            )),
        }
    }

    fn finish_job(
        &self,
        job_id: u64,
        result: std::io::Result<ExecOutcome>,
    ) -> Result<ExecOutcome, ExecRunError> {
        let job = self.state.lock().unwrap().jobs.remove(&job_id);
        if let Some(mut job) = job {
            if let Some(join) = job.join.take() {
                join.join().map_err(|_| {
                    ExecRunError::Internal("backend exec worker panicked".to_string())
                })?;
            }
        }
        result.map_err(|error| ExecRunError::Internal(error.to_string()))
    }

    /// Cancel and reap all jobs owned by this backend generation.
    pub(super) fn shutdown(&self) {
        let jobs = {
            let mut state = self.state.lock().unwrap();
            state.shutting_down = true;
            state.jobs.drain().map(|(_, job)| job).collect::<Vec<_>>()
        };
        for job in &jobs {
            job.cancel.store(true, Ordering::Release);
        }
        for mut job in jobs {
            if let Some(join) = job.join.take() {
                let _ = join.join();
            }
        }
    }
}

fn mark_deadline_outcome(mut outcome: ExecOutcome) -> ExecOutcome {
    outcome.success = false;
    outcome.stderr = "process timed out".to_string();
    outcome.code = None;
    outcome
}

struct ChildBudgetGuard {
    resources: ResourceBudget,
}

impl Drop for ChildBudgetGuard {
    fn drop(&mut self) {
        self.resources.release_child();
    }
}

pub(super) fn run_exec(
    lua: &Lua,
    program: &str,
    argv0: &str,
    args: &[String],
    locale: &ExecLocale,
    service: &ExecService,
) -> mlua::Result<LuaValue> {
    let resources = &service.resources;
    let outcome = {
        let _frame_pause = resources.pause_frame_clock();
        command_outcome(program, argv0, args, locale, service).map_err(mlua::Error::external)?
    };
    let output_size = outcome.stdout.len().saturating_add(outcome.stderr.len());
    resources
        .reserve_output(output_size)
        .map_err(|error| mlua::Error::external(error.to_string()))?;
    exec_outcome_to_lua(lua, outcome)
}

fn command_outcome(
    program: &str,
    argv0: &str,
    args: &[String],
    locale: &ExecLocale,
    service: &ExecService,
) -> Result<ExecOutcome, String> {
    let resources = &service.resources;
    match service.run_with_argv0(
        program,
        argv0,
        args,
        locale,
        resources.output_limit() as usize,
        resources.child_process_timeout(),
    ) {
        Ok(outcome) => Ok(outcome),
        Err(ExecRunError::Resource(error)) => Err(error.to_string()),
        Err(ExecRunError::Internal(error)) => Ok(ExecOutcome {
            success: false,
            stdout: String::new(),
            stderr: error,
            code: None,
        }),
    }
}

pub(super) fn run_exec_json(
    program: &str,
    argv0: &str,
    args: &[String],
    locale: &ExecLocale,
    service: &ExecService,
) -> Result<serde_json::Value, String> {
    let outcome = command_outcome(program, argv0, args, locale, service)?;
    let mut value = serde_json::json!({ "success": outcome.success, "stdout": outcome.stdout, "stderr": outcome.stderr });
    if let Some(code) = outcome.code {
        value["code"] = code.into();
    }
    Ok(value)
}

pub(super) fn missing_exec_capability(
    policy: &ExecutableCapabilityPolicy,
    program: &str,
    args: &[String],
) -> Option<String> {
    policy.missing_capability(program, args)
}

pub(super) fn missing_exec_stream_capability(
    policy: &ExecutableCapabilityPolicy,
    program: &str,
    args: &[String],
) -> Option<String> {
    policy.missing_capability(program, args)
}

pub(super) fn exec_denied_to_lua(
    lua: &Lua,
    program: &str,
    required: &str,
    resources: &ResourceBudget,
) -> mlua::Result<LuaValue> {
    let requirement = format!("without {required} or exec.command");
    let outcome = ExecOutcome {
        success: false,
        stdout: String::new(),
        stderr: format!("denied mesh.exec(\"{program}\") {requirement}"),
        code: None,
    };
    resources
        .reserve_output(outcome.stderr.len())
        .map_err(|error| mlua::Error::external(error.to_string()))?;
    exec_outcome_to_lua(lua, outcome)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Termination {
    Completed,
    TimedOut,
    Cancelled,
    OutputLimit,
}

#[derive(Debug)]
struct OutputCapture {
    used: AtomicUsize,
    limit: usize,
    overflowed: AtomicBool,
}

impl OutputCapture {
    fn new(limit: usize) -> Self {
        Self {
            used: AtomicUsize::new(0),
            limit,
            overflowed: AtomicBool::new(false),
        }
    }

    fn reserve(&self, requested: usize) -> usize {
        let previous = self
            .used
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |used| {
                Some(used.saturating_add(requested).min(self.limit))
            })
            .unwrap_or(self.limit);
        let remaining = self.limit.saturating_sub(previous);
        if requested > remaining {
            self.overflowed.store(true, Ordering::Release);
        }
        requested.min(remaining)
    }

    fn overflowed(&self) -> bool {
        self.overflowed.load(Ordering::Acquire)
    }
}

fn run_bounded_command(
    program: &str,
    argv0: &str,
    args: &[String],
    locale: &ExecLocale,
    output_limit: usize,
    timeout: Duration,
    cancel: Arc<AtomicBool>,
) -> std::io::Result<ExecOutcome> {
    let mut command = StdCommand::new(program);
    if let Some(value) = locale.lc_all() {
        command.env(CHILD_LOCALE.0, value);
    }
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    configure_argv0(&mut command, argv0);
    configure_process_group(&mut command);
    let mut child = command.spawn()?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| std::io::Error::other("stdout was not piped"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| std::io::Error::other("stderr was not piped"))?;
    let capture = Arc::new(OutputCapture::new(output_limit));
    let stdout_capture = Arc::clone(&capture);
    let stdout_reader = thread::spawn({
        let cancel = Arc::clone(&cancel);
        move || read_limited(stdout, stdout_capture, cancel)
    });
    let stderr_capture = Arc::clone(&capture);
    let stderr_reader = thread::spawn({
        let cancel = Arc::clone(&cancel);
        move || read_limited(stderr, stderr_capture, cancel)
    });

    let started = Instant::now();
    let mut termination = Termination::Completed;
    let status = loop {
        if cancel.load(Ordering::Acquire) {
            termination = if capture.overflowed() {
                Termination::OutputLimit
            } else {
                Termination::Cancelled
            };
            terminate_child(&mut child);
            break child.wait()?;
        }
        if started.elapsed() >= timeout {
            termination = Termination::TimedOut;
            terminate_child(&mut child);
            break child.wait()?;
        }
        if let Some(status) = child.try_wait()? {
            break status;
        }
        thread::sleep(Duration::from_millis(1));
    };

    let mut stdout = stdout_reader
        .join()
        .map_err(|_| std::io::Error::other("stdout reader panicked"))?;
    let mut stderr = stderr_reader
        .join()
        .map_err(|_| std::io::Error::other("stderr reader panicked"))?;
    if capture.overflowed() && termination == Termination::Completed {
        termination = Termination::OutputLimit;
    }

    let diagnostic = match termination {
        Termination::Completed => None,
        Termination::TimedOut => Some("process timed out"),
        Termination::Cancelled => Some("process cancelled"),
        Termination::OutputLimit => Some("process output exceeded budget"),
    };
    if let Some(diagnostic) = diagnostic {
        append_diagnostic(
            &mut stdout,
            &mut stderr,
            output_limit,
            diagnostic.as_bytes(),
        );
    }
    Ok(ExecOutcome {
        success: termination == Termination::Completed && status.success(),
        stdout: String::from_utf8_lossy(&stdout).into_owned(),
        stderr: String::from_utf8_lossy(&stderr).into_owned(),
        code: (termination == Termination::Completed)
            .then(|| status.code())
            .flatten(),
    })
}

fn configure_argv0(command: &mut StdCommand, argv0: &str) {
    #[cfg(unix)]
    command.arg0(argv0);
}

fn read_limited(
    mut reader: impl Read,
    capture: Arc<OutputCapture>,
    cancel: Arc<AtomicBool>,
) -> Vec<u8> {
    let mut output = Vec::new();
    let mut buffer = [0_u8; 8 * 1024];
    loop {
        match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => {
                let kept = capture.reserve(read);
                output.extend_from_slice(&buffer[..kept]);
                if kept < read {
                    cancel.store(true, Ordering::Release);
                }
            }
            Err(_) => break,
        }
    }
    output
}

fn append_diagnostic(stdout: &mut Vec<u8>, stderr: &mut Vec<u8>, limit: usize, diagnostic: &[u8]) {
    let remaining = limit.saturating_sub(stdout.len().saturating_add(stderr.len()));
    if remaining == 0 {
        return;
    }
    let separator = if stderr.is_empty() { &[][..] } else { b"; " };
    let available = remaining.saturating_sub(separator.len());
    stderr.extend_from_slice(&separator[..separator.len().min(remaining)]);
    stderr.extend_from_slice(&diagnostic[..diagnostic.len().min(available)]);
}

fn configure_process_group(command: &mut StdCommand) {
    #[cfg(unix)]
    unsafe {
        command.pre_exec(|| {
            if libc::setpgid(0, 0) == -1 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
}

fn terminate_child(child: &mut Child) {
    #[cfg(unix)]
    if let Ok(pid) = i32::try_from(child.id()) {
        // The child puts itself in a fresh process group before exec. Killing
        // the group prevents `sh -c` descendants from retaining our pipes.
        unsafe {
            let _ = libc::kill(-pid, libc::SIGKILL);
        }
    }
    let _ = child.kill();
}

fn exec_outcome_to_lua(lua: &Lua, outcome: ExecOutcome) -> mlua::Result<LuaValue> {
    let table = lua.create_table()?;
    table.set("success", outcome.success)?;
    table.set("stdout", outcome.stdout)?;
    table.set("stderr", outcome.stderr)?;
    table.set("code", outcome.code)?;
    Ok(LuaValue::Table(table))
}

#[cfg(test)]
mod tests {
    use super::*;
    use mesh_core_runtime::SandboxConfig;

    fn service(config: SandboxConfig) -> ExecService {
        ExecService::new(ResourceBudget::new(config))
    }

    #[test]
    fn in_argument_globs_cannot_escape_their_literal() {
        let dispatch = "hl.dsp.focus({ workspace = \"*\" })";
        assert!(glob_match(dispatch, "hl.dsp.focus({ workspace = \"3\" })"));
        assert!(!glob_match(
            dispatch,
            "hl.dsp.focus({ workspace = \"\" }) os.execute(\"x\") --\" })"
        ));
        assert!(glob_match(
            "UNIX-CONNECT:*",
            "UNIX-CONNECT:/run/user/1000/hypr/s.sock"
        ));
        assert!(!glob_match("UNIX-CONNECT:*", "UNIX-CONNECT:/tmp/s,fork"));
    }

    #[test]
    fn interpreter_rules_whose_wildcards_reach_code_are_refused() {
        assert!(parse_executable_rule("exec.argv:sh:[\"-c\",\"test -S * && echo ok\"]").is_none());
        assert!(parse_executable_rule("exec.argv:sh:*").is_none());
        assert!(parse_executable_rule("exec.argv:python3:[\"-c\",\"*\"]").is_none());
        assert!(
            parse_executable_rule("exec.argv:sh:[\"-c\",\"test -S \\\"$1\\\"\",\"sh\",\"*\"]")
                .is_some()
        );
    }

    #[test]
    fn exec_children_run_in_a_fixed_locale() {
        let service = service(SandboxConfig::default());
        let result = service
            .run(
                "sh",
                &["-c".to_string(), "printf '%s' \"$LC_ALL\"".to_string()],
                1024,
                Duration::from_secs(2),
            )
            .unwrap();
        assert!(result.success);
        assert_eq!(result.stdout, "C.UTF-8");
    }

    #[test]
    fn a_named_exec_locale_reaches_the_child_and_inherit_leaves_the_shells() {
        let service = service(SandboxConfig::default());
        let print_lc_all = ["-c".to_string(), "printf '%s' \"$LC_ALL\"".to_string()];
        let named = service
            .run_with_argv0(
                "sh",
                "sh",
                &print_lc_all,
                &ExecLocale::parse("sk_SK.UTF-8").unwrap(),
                1024,
                Duration::from_secs(2),
            )
            .unwrap();
        assert_eq!(named.stdout, "sk_SK.UTF-8");

        let inherited = service
            .run_with_argv0(
                "sh",
                "sh",
                &print_lc_all,
                &ExecLocale::Inherit,
                1024,
                Duration::from_secs(2),
            )
            .unwrap();
        assert_eq!(
            inherited.stdout,
            std::env::var("LC_ALL").unwrap_or_default()
        );
    }

    #[test]
    fn exec_locale_names_are_restricted() {
        assert_eq!(ExecLocale::parse("inherit"), Ok(ExecLocale::Inherit));
        assert_eq!(ExecLocale::parse("C.UTF-8"), Ok(ExecLocale::Neutral));
        assert_eq!(
            ExecLocale::parse("sr_RS.UTF-8@latin"),
            Ok(ExecLocale::Named("sr_RS.UTF-8@latin".into()))
        );
        for bad in ["", "sk SK", "x;rm", "a/b", &"x".repeat(65)] {
            assert!(ExecLocale::parse(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn worker_deadline_kills_process_group() {
        let mut config = SandboxConfig::default();
        config.child_process_timeout_ms = 25;
        let service = service(config);
        let started = Instant::now();
        let result = service
            .run(
                "sh",
                &["-c".to_string(), "sleep 60".to_string()],
                1024,
                Duration::from_millis(25),
            )
            .unwrap();
        assert!(!result.success);
        assert!(result.stderr.contains("timed out") || result.stderr.contains("deadline"));
        assert!(started.elapsed() < Duration::from_secs(1));
        service.shutdown();
    }

    #[test]
    fn output_overflow_stops_child_and_keeps_result_bounded() {
        let service = service(SandboxConfig::default());
        let result = service
            .run(
                "sh",
                &["-c".to_string(), "yes x".to_string()],
                64,
                Duration::from_secs(2),
            )
            .unwrap();
        assert!(!result.success);
        assert!(result.stdout.len() + result.stderr.len() <= 64);
        assert!(result.stderr.contains("output") || result.stdout.len() == 64);
        service.shutdown();
    }

    #[test]
    fn cancellation_reaps_worker_and_child() {
        let service = service(SandboxConfig::default());
        let clone = service.clone();
        let join = thread::spawn(move || {
            clone
                .run(
                    "sh",
                    &["-c".to_string(), "sleep 60".to_string()],
                    1024,
                    Duration::from_secs(60),
                )
                .unwrap()
        });
        thread::sleep(Duration::from_millis(25));
        service.shutdown();
        let result = join.join().unwrap();
        assert!(!result.success);
    }

    #[test]
    fn shell_style_execution_requires_high_risk_capability() {
        let capabilities = HashSet::from(["exec.argv:sh:[\"-c\",\"printf ok\"]".to_string()]);
        let policy = ExecutableCapabilityPolicy::new(&capabilities);
        let args = vec!["-c".to_string(), "printf ok".to_string()];
        assert_eq!(missing_exec_capability(&policy, "sh", &args), None);
        let denied = vec!["-c".to_string(), "rm -rf /".to_string()];
        assert!(
            missing_exec_capability(&policy, "sh", &denied)
                .is_some_and(|capability| capability.starts_with("exec.argv:"))
        );
    }

    #[test]
    fn canonical_path_and_arguments_prevent_basename_substitution() {
        let root = std::env::temp_dir().join(format!(
            "mesh-exec-policy-{}-{}",
            std::process::id(),
            NEXT_TEST_PATH.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        let first = root.join("first").join("tool");
        let second = root.join("second").join("tool");
        fs::create_dir_all(first.parent().unwrap()).unwrap();
        fs::create_dir_all(second.parent().unwrap()).unwrap();
        fs::write(&first, "#!/bin/sh\nexit 0\n").unwrap();
        fs::write(&second, "#!/bin/sh\nexit 0\n").unwrap();
        make_executable(&first);
        make_executable(&second);

        let grant = format!("{EXEC_ARGV_PREFIX}{}:[\"allowed\"]", first.display());
        let policy = ExecutableCapabilityPolicy::new(&HashSet::from([grant]));
        assert!(policy.allows(first.to_str().unwrap(), &["allowed".into()]));
        assert!(!policy.allows(second.to_str().unwrap(), &["allowed".into()]));
        assert!(!policy.allows(first.to_str().unwrap(), &["other".into()]));

        #[cfg(unix)]
        {
            let alias = root.join("alias");
            std::os::unix::fs::symlink(&second, &alias).unwrap();
            assert!(!policy.allows(alias.to_str().unwrap(), &["allowed".into()]));
        }

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn command_name_policy_resolves_path_and_runs() {
        let policy = ExecutableCapabilityPolicy::new(&HashSet::from([
            "exec.argv:printf:[\"*\"]".to_string()
        ]));
        let args = vec!["hello".to_string()];
        assert!(policy.allows("printf", &args));
        let launch = policy.canonical_launch_program("printf", &args).unwrap();
        let outcome = service(SandboxConfig::default())
            .run_with_argv0(
                &launch,
                "printf",
                &args,
                &ExecLocale::Neutral,
                1024,
                Duration::from_secs(1),
            )
            .unwrap();
        assert!(outcome.success, "{outcome:?}");
    }

    static NEXT_TEST_PATH: AtomicUsize = AtomicUsize::new(1);

    #[cfg(unix)]
    fn make_executable(path: &Path) {
        use std::os::unix::fs::PermissionsExt;

        let mut permissions = fs::metadata(path).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions).unwrap();
    }

    #[cfg(not(unix))]
    fn make_executable(_path: &Path) {}
}
