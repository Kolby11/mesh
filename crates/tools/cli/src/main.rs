mod update;

use mesh_core_module::ModuleType;
use mesh_core_shell::{Shell, default_ipc_socket_path};
use serde::{Deserialize, Serialize};
use std::io::{self, BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::process::Command;

#[cfg(all(feature = "perf-tracy", feature = "allocation-profiling"))]
compile_error!("perf-tracy and allocation-profiling use different global allocators; enable one");

#[cfg(feature = "perf-tracy")]
#[global_allocator]
static GLOBAL: tracy_client::ProfiledAllocator<std::alloc::System> =
    tracy_client::ProfiledAllocator::new(std::alloc::System, 16);

#[cfg(all(feature = "allocation-profiling", not(feature = "perf-tracy")))]
#[global_allocator]
static GLOBAL: mesh_core_debug::allocation::CountingAllocator<std::alloc::System> =
    mesh_core_debug::allocation::CountingAllocator::new(std::alloc::System);

fn main() {
    init_tracing();

    let args: Vec<String> = std::env::args().collect();
    let command = args.get(1).map(|s| s.as_str());

    match command {
        Some("start") | None => cmd_start(),
        Some("list") => cmd_list(),
        Some("services") => cmd_services(),
        Some("debug") => cmd_debug(&args[2..]),
        Some("ipc") => cmd_ipc(&args[2..]),
        Some("ipc-socket-path") => cmd_ipc_socket_path(),
        Some("config") => cmd_config(&args[2..]),
        Some("profile") => cmd_profile(&args[2..]),
        Some("install") => cmd_install(&args[2..]),
        Some("update") => cmd_update(&args[2..]),
        Some("rollback") => cmd_rollback(&args[2..]),
        Some("uninstall") => cmd_uninstall(&args[2..]),
        Some("lock") => cmd_lock(&args[2..]),
        Some("status") => cmd_status(),
        Some("locale") => cmd_locale(&args[2..]),
        Some("resources") => cmd_resources(&args[2..]),
        Some("version") => cmd_version(),
        Some("help") | Some("--help") | Some("-h") => cmd_help(),
        Some(other) => {
            eprintln!("unknown command: {other}");
            eprintln!("run 'mesh-shell help' for usage");
            std::process::exit(1);
        }
    }
}

fn init_tracing() {
    use tracing_subscriber::prelude::*;

    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    let fmt_layer = tracing_subscriber::fmt::layer().with_filter(env_filter);

    #[cfg(feature = "perf-tracy")]
    tracing_subscriber::registry()
        .with(fmt_layer)
        .with(tracing_tracy::TracyLayer::default())
        .init();

    #[cfg(not(feature = "perf-tracy"))]
    tracing_subscriber::registry().with(fmt_layer).init();
}

fn cmd_start() {
    tracing::info!(
        build_profile = env!("MESH_BUILD_PROFILE"),
        "starting MESH shell"
    );
    let mut shell = Shell::new();
    if let Err(err) = shell.run() {
        tracing::error!("MESH shell exited with error: {err}");
        std::process::exit(1);
    }
}

fn cmd_list() {
    let graph = load_authoring_snapshot();
    let mut count = 0;
    for module in graph.modules() {
        let manifest = module.manifest.clone().into_runtime_manifest();
        let id = &module.id;
        let kind = manifest.package.module_type;
        match (&kind, manifest.primary_service()) {
            (ModuleType::Backend, Some(svc)) => {
                println!(
                    "{id}  ({kind}, provides: {}, backend: {}, manifest: {})",
                    svc.provides, svc.backend_name, module.manifest_source
                );
            }
            _ => {
                println!("{id}  ({kind}, manifest: {})", module.manifest_source);
            }
        }
        count += 1;
    }

    if count == 0 {
        println!("no modules found");
    }
}

fn cmd_services() {
    let graph = load_authoring_snapshot();

    // Group backends by service type.
    let mut by_service: std::collections::HashMap<String, Vec<(String, String, u32)>> =
        std::collections::HashMap::new();

    for module in graph.modules() {
        let manifest = module.manifest.clone().into_runtime_manifest();
        if manifest.package.module_type == ModuleType::Backend {
            if let Some(svc) = manifest.primary_service() {
                by_service.entry(svc.provides.clone()).or_default().push((
                    module.id.clone(),
                    svc.backend_name.clone(),
                    svc.priority,
                ));
            }
        }
    }

    if by_service.is_empty() {
        println!("no service backends found");
        return;
    }

    for (service, mut backends) in by_service {
        backends.sort_by(|a, b| b.2.cmp(&a.2));
        println!("{service}:");
        for (id, name, priority) in &backends {
            println!("  {name} ({id}) priority={priority}");
        }
    }
}

fn cmd_status() {
    let shell = Shell::new();
    println!("MESH v{}", env!("CARGO_PKG_VERSION"));
    println!("theme: {}", shell.theme.active().name);
    println!("locale: {}", shell.locale.current());
}

fn resource_snapshot_for_cli() -> mesh_core_resources::ResourceExplanationSnapshot {
    let mut shell = Shell::new();
    shell.discover_modules();
    if let Err(error) = shell.resolve_modules() {
        exit_error(format!("failed to resolve resources: {error}"));
    }
    shell.resource_explanation_snapshot()
}

fn cmd_resources(args: &[String]) {
    let snapshot = resource_snapshot_for_cli();
    match args.first().map(String::as_str) {
        Some("show") | None => println!(
            "{}",
            serde_json::to_string_pretty(&snapshot).expect("resource snapshot serialization")
        ),
        Some("icons") => println!(
            "{}",
            serde_json::to_string_pretty(&snapshot.icons)
                .expect("icon resource explanation serialization")
        ),
        Some("fonts") => println!(
            "{}",
            serde_json::to_string_pretty(&snapshot.fonts)
                .expect("font resource explanation serialization")
        ),
        Some("coverage") => {
            let mut request =
                mesh_core_resources::ResourceCoverageRequest::from_snapshot(&snapshot);
            let mut index = 1;
            while index < args.len() {
                let required = match args[index].as_str() {
                    "--font-script" => true,
                    "--optional-font-script" => false,
                    other => exit_error(format!(
                        "unknown resources coverage option: {other}\nusage: mesh-shell resources coverage [--font-script <module>:<role>:<script>] [--optional-font-script <module>:<role>:<script>]"
                    )),
                };
                let value = args.get(index + 1).unwrap_or_else(|| {
                    exit_error(format!("{} requires <module>:<role>:<script>", args[index]))
                });
                let mut parts = value.splitn(3, ':');
                let (Some(module_id), Some(role), Some(script)) =
                    (parts.next(), parts.next(), parts.next())
                else {
                    exit_error(format!(
                        "invalid font script need '{value}'; expected <module>:<role>:<script>"
                    ));
                };
                if module_id.is_empty() || role.is_empty() || script.is_empty() {
                    exit_error(format!(
                        "invalid font script need '{value}'; module, role, and script must be non-empty"
                    ));
                }
                request.add_font_script(module_id, role, script, required);
                index += 2;
            }
            let advice = snapshot.advise_coverage(&request);
            println!(
                "{}",
                serde_json::to_string_pretty(&advice)
                    .expect("resource coverage advice serialization")
            );
        }
        Some("doctor") => {
            if snapshot.diagnostics.is_empty() {
                println!("resource snapshot: no problems found");
                return;
            }
            for diagnostic in &snapshot.diagnostics {
                let owner = diagnostic
                    .module_id
                    .as_deref()
                    .or(diagnostic.pack_id.as_deref())
                    .unwrap_or("resources");
                println!("{} {}: {}", diagnostic.severity, owner, diagnostic.message);
            }
            if snapshot
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.severity == "error")
            {
                std::process::exit(1);
            }
        }
        Some(other) => exit_error(format!(
            "unknown resources subcommand: {other}\nsubcommands: show, icons, fonts, coverage, doctor"
        )),
    }
}

fn locale_read_model() -> (
    mesh_core_module::package::AuthoringSnapshot,
    mesh_core_locale::LocaleEngine,
    Vec<mesh_core_locale::CatalogSourceDiagnostics>,
    mesh_core_config::LocalePolicy,
) {
    let graph = load_authoring_snapshot();
    let settings = mesh_core_config::load_shell_settings()
        .unwrap_or_else(|error| exit_error(format!("failed to load locale settings: {error}")));
    let policy = settings.i18n.policy;
    let settings = mesh_core_config::resolve_shell_locale_settings(&settings);
    let mut engine = mesh_core_locale::LocaleEngine::try_with_fallback_locale(
        settings.i18n.locale,
        settings.i18n.fallback_locale,
    )
    .unwrap_or_else(|error| exit_error(format!("invalid locale settings: {error}")));
    let (sources, defaults) = graph
        .locale_catalog_sources()
        .unwrap_or_else(|error| exit_error(error));
    let prepared = engine
        .prepare_catalog_snapshot_off_thread(sources, defaults)
        .unwrap_or_else(|error| exit_error(format!("failed to prepare locale catalogs: {error}")));
    let diagnostics = prepared.diagnostics().to_vec();
    engine.replace_catalog_snapshot(prepared.snapshot());
    (graph, engine, diagnostics, policy)
}

