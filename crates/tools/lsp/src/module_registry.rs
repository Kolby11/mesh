use mesh_core_component::parse_luau_script;
use mesh_core_config::{default_config_path, load_config, resolve_discovery_paths};
use mesh_core_module::manifest::{Manifest, ModuleType, load_canonical_manifest};
use mesh_core_resources::{
    ResourceAssetExplanation, ResourceExplanationSnapshot, ResourceMappingExplanation,
    ResourcePackExplanation,
};
use mesh_core_service::{InterfaceContract, canonical_interface_name, parse_interface_contract};
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
    /// Maps module-id → directory containing its manifest.
    pub module_dirs: HashMap<String, PathBuf>,
    /// Maps module-id → resolved main entrypoint path when present.
    pub module_entrypoints: HashMap<String, PathBuf>,
    /// Maps interface name (e.g. "mesh.audio") → list of field names it emits.
    pub interface_fields: HashMap<String, Vec<String>>,
    /// Maps interface name → inferred shape (state fields + commands) from backend script.
    pub interface_shapes: HashMap<String, InterfaceShape>,
    /// Validated declared contracts. These are authoritative over shapes
    /// inferred from a provider implementation.
    pub interface_contracts: HashMap<String, InterfaceContract>,
    /// Maps component tag name → module-id for modules that export a component tag.
    pub exported_tags: HashMap<String, String>,
    /// Theme ids installed on this machine, from the theme directory
    /// (`config/themes` in a checkout, `$MESH_HOME/themes` otherwise) plus any
    /// theme modules in the graph. Sorted, deduplicated.
    pub themes: Vec<String>,
    /// Locale codes some module ships a catalog for, plus the default locales
    /// modules declare. Sorted, deduplicated.
    pub locales: Vec<String>,
    /// The same serializable effective-resource explanation consumed by shell
    /// diagnostics and the CLI. LSP does not prepare render assets, so its
    /// records are marked as metadata-only until the runtime snapshot is
    /// available; identifiers and ordered graph ownership stay identical.
    pub resource_snapshot: ResourceExplanationSnapshot,
}

impl ModuleRegistry {
    pub fn empty() -> Self {
        Self {
            manifests: HashMap::new(),
            module_dirs: HashMap::new(),
            module_entrypoints: HashMap::new(),
            interface_fields: HashMap::new(),
            interface_shapes: HashMap::new(),
            interface_contracts: HashMap::new(),
            exported_tags: HashMap::new(),
            themes: Vec::new(),
            locales: Vec::new(),
            resource_snapshot: ResourceExplanationSnapshot::default(),
        }
    }

