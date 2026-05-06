use mesh_core_config::{default_config_path, load_config, resolve_discovery_paths};
use mesh_core_module::manifest::{Manifest, ModuleType, load_manifest};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// State fields and commands exposed by a backend service module.
#[derive(Debug, Default, Clone)]
pub struct InterfaceShape {
    /// Fields emitted via `mesh.service.emit({...})` in the backend script.
    pub state_fields: Vec<String>,
    /// Commands inferred from `function on_command_<name>()` in the backend script.
    pub commands: Vec<String>,
}

/// A discovered and indexed view of all modules available in the workspace.
pub struct ModuleRegistry {
    /// Maps module-id → Manifest for all discovered modules.
    pub manifests: HashMap<String, Manifest>,
    /// Maps interface name (e.g. "mesh.audio") → list of field names it emits.
    pub interface_fields: HashMap<String, Vec<String>>,
    /// Maps interface name → inferred shape (state fields + commands) from backend script.
    pub interface_shapes: HashMap<String, InterfaceShape>,
    /// Maps component tag name → module-id for modules that export a component tag.
    pub exported_tags: HashMap<String, String>,
}

impl ModuleRegistry {
    pub fn empty() -> Self {
        Self {
            manifests: HashMap::new(),
            interface_fields: HashMap::new(),
            interface_shapes: HashMap::new(),
            exported_tags: HashMap::new(),
        }
    }

    /// Discover modules from the workspace root and standard system paths.
    pub fn discover(workspace_root: &Path) -> Self {
        let mut registry = Self::empty();

        let search_roots = search_paths(workspace_root);
        for root in search_roots {
            registry.scan_dir(&root);
        }

        registry
    }

    fn scan_dir(&mut self, root: &Path) {
        let Ok(entries) = std::fs::read_dir(root) else {
            return;
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            // Try direct module dir (e.g. packages/modules/backend/core/pipewire-audio)
            self.try_load_module(&path);
            // Recurse one level (e.g. packages/modules/frontend/core/<name>)
            let Ok(sub) = std::fs::read_dir(&path) else {
                continue;
            };
            for sub_entry in sub.flatten() {
                let sub_path = sub_entry.path();
                if sub_path.is_dir() {
                    self.try_load_module(&sub_path);
                    // One more level (e.g. packages/modules/frontend/core/panel/src - skip)
                    let Ok(sub2) = std::fs::read_dir(&sub_path) else {
                        continue;
                    };
                    for sub2_entry in sub2.flatten() {
                        let sub2_path = sub2_entry.path();
                        if sub2_path.is_dir() {
                            self.try_load_module(&sub2_path);
                        }
                    }
                }
            }
        }
    }

    fn try_load_module(&mut self, dir: &Path) {
        let Ok(loaded) = load_manifest(dir) else {
            return;
        };
        let manifest = loaded.manifest;
        let module_id = manifest.package.id.clone();

        // Record exported component tag
        if let Some(tag) = manifest.exported_component_tag() {
            self.exported_tags
                .insert(tag.to_string(), module_id.clone());
        }

        // For interface modules, record the interface name
        if manifest.package.module_type == ModuleType::Interface {
            if let Some(iface) = &manifest.interface {
                self.interface_fields.entry(iface.name.clone()).or_default();
            }
        }

        // For backend modules, record what interfaces they provide and analyze
        // the main script to infer state fields + commands.
        let is_backend = manifest.package.module_type == ModuleType::Backend;
        let interface_names: Vec<String> = {
            let mut names: Vec<String> = manifest
                .provides
                .iter()
                .map(|p| p.interface.clone())
                .collect();
            if let Some(svc) = manifest.primary_service() {
                if !names.contains(&svc.provides) {
                    names.push(svc.provides.clone());
                }
            }
            names
        };

        for iface in &interface_names {
            self.interface_fields.entry(iface.clone()).or_default();
        }

        if is_backend && !interface_names.is_empty() {
            if let Some(entry) = &manifest.entrypoints.main {
                let script_path = dir.join(entry);
                if let Ok(source) = std::fs::read_to_string(&script_path) {
                    let shape = analyze_backend_script(&source);
                    for iface in &interface_names {
                        self.interface_shapes
                            .entry(iface.clone())
                            .and_modify(|existing| {
                                for f in &shape.state_fields {
                                    if !existing.state_fields.contains(f) {
                                        existing.state_fields.push(f.clone());
                                    }
                                }
                                for c in &shape.commands {
                                    if !existing.commands.contains(c) {
                                        existing.commands.push(c.clone());
                                    }
                                }
                            })
                            .or_insert_with(|| shape.clone());
                    }
                }
            }
        }

        self.manifests.insert(module_id, manifest);
    }

    /// All discovered interface/service names (e.g. "mesh.audio").
    pub fn service_names(&self) -> Vec<&str> {
        self.interface_fields.keys().map(String::as_str).collect()
    }

    /// Component tags exported by modules: tag name → module-id.
    pub fn exported_component_tags(&self) -> &HashMap<String, String> {
        &self.exported_tags
    }
}

fn search_paths(workspace_root: &Path) -> Vec<PathBuf> {
    let configured_paths = load_config(&default_config_path())
        .map(|config| config.shell.discovery_paths)
        .unwrap_or_default();
    resolve_discovery_paths(workspace_root, &configured_paths)
}

/// Analyze a backend Luau script to infer the service shape:
/// - State fields from table literals (`return { key = ... }` or
///   `mesh.service.emit({ key = ... })`).
/// - Commands from `function on_command_<name>()` definitions.
fn analyze_backend_script(source: &str) -> InterfaceShape {
    let mut state_fields: Vec<String> = Vec::new();
    let mut commands: Vec<String> = Vec::new();

    for line in source.lines() {
        let t = line.trim();
        if t.starts_with("--") {
            continue;
        }

        // Command: `function on_command_<name>(`
        if let Some(rest) = t.strip_prefix("function on_command_") {
            if let Some(name) = rest.split('(').next() {
                let name = name.trim().to_string();
                if is_lua_identifier(&name) && !commands.contains(&name) {
                    commands.push(name);
                }
            }
            continue;
        }

        // State field: indented `key = value` line inside a table literal.
        // Must be indented (leading whitespace) to distinguish from top-level assignments.
        let indented = line.starts_with("    ") || line.starts_with('\t');
        if indented {
            // Split on ` = ` (space-padded) to avoid matching `==`
            if let Some((key, rest)) = t.split_once(" = ") {
                let key = key.trim();
                let rest = rest.trim();
                // Skip `==` and `~=` comparisons that got through
                if !rest.starts_with('=')
                    && is_lua_identifier(key)
                    && !is_lua_keyword(key)
                    && !state_fields.contains(&key.to_string())
                {
                    state_fields.push(key.to_string());
                }
            }
        }
    }

    InterfaceShape {
        state_fields,
        commands,
    }
}

fn is_lua_identifier(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .next()
            .map_or(false, |c| c.is_ascii_alphabetic() || c == '_')
        && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn is_lua_keyword(s: &str) -> bool {
    matches!(
        s,
        "local"
            | "function"
            | "if"
            | "then"
            | "else"
            | "elseif"
            | "end"
            | "for"
            | "while"
            | "do"
            | "return"
            | "and"
            | "or"
            | "not"
            | "true"
            | "false"
            | "nil"
            | "in"
            | "repeat"
            | "until"
            | "break"
    )
}