fn cmd_locale(args: &[String]) {
    match args.first().map(String::as_str) {
        Some("list") | None => {
            let graph = load_authoring_snapshot();
            let (sources, defaults) = graph
                .locale_catalog_sources()
                .unwrap_or_else(|error| exit_error(error));
            let mut locales = sources
                .into_iter()
                .map(|source| source.locale)
                .chain(defaults.into_values())
                .collect::<Vec<_>>();
            locales.sort();
            locales.dedup();
            if locales.is_empty() {
                println!("no locales available");
            } else {
                for locale in locales {
                    println!("{locale}");
                }
            }
        }
        Some("active") => {
            let (_, engine, _, policy) = locale_read_model();
            println!("policy: {}", policy.as_str());
            println!("locale: {}", engine.current());
            println!("chain: {}", engine.fallback_chain().join(", "));
            println!("direction: {}", engine.direction().as_str());
            println!("revision: {}", engine.revision());
            println!("catalog_revision: {}", engine.catalog_snapshot().revision());
        }
        Some("set") => cmd_locale_set(args),
        Some("set-system") => cmd_locale_set_system(),
        Some("which") => {
            let module_id = required_arg(args, 1, "mesh-shell locale which <module> <key>");
            let key = required_arg(args, 2, "mesh-shell locale which <module> <key>");
            let (_, engine, _, _) = locale_read_model();
            match engine.module_translator(module_id).source(key) {
                Some(source) => println!(
                    "{} {} {} {} {}",
                    source.kind_name(),
                    source.owner_module_id,
                    source.target_module_id,
                    source.locale,
                    source.path.display()
                ),
                None => println!("missing {module_id} {key}"),
            }
        }
        Some("missing") => {
            let module_id = required_arg(args, 1, "mesh-shell locale missing <module>");
            let (graph, engine, _, _) = locale_read_model();
            let translator = engine.module_translator(module_id);
            for key in graph.localized_keys(module_id) {
                if translator.translate(&key).is_none() {
                    println!("{key}");
                }
            }
        }
        Some("extract") => {
            let module_id = required_arg(args, 1, "mesh-shell locale extract <module>");
            let graph = load_authoring_snapshot();
            let mut catalog = serde_json::Map::new();
            for key in graph.localized_keys(module_id) {
                catalog.insert(key, serde_json::Value::String(String::new()));
            }
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::Value::Object(catalog))
                    .expect("locale extraction serialization")
            );
        }
        Some("doctor") => {
            let (graph, engine, catalog_diagnostics, _) = locale_read_model();
            let mut issues = 0;
            for source in catalog_diagnostics {
                for diagnostic in source.diagnostics {
                    issues += 1;
                    println!(
                        "error {} {} {}: {}",
                        source.module_id, source.locale, diagnostic.key, diagnostic.message
                    );
                }
            }
            for module in graph.enabled_modules() {
                for (path, error) in graph.component_source_errors(&module.id) {
                    issues += 1;
                    println!("error {} {}: {}", module.id, path.display(), error);
                }
                let translator = engine.module_translator(&module.id);
                for key in graph.localized_keys(&module.id) {
                    if translator.translate(&key).is_none() {
                        issues += 1;
                        println!(
                            "warning {}: static key '{}' has no entry in the active locale chain",
                            module.id, key
                        );
                    }
                }
            }
            if issues == 0 {
                println!("locale catalogs and static keys are healthy");
            }
        }
        Some(other) => exit_error(format!(
            "unknown locale subcommand: {other}\nsubcommands: list, active, set, set-system, which, missing, extract, doctor"
        )),
    }
}

fn cmd_locale_set(args: &[String]) {
    let requested = required_arg(args, 1, "mesh-shell locale set <code>");
    cmd_locale_set_with_policy(
        Some(requested.to_string()),
        mesh_core_config::LocalePolicy::Manual,
    );
}

fn cmd_locale_set_system() {
    cmd_locale_set_with_policy(None, mesh_core_config::LocalePolicy::FollowSystem);
}

fn cmd_locale_set_with_policy(requested: Option<String>, policy: mesh_core_config::LocalePolicy) {
    let shared = load_settings_store();
    let shared_revision = shared.revision();
    let paths = profile_paths();
    let active_profile_id = paths
        .active_profile_id()
        .unwrap_or_else(|error| exit_error(error));
    let (settings, mut profile_commit) = if let Some(profile_id) = active_profile_id {
        let profile = paths
            .load(&profile_id)
            .unwrap_or_else(|error| exit_error(error));
        let mut effective = shared.to_value();
        let root = effective
            .as_object_mut()
            .expect("settings store serializes an object");
        for (namespace, overrides) in &profile.settings {
            let target = root
                .entry(namespace.clone())
                .or_insert_with(|| serde_json::json!({}));
            mesh_core_config::merge_json(target, overrides);
        }
        let effective =
            mesh_core_config::SettingsStore::from_value(shared.path().to_path_buf(), effective)
                .unwrap_or_else(|error| {
                    exit_error(format!("failed to resolve profile settings: {error}"))
                });
        let expected_revision = profile.revision;
        (
            effective.shell().clone(),
            Some((profile_id, profile, expected_revision)),
        )
    } else {
        (shared.shell().clone(), None)
    };
    let settings = mesh_core_config::resolve_shell_locale_settings(&settings);
    let requested = requested.unwrap_or_else(|| {
        mesh_core_locale::system_locale().unwrap_or_else(|| settings.i18n.locale.clone())
    });
    let mut candidate = mesh_core_locale::LocaleEngine::try_with_fallback_locale(
        requested,
        settings.i18n.fallback_locale.clone(),
    )
    .unwrap_or_else(|error| exit_error(format!("invalid locale: {error}")));
    let graph = load_authoring_snapshot();
    let (sources, defaults) = graph
        .locale_catalog_sources()
        .unwrap_or_else(|error| exit_error(error));
    let prepared = candidate
        .prepare_catalog_snapshot_off_thread(sources, defaults)
        .unwrap_or_else(|error| exit_error(format!("failed to prepare locale catalogs: {error}")));
    candidate.replace_catalog_snapshot(prepared.snapshot());

    let locale_settings = serde_json::json!({
        "i18n": {
            "policy": policy.as_str(),
            "locale": candidate.current(),
            "fallback_locale": candidate.fallback_locale(),
        }
    });
    if let Some((profile_id, mut profile, expected_revision)) = profile_commit.take() {
        mesh_core_config::SettingsStore::load_from(shared.path())
            .unwrap_or_else(|error| exit_error(format!("failed to recheck settings: {error}")))
            .check_revision(shared_revision)
            .unwrap_or_else(|error| {
                exit_error(format!(
                    "locale settings changed during preparation: {error}"
                ))
            });
        let shell = profile
            .settings
            .entry(mesh_core_config::SHELL_NAMESPACE.to_string())
            .or_insert_with(|| serde_json::json!({}));
        mesh_core_config::merge_json(shell, &locale_settings);
        paths
            .save_if_revision(&profile_id, &profile, expected_revision)
            .unwrap_or_else(|error| {
                exit_error(format!("failed to persist profile locale: {error}"))
            });
    } else {
        let mut store = shared;
        store.merge_namespace(mesh_core_config::SHELL_NAMESPACE, &locale_settings);
        store
            .save_if_revision(shared_revision)
            .unwrap_or_else(|error| exit_error(format!("failed to persist locale: {error}")));
    }
    println!("active locale: {}", candidate.current());
}

fn cmd_debug(args: &[String]) {
    match args.first().map(String::as_str) {
        Some("profiling") => send_ipc_command("shell:debug_profiling"),
        Some("tab") => send_ipc_command("shell:debug_cycle_tab"),
        Some(other) => {
            eprintln!("unknown debug command: {other}");
            eprintln!("usage: mesh-shell debug [profiling|tab]");
            std::process::exit(1);
        }
        None => send_ipc_command("shell:debug_overlay"),
    }
}

fn send_ipc_command(command: &str) {
    if let Err(error) = try_send_ipc_command(command) {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

#[derive(Debug)]
enum IpcCommandError {
    Connect {
        socket_path: std::path::PathBuf,
        source: io::Error,
    },
    Send(io::Error),
    Receive(io::Error),
    EmptyResponse,
    Rejected(String),
}

impl IpcCommandError {
    fn is_absent_shell(&self) -> bool {
        matches!(
            self,
            Self::Connect { source, .. }
                if matches!(
                    source.kind(),
                    io::ErrorKind::NotFound | io::ErrorKind::ConnectionRefused
                )
        )
    }
}

impl std::fmt::Display for IpcCommandError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Connect {
                socket_path,
                source,
            } => write!(
                formatter,
                "failed to connect to shell ipc socket {}: {source}",
                socket_path.display()
            ),
            Self::Send(source) => write!(formatter, "failed to send ipc command: {source}"),
            Self::Receive(source) => write!(formatter, "failed to read ipc response: {source}"),
            Self::EmptyResponse => {
                formatter.write_str("shell ipc socket closed without a response")
            }
            Self::Rejected(response) => formatter.write_str(response),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "status")]
enum ProfileSwitchAck {
    #[serde(rename = "committed")]
    Committed { profile_id: String, generation: u64 },
    #[serde(rename = "rejected")]
    Rejected {
        profile_id: String,
        generation: u64,
        reason: String,
    },
}

