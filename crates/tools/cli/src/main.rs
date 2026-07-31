use mesh_core_module::ModuleType;
use mesh_core_shell::{Shell, default_ipc_socket_path};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;

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
        Some("status") => cmd_status(),
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
    tracing::info!("starting MESH shell");
    let mut shell = Shell::new();
    if let Err(err) = shell.run() {
        tracing::error!("MESH shell exited with error: {err}");
        std::process::exit(1);
    }
}

fn cmd_list() {
    let mut shell = Shell::new();
    shell.discover_modules();
    if let Err(err) = shell.resolve_modules() {
        eprintln!("failed to resolve modules: {err}");
        std::process::exit(1);
    }

    let mut count = 0;
    for (id, _state) in shell.modules() {
        let module = shell.module(id).unwrap();
        let kind = module.manifest.package.module_type;
        match (&kind, module.manifest.primary_service()) {
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
    let mut shell = Shell::new();
    shell.discover_modules();
    if let Err(err) = shell.resolve_modules() {
        eprintln!("failed to resolve modules: {err}");
        std::process::exit(1);
    }

    // Group backends by service type.
    let mut by_service: std::collections::HashMap<String, Vec<(String, String, u32)>> =
        std::collections::HashMap::new();

    for (id, _) in shell.modules() {
        let module = shell.module(id).unwrap();
        if module.manifest.package.module_type == ModuleType::Backend {
            if let Some(svc) = module.manifest.primary_service() {
                by_service.entry(svc.provides.clone()).or_default().push((
                    id.to_string(),
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
    let _ = reader.read_line(&mut response);
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
            paths
                .set_active(profile_id)
                .unwrap_or_else(|error| exit_error(error));
            println!("active profile: {profile_id} (takes effect on shell restart)");
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
            let graph =
                mesh_core_module::package::load_installed_module_graph(&root_module_graph_path())
                    .unwrap_or_else(|error| exit_error(error));
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
        Some(other) => exit_error(format!(
            "unknown profile subcommand: {other}\nsubcommands: list, create, use, show, add, enable, disable, remove"
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
        "mesh-shell install <path> [--available-only] [--profile <profile>] [--allow-elevated] [--allow-high]",
    );
    let source = std::path::PathBuf::from(source_arg);
    if !source.is_dir() {
        exit_error(format!(
            "module source is not a directory: {}",
            source.display()
        ));
    }
    let manifest_path = source.join("module.json");
    let manifest = mesh_core_module::package::ModuleManifest::from_path(&manifest_path)
        .unwrap_or_else(|error| exit_error(error));

    let allow_elevated = args.iter().any(|arg| arg == "--allow-elevated");
    let allow_high = args.iter().any(|arg| arg == "--allow-high");
    let requested = manifest
        .mesh
        .capabilities
        .required
        .iter()
        .chain(manifest.mesh.uses.capabilities.iter())
        .map(|id| mesh_core_capability::Capability::new(id.clone()))
        .collect::<Vec<_>>();
    for capability in &requested {
        use mesh_core_capability::PrivilegeLevel;
        match capability.privilege_level() {
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

    let root_path = root_module_graph_path();
    let root = mesh_core_module::package::RootModuleGraphManifest::from_path(&root_path)
        .unwrap_or_else(|error| exit_error(error));
    let config_dir = root_path
        .parent()
        .expect("root graph path has a parent directory");
    let modules_dir = config_dir.join(&root.modules_dir);
    let destination = modules_dir.join(manifest.name.trim_start_matches('@'));
    if destination.exists() {
        exit_error(format!(
            "module {} is already installed at {}",
            manifest.name,
            destination.display()
        ));
    }

    copy_module_tree(&source, &destination).unwrap_or_else(|error| {
        let _ = std::fs::remove_dir_all(&destination);
        exit_error(format!("failed to install {}: {error}", manifest.name));
    });

    let install_result = (|| -> Result<Option<String>, String> {
        let graph = mesh_core_module::package::load_installed_module_graph(&root_path)
            .map_err(|error| error.to_string())?;
        let installed = graph
            .module(&manifest.name)
            .ok_or_else(|| "installed module was not discovered".to_string())?;
        if installed.kind != manifest.mesh.kind {
            return Err("installed module kind changed while copying".into());
        }

        if args.iter().any(|arg| arg == "--available-only")
            || manifest.mesh.kind != mesh_core_module::package::ModuleKind::Frontend
        {
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
            let _ = std::fs::remove_dir_all(&destination);
            exit_error(format!(
                "installation validation failed; removed staged module: {error}"
            ));
        }
    }
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
    let mut shell = Shell::new();
    shell.discover_modules();
    if let Err(err) = shell.resolve_modules() {
        eprintln!("failed to resolve modules: {err}");
        std::process::exit(1);
    }

    for namespace in store.namespace_names() {
        let module_id = namespace.split('#').next().unwrap_or(namespace);
        let Some(module) = shell.module(module_id) else {
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

        diagnostics.extend(
            mesh_core_surface_config::resolve_frontend_module_settings(
                namespace,
                store.namespace(namespace),
                &module.manifest,
            )
            .diagnostics,
        );
    }

    println!("settings: {}", store.path().display());
    if diagnostics.is_empty() {
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

    println!();
    println!("{}, {}", count("error", errors), count("warning", warnings));
    if errors > 0 {
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

/// Materialize a module's effective surface placement into the settings file.
///
/// The store is sparse by design, so a module the user never touched has no
/// entry to hand-edit. Ejecting writes the block the module is *currently*
/// running with, which is then an ordinary override like any other. Values it
/// writes are pinned: later changes to the module's own defaults no longer
/// reach them.
fn cmd_config_eject(module_id: &str) {
    let mut shell = Shell::new();
    shell.discover_modules();
    if let Err(err) = shell.resolve_modules() {
        eprintln!("failed to resolve modules: {err}");
        std::process::exit(1);
    }

    let Some(module) = shell.module(module_id) else {
        eprintln!("no such module: {module_id}");
        eprintln!("run 'mesh-shell list' to see discovered modules");
        std::process::exit(1);
    };

    let mut store = load_settings_store();
    let resolved = mesh_core_surface_config::resolve_frontend_module_settings(
        module_id,
        store.namespace(module_id),
        &module.manifest,
    );
    let block = mesh_core_surface_config::surface_layout_to_json(&resolved.layout);

    store.merge_namespace(module_id, &serde_json::json!({ "surface": block }));
    if let Err(err) = store.save() {
        eprintln!("failed to write settings: {err}");
        std::process::exit(1);
    }

    println!(
        "ejected {module_id} surface placement into {}",
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
    println!("            eject <module-id>    write a module's effective surface");
    println!("                                 placement in, ready to hand-edit");
    println!("            reset <namespace>    drop a namespace's overrides");
    println!("  profile   Manage shell compositions");
    println!("            list                 list profiles (* is active)");
    println!("            create <profile>     create an empty profile");
    println!("            use <profile>        select the active profile");
    println!("            show [profile]       print a profile");
    println!("            add <profile> <module>  add/enable a frontend instance");
    println!("            enable|disable <profile> <module#instance>");
    println!("            remove <profile> <module#instance>");
    println!("  install <path>  Install a local module; frontends are added to the active profile");
    println!("            flags: --available-only, --profile <id>, --allow-elevated, --allow-high");
    println!("  status    Show shell status");
    println!("  version   Print version");
    println!("  help      Show this help");
}
