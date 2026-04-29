use mesh_plugin::manifest::{Manifest, PluginType, load_manifest};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// A discovered and indexed view of all plugins available in the workspace.
pub struct PluginRegistry {
    /// Maps plugin-id → Manifest for all discovered plugins.
    pub manifests: HashMap<String, Manifest>,
    /// Maps interface name (e.g. "mesh.audio") → list of field names it emits.
    pub interface_fields: HashMap<String, Vec<String>>,
    /// Maps component tag name → plugin-id for plugins that export a component tag.
    pub exported_tags: HashMap<String, String>,
}

impl PluginRegistry {
    pub fn empty() -> Self {
        Self {
            manifests: HashMap::new(),
            interface_fields: HashMap::new(),
            exported_tags: HashMap::new(),
        }
    }

    /// Discover plugins from the workspace root and standard system paths.
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
            // Try direct plugin dir (e.g. plugins/backend/core/pipewire-audio)
            self.try_load_plugin(&path);
            // Recurse one level (e.g. plugins/frontend/core/<name>)
            let Ok(sub) = std::fs::read_dir(&path) else {
                continue;
            };
            for sub_entry in sub.flatten() {
                let sub_path = sub_entry.path();
                if sub_path.is_dir() {
                    self.try_load_plugin(&sub_path);
                    // One more level (e.g. plugins/frontend/core/panel/src — skip)
                    let Ok(sub2) = std::fs::read_dir(&sub_path) else {
                        continue;
                    };
                    for sub2_entry in sub2.flatten() {
                        let sub2_path = sub2_entry.path();
                        if sub2_path.is_dir() {
                            self.try_load_plugin(&sub2_path);
                        }
                    }
                }
            }
        }
    }

    fn try_load_plugin(&mut self, dir: &Path) {
        let Ok(loaded) = load_manifest(dir) else {
            return;
        };
        let manifest = loaded.manifest;
        let plugin_id = manifest.package.id.clone();

        // Record exported component tag
        if let Some(tag) = manifest.exported_component_tag() {
            self.exported_tags
                .insert(tag.to_string(), plugin_id.clone());
        }

        // For interface plugins, record the interface name
        if manifest.package.plugin_type == PluginType::Interface {
            if let Some(iface) = &manifest.interface {
                self.interface_fields.entry(iface.name.clone()).or_default();
            }
        }

        // For backend plugins, record what interfaces they provide
        for provided in &manifest.provides {
            self.interface_fields
                .entry(provided.interface.clone())
                .or_default();
        }
        if let Some(svc) = manifest.primary_service() {
            self.interface_fields
                .entry(svc.provides.clone())
                .or_default();
        }

        self.manifests.insert(plugin_id, manifest);
    }

    /// All discovered interface/service names (e.g. "mesh.audio").
    pub fn service_names(&self) -> Vec<&str> {
        self.interface_fields.keys().map(String::as_str).collect()
    }

    /// Component tags exported by plugins: tag name → plugin-id.
    pub fn exported_component_tags(&self) -> &HashMap<String, String> {
        &self.exported_tags
    }
}

fn search_paths(workspace_root: &Path) -> Vec<PathBuf> {
    let mut paths = vec![workspace_root.join("plugins")];

    if let Some(home) = home_dir() {
        paths.push(home.join(".local/share/mesh/plugins"));
        paths.push(home.join(".local/share/mesh/dev-plugins"));
    }

    paths.push(PathBuf::from("/usr/share/mesh/plugins"));

    paths
}

fn home_dir() -> Option<PathBuf> {
    std::env::var("HOME").ok().map(PathBuf::from)
}