fn parse_profile_switch_ack(
    response: &str,
    requested_profile_id: &str,
) -> Result<ProfileSwitchAck, String> {
    let ack: ProfileSwitchAck = serde_json::from_str(response.trim())
        .map_err(|error| format!("expected a profile switch result, got invalid JSON: {error}"))?;
    let acknowledged_profile_id = match &ack {
        ProfileSwitchAck::Committed { profile_id, .. }
        | ProfileSwitchAck::Rejected { profile_id, .. } => profile_id,
    };
    if acknowledged_profile_id != requested_profile_id {
        return Err(format!(
            "acknowledged profile '{acknowledged_profile_id}', expected '{requested_profile_id}'"
        ));
    }
    Ok(ack)
}

fn try_send_ipc_command(command: &str) -> Result<String, IpcCommandError> {
    let socket_path = default_ipc_socket_path();
    let mut stream =
        UnixStream::connect(&socket_path).map_err(|source| IpcCommandError::Connect {
            socket_path: socket_path.clone(),
            source,
        })?;
    writeln!(stream, "{command}").map_err(IpcCommandError::Send)?;
    let mut reader = BufReader::new(stream);
    let mut response = String::new();
    let read = reader
        .read_line(&mut response)
        .map_err(IpcCommandError::Receive)?;
    if read == 0 {
        return Err(IpcCommandError::EmptyResponse);
    }
    if response.starts_with("error ") {
        return Err(IpcCommandError::Rejected(response.trim().to_string()));
    }
    Ok(response)
}

fn cmd_ipc(args: &[String]) {
    if args.is_empty() {
        eprintln!("usage: mesh-shell ipc <command>");
        eprintln!("example: mesh-shell ipc shell:open_launcher");
        std::process::exit(1);
    }

    let command = args.join(" ");
    let socket_path = default_ipc_socket_path();
    let mut stream = match UnixStream::connect(&socket_path) {
        Ok(stream) => stream,
        Err(err) => {
            eprintln!(
                "failed to connect to shell ipc socket {}: {err}",
                socket_path.display()
            );
            std::process::exit(1);
        }
    };

    if let Err(err) = writeln!(stream, "{command}") {
        eprintln!("failed to send ipc command: {err}");
        std::process::exit(1);
    }

    let mut reader = BufReader::new(stream);
    let mut response = String::new();
    match reader.read_line(&mut response) {
        Ok(0) => {
            eprintln!("shell ipc socket closed without a response");
            std::process::exit(1);
        }
        Ok(_) => {
            print!("{response}");
            if response.starts_with("error ") {
                std::process::exit(1);
            }
        }
        Err(err) => {
            eprintln!("failed to read ipc response: {err}");
            std::process::exit(1);
        }
    }
}

fn cmd_ipc_socket_path() {
    println!("{}", default_ipc_socket_path().display());
}

fn cmd_version() {
    println!("mesh-shell {}", env!("CARGO_PKG_VERSION"));
}

fn root_module_graph_path() -> std::path::PathBuf {
    if let Ok(path) = std::env::var("MESH_MODULE_GRAPH_PATH")
        && !path.trim().is_empty()
    {
        return path.into();
    }
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .join("config/module.json")
}

fn load_authoring_snapshot_at(
    root_path: &std::path::Path,
) -> Result<
    mesh_core_module::package::AuthoringSnapshot,
    mesh_core_module::package::ModuleManifestError,
> {
    mesh_core_module::package::load_authoring_snapshot(root_path)
}

fn load_authoring_snapshot() -> mesh_core_module::package::AuthoringSnapshot {
    load_authoring_snapshot_at(&root_module_graph_path())
        .unwrap_or_else(|error| exit_error(format!("failed to resolve authoring graph: {error}")))
}

fn profile_paths() -> mesh_core_module::package::ProfilePaths {
    mesh_core_module::package::ProfilePaths::from_root_graph(&root_module_graph_path())
        .unwrap_or_else(|error| exit_error(error.to_string()))
}

fn exit_error(message: impl std::fmt::Display) -> ! {
    eprintln!("{message}");
    std::process::exit(1);
}

fn cmd_profile(args: &[String]) {
    let paths = profile_paths();
    match args.first().map(String::as_str) {
        Some("list") | None => {
            let active = paths
                .active_profile_id()
                .unwrap_or_else(|error| exit_error(error));
            let profiles = paths.list().unwrap_or_else(|error| exit_error(error));
            if profiles.is_empty() {
                println!("no profiles; the legacy root graph is active");
            }
            for profile in profiles {
                let marker = if active.as_deref() == Some(profile.as_str()) {
                    "*"
                } else {
                    " "
                };
                println!("{marker} {profile}");
            }
        }
        Some("create") => {
            let profile_id = required_arg(args, 1, "mesh-shell profile create <profile>");
            let path = paths
                .profile_path(profile_id)
                .unwrap_or_else(|error| exit_error(error));
            if path.exists() {
                exit_error(format!("profile {profile_id} already exists"));
            }
            paths
                .save(profile_id, &mesh_core_module::package::ShellProfile::new())
                .unwrap_or_else(|error| exit_error(error));
            println!("created profile {profile_id}; add roots, then select it with 'profile use'");
        }
        Some("use") => {
            let profile_id = required_arg(args, 1, "mesh-shell profile use <profile>");
            // Validate before changing the pointer. A malformed profile never
            // replaces the currently active composition.
            paths
                .load(profile_id)
                .unwrap_or_else(|error| exit_error(error));
            let command = format!("shell:switch_profile:{profile_id}");
            match try_send_ipc_command(&command) {
                Ok(response) => match parse_profile_switch_ack(&response, profile_id) {
                    Ok(ProfileSwitchAck::Committed { generation, .. }) => println!(
                        "profile switched live: {profile_id} (activation generation {generation})"
                    ),
                    Ok(ProfileSwitchAck::Rejected {
                        generation, reason, ..
                    }) => exit_error(format!(
                        "live profile switch rejected for {profile_id} at activation generation {generation}: {reason}; active profile unchanged"
                    )),
                    Err(error) => exit_error(format!(
                        "live profile switch returned an invalid acknowledgement: {error}; active profile unchanged"
                    )),
                },
                Err(error) if error.is_absent_shell() => {
                    paths
                        .set_active(profile_id)
                        .unwrap_or_else(|error| exit_error(error));
                    println!("active profile: {profile_id} (applies when the shell starts)");
                }
                Err(error) => exit_error(format!(
                    "live profile switch failed: {error}; active profile unchanged"
                )),
            }
        }
        Some("show") => {
            let profile_id = args
                .get(1)
                .cloned()
                .or_else(|| {
                    paths
                        .active_profile_id()
                        .unwrap_or_else(|error| exit_error(error))
                })
                .unwrap_or_else(|| exit_error("no active profile; specify a profile id"));
            let profile = paths
                .load(&profile_id)
                .unwrap_or_else(|error| exit_error(error));
            println!(
                "{}",
                serde_json::to_string_pretty(&profile).expect("profile serialization")
            );
        }
        Some("add") => {
            let profile_id = required_arg(
                args,
                1,
                "mesh-shell profile add <profile> <frontend-module>",
            );
            let module_id = required_arg(
                args,
                2,
                "mesh-shell profile add <profile> <frontend-module>",
            );
            let mut profile = paths
                .load_or_default(profile_id)
                .unwrap_or_else(|error| exit_error(error));
            let graph = load_authoring_snapshot();
            let module = graph
                .module(module_id)
                .unwrap_or_else(|| exit_error(format!("module {module_id} is not installed")));
            let instance_id = profile
                .add_frontend(&module.manifest)
                .unwrap_or_else(|error| exit_error(error));
            paths
                .save(profile_id, &profile)
                .unwrap_or_else(|error| exit_error(error));
            println!("added active instance {instance_id} to profile {profile_id}");
            println!("placement and props inherit the module's declared defaults");
        }
        Some("enable") | Some("disable") => {
            let active = args[0] == "enable";
            let usage = format!("mesh-shell profile {} <profile> <module#instance>", args[0]);
            let profile_id = required_arg(args, 1, &usage);
            let instance_id = required_arg(args, 2, &usage);
            let mut profile = paths
                .load(profile_id)
                .unwrap_or_else(|error| exit_error(error));
            profile
                .set_instance_active(instance_id, active)
                .unwrap_or_else(|error| exit_error(error));
            paths
                .save(profile_id, &profile)
                .unwrap_or_else(|error| exit_error(error));
            println!(
                "{} {instance_id} in profile {profile_id}",
                if active { "enabled" } else { "disabled" }
            );
        }
        Some("remove") => {
            let profile_id = required_arg(
                args,
                1,
                "mesh-shell profile remove <profile> <module#instance>",
            );
            let instance_id = required_arg(
                args,
                2,
                "mesh-shell profile remove <profile> <module#instance>",
            );
            let mut profile = paths
                .load(profile_id)
                .unwrap_or_else(|error| exit_error(error));
            if !profile.remove_instance(instance_id) {
                exit_error(format!(
                    "profile {profile_id} has no instance {instance_id}"
                ));
            }
            paths
                .save(profile_id, &profile)
                .unwrap_or_else(|error| exit_error(error));
            println!("removed {instance_id} from profile {profile_id}");
        }
        Some("set") => {
            let profile_id = required_arg(
                args,
                1,
                "mesh-shell profile set <profile> <namespace> <json-object>",
            );
            let namespace = required_arg(
                args,
                2,
                "mesh-shell profile set <profile> <namespace> <json-object>",
            );
            let value = required_arg(
                args,
                3,
                "mesh-shell profile set <profile> <namespace> <json-object>",
            );
            let value: serde_json::Value = serde_json::from_str(value)
                .unwrap_or_else(|error| exit_error(format!("invalid settings JSON: {error}")));
            if !value.is_object() {
                exit_error("profile settings value must be a JSON object");
            }
            let mut profile = paths
                .load(profile_id)
                .unwrap_or_else(|error| exit_error(error));
            profile.settings.insert(namespace.to_string(), value);
            paths
                .save(profile_id, &profile)
                .unwrap_or_else(|error| exit_error(error));
            println!("updated {namespace} settings in profile {profile_id}");
        }
        Some("unset") => {
            let profile_id =
                required_arg(args, 1, "mesh-shell profile unset <profile> <namespace>");
            let namespace = required_arg(args, 2, "mesh-shell profile unset <profile> <namespace>");
            let mut profile = paths
                .load(profile_id)
                .unwrap_or_else(|error| exit_error(error));
            profile.settings.remove(namespace);
            paths
                .save(profile_id, &profile)
                .unwrap_or_else(|error| exit_error(error));
            println!("removed {namespace} settings from profile {profile_id}");
        }
        Some("prune") => {
            let profile_id = required_arg(args, 1, "mesh-shell profile prune <profile>");
            let root_path = root_module_graph_path();
            let graph =
                load_authoring_snapshot_at(&root_path).unwrap_or_else(|error| exit_error(error));
            let mut profile = paths
                .load(profile_id)
                .unwrap_or_else(|error| exit_error(error));
            let manifests = graph
                .modules()
                .into_iter()
                .map(|module| module.manifest.clone())
                .collect::<Vec<_>>();
            let resolved =
                mesh_core_module::package::resolve_composition(&profile, manifests.iter())
                    .unwrap_or_else(|error| exit_error(error));
            if resolved.orphaned_overrides.is_empty() {
                println!("profile {profile_id} has no orphaned overrides");
                return;
            }
            for instance_id in &resolved.orphaned_overrides {
                profile.roots.remove(instance_id);
                profile.settings.remove(instance_id);
                println!("pruned {instance_id}");
            }
            paths
                .save(profile_id, &profile)
                .unwrap_or_else(|error| exit_error(error));
        }
        Some(other) => exit_error(format!(
            "unknown profile subcommand: {other}\nsubcommands: list, create, use, show, add, enable, disable, remove, set, unset, prune"
        )),
    }
}

