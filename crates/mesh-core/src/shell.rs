/// The top-level Shell struct that owns all subsystems.
use mesh_config::{load_config, ShellConfig};
use mesh_diagnostics::DiagnosticsCollector;
use mesh_events::EventBus;
use mesh_locale::LocaleEngine;
use mesh_plugin::lifecycle::{PluginInstance, PluginState};
use mesh_plugin::manifest;
use mesh_service::ServiceRegistry;
use mesh_theme::{ThemeEngine, default_theme};

use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// The central shell runtime.
#[derive(Debug)]
pub struct Shell {
    pub config: ShellConfig,
    pub theme: ThemeEngine,
    pub locale: LocaleEngine,
    pub events: EventBus,
    pub diagnostics: DiagnosticsCollector,
    pub services: ServiceRegistry,
    plugins: HashMap<String, PluginInstance>,
    plugin_dirs: Vec<PathBuf>,
}

impl Shell {
    /// Create a new shell with default configuration.
    pub fn new() -> Self {
        let config_path = mesh_config::default_config_path();
        let config = load_config(&config_path).unwrap_or_else(|e| {
            tracing::warn!("failed to load config, using defaults: {e}");
            ShellConfig {
                shell: Default::default(),
                plugins: HashMap::new(),
            }
        });

        Self {
            config,
            theme: ThemeEngine::new(default_theme()),
            locale: LocaleEngine::new("en"),
            events: EventBus::new(),
            diagnostics: DiagnosticsCollector::new(),
            services: ServiceRegistry::new(),
            plugins: HashMap::new(),
            plugin_dirs: default_plugin_dirs(),
        }
    }

    /// Discover plugins in all configured plugin directories.
    pub fn discover_plugins(&mut self) {
        for dir in &self.plugin_dirs.clone() {
            if !dir.exists() {
                tracing::debug!("plugin directory does not exist: {}", dir.display());
                continue;
            }
            self.scan_plugin_dir(dir);
        }
        tracing::info!("discovered {} plugins", self.plugins.len());
    }

    fn scan_plugin_dir(&mut self, dir: &Path) {
        let entries = match std::fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(e) => {
                tracing::warn!("failed to read plugin directory {}: {e}", dir.display());
                return;
            }
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }

            match manifest::load_manifest(&path) {
                Ok(manifest) => {
                    let id = manifest.package.id.clone();
                    tracing::info!(
                        "discovered plugin: {} v{} ({})",
                        id,
                        manifest.package.version,
                        manifest.package.plugin_type
                    );
                    let instance = PluginInstance::new(manifest, path);
                    self.plugins.insert(id, instance);
                }
                Err(e) => {
                    tracing::debug!("skipping {}: {e}", path.display());
                }
            }
        }
    }

    /// Resolve the dependency graph and transition discovered plugins to Resolved.
    pub fn resolve_plugins(&mut self) {
        // Collect IDs first to satisfy the borrow checker.
        let ids: Vec<String> = self.plugins.keys().cloned().collect();
        for id in ids {
            if let Some(plugin) = self.plugins.get_mut(&id) {
                if plugin.state == PluginState::Discovered {
                    if let Err(e) = plugin.transition(PluginState::Resolved) {
                        tracing::warn!("failed to resolve plugin {id}: {e}");
                    }
                }
            }
        }
    }

    /// Return a reference to a loaded plugin by ID.
    pub fn plugin(&self, id: &str) -> Option<&PluginInstance> {
        self.plugins.get(id)
    }

    /// Return all plugins and their current states.
    pub fn plugins(&self) -> impl Iterator<Item = (&str, PluginState)> {
        self.plugins
            .iter()
            .map(|(id, inst)| (id.as_str(), inst.state))
    }
}

impl Default for Shell {
    fn default() -> Self {
        Self::new()
    }
}

/// Standard plugin search directories following XDG conventions.
fn default_plugin_dirs() -> Vec<PathBuf> {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    vec![
        PathBuf::from("/usr/share/mesh/plugins"),
        PathBuf::from(&home).join(".local/share/mesh/plugins"),
        PathBuf::from(&home).join(".local/share/mesh/dev-plugins"),
    ]
}
