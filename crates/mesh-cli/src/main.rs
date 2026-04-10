use mesh_core::Shell;
use mesh_plugin::PluginType;

fn main() {
    tracing_subscriber::fmt().init();

    let args: Vec<String> = std::env::args().collect();
    let command = args.get(1).map(|s| s.as_str());

    match command {
        Some("start") | None => cmd_start(),
        Some("list") => cmd_list(),
        Some("services") => cmd_services(),
        Some("status") => cmd_status(),
        Some("version") => cmd_version(),
        Some("help") | Some("--help") | Some("-h") => cmd_help(),
        Some(other) => {
            eprintln!("unknown command: {other}");
            eprintln!("run 'mesh help' for usage");
            std::process::exit(1);
        }
    }
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
    shell.discover_plugins();

    let mut count = 0;
    for (id, _state) in shell.plugins() {
        let plugin = shell.plugin(id).unwrap();
        let kind = plugin.manifest.package.plugin_type;
        match (&kind, &plugin.manifest.service) {
            (PluginType::Backend, Some(svc)) => {
                println!("{id}  ({kind}, provides: {}, backend: {})", svc.provides, svc.backend_name);
            }
            _ => {
                println!("{id}  ({kind})");
            }
        }
        count += 1;
    }

    if count == 0 {
        println!("no plugins found");
    }
}

fn cmd_services() {
    let mut shell = Shell::new();
    shell.discover_plugins();

    // Group backends by service type.
    let mut by_service: std::collections::HashMap<String, Vec<(String, String, u32)>> =
        std::collections::HashMap::new();

    for (id, _) in shell.plugins() {
        let plugin = shell.plugin(id).unwrap();
        if plugin.manifest.package.plugin_type == PluginType::Backend {
            if let Some(svc) = &plugin.manifest.service {
                by_service
                    .entry(svc.provides.clone())
                    .or_default()
                    .push((id.to_string(), svc.backend_name.clone(), svc.priority));
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

fn cmd_version() {
    println!("mesh {}", env!("CARGO_PKG_VERSION"));
}

fn cmd_help() {
    println!("mesh — MESH shell framework");
    println!();
    println!("USAGE:");
    println!("  mesh [COMMAND]");
    println!();
    println!("COMMANDS:");
    println!("  start     Start the shell (default)");
    println!("  list      List discovered plugins");
    println!("  services  List available service backends");
    println!("  status    Show shell status");
    println!("  version   Print version");
    println!("  help      Show this help");
}