fn required_arg<'a>(args: &'a [String], index: usize, usage: &str) -> &'a str {
    args.get(index)
        .map(String::as_str)
        .unwrap_or_else(|| exit_error(format!("usage: {usage}")))
}

fn cmd_install(args: &[String]) {
    let source_arg = required_arg(
        args,
        0,
        "mesh-shell install <path-or-git-url>[#ref] [--available-only] [--profile <profile>] [--allow-elevated] [--allow-high]",
    );
    let root_path = root_module_graph_path();
    let config_dir = root_path
        .parent()
        .expect("root graph path has a parent directory")
        .to_path_buf();
    let mut transaction = mesh_core_module::package::PackageTransaction::begin(
        &config_dir,
        mesh_core_module::package::PackageOwner::Cli,
        mesh_core_module::package::PackageOperation::Install,
    )
    .unwrap_or_else(|error| exit_error(error));
    let mut root = mesh_core_module::package::RootModuleGraphManifest::from_path(&root_path)
        .unwrap_or_else(|error| exit_error(error));
    let modules_dir = config_dir.join(&root.modules_dir);
    transaction
        .protect_package_state(&root_path, &modules_dir)
        .unwrap_or_else(|error| exit_error(error));
    std::fs::create_dir_all(&modules_dir)
        .unwrap_or_else(|error| exit_error(format!("failed to create modules directory: {error}")));
    let source = install_source(source_arg, &transaction.staging_dir())
        .unwrap_or_else(|error| exit_error(error));
    let source_path = source.path();
    let manifest_path = source_path.join("module.json");
    let manifest = mesh_core_module::package::ModuleManifest::from_path(&manifest_path)
        .unwrap_or_else(|error| exit_error(error));

    let signature = mesh_core_module::package::load_module_signature(source_path)
        .unwrap_or_else(|error| exit_error(error.to_string()));
    let digest = mesh_core_module::package::module_tree_digest(source_path)
        .unwrap_or_else(|error| exit_error(error.to_string()));
    let trust = if signature.is_some() {
        mesh_core_module::package::TrustTier::Verified
    } else {
        mesh_core_module::package::TrustTier::for_source(
            &manifest.name,
            matches!(&source, InstallSource::Git { .. }),
        )
    };
    if let Err(error) = root.trust_policy.validate_candidate(
        &manifest.name,
        &manifest.version,
        &digest,
        trust,
        signature.as_ref(),
    ) {
        exit_error(format!(
            "module {} provenance rejected: {error}",
            manifest.name
        ));
    }

    let allow_elevated = args.iter().any(|arg| arg == "--allow-elevated");
    let allow_high = args.iter().any(|arg| arg == "--allow-high");
    let catalog = mesh_core_capability::CapabilityCatalog::builtin();
    let requested = manifest
        .mesh
        .capabilities
        .required
        .iter()
        .chain(manifest.mesh.capabilities.optional.iter())
        .map(|id| mesh_core_capability::Capability::new(id.clone()))
        .collect::<Vec<_>>();
    for capability in &requested {
        let level = catalog
            .validate(capability.id())
            .unwrap_or_else(|error| exit_error(error.to_string()));
        use mesh_core_capability::PrivilegeLevel;
        match level {
            PrivilegeLevel::High if !allow_high => exit_error(format!(
                "{} requests high capability {}; review it and repeat with --allow-high",
                manifest.name, capability
            )),
            PrivilegeLevel::Elevated if !allow_elevated && !allow_high => exit_error(format!(
                "{} requests elevated capability {}; review it and repeat with --allow-elevated",
                manifest.name, capability
            )),
            _ => {}
        }
    }
    let destination = mesh_core_module::package::module_install_path(&modules_dir, &manifest.name)
        .unwrap_or_else(|error| exit_error(error));
    if destination.exists() {
        exit_error(format!(
            "module {} is already installed at {}",
            manifest.name,
            destination.display()
        ));
    }
    transaction
        .protect(&destination)
        .unwrap_or_else(|error| exit_error(error));

    if let Err(error) = source.place_at(&destination) {
        let _ = transaction.abort();
        exit_error(format!("failed to install {}: {error}", manifest.name));
    }

    let approvals = root
        .capability_approvals
        .entry(manifest.name.clone())
        .or_default();
    for capability in &manifest.mesh.capabilities.required {
        if !approvals.contains(capability) {
            approvals.push(capability.clone());
        }
    }
    approvals.sort();
    approvals.dedup();
    if let Err(error) = root.save(&root_path) {
        let _ = transaction.abort();
        exit_error(error);
    }

    let install_result = (|| -> Result<Option<String>, String> {
        let graph = load_authoring_snapshot_at(&root_path).map_err(|error| error.to_string())?;
        let installed = graph
            .module(&manifest.name)
            .ok_or_else(|| "installed module was not discovered".to_string())?;
        if installed.kind != manifest.mesh.kind {
            return Err("installed module kind changed while copying".into());
        }
        let installed_manifests = graph
            .modules()
            .into_iter()
            .map(|module| module.manifest.clone())
            .collect::<Vec<_>>();

        use mesh_core_module::package::ModuleKind;

        // Installing a composition selects it: the profile records `from` and
        // inherits its roots, bindings, resources, and slot arrangement. It
        // creates no root of its own — a composition binds, it never owns.
        if manifest.mesh.kind == ModuleKind::Composition
            && !args.iter().any(|arg| arg == "--available-only")
        {
            let paths = profile_paths();
            let profile_id = args
                .iter()
                .position(|arg| arg == "--profile")
                .and_then(|index| args.get(index + 1))
                .cloned()
                .or_else(|| paths.active_profile_id().ok().flatten())
                .unwrap_or_else(|| mesh_core_module::package::DEFAULT_PROFILE_ID.to_string());
            let mut profile = paths
                .load_or_default(&profile_id)
                .map_err(|error| error.to_string())?;
            profile.from = Some(mesh_core_module::package::CompositionRef {
                module: manifest.name.clone(),
                version: Some(manifest.version.clone()),
            });
            paths
                .save(&profile_id, &profile)
                .map_err(|error| error.to_string())?;
            if paths
                .active_profile_id()
                .map_err(|error| error.to_string())?
                .is_none()
            {
                paths
                    .set_active(&profile_id)
                    .map_err(|error| error.to_string())?;
            }
            record_lock_entry(
                &config_dir,
                &manifest.name,
                &manifest.version,
                &destination,
                &modules_dir,
                &source,
                &installed_manifests,
                Some(mesh_core_module::package::LockedComposition {
                    module: manifest.name.clone(),
                    version: manifest.version.clone(),
                }),
            )?;
            return Ok(Some(format!(
                "composition {} in profile {profile_id}",
                manifest.name
            )));
        }

        if args.iter().any(|arg| arg == "--available-only")
            || manifest.mesh.kind != ModuleKind::Frontend
        {
            record_lock_entry(
                &config_dir,
                &manifest.name,
                &manifest.version,
                &destination,
                &modules_dir,
                &source,
                &installed_manifests,
                None,
            )?;
            return Ok(None);
        }

        let paths = profile_paths();
        let explicit_profile = args
            .iter()
            .position(|arg| arg == "--profile")
            .and_then(|index| args.get(index + 1))
            .cloned();
        let profile_id = explicit_profile.or_else(|| paths.active_profile_id().ok().flatten());
        let Some(profile_id) = profile_id else {
            // Legacy auto-discovery already activates new modules by default.
            record_lock_entry(
                &config_dir,
                &manifest.name,
                &manifest.version,
                &destination,
                &modules_dir,
                &source,
                &installed_manifests,
                None,
            )?;
            return Ok(Some("legacy root graph (auto-enabled)".into()));
        };
        let mut profile = paths
            .load_or_default(&profile_id)
            .map_err(|error| error.to_string())?;
        let instance_id = profile
            .add_frontend(&manifest)
            .map_err(|error| error.to_string())?;
        let manifests = graph
            .modules()
            .into_iter()
            .map(|module| &module.manifest)
            .collect::<Vec<_>>();
        profile
            .active_module_ids(manifests)
            .map_err(|error| error.to_string())?;
        let profile_path = paths
            .profile_path(&profile_id)
            .map_err(|error| error.to_string())?;
        let previous_profile = std::fs::read(&profile_path).ok();
        paths
            .save(&profile_id, &profile)
            .map_err(|error| error.to_string())?;
        if paths
            .active_profile_id()
            .map_err(|error| error.to_string())?
            .is_none()
        {
            if let Err(error) = paths.set_active(&profile_id) {
                match previous_profile {
                    Some(content) => {
                        let _ = std::fs::write(&profile_path, content);
                    }
                    None => {
                        let _ = std::fs::remove_file(&profile_path);
                    }
                }
                return Err(error.to_string());
            }
        }
        record_lock_entry(
            &config_dir,
            &manifest.name,
            &manifest.version,
            &destination,
            &modules_dir,
            &source,
            &installed_manifests,
            None,
        )?;
        Ok(Some(format!("{instance_id} in profile {profile_id}")))
    })();

    match install_result {
        Ok(composition) => {
            println!("installed {} at {}", manifest.name, destination.display());
            if let Some(composition) = composition {
                println!("activated {composition}; declared defaults remain inherited");
            } else {
                println!(
                    "available but not independently activated ({:?})",
                    manifest.mesh.kind
                );
            }
        }
        Err(error) => {
            let _ = transaction.abort();
            exit_error(format!(
                "installation validation failed; transaction aborted: {error}"
            ));
        }
    }

    transaction
        .commit()
        .unwrap_or_else(|error| exit_error(format!("failed to commit installation: {error}")));
}