    /// Discover modules from the workspace root and standard system paths.
    pub fn discover(workspace_root: &Path) -> Self {
        let mut registry = Self::empty();

        let search_roots = search_paths(workspace_root);
        for root in search_roots {
            registry.scan_dir(&root);
        }

        registry.themes = discover_themes(workspace_root, &registry);
        registry.locales = discover_locales(workspace_root);
        registry.resource_snapshot = discover_resources(workspace_root);

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
        let Ok(loaded) = load_canonical_manifest(dir) else {
            return;
        };
        let manifest = loaded.manifest;
        let module_id = manifest.package.id.clone();
        let manifest_dir = loaded.path.parent().unwrap_or(dir).to_path_buf();

        // Record exported component tag
        if let Some(tag) = manifest.exported_component_tag() {
            self.exported_tags
                .insert(tag.to_string(), module_id.clone());
        }

        if let Some(entry) = &manifest.entrypoints.main {
            self.module_entrypoints
                .insert(module_id.clone(), manifest_dir.join(entry));
        }
        self.module_dirs.insert(module_id.clone(), manifest_dir);

        // Index declared contracts before provider inference. A standalone
        // interface module is authoritative if it collides with an inline
        // backend declaration, matching installed-graph precedence.
        let standalone_interface = manifest.package.module_type == ModuleType::Interface;
        let declarations = manifest.interface.iter().chain(manifest.interfaces.iter());
        for declaration in declarations {
            let interface_name = canonical_interface_name(&declaration.name);
            self.interface_fields
                .entry(interface_name.clone())
                .or_default();
            let Some(contract_json) = declaration.contract.as_ref() else {
                continue;
            };
            let Ok(contract) =
                parse_interface_contract(&interface_name, &declaration.version, contract_json)
            else {
                continue;
            };
            if standalone_interface {
                self.interface_contracts.insert(interface_name, contract);
            } else {
                self.interface_contracts
                    .entry(interface_name)
                    .or_insert(contract);
            }
        }

        // For interface modules, record the interface name even when the
        // declaration has no contract yet.
        if manifest.package.module_type == ModuleType::Interface {
            if let Some(iface) = &manifest.interface {
                self.interface_fields
                    .entry(canonical_interface_name(&iface.name))
                    .or_default();
            }
        }

        // For backend modules, record what interfaces they provide and analyze
        // the main script to infer state fields + commands.
        let is_backend = manifest.package.module_type == ModuleType::Backend;
        let interface_names: Vec<String> = {
            let mut names: Vec<String> = manifest
                .provides
                .iter()
                .map(|p| canonical_interface_name(&p.interface))
                .collect();
            if let Some(svc) = manifest.primary_service() {
                let provides = canonical_interface_name(&svc.provides);
                if !names.contains(&provides) {
                    names.push(provides);
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

    pub fn module_entrypoint(&self, module_id: &str) -> Option<&Path> {
        self.module_entrypoints.get(module_id).map(PathBuf::as_path)
    }

    /// Ids of every discovered module, sorted.
    pub fn module_ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = self.manifests.keys().cloned().collect();
        ids.sort();
        ids
    }

    /// Ids of the discovered modules of one kind, sorted. Used to offer only
    /// icon packs where an icon pack belongs.
    pub fn module_ids_of_type(&self, module_type: ModuleType) -> Vec<String> {
        let mut ids: Vec<String> = self
            .manifests
            .iter()
            .filter(|(_, manifest)| manifest.package.module_type == module_type)
            .map(|(id, _)| id.clone())
            .collect();
        ids.sort();
        ids
    }

    /// Interface ids (`mesh.audio`), sorted.
    pub fn interface_ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = self.interface_fields.keys().cloned().collect();
        ids.sort();
        ids
    }

    /// The validated contract for an interface, when one was declared.
    pub fn interface_contract(&self, interface: &str) -> Option<&InterfaceContract> {
        self.interface_contracts
            .get(&canonical_interface_name(interface))
    }

    pub fn interface_shape(&self, interface: &str) -> Option<&InterfaceShape> {
        self.interface_shapes
            .get(&canonical_interface_name(interface))
    }

    /// A one-line description of a module, for completion documentation.
    pub fn module_summary(&self, module_id: &str) -> Option<String> {
        let manifest = self.manifests.get(module_id)?;
        let kind = manifest.package.module_type.to_string();
        Some(match &manifest.package.description {
            Some(description) => format!("`{kind}` module — {description}"),
            None => format!("`{kind}` module"),
        })
    }
}

fn discover_resources(workspace_root: &Path) -> ResourceExplanationSnapshot {
    let catalog = mesh_core_resources::refresh_system_resource_catalog();
    let mut snapshot = ResourceExplanationSnapshot::from_catalog(&catalog);
    let root_graph = std::env::var_os("MESH_MODULE_GRAPH_PATH")
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| workspace_root.join("config/module.json"));
    let Ok(graph) = mesh_core_module::package::load_installed_module_graph(&root_graph) else {
        return snapshot;
    };

    snapshot.revision = mesh_core_resources::resource_revision();
    let icon_chain = graph.icon_pack_chain().to_vec();
    let font_chain = graph.font_pack_chain().to_vec();
    snapshot.icons.available.extend(icon_chain.iter().cloned());
    snapshot.icons.available.sort();
    snapshot.icons.available.dedup();
    snapshot.fonts.available.extend(font_chain.iter().cloned());

    for (chain_position, module_id) in icon_chain.iter().enumerate() {
        let Some(module) = graph.module(module_id) else {
            snapshot.diagnostics.push(resource_diagnostic(
                "error",
                "missing_pack_module",
                Some(module_id.clone()),
                None,
                format!("effective icon chain references missing module '{module_id}'"),
            ));
            continue;
        };
        let Some(section) = module.manifest.mesh.icon_pack.as_ref() else {
            snapshot.diagnostics.push(resource_diagnostic(
                "error",
                "missing_icon_pack_section",
                Some(module_id.clone()),
                None,
                format!("effective icon chain module '{module_id}' has no icon-pack section"),
            ));
            continue;
        };
        let mut mappings = section
            .mappings
            .iter()
            .map(|(name, mapping)| ResourceMappingExplanation {
                semantic_name: name.clone(),
                target: mapping.target.clone(),
                multicolor: mapping.multicolor,
                owner_module: module_id.clone(),
                fallback_stage: "pack-chain".into(),
            })
            .collect::<Vec<_>>();
        for vocabulary in section.vocabularies.values() {
            mappings.extend(
                vocabulary
                    .iter()
                    .map(|(name, mapping)| ResourceMappingExplanation {
                        semantic_name: name.clone(),
                        target: mapping.target.clone(),
                        multicolor: mapping.multicolor,
                        owner_module: module_id.clone(),
                        fallback_stage: "pack-vocabulary".into(),
                    }),
            );
        }
        mappings.sort_by(|left, right| left.semantic_name.cmp(&right.semantic_name));
        snapshot.icons.chain.push(ResourcePackExplanation {
            module_id: module_id.clone(),
            pack_id: section.id.clone(),
            chain_position,
            status: "selected".into(),
            assets: Vec::new(),
            mappings,
            script_coverage: Vec::new(),
        });
    }

    for (chain_position, module_id) in font_chain.iter().enumerate() {
        let Some(module) = graph.module(module_id) else {
            snapshot.diagnostics.push(resource_diagnostic(
                "error",
                "missing_pack_module",
                Some(module_id.clone()),
                None,
                format!("effective font chain references missing module '{module_id}'"),
            ));
            continue;
        };
        let Some(section) = module.manifest.mesh.font_pack.as_ref() else {
            snapshot.diagnostics.push(resource_diagnostic(
                "error",
                "missing_font_pack_section",
                Some(module_id.clone()),
                None,
                format!("effective font chain module '{module_id}' has no font-pack section"),
            ));
            continue;
        };
        let root = module.manifest_path.parent().unwrap_or(workspace_root);
        let mut assets = section
            .faces
            .iter()
            .map(|face| ResourceAssetExplanation {
                id: format!("face:{}", face.family),
                path: root.join(&face.file).display().to_string(),
                fingerprint: None,
                prepared: false,
            })
            .collect::<Vec<_>>();
        assets.sort_by(|left, right| left.id.cmp(&right.id));
        let mut mappings = section
            .mappings
            .iter()
            .map(|(name, family)| ResourceMappingExplanation {
                semantic_name: name.clone(),
                target: family.clone(),
                multicolor: false,
                owner_module: module_id.clone(),
                fallback_stage: "font-chain".into(),
            })
            .collect::<Vec<_>>();
        mappings.sort_by(|left, right| left.semantic_name.cmp(&right.semantic_name));
        let mut script_coverage = section.covers.keys().cloned().collect::<Vec<_>>();
        script_coverage.extend(
            section
                .faces
                .iter()
                .flat_map(|face| face.coverage.iter().cloned()),
        );
        script_coverage.sort();
        script_coverage.dedup();
        snapshot.fonts.available.push(section.id.clone());
        snapshot.fonts.chain.push(ResourcePackExplanation {
            module_id: module_id.clone(),
            pack_id: section.id.clone(),
            chain_position,
            status: "selected".into(),
            assets,
            mappings,
            script_coverage,
        });
    }

    for module in graph.enabled_modules() {
        if !matches!(
            module.kind,
            mesh_core_module::package::ModuleKind::Frontend
                | mesh_core_module::package::ModuleKind::Component
        ) {
            continue;
        }
        snapshot
            .frontends
            .push(mesh_core_resources::ResourceFrontendExplanation {
                module_id: module.id.clone(),
                icon_chain: module.manifest.mesh.uses.resources.icons.clone(),
                font_chain: module.manifest.mesh.uses.resources.fonts.clone(),
            });
    }
    snapshot
        .frontends
        .sort_by(|left, right| left.module_id.cmp(&right.module_id));
    snapshot.fonts.available.sort();
    snapshot.fonts.available.dedup();
    snapshot
}

fn resource_diagnostic(
    severity: &str,
    code: &str,
    module_id: Option<String>,
    pack_id: Option<String>,
    message: String,
) -> mesh_core_resources::ResourceExplanationDiagnostic {
    mesh_core_resources::ResourceExplanationDiagnostic {
        severity: severity.into(),
        code: code.into(),
        module_id,
        pack_id,
        message,
    }
}

/// Theme ids the shell could activate: the theme packages and legacy `*.json`
/// themes in the theme directory, plus modules of kind `theme`.
fn discover_themes(workspace_root: &Path, registry: &ModuleRegistry) -> Vec<String> {
    let mut ids: Vec<String> =
        mesh_core_theme::load_themes_from_dir(&mesh_core_theme::theme_dir_path())
            .into_iter()
            .map(|theme| theme.id)
            .collect();

    // A checkout being edited is not necessarily the checkout the LSP binary
    // was built from, so look next to the workspace root as well.
    ids.extend(
        mesh_core_theme::load_themes_from_dir(&workspace_root.join("config/themes"))
            .into_iter()
            .map(|theme| theme.id),
    );

    ids.extend(registry.module_ids_of_type(ModuleType::Theme));
    ids.sort();
    ids.dedup();
    ids
}

/// Locale codes from the resolved installed graph. Catalog paths are arbitrary
/// contained paths, so directory naming is not a source of truth for LSP
/// completion. The graph also supplies enabled language-pack contributions and
/// module defaults consistently with the runtime.
fn discover_locales(workspace_root: &Path) -> Vec<String> {
    let mut locales: Vec<String> = Vec::new();

    let root_graph = std::env::var_os("MESH_MODULE_GRAPH_PATH")
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| workspace_root.join("config/module.json"));
    let Ok(graph) = mesh_core_module::package::load_installed_module_graph(&root_graph) else {
        return locales;
    };
    locales.extend(
        graph
            .contributed_i18n()
            .iter()
            .map(|catalog| catalog.locale.clone()),
    );
    locales.extend(
        graph
            .modules()
            .into_iter()
            .filter(|module| module.enabled)
            .filter_map(|module| module.manifest.mesh.i18n.default_locale.clone()),
    );

