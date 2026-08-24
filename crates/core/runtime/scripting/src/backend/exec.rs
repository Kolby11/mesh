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
/// by an explicit `*` glob. A bare `*` argument matches one value, while a JSON
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
    let specification = capability.strip_prefix(EXEC_ARGV_PREFIX)?;
    let (program, argument_specification) = specification.split_once(':')?;
    if program.is_empty() || argument_specification.is_empty() {
        return None;
    }
    let arguments = if argument_specification == "*" {
        None
    } else {
        let arguments = serde_json::from_str::<Vec<String>>(argument_specification).ok()?;
        Some(
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
                .collect(),
        )
    };
    Some(ExecutableRule {
        canonical_path: canonical_program_path(program)?,
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

fn glob_match(pattern: &str, value: &str) -> bool {
    let parts = pattern.split('*').collect::<Vec<_>>();
    let mut cursor = 0;
    if let Some(first) = parts.first().filter(|part| !part.is_empty()) {
        if !value.starts_with(first) {
            return false;
        }
        cursor = first.len();
    }
    for (index, part) in parts.iter().enumerate().skip(1) {
        if part.is_empty() {
            continue;
        }
        let is_last_literal = index == parts.len() - 1 && !pattern.ends_with('*');
        let remainder = &value[cursor..];
        if is_last_literal {
            return remainder.ends_with(part);
        }
        let Some(found) = remainder.find(part) else {
            return false;
        };
        cursor += found + part.len();
    }
    true
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
        self.run_with_argv0(program, argv0, args, output_limit, timeout)
    }

    fn run_with_argv0(
        &self,
        program: &str,
        argv0: &str,
        args: &[String],
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
    service: &ExecService,
) -> mlua::Result<LuaValue> {
    let resources = &service.resources;
    let outcome = match service.run_with_argv0(
        program,
        argv0,
        args,
        resources.output_limit() as usize,
        resources.child_process_timeout(),
    ) {
        Ok(outcome) => outcome,
        Err(ExecRunError::Resource(error)) => {
            return Err(mlua::Error::external(error.to_string()));
        }
        Err(ExecRunError::Internal(error)) => {
            tracing::debug!(program, error = %error, "backend exec worker failed");
            ExecOutcome {
                success: false,
                stdout: String::new(),
                stderr: error,
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
    output_limit: usize,
    timeout: Duration,
    cancel: Arc<AtomicBool>,
) -> std::io::Result<ExecOutcome> {
    let mut command = StdCommand::new(program);
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
            .run_with_argv0(&launch, "printf", &args, 1024, Duration::from_secs(1))
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