fn copy_module_tree(
    source: &std::path::Path,
    destination: &std::path::Path,
) -> std::io::Result<()> {
    std::fs::create_dir_all(destination)?;
    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let target = destination.join(entry.file_name());
        if file_type.is_symlink() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!(
                    "module contains unsupported symlink {}",
                    entry.path().display()
                ),
            ));
        }
        if file_type.is_dir() {
            copy_module_tree(&entry.path(), &target)?;
        } else if file_type.is_file() {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

/// A local directory or a staged checkout. Git sources are cloned outside the
/// installed tree, validated exactly like local sources, then renamed into
/// place so a failed clone or manifest check never leaves a partial module.
enum InstallSource {
    Local(std::path::PathBuf),
    Git {
        checkout: std::path::PathBuf,
        provenance: GitProvenance,
    },
}

impl InstallSource {
    fn path(&self) -> &std::path::Path {
        match self {
            Self::Local(path) => path,
            Self::Git { checkout, .. } => checkout,
        }
    }

    fn place_at(&self, destination: &std::path::Path) -> Result<(), String> {
        mesh_core_module::package::validate_module_tree(self.path())
            .map_err(|error| error.to_string())?;
        match self {
            Self::Local(path) => {
                copy_module_tree(path, destination).map_err(|error| error.to_string())
            }
            Self::Git { checkout, .. } => std::fs::rename(checkout, destination)
                .map_err(|error| format!("failed to move staged Git checkout into place: {error}")),
        }
    }
}

impl Drop for InstallSource {
    fn drop(&mut self) {
        // A successful Git install renames this path, so this is a no-op on
        // success. Every early return instead removes only its private stage.
        if let Self::Git { checkout, .. } = self {
            let _ = std::fs::remove_dir_all(checkout);
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GitProvenance {
    source: String,
    requested_ref: Option<String>,
    revision: String,
}

fn install_source(source: &str, modules_dir: &std::path::Path) -> Result<InstallSource, String> {
    let local = std::path::PathBuf::from(source);
    if std::fs::symlink_metadata(&local)
        .map(|metadata| metadata.is_dir())
        .unwrap_or(false)
    {
        mesh_core_module::package::validate_module_tree(&local)
            .map_err(|error| error.to_string())?;
        return Ok(InstallSource::Local(local));
    }

    let (url, requested_ref) = parse_git_source(source)?;
    std::fs::create_dir_all(modules_dir).map_err(|error| {
        format!(
            "failed to create module directory {}: {error}",
            modules_dir.display()
        )
    })?;
    let checkout_name = format!(
        ".mesh-install-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    );
    let checkout = mesh_core_module::package::contained_path(
        modules_dir,
        &checkout_name,
        "staged module path",
    )
    .map_err(|error| error.to_string())?;
    let cleanup = |message: String| {
        let _ = std::fs::remove_dir_all(&checkout);
        Err(message)
    };
    let clone = Command::new("git")
        .args(["clone", "--quiet", &url])
        .arg(&checkout)
        .output()
        .map_err(|error| format!("failed to run git clone: {error}"))?;
    if !clone.status.success() {
        return cleanup(format!("git clone failed: {}", command_error(&clone)));
    }
    if let Some(reference) = &requested_ref {
        let checkout_ref = Command::new("git")
            .args(["-C"])
            .arg(&checkout)
            .args(["checkout", "--quiet", reference])
            .output();
        let checkout_ref = match checkout_ref {
            Ok(output) => output,
            Err(error) => return cleanup(format!("failed to run git checkout: {error}")),
        };
        if !checkout_ref.status.success() {
            return cleanup(format!(
                "git checkout of {reference:?} failed: {}",
                command_error(&checkout_ref)
            ));
        }
    }
    let revision = Command::new("git")
        .args(["-C"])
        .arg(&checkout)
        .args(["rev-parse", "HEAD"])
        .output();
    let revision = match revision {
        Ok(output) => output,
        Err(error) => return cleanup(format!("failed to read cloned revision: {error}")),
    };
    if !revision.status.success() {
        return cleanup(format!(
            "git rev-parse failed: {}",
            command_error(&revision)
        ));
    }
    Ok(InstallSource::Git {
        checkout,
        provenance: GitProvenance {
            source: url,
            requested_ref,
            revision: String::from_utf8_lossy(&revision.stdout).trim().to_string(),
        },
    })
}

fn parse_git_source(source: &str) -> Result<(String, Option<String>), String> {
    let (url, requested_ref) = match source.rsplit_once('#') {
        Some((url, reference)) if !reference.is_empty() => (url, Some(reference.to_string())),
        Some(_) => {
            return Err("Git source has an empty ref after '#'; omit '#' or provide a ref".into());
        }
        None => (source, None),
    };
    if url.trim().is_empty() {
        return Err("Git source URL cannot be empty".into());
    }
    Ok((url.to_string(), requested_ref))
}

fn command_error(output: &std::process::Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if stderr.is_empty() {
        format!("exit status {}", output.status)
    } else {
        stderr
    }
}

/// Record an installed module in `mesh.lock`.
///
/// The lock is the rollback record, so a write failure fails the install rather
/// than being reported as a warning after the fact.
fn record_lock_entry(
    config_dir: &std::path::Path,
    module_id: &str,
    version: &str,
    installed_at: &std::path::Path,
    modules_dir: &std::path::Path,
    source: &InstallSource,
    installed_manifests: &[mesh_core_module::package::ModuleManifest],
    composition: Option<mesh_core_module::package::LockedComposition>,
) -> Result<(), String> {
    use mesh_core_module::package::{
        LockedModule, MeshLock, ModuleSource, TrustTier, module_tree_digest,
    };

    let path = config_dir.join("mesh.lock");
    let history = lock_history_dir(config_dir);
    let mut lock = MeshLock::load_or_default(&path).map_err(|error| error.to_string())?;
    let digest = module_tree_digest(installed_at).map_err(|error| error.to_string())?;
    let (module_source, revision) = match source {
        InstallSource::Local(path) => (
            ModuleSource::Path {
                path: path.display().to_string(),
            },
            None,
        ),
        InstallSource::Git { provenance, .. } => (
            ModuleSource::Git {
                url: provenance.source.clone(),
                reference: provenance.requested_ref.clone(),
            },
            Some(provenance.revision.clone()),
        ),
    };
    let signature = mesh_core_module::package::load_module_signature(installed_at)
        .map_err(|error| error.to_string())?;
    let trust = if signature.is_some() {
        TrustTier::Verified
    } else {
        TrustTier::for_source(
            module_id,
            matches!(&module_source, ModuleSource::Git { .. }),
        )
    };
    lock.modules.insert(
        module_id.to_string(),
        LockedModule {
            version: version.to_string(),
            source: module_source,
            revision,
            digest,
            trust,
            signature,
            dependencies: Default::default(),
            requested_by: Default::default(),
        },
    );
    if composition.is_some() {
        lock.composition = composition;
    }
    lock.refresh_metadata(installed_manifests.iter());
    MeshLock::archive(&path, &history).map_err(|error| error.to_string())?;
    lock.save_with_store(
        &path,
        modules_dir,
        &mesh_core_module::package::module_store_dir(config_dir),
    )
    .map_err(|error| error.to_string())
}

fn lock_history_dir(config_dir: &std::path::Path) -> std::path::PathBuf {
    config_dir.join("lock-history")
}

/// Resolve `(root graph path, config dir, modules dir)` for lock-aware commands.
fn lock_paths() -> (std::path::PathBuf, std::path::PathBuf, std::path::PathBuf) {
    let root_path = root_module_graph_path();
    let root = mesh_core_module::package::RootModuleGraphManifest::from_path(&root_path)
        .unwrap_or_else(|error| exit_error(error));
    let config_dir = root_path
        .parent()
        .expect("root graph path has a parent directory")
        .to_path_buf();
    let modules_dir = config_dir.join(&root.modules_dir);
    (root_path, config_dir, modules_dir)
}

fn installed_manifests(
    root_path: &std::path::Path,
) -> std::collections::BTreeMap<String, mesh_core_module::package::ModuleManifest> {
    load_authoring_snapshot_at(root_path)
        .map(|graph| {
            graph
                .modules()
                .into_iter()
                .map(|module| (module.id.clone(), module.manifest.clone()))
                .collect()
        })
        .unwrap_or_default()
}

fn cmd_update(args: &[String]) {
    use update::EditPolicy;

    let root_path = root_module_graph_path();
    let config_dir = root_path
        .parent()
        .expect("root graph path has a parent directory")
        .to_path_buf();
    let mut transaction = mesh_core_module::package::PackageTransaction::begin(
        &config_dir,
        mesh_core_module::package::PackageOwner::Cli,
        mesh_core_module::package::PackageOperation::Update,
    )
    .unwrap_or_else(|error| exit_error(error));
    let root = mesh_core_module::package::RootModuleGraphManifest::from_path(&root_path)
        .unwrap_or_else(|error| exit_error(error));
    let modules_dir = config_dir.join(&root.modules_dir);
    transaction
        .protect_package_state(&root_path, &modules_dir)
        .unwrap_or_else(|error| exit_error(error));
    let lock_path = config_dir.join("mesh.lock");
    let mut lock = mesh_core_module::package::MeshLock::load_or_default(&lock_path)
        .unwrap_or_else(|error| exit_error(error));

    let policy = if args.iter().any(|arg| arg == "--replace") {
        EditPolicy::Replace
    } else if args.iter().any(|arg| arg == "--keep") {
        EditPolicy::Keep
    } else {
        EditPolicy::Refuse
    };
    let dry_run = args.iter().any(|arg| arg == "--dry-run");
    let only = args
        .iter()
        .find(|arg| arg.starts_with('@'))
        .map(String::as_str);

    let installed = installed_manifests(&root_path);
    let approvals = root.capability_approvals;
    let plan = update::plan_update_from_staged_graph(
        &root_path,
        &modules_dir,
        &lock,
        only,
        policy,
        &installed,
        &approvals,
        &mut transaction,
    )
    .unwrap_or_else(|error| exit_error(error));

    let changed = plan.changed().collect::<Vec<_>>();
    if changed.is_empty() && !plan.is_refused() {
        println!("everything is already at its locked revision");
        transaction.abort().unwrap_or_else(|error| {
            exit_error(format!("failed to close update transaction: {error}"))
        });
        return;
    }
    for candidate in &changed {
        println!(
            "{}: {} → {} ({})",
            candidate.module_id,
            candidate.locked.version,
            candidate.candidate_version,
            candidate.candidate_revision.as_deref().unwrap_or("?")
        );
    }

    // Everything that can refuse runs before anything is written.
    for module_id in &plan.edited {
        eprintln!(
            "{module_id} has local edits since it was installed; \
             repeat with --merge to rebase them, --keep to pin it, or --replace to discard them"
        );
    }
    for breaking in &plan.breaking {
        eprintln!("breaking: {breaking}");
    }
    for breaking in &plan.provider_breaking {
        eprintln!("provider-breaking: {breaking}");
    }
    for (module_id, capability, level) in &plan.capability_additions {
        eprintln!("{module_id} now requests {level:?} capability {capability}");
    }
    for graph_breaking in &plan.graph_breaking {
        eprintln!("breaking: {graph_breaking}");
    }
    print_graph_diff(plan.graph_diff.as_ref());
    if plan.is_refused() {
        let _ = transaction.abort();
        exit_error("update refused; nothing was changed");
    }
    if dry_run {
        println!("dry run: no source, lock, or profile was changed");
        transaction.abort().unwrap_or_else(|error| {
            exit_error(format!("failed to close update transaction: {error}"))
        });
        return;
    }

    let updated = match update::commit_update(
        &modules_dir,
        &config_dir,
        &mut lock,
        &plan,
        &mut transaction,
    ) {
        Ok(updated) => updated,
        Err(error) => {
            let _ = transaction.abort();
            exit_error(error);
        }
    };
    transaction
        .commit()
        .unwrap_or_else(|error| exit_error(format!("failed to commit update: {error}")));
    for entry in updated {
        println!("updated {entry}");
    }
    println!("lock generation {}", lock.generation);
}

fn print_graph_diff(diff: Option<&mesh_core_module::package::ModuleGraphDiff>) {
    let Some(diff) = diff else {
        return;
    };
    print_graph_diff_list("added", &diff.added_modules);
    print_graph_diff_list("removed", &diff.removed_modules);
    print_graph_diff_list("updated", &diff.updated_modules);
    print_graph_diff_list("enabled", &diff.enabled_modules);
    print_graph_diff_list("disabled", &diff.disabled_modules);
    for provider in &diff.provider_changes {
        println!(
            "provider {}: {} → {}",
            provider.interface,
            provider.before.as_deref().unwrap_or("none"),
            provider.after.as_deref().unwrap_or("none")
        );
    }
    for effect in &diff.profile_effects {
        println!("profile effect: {effect}");
    }
}

fn print_graph_diff_list(label: &str, modules: &[String]) {
    for module_id in modules {
        println!("graph {label}: {module_id}");
    }
}

fn cmd_rollback(args: &[String]) {
    let root_path = root_module_graph_path();
    let config_dir = root_path
        .parent()
        .expect("root graph path has a parent directory")
        .to_path_buf();
    let mut transaction = mesh_core_module::package::PackageTransaction::begin(
        &config_dir,
        mesh_core_module::package::PackageOwner::Cli,
        mesh_core_module::package::PackageOperation::Rollback,
    )
    .unwrap_or_else(|error| exit_error(error));
    let root = mesh_core_module::package::RootModuleGraphManifest::from_path(&root_path)
        .unwrap_or_else(|error| exit_error(error));
    let modules_dir = config_dir.join(&root.modules_dir);
    transaction
        .protect_package_state(&root_path, &modules_dir)
        .unwrap_or_else(|error| exit_error(error));
    let generation = args.first().and_then(|arg| arg.parse::<u64>().ok());
    let restored = match update::rollback(&modules_dir, &config_dir, generation, &mut transaction) {
        Ok(restored) => restored,
        Err(error) => {
            let _ = transaction.abort();
            exit_error(error);
        }
    };
    transaction
        .commit()
        .unwrap_or_else(|error| exit_error(format!("failed to commit rollback: {error}")));
    for entry in restored {
        println!("{entry}");
    }
}

fn cmd_uninstall(args: &[String]) {
    let module_id = required_arg(args, 0, "mesh-shell uninstall <module-id>");
    mesh_core_module::package::ModuleId::parse(module_id).unwrap_or_else(|error| exit_error(error));
    let root_path = root_module_graph_path();
    let config_dir = root_path
        .parent()
        .expect("root graph path has a parent directory")
        .to_path_buf();
    let mut transaction = mesh_core_module::package::PackageTransaction::begin(
        &config_dir,
        mesh_core_module::package::PackageOwner::Cli,
        mesh_core_module::package::PackageOperation::Uninstall,
    )
    .unwrap_or_else(|error| exit_error(error));
    let root = mesh_core_module::package::RootModuleGraphManifest::from_path(&root_path)
        .unwrap_or_else(|error| exit_error(error));
    let modules_dir = config_dir.join(&root.modules_dir);
    transaction
        .protect_package_state(&root_path, &modules_dir)
        .unwrap_or_else(|error| exit_error(error));
    let lock_path = config_dir.join("mesh.lock");
    let mut lock = mesh_core_module::package::MeshLock::load_or_default(&lock_path)
        .unwrap_or_else(|error| exit_error(error));

    let dependents = update::dependents(module_id, &lock);
    if !dependents.is_empty() && !args.iter().any(|arg| arg == "--force") {
        exit_error(format!(
            "{module_id} is still required by {}; remove those first or repeat with --force",
            dependents.join(", ")
        ));
    }

    let installed_at = match root.modules.get(module_id) {
        Some(entry) => mesh_core_module::package::contained_path(
            &modules_dir,
            &entry.path,
            "installed module path",
        )
        .unwrap_or_else(|error| exit_error(error)),
        None => mesh_core_module::package::module_install_path(&modules_dir, module_id)
            .unwrap_or_else(|error| exit_error(error)),
    };
    if let Err(error) = transaction.remove(&installed_at) {
        let _ = transaction.abort();
        exit_error(format!(
            "failed to remove {}: {error}",
            installed_at.display()
        ));
    }
    lock.modules.remove(module_id);
    if lock
        .composition
        .as_ref()
        .is_some_and(|composition| composition.module == module_id)
    {
        lock.composition = None;
    }
    let remaining_manifests = installed_manifests(&root_path);
    lock.refresh_metadata(remaining_manifests.values());
    let history = config_dir.join("lock-history");
    if let Err(error) = mesh_core_module::package::MeshLock::archive(&lock_path, &history) {
        let _ = transaction.abort();
        exit_error(error);
    }
    if let Err(error) = lock.save_with_store(
        &lock_path,
        &modules_dir,
        &mesh_core_module::package::module_store_dir(&config_dir),
    ) {
        let _ = transaction.abort();
        exit_error(error);
    }
    transaction
        .commit()
        .unwrap_or_else(|error| exit_error(format!("failed to commit uninstall: {error}")));
    println!("uninstalled {module_id}");
}

fn cmd_lock(args: &[String]) {
    match args.first().map(String::as_str) {
        Some("verify") | None => {
            let (root_path, config_dir, modules_dir) = lock_paths();
            let root = mesh_core_module::package::RootModuleGraphManifest::from_path(&root_path)
                .unwrap_or_else(|error| exit_error(error));
            let lock =
                mesh_core_module::package::MeshLock::load_or_default(&config_dir.join("mesh.lock"))
                    .unwrap_or_else(|error| exit_error(error));
            let results = update::verify(&modules_dir, &lock);
            let provenance = update::verify_provenance(&root, &lock);
            let provenance_failures = provenance
                .iter()
                .filter_map(|(module_id, error)| error.as_ref().map(|error| (module_id, error)))
                .collect::<Vec<_>>();
            for (module_id, error) in provenance_failures {
                println!("untrusted {module_id}: {error}");
            }
            if results.is_empty() && provenance.iter().all(|(_, error)| error.is_none()) {
                println!("no locked modules");
                return;
            }
            let mut edited = 0;
            for (module_id, is_edited) in results {
                if is_edited {
                    edited += 1;
                    println!("edited   {module_id}");
                } else {
                    println!("verified {module_id}");
                }
            }
            println!("{edited} module(s) differ from their locked digest");
            if provenance.iter().any(|(_, error)| error.is_some()) {
                std::process::exit(1);
            }
        }
        Some(other) => exit_error(format!(
            "unknown lock subcommand: {other}\nsubcommands: verify"
        )),
    }
}

fn cmd_config(args: &[String]) {
    match args.first().map(String::as_str) {
        Some("path") => cmd_config_path(),
        Some("show") => cmd_config_show(args.get(1).map(String::as_str)),
        Some("eject") => match args.get(1) {
            Some(module_id) => cmd_config_eject(module_id),
            None => {
                eprintln!("usage: mesh-shell config eject <module-id>");
                std::process::exit(1);
            }
        },
        Some("doctor") => cmd_config_doctor(),
        Some("reset") => match args.get(1) {
            Some(namespace) => cmd_config_reset(namespace),
            None => {
                eprintln!("usage: mesh-shell config reset <namespace>");
                std::process::exit(1);
            }
        },
        Some(other) => {
            eprintln!("unknown config subcommand: {other}");
            eprintln!("subcommands: path, show, doctor, eject, reset");
            std::process::exit(1);
        }
        None => {
            eprintln!("usage: mesh-shell config <path|show|doctor|eject|reset>");
            std::process::exit(1);
        }
    }
}

fn load_settings_store() -> mesh_core_config::SettingsStore {
    match mesh_core_config::SettingsStore::load() {
        Ok(store) => store,
        Err(err) => {
            eprintln!("failed to read settings: {err}");
            std::process::exit(1);
        }
    }
}

fn cmd_config_path() {
    println!("{}", mesh_core_config::default_settings_path().display());
}

fn cmd_config_show(namespace: Option<&str>) {
    let store = load_settings_store();
    let value = match namespace {
        Some(namespace) => store.namespace(namespace),
        None => store.to_value(),
    };
    println!("{}", serde_json::to_string_pretty(&value).unwrap());
}

/// Report every stored value MESH cannot use, without starting a shell.
///
/// Same validation the shell runs at startup, in one place a user can run
/// before restarting: the `shell` namespace and the file's own top level from
/// the store, each module's `surface` block against that module's manifest.
/// Exits non-zero only for errors — a warning is something to know about, not a
/// reason to fail a script.
fn cmd_config_doctor() {
    let store = load_settings_store();
    let mut diagnostics: Vec<mesh_core_config::SettingsDiagnostic> = store.diagnostics().to_vec();

    // Module namespaces need manifests to validate against, so the module graph
    // is resolved here — no surfaces are created and no Wayland connection is
    // made.
    let authoring_graph = load_authoring_snapshot();
    let mut shell = Shell::new();
    shell.discover_modules();
    if let Err(err) = shell.resolve_modules() {
        eprintln!("failed to resolve modules: {err}");
        std::process::exit(1);
    }
    let resource_diagnostics = shell.resource_explanation_snapshot().diagnostics;

    for namespace in store.namespace_names() {
        let module_id = namespace.split('#').next().unwrap_or(namespace);
        let Some(module) = authoring_graph.module(module_id) else {
            if !module_id.starts_with('@') {
                // An interface namespace (`mesh.audio`) is owned by a contract,
                // not a module directory; nothing to check it against here.
                continue;
            }
            // Kept, not deleted: reinstalling the module restores the user's
            // configuration (`docs/spec/08-settings.md` §7).
            diagnostics.push(mesh_core_config::SettingsDiagnostic::warning(
                namespace,
                "",
                "no installed module owns this namespace",
                "reinstall the module, or run 'mesh-shell config reset' to drop these overrides",
            ));
            continue;
        };

        let manifest = module.manifest.clone().into_runtime_manifest();
        let module_path = module
            .manifest_path
            .parent()
            .unwrap_or(std::path::Path::new("."));
        let compiled = if mesh_core_frontend::is_frontend_module(&manifest) {
            match mesh_core_frontend::compile_frontend_module(&manifest, module_path) {
                Ok(compiled) => Some(compiled),
                Err(error) => {
                    diagnostics.push(mesh_core_config::SettingsDiagnostic::warning(
                        namespace,
                        "props",
                        format!("could not load the component prop declarations: {error}"),
                        "fix the component source before validating its stored props",
                    ));
                    None
                }
            }
        } else {
            None
        };
        diagnostics.extend(
            mesh_core_surface_config::resolve_frontend_module_settings_with_props(
                namespace,
                store.namespace(namespace),
                &manifest,
                compiled
                    .as_ref()
                    .and_then(|compiled| compiled.component.props.as_ref()),
            )
            .diagnostics,
        );
    }

    let authoring_diagnostics = authoring_graph.authoring_diagnostics();

    println!("settings: {}", store.path().display());
    if diagnostics.is_empty() && authoring_diagnostics.is_empty() && resource_diagnostics.is_empty()
    {
        println!("no problems found");
        return;
    }

    let errors = diagnostics.iter().filter(|d| d.is_error()).count();
    let warnings = diagnostics.len() - errors;
    // Errors first: the values that are not doing what the user asked.
    diagnostics.sort_by(|left, right| {
        right
            .severity
            .cmp(&left.severity)
            .then_with(|| left.location().cmp(&right.location()))
    });

    for diagnostic in &diagnostics {
        println!();
        println!(
            "{:<7} {}: {}",
            diagnostic.severity.label(),
            diagnostic.location(),
            diagnostic.message
        );
        println!("        → {}", diagnostic.suggested_action);
    }

    for diagnostic in &authoring_diagnostics {
        println!();
        println!("warning {}: {}", diagnostic.module_id, diagnostic.message);
        println!("        → fix the module source or manifest declaration");
    }

    for diagnostic in &resource_diagnostics {
        println!();
        let owner = diagnostic
            .module_id
            .as_deref()
            .or(diagnostic.pack_id.as_deref())
            .unwrap_or("resources");
        println!("{} {}: {}", diagnostic.severity, owner, diagnostic.message);
        println!("        → inspect 'mesh-shell resources show'");
    }

    println!();
    println!(
        "{}, {}",
        count("error", errors),
        count(
            "warning",
            warnings
                + authoring_diagnostics.len()
                + resource_diagnostics
                    .iter()
                    .filter(|diagnostic| diagnostic.severity != "error")
                    .count(),
        )
    );
    if errors > 0
        || resource_diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == "error")
    {
        println!("invalid values are ignored; the declared defaults apply until they are fixed");
        std::process::exit(1);
    }
}

fn count(noun: &str, n: usize) -> String {
    if n == 1 {
        format!("1 {noun}")
    } else {
        format!("{n} {noun}s")
    }
}

/// Materialize a module's effective surface placement and exposed prop values
/// into the settings file.
///
/// The store is sparse by design, so a module the user never touched has no
/// entry to hand-edit. Ejecting writes the block the module is *currently*
/// running with, which is then an ordinary override like any other. Values it
/// writes are pinned: later changes to the module's own defaults no longer
/// reach them.
fn cmd_config_eject(module_id: &str) {
    let graph = load_authoring_snapshot();
    let Some(module) = graph.module(module_id) else {
        eprintln!("no such module: {module_id}");
        eprintln!("run 'mesh-shell list' to see discovered modules");
        std::process::exit(1);
    };

    let mut store = load_settings_store();
    let manifest = module.manifest.clone().into_runtime_manifest();
    let module_path = module
        .manifest_path
        .parent()
        .unwrap_or(std::path::Path::new("."));
    let compiled = if mesh_core_frontend::is_frontend_module(&manifest) {
        match mesh_core_frontend::compile_frontend_module(&manifest, module_path) {
            Ok(compiled) => Some(compiled),
            Err(error) => {
                eprintln!("failed to load {module_id} component props: {error}");
                std::process::exit(1);
            }
        }
    } else {
        None
    };
    let props_block = compiled
        .as_ref()
        .and_then(|compiled| compiled.component.props.as_ref());
    let resolved = mesh_core_surface_config::resolve_frontend_module_settings_with_props(
        module_id,
        store.namespace(module_id),
        &manifest,
        props_block,
    );
    let surface =
        mesh_core_surface_config::effective_surface_layout_to_json(module_id, &resolved.layout);
    let props =
        mesh_core_surface_config::effective_global_props_to_json(props_block, &resolved.props);

    let mut ejected = serde_json::json!({ "surface": surface });
    if props.as_object().is_some_and(|props| !props.is_empty()) {
        ejected["props"] = serde_json::json!({ "global": props });
    }
    store.merge_namespace(module_id, &ejected);
    if let Err(err) = store.save() {
        eprintln!("failed to write settings: {err}");
        std::process::exit(1);
    }

    println!(
        "ejected {module_id} surface placement and exposed props into {}",
        store.path().display()
    );
    println!(
        "{}",
        serde_json::to_string_pretty(&store.namespace(module_id)).unwrap()
    );
}

fn cmd_config_reset(namespace: &str) {
    let mut store = load_settings_store();
    if !store.has_namespace(namespace) {
        println!("no stored overrides for {namespace}");
        return;
    }
    store.reset_namespace(namespace);
    if let Err(err) = store.save() {
        eprintln!("failed to write settings: {err}");
        std::process::exit(1);
    }
    println!("cleared overrides for {namespace}; declared defaults apply again");
}

fn cmd_help() {
    println!("mesh-shell - MESH shell framework");
    println!();
    println!("USAGE:");
    println!("  mesh-shell [COMMAND]");
    println!();
    println!("COMMANDS:");
    println!("  start     Start the shell (default)");
    println!("  list      List discovered modules");
    println!("  services  List available service backends");
    println!("  debug     Toggle the debug overlay on the running shell");
    println!("            subcommands: profiling, tab");
    println!("  ipc       Send an IPC command to the running shell");
    println!("  ipc-socket-path  Print the shell IPC socket path");
    println!("  config    Inspect and edit the settings file");
    println!("            path                 print the settings file path");
    println!("            show [namespace]     print the whole file, or one namespace");
    println!("            doctor               check the file for values MESH cannot use");
    println!("            eject <module-id>    write a module's effective surface and");
    println!("                                 exposed props, ready to hand-edit");
    println!("            reset <namespace>    drop a namespace's overrides");
    println!("  profile   Manage shell compositions");
    println!("            list                 list profiles (* is active)");
    println!("            create <profile>     create an empty profile");
    println!("            use <profile>        select the active profile");
    println!("            show [profile]       print a profile");
    println!("            add <profile> <module>  add/enable a frontend instance");
    println!("            enable|disable <profile> <module#instance>");
    println!("            remove <profile> <module#instance>");
    println!("            set <profile> <namespace> <json>  set scoped preferences");
    println!("            unset <profile> <namespace>      clear scoped preferences");
    println!("            prune <profile>      drop overrides the composition no longer has");
    println!("  install   Install a module or composition from a path or git URL");
    println!("  update    Update locked modules to their source's current revision");
    println!("            [<module-id>] [--dry-run] [--keep|--replace]");
    println!("  rollback  Restore the previous lock generation [<generation>]");
    println!("  uninstall Remove a module [--force]");
    println!("  lock      verify   recompute digests and report local edits");
    println!(
        "  install <path-or-git-url>[#ref]  Install a module; frontends are added to the active profile"
    );
    println!("            flags: --available-only, --profile <id>, --allow-elevated, --allow-high");
    println!("  status    Show shell status");
    println!("  resources Inspect the effective host/module resource snapshot");
    println!("            show                 print the complete snapshot (default)");
    println!("            icons|fonts          print one resource chain");
    println!("            coverage             preview semantic and font-script gaps");
    println!("              --font-script <module>:<role>:<script>");
    println!("              --optional-font-script <module>:<role>:<script>");
    println!("            doctor               print structured resource diagnostics");
    println!("  locale    Inspect graph-backed catalogs");
    println!("            list                 list available locales");
    println!("            active               show locale and fallback chain");
    println!("            set <code>           persist a locale selection");
    println!("            set-system           follow the host locale");
    println!("            which <module> <key> show the winning catalog source");
    println!("            missing <module>    list statically used missing keys");
    println!("            extract <module>    print a translator catalog template");
    println!("            doctor              check catalogs and static keys");
    println!("  version   Print version");
    println!("  help      Show this help");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn git_source_splits_an_optional_ref() {
        assert_eq!(
            parse_git_source("https://example.invalid/widgets.git#v1.2.3").unwrap(),
            (
                "https://example.invalid/widgets.git".to_string(),
                Some("v1.2.3".to_string())
            )
        );
        assert_eq!(
            parse_git_source("git@host:group/widgets.git").unwrap(),
            ("git@host:group/widgets.git".to_string(), None)
        );
        assert!(parse_git_source("https://example.invalid/widgets.git#").is_err());
    }

    #[test]
    fn profile_switch_ack_requires_matching_committed_profile() {
        let ack = parse_profile_switch_ack(
            r#"{"ok":true,"status":"committed","profile_id":"work","generation":7}"#,
            "work",
        )
        .unwrap();
        assert!(matches!(
            ack,
            ProfileSwitchAck::Committed { generation: 7, .. }
        ));
    }

    #[test]
    fn profile_switch_rejection_is_not_a_restart_fallback() {
        let ack = parse_profile_switch_ack(
            r#"{"ok":false,"status":"rejected","profile_id":"work","generation":3,"reason":"invalid profile"}"#,
            "work",
        )
        .unwrap();
        assert!(matches!(
            ack,
            ProfileSwitchAck::Rejected { reason, .. } if reason == "invalid profile"
        ));
    }

    #[test]
    fn profile_switch_ack_rejects_a_different_profile_generation() {
        let error = parse_profile_switch_ack(
            r#"{"status":"committed","profile_id":"other","generation":9}"#,
            "work",
        )
        .unwrap_err();
        assert!(error.contains("acknowledged profile 'other'"));
    }

    #[test]
    fn ipc_transport_only_classifies_missing_listener_as_absent_shell() {
        assert!(
            IpcCommandError::Connect {
                socket_path: "/tmp/mesh.sock".into(),
                source: io::Error::from(io::ErrorKind::NotFound),
            }
            .is_absent_shell()
        );
        assert!(!IpcCommandError::EmptyResponse.is_absent_shell());
        assert!(
            !IpcCommandError::Receive(io::Error::from(io::ErrorKind::BrokenPipe)).is_absent_shell()
        );
    }

    #[test]
    fn installing_records_version_source_and_digest_in_the_lock() {
        use mesh_core_module::package::{MeshLock, ModuleSource};

        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let config_dir = std::env::temp_dir().join(format!("mesh-cli-lock-{nonce}"));
        let installed = config_dir.join("modules/example/widget");
        std::fs::create_dir_all(&installed).unwrap();
        std::fs::write(
            installed.join("module.json"),
            r#"{"name":"@example/widget","version":"1.2.3","mesh":{"apiVersion":"0.1","kind":"component","entry":"main.mesh"}}"#,
        )
        .unwrap();
        std::fs::write(installed.join("main.mesh"), "<template><box/></template>").unwrap();

        let source = InstallSource::Git {
            checkout: installed.clone(),
            provenance: GitProvenance {
                source: "https://example.invalid/widgets.git".to_string(),
                requested_ref: Some("main".to_string()),
                revision: "abc123".to_string(),
            },
        };
        record_lock_entry(
            &config_dir,
            "@example/widget",
            "1.2.3",
            &installed,
            &config_dir.join("modules"),
            &source,
            &[],
            None,
        )
        .unwrap();

        let lock = MeshLock::from_path(&config_dir.join("mesh.lock")).unwrap();
        let entry = &lock.modules["@example/widget"];
        assert_eq!(entry.version, "1.2.3");
        assert_eq!(entry.revision.as_deref(), Some("abc123"));
        assert!(entry.digest.starts_with("sha256:"));
        assert_eq!(
            entry.source,
            ModuleSource::Git {
                url: "https://example.invalid/widgets.git".into(),
                reference: Some("main".into()),
            }
        );
        assert_eq!(lock.generation, 1);

        std::fs::remove_dir_all(&config_dir).ok();
    }
}