    locales.sort();
    locales.dedup();
    locales
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
    let Ok(script) = parse_luau_script(source) else {
        return InterfaceShape::default();
    };
    InterfaceShape {
        state_fields: script.metadata.backend_state_fields,
        commands: script.metadata.backend_commands,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indexes_external_contract_as_authoritative_interface_shape() {
        let dir =
            std::env::temp_dir().join(format!("mesh-lsp-external-contract-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("module.json"),
            r#"{
  "name": "@mesh/audio-interface",
  "version": "1.0.0",
  "mesh": {
    "apiVersion": "0.1",
    "kind": "interface",
    "interface": {
      "name": "mesh.audio",
      "version": "1.0",
      "contract": "contract.json"
    }
  }
}"#,
        )
        .unwrap();
        std::fs::write(
            dir.join("contract.json"),
            r#"{
  "state": { "percent": { "type": "float" } },
  "methods": { "set_volume": {
    "args": [{ "name": "percent", "type": "float" }],
    "returns": "Result"
  } },
  "events": {},
  "types": {}
}"#,
        )
        .unwrap();

        let mut registry = ModuleRegistry::empty();
        registry.try_load_module(&dir);
        let contract = registry
            .interface_contract("mesh.audio")
            .expect("validated external contract");
        assert_eq!(contract.state_fields[0].name, "percent");
        assert_eq!(contract.methods[0].args[0].arg_type, "float");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn infers_backend_shape_from_luau_ast() {
        let shape = analyze_backend_script(
            r#"
-- function on_command_fake() end
local documentation = "percent = false"
function
  on_command_set_volume()
end
function on_command_toggle(
)
end
mesh.service.emit(
  {
    percent = 65,
    muted = false,
  }
)
return {
  available = true,
}
"#,
        );

        assert_eq!(shape.state_fields, ["percent", "muted", "available"]);
        assert_eq!(shape.commands, ["set_volume", "toggle"]);
    }
}
