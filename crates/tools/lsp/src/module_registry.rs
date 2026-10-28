use mesh_core_icon::{FontAsset, FrontendIconBindings, IconMapping};
#[cfg(test)]
use mesh_core_module::manifest::load_canonical_manifest;
use mesh_core_module::manifest::{Manifest, ModuleType};
use mesh_core_module::package::{AuthoringSnapshot, ModuleManifestError};
use mesh_core_resources::{
    ResourceAssetExplanation, ResourceExplanationSnapshot, ResourceMappingExplanation,
    ResourcePackExplanation,
};
#[cfg(test)]
use mesh_core_service::parse_interface_contract;
use mesh_core_service::{InterfaceContract, canonical_interface_name};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// A discovered and indexed view of all modules available in the workspace.
pub struct ModuleRegistry {
    /// The canonical graph snapshot from which every module-owned index below
    /// is derived. Keeping it here lets LSP consumers observe the same graph
    /// revision as CLI, doctor, and the runtime.
    pub snapshot: Option<AuthoringSnapshot>,
    /// Maps module-id → Manifest for all discovered modules.
    pub manifests: HashMap<String, Manifest>,
    pub style_parts: HashMap<String, std::collections::BTreeSet<String>>,
    /// Maps module-id → directory containing its manifest.
    pub module_dirs: HashMap<String, PathBuf>,
    /// Maps module-id → resolved main entrypoint path when present.
    pub module_entrypoints: HashMap<String, PathBuf>,
    /// Maps interface name (e.g. "mesh.audio") → list of field names it emits.
    pub interface_fields: HashMap<String, Vec<String>>,
    /// Validated contracts are the sole authority for service members.
    pub interface_contracts: HashMap<String, InterfaceContract>,
    /// Maps component tag name → module-id for modules that export a component tag.
    pub exported_tags: HashMap<String, String>,
    /// Theme identities from the graph-authorized theme descriptor catalog.
    /// Sorted, deduplicated, with both scoped and unambiguous local ids.
    pub themes: Vec<String>,
    /// Locale codes some module ships a catalog for, plus the default locales
    /// modules declare. Sorted, deduplicated.
    pub locales: Vec<String>,
    /// The same serializable effective-resource explanation consumed by shell
    /// diagnostics and the CLI. LSP does not publish render assets, but it
    /// resolves canonical mappings and records the same requirement status,
    /// provenance, and bounded diagnostics against its graph snapshot.
    pub resource_snapshot: ResourceExplanationSnapshot,
}

impl ModuleRegistry {
    pub fn empty() -> Self {
        Self {
            snapshot: None,
            manifests: HashMap::new(),
            style_parts: HashMap::new(),
            module_dirs: HashMap::new(),
            module_entrypoints: HashMap::new(),
            interface_fields: HashMap::new(),
            interface_contracts: HashMap::new(),
            exported_tags: HashMap::new(),
            themes: Vec::new(),
            locales: Vec::new(),
            resource_snapshot: ResourceExplanationSnapshot::default(),
        }
    }

    /// Discover modules from the workspace root and standard system paths.
    pub fn discover(workspace_root: &Path) -> Self {
        match Self::try_discover(workspace_root) {
            Ok(registry) => registry,
            Err(error) => {
                tracing::warn!(
                    workspace = %workspace_root.display(),
                    "failed to load canonical authoring snapshot: {error}"
                );
                Self::empty()
            }
        }
    }

    /// Build all authoring indexes from one canonical graph snapshot.
    pub fn try_discover(workspace_root: &Path) -> Result<Self, ModuleManifestError> {
        let root_graph = root_graph_path(workspace_root);
        let snapshot = mesh_core_module::package::load_authoring_snapshot(&root_graph)?;
        let mut registry = Self::from_snapshot(workspace_root, &snapshot);
        registry.snapshot = Some(snapshot);
        Ok(registry)
    }

    /// Replace the registry only after the next canonical snapshot has loaded
    /// successfully. Callers can therefore keep serving the last-known-good
    /// authoring state when a manifest is temporarily being edited.
    pub fn refresh(&mut self, workspace_root: &Path) -> Result<(), ModuleManifestError> {
        let next = Self::try_discover(workspace_root)?;
        *self = next;
        Ok(())
    }

    pub fn snapshot_revision(&self) -> Option<u64> {
        self.snapshot.as_ref().map(AuthoringSnapshot::revision)
    }

    fn from_snapshot(workspace_root: &Path, snapshot: &AuthoringSnapshot) -> Self {
        let mut registry = Self::empty();

        for module in snapshot.modules() {
            let manifest = module.manifest.clone().into_runtime_manifest();
            let module_id = module.id.clone();
            let module_dir = module
                .manifest_path
                .parent()
                .unwrap_or(workspace_root)
                .to_path_buf();

            if let Some(tag) = manifest.exported_component_tag() {
                registry
                    .exported_tags
                    .insert(tag.to_string(), module_id.clone());
            }
            if let Some(entry) = &module.manifest.mesh.entrypoints.main {
                registry
                    .module_entrypoints
                    .insert(module_id.clone(), module_dir.join(entry));
            }
            registry
                .module_dirs
                .insert(module_id.clone(), module_dir.clone());
            registry
                .style_parts
                .insert(module_id.clone(), snapshot.style_parts(&module_id));
            registry.manifests.insert(module_id, manifest);
        }

        for declaration in snapshot.declared_interfaces() {
            let fields = snapshot
                .interface_contract(&declaration.name)
                .map(|contract| {
                    contract
                        .state_fields
                        .iter()
                        .map(|field| field.name.clone())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            registry
                .interface_fields
                .entry(declaration.name.clone())
                .or_insert(fields);
        }
        registry.interface_contracts = snapshot.interface_contracts().clone();

        registry.themes = discover_themes(snapshot);
        registry.locales = discover_locales(snapshot);
        registry.resource_snapshot = discover_resources(workspace_root, snapshot);
        registry
    }

    #[cfg(test)]
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

        // Index declared contracts. A standalone
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

        self.manifests.insert(module_id, manifest);
    }

    pub fn theme_independent(&self, path: &Path) -> bool {
        self.module_dirs
            .iter()
            .filter(|(_, root)| path.starts_with(root))
            .max_by_key(|(_, root)| root.components().count())
            .and_then(|(id, _)| self.manifests.get(id))
            .and_then(|manifest| manifest.theme.as_ref())
            .is_some_and(|theme| theme.independent)
    }

    pub fn local_theme_tokens(&self, path: &Path) -> Vec<String> {
        let owner = self
            .module_dirs
            .iter()
            .filter(|(_, root)| path.starts_with(root))
            .max_by_key(|(_, root)| root.components().count())
            .map(|(id, _)| id);
        let Some(manifest) = owner.and_then(|id| self.manifests.get(id)) else {
            return Vec::new();
        };
        let mut families = manifest
            .dependencies
            .modules
            .keys()
            .filter_map(|id| self.manifests.get(id))
            .collect::<Vec<_>>();
        families.push(manifest);
        let mut tokens = families
            .into_iter()
            .filter_map(|manifest| manifest.theme.as_ref())
            .flat_map(|theme| theme.tokens.keys())
            .map(|name| mesh_core_theme::module_token_name(name))
            .collect::<Vec<_>>();
        tokens.sort();
        tokens.dedup();
        tokens
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

fn discover_resources(
    workspace_root: &Path,
    graph: &AuthoringSnapshot,
) -> ResourceExplanationSnapshot {
    let catalog = mesh_core_resources::discover_system_resources();
    let mut snapshot = ResourceExplanationSnapshot::from_catalog(&catalog);

    snapshot.revision = mesh_core_resources::resource_revision();
    let icon_chain = graph.icon_pack_chain().to_vec();
    let font_chain = graph.font_pack_chain().to_vec();
    snapshot.icons.available.extend(icon_chain.iter().cloned());
    snapshot.icons.available.sort();
    snapshot.icons.available.dedup();
    snapshot.fonts.available.extend(font_chain.iter().cloned());

    let mut icon_packs = Vec::new();
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
        let Some(section) = module.manifest.mesh.icon_pack() else {
            snapshot.diagnostics.push(resource_diagnostic(
                "error",
                "missing_icon_pack_section",
                Some(module_id.clone()),
                None,
                format!("effective icon chain module '{module_id}' has no icon-pack section"),
            ));
            continue;
        };
        let root = module.manifest_path.parent().unwrap_or(workspace_root);
        let bindings = lsp_icon_bindings(
            module_id,
            root,
            section,
            &catalog,
            &mut snapshot.diagnostics,
        );
        let mappings = resource_icon_mappings(module_id, section);
        snapshot.icons.chain.push(ResourcePackExplanation {
            module_id: module_id.clone(),
            pack_id: section.id.clone(),
            chain_position,
            status: "selected".into(),
            assets: Vec::new(),
            mappings,
            script_coverage: Vec::new(),
        });
        icon_packs.push(bindings);
    }
    snapshot.icons.available.extend(
        snapshot
            .icons
            .chain
            .iter()
            .flat_map(|pack| [pack.module_id.clone(), pack.pack_id.clone()]),
    );
    snapshot.icons.available.sort();
    snapshot.icons.available.dedup();

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
        if module.kind != mesh_core_module::package::ModuleKind::Ui {
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

    let frontend_bindings = snapshot
        .frontends
        .iter()
        .map(|frontend| {
            (
                frontend.module_id.clone(),
                FrontendIconBindings {
                    declared_pack_chain: frontend.icon_chain.clone(),
                    ..Default::default()
                },
            )
        })
        .collect::<Vec<_>>();
    let known_pack_ids = icon_packs
        .iter()
        .flat_map(|pack| [pack.module_id.as_str(), pack.pack_id.as_str()])
        .collect::<std::collections::HashSet<_>>();
    let known_host_themes = catalog
        .icon_themes
        .iter()
        .filter(|theme| !theme.hidden)
        .map(|theme| theme.id.as_str())
        .collect::<std::collections::HashSet<_>>();
    for (module_id, bindings) in &frontend_bindings {
        for pack_id in bindings.effective_chain(None) {
            if !known_pack_ids.contains(pack_id.as_str())
                && !known_host_themes.contains(pack_id.as_str())
            {
                snapshot.diagnostics.push(resource_diagnostic(
                    "error",
                    "missing_icon_chain_pack",
                    Some(module_id.clone()),
                    Some(pack_id.clone()),
                    format!(
                        "frontend '{module_id}' effective icon chain references unavailable pack '{pack_id}'"
                    ),
                ));
            }
        }
    }
    if let Ok(mut registry) = mesh_core_icon::IconRegistry::from_catalog(Arc::new(catalog)) {
        if let Err(error) = registry.replace_bindings(icon_packs, frontend_bindings, None) {
            snapshot.diagnostics.push(resource_diagnostic(
                "error",
                "invalid_icon_snapshot",
                None,
                None,
                format!("canonical icon snapshot could not be inspected: {error}"),
            ));
        } else {
            let mut requirements = std::collections::BTreeMap::<(String, String), bool>::new();
            for requirement in graph.icon_requirements() {
                requirements
                    .entry((requirement.module_id.clone(), requirement.name.clone()))
                    .and_modify(|required| *required |= requirement.required)
                    .or_insert(requirement.required);
            }
            for ((module_id, semantic_name), required) in requirements {
                let resolution = registry.resolve_for_module(&module_id, &semantic_name, 24);
                let explanation =
                    lsp_resolution_explanation(&module_id, &semantic_name, required, resolution);
                if explanation.status == "missing" {
                    snapshot.diagnostics.push(resource_diagnostic(
                        if required { "error" } else { "warning" },
                        if required {
                            "missing_required_icon"
                        } else {
                            "missing_optional_icon"
                        },
                        Some(module_id.clone()),
                        None,
                        format!(
                            "icon requirement '{semantic_name}' for '{module_id}' is missing from the canonical effective chain"
                        ),
                    ));
                }
                snapshot.icons.resolutions.push(explanation);
            }
            snapshot.icons.resolutions.sort_by(|left, right| {
                left.module_id
                    .cmp(&right.module_id)
                    .then(left.semantic_name.cmp(&right.semantic_name))
            });
        }
    }
    snapshot
}

fn resource_icon_mappings(
    module_id: &str,
    section: &mesh_core_module::manifest::IconPackSection,
) -> Vec<ResourceMappingExplanation> {
    let mut mappings = Vec::new();
    let mut declared = section.mappings.iter().collect::<Vec<_>>();
    declared.sort_by(|left, right| left.0.cmp(right.0));
    for (name, mapping) in declared
        .into_iter()
        .take(mesh_core_icon::MAX_ICON_PACK_MAPPINGS)
    {
        mappings.push(ResourceMappingExplanation {
            semantic_name: name.clone(),
            target: mapping.target.clone(),
            multicolor: mapping.multicolor,
            owner_module: module_id.into(),
            fallback_stage: "pack-chain".into(),
        });
    }
    let mut remaining = mesh_core_icon::MAX_ICON_PACK_MAPPINGS.saturating_sub(mappings.len());
    let mut vocabulary_owners = section.vocabularies.iter().collect::<Vec<_>>();
    vocabulary_owners.sort_by(|left, right| left.0.cmp(right.0));
    for (owner, vocabulary) in vocabulary_owners
        .into_iter()
        .take(mesh_core_icon::MAX_ICON_PACK_VOCABULARY_OWNERS)
    {
        if remaining == 0 {
            break;
        }
        let mut entries = vocabulary.iter().collect::<Vec<_>>();
        entries.sort_by(|left, right| left.0.cmp(right.0));
        for (name, mapping) in entries.into_iter().take(remaining) {
            mappings.push(ResourceMappingExplanation {
                semantic_name: format!("{owner}:{name}"),
                target: mapping.target.clone(),
                multicolor: mapping.multicolor,
                owner_module: module_id.into(),
                fallback_stage: "vocabulary-chain".into(),
            });
            remaining = remaining.saturating_sub(1);
        }
    }
    mappings.sort_by(|left, right| left.semantic_name.cmp(&right.semantic_name));
    mappings
}

fn lsp_icon_bindings(
    module_id: &str,
    module_root: &Path,
    section: &mesh_core_module::manifest::IconPackSection,
    catalog: &mesh_core_resources::SystemResourceCatalog,
    diagnostics: &mut Vec<mesh_core_resources::ResourceExplanationDiagnostic>,
) -> mesh_core_icon::IconPackBindings {
    if let Err(error) = section.validate() {
        diagnostics.push(resource_diagnostic(
            "error",
            "invalid_icon_pack",
            Some(module_id.into()),
            Some(section.id.clone()),
            error,
        ));
    }
    let mut font_aliases = HashMap::new();
    for requirement in section
        .requires
        .fonts
        .iter()
        .take(mesh_core_icon::MAX_ICON_PACK_FONT_REQUIREMENTS)
    {
        let glyph_map_path = requirement.glyph_map.as_deref().and_then(|declared| {
            mesh_core_resources::ResourceAssetHandle::new(module_root, declared)
                .map(|handle| handle.candidate_path())
                .map_err(|error| {
                    diagnostics.push(resource_diagnostic(
                        "error",
                        "unsafe_icon_glyph_map",
                        Some(module_id.into()),
                        Some(section.id.clone()),
                        format!("glyph map '{declared}' is unsafe: {error}"),
                    ));
                })
                .ok()
        });
        let resolved_font_path = match requirement.file.as_deref() {
            Some(declared) => mesh_core_resources::ResourceAssetHandle::new(module_root, declared)
                .map(|handle| handle.candidate_path())
                .map_err(|error| {
                    diagnostics.push(resource_diagnostic(
                        "error",
                        "unsafe_icon_font",
                        Some(module_id.into()),
                        Some(section.id.clone()),
                        format!("font file '{declared}' is unsafe: {error}"),
                    ));
                })
                .ok(),
            None => catalog.font_path_for_family(&requirement.family),
        };
        font_aliases.insert(
            requirement.alias.clone(),
            FontAsset {
                family: requirement.family.clone(),
                glyph_map_path,
                resolved_font_path,
                prepared_font: None,
                font_fingerprint: None,
                prepared_glyphs: None,
            },
        );
    }

    let mappings: HashMap<String, IconMapping> = section
        .mappings
        .iter()
        .take(mesh_core_icon::MAX_ICON_PACK_MAPPINGS)
        .filter_map(|(name, mapping)| {
            lsp_icon_mapping(module_id, &section.id, name, mapping, diagnostics)
        })
        .collect();
    let mut remaining_mappings =
        mesh_core_icon::MAX_ICON_PACK_MAPPINGS.saturating_sub(mappings.len());
    let mut vocabularies = HashMap::new();
    for (owner, declared_mappings) in section
        .vocabularies
        .iter()
        .take(mesh_core_icon::MAX_ICON_PACK_VOCABULARY_OWNERS)
    {
        if remaining_mappings == 0 {
            break;
        }
        let mut normalized = HashMap::new();
        for (name, mapping) in declared_mappings.iter().take(remaining_mappings) {
            if remaining_mappings == 0 {
                break;
            }
            remaining_mappings = remaining_mappings.saturating_sub(1);
            if let Some(mapping) =
                lsp_icon_mapping(module_id, &section.id, name, mapping, diagnostics)
            {
                normalized.insert(mapping.0, mapping.1);
            }
        }
        vocabularies.insert(owner.clone(), normalized);
    }
    mesh_core_icon::IconPackBindings {
        pack_id: section.id.clone(),
        module_id: module_id.into(),
        mappings,
        vocabularies,
        axes: mesh_core_icon::SupportedAxes {
            fill: section.axes.fill,
            weight: section.axes.weight,
            grade: section.axes.grade,
            optical_size: section.axes.optical_size,
        },
        font_aliases,
    }
}

fn lsp_icon_mapping(
    module_id: &str,
    pack_id: &str,
    name: &str,
    mapping: &mesh_core_module::manifest::IconMappingTarget,
    diagnostics: &mut Vec<mesh_core_resources::ResourceExplanationDiagnostic>,
) -> Option<(String, IconMapping)> {
    if let Err(error) = mesh_core_icon::validate_icon_mapping(name, &mapping.target) {
        diagnostics.push(resource_diagnostic(
            "error",
            "invalid_icon_mapping",
            Some(module_id.into()),
            Some(pack_id.into()),
            format!("icon mapping '{name}': {error}"),
        ));
        return None;
    }
    Some((
        name.into(),
        IconMapping {
            target: mapping.target.clone(),
            multicolor: mapping.multicolor,
        },
    ))
}

fn lsp_resolution_explanation(
    module_id: &str,
    semantic_name: &str,
    required: bool,
    resolution: mesh_core_icon::IconResolution,
) -> mesh_core_resources::ResourceResolutionExplanation {
    match resolution {
        mesh_core_icon::IconResolution::Found {
            provenance, target, ..
        } => {
            let asset = match target {
                mesh_core_icon::ResolvedTarget::File(path) => Some(ResourceAssetExplanation {
                    id: "resolved-icon".into(),
                    path: path.display().to_string(),
                    fingerprint: mesh_core_resources::resource_fingerprint(&path),
                    prepared: false,
                }),
                mesh_core_icon::ResolvedTarget::Glyph {
                    font_path,
                    font_fingerprint,
                    ..
                } => Some(ResourceAssetExplanation {
                    id: "resolved-glyph".into(),
                    path: font_path.display().to_string(),
                    fingerprint: font_fingerprint,
                    prepared: false,
                }),
            };
            mesh_core_resources::ResourceResolutionExplanation {
                module_id: module_id.into(),
                semantic_name: semantic_name.into(),
                required,
                status: "found".into(),
                owner_module: provenance.owner_module,
                pack_id: provenance.pack_id,
                candidate: Some(provenance.candidate),
                fallback_stage: Some(provenance.fallback_stage),
                tried: Vec::new(),
                asset,
            }
        }
        mesh_core_icon::IconResolution::Missing { tried, .. } => {
            mesh_core_resources::ResourceResolutionExplanation {
                module_id: module_id.into(),
                semantic_name: semantic_name.into(),
                required,
                status: "missing".into(),
                owner_module: None,
                pack_id: None,
                candidate: None,
                fallback_stage: None,
                tried,
                asset: None,
            }
        }
    }
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

/// Theme ids the shell could activate, derived only from the graph-authorized
/// descriptor catalog. Filesystem presence and module inventory are not
/// activation identities.
fn discover_themes(graph: &AuthoringSnapshot) -> Vec<String> {
    let mut ids = Vec::new();
    for descriptor in graph.theme_catalog().iter() {
        ids.push(descriptor.id.clone());
        ids.push(descriptor.local_id.clone());
    }
    ids.sort();
    ids.dedup();
    ids
}

/// Locale codes from the resolved installed graph. Catalog paths are arbitrary
/// contained paths, so directory naming is not a source of truth for LSP
/// completion. The graph also supplies enabled language-pack contributions and
/// module defaults consistently with the runtime.
fn discover_locales(graph: &AuthoringSnapshot) -> Vec<String> {
    let Ok((sources, defaults)) = graph.locale_catalog_sources() else {
        return Vec::new();
    };
    let mut locales: Vec<String> = sources
        .into_iter()
        .map(|source| source.locale)
        .chain(defaults.into_values())
        .collect();

    locales.sort();
    locales.dedup();
    locales
}

fn root_graph_path(workspace_root: &Path) -> PathBuf {
    std::env::var_os("MESH_MODULE_GRAPH_PATH")
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| workspace_root.join("config/module.json"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variable_completion_uses_the_owning_modules_declared_tokens_and_families() {
        use crate::{analyzer, document::Document, util::offset_to_position};
        use tower_lsp::lsp_types::Url;

        let dir =
            std::env::temp_dir().join(format!("mesh-lsp-token-completion-{}", std::process::id()));
        let mut registry = ModuleRegistry::empty();
        for (path, name, kind, tokens, dependencies) in [
            (
                dir.clone(),
                "@alice/parent",
                "ui",
                serde_json::json!({"parent.hidden": "var(--color-primary)"}),
                serde_json::json!({}),
            ),
            (
                dir.join("family"),
                "@alice/material",
                "library",
                serde_json::json!({"family.rim": "var(--color-outline)", "widget.accent": "var(--color-primary)"}),
                serde_json::json!({}),
            ),
            (
                dir.join("widget"),
                "@alice/widget",
                "ui",
                serde_json::json!({"widget.accent": "var(--color-secondary)"}),
                serde_json::json!({"@alice/material": "*"}),
            ),
            (
                dir.join("other"),
                "@alice/other",
                "library",
                serde_json::json!({"other.hidden": "var(--color-primary)"}),
                serde_json::json!({}),
            ),
        ] {
            std::fs::create_dir_all(&path).unwrap();
            std::fs::write(path.join("module.json"), serde_json::to_string(&serde_json::json!({
                "name": name, "version": "1.0.0",
                "mesh": {"apiVersion": "0.1", "kind": kind, "entry": "src/main.mesh", "theme": {"tokens": tokens}, "uses": {"modules": dependencies}}
            })).unwrap()).unwrap();
            registry.try_load_module(&path);
            assert!(registry.manifests.contains_key(name), "{name}");
        }

        for (path, prefix, expected) in [
            (
                dir.join("widget/src/main.mesh"),
                "--",
                vec!["--family-rim", "--widget-accent"],
            ),
            (
                dir.join("widget/src/main.mesh"),
                "--family",
                vec!["--family-rim"],
            ),
            (
                dir.join("widget/src/main.mesh"),
                "family",
                vec!["--family-rim"],
            ),
            (dir.join("src/main.mesh"), "--widget", vec![]),
        ] {
            let source = format!(
                "<template></template><style>.panel {{ --color-primary: var(--color-outline); color: var({prefix}$0); }}</style>"
            );
            let cursor = source.find("$0").unwrap();
            let source = source.replace("$0", "");
            let position = offset_to_position(&source, cursor);
            let doc = Document::new(Url::from_file_path(path).unwrap(), source);
            let items = analyzer::complete(&doc, position, &registry);
            let labels = items
                .iter()
                .map(|item| item.label.as_str())
                .collect::<Vec<_>>();
            for token in expected {
                let matching = items
                    .iter()
                    .filter(|item| item.label == token)
                    .collect::<Vec<_>>();
                assert_eq!(matching.len(), 1, "{token}: {labels:?}");
                assert_eq!(matching[0].insert_text.as_deref(), Some(token));
            }
            assert!(!labels.contains(&"--parent-hidden"));
            assert!(!labels.contains(&"--other-hidden"));
            if prefix == "--" {
                assert!(labels.contains(&"--color-primary"));
            } else if prefix == "--widget" {
                assert!(labels.is_empty());
            } else {
                assert_eq!(labels, vec!["--family-rim"]);
            }
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn independence_uses_the_nearest_owning_module() {
        let dir = std::env::temp_dir().join(format!("mesh-lsp-independent-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("child")).unwrap();
        for (root, name, independent) in [
            (&dir, "@alice/logo", true),
            (&dir.join("child"), "@alice/widget", false),
        ] {
            std::fs::write(root.join("module.json"), serde_json::to_string(&serde_json::json!({
                "name":name,"version":"1.0.0",
                "mesh":{"apiVersion":"0.1","kind":"ui","entry":"src/main.mesh", "theme":{"independent":independent}}
            })).unwrap()).unwrap();
        }
        let mut registry = ModuleRegistry::empty();
        registry.try_load_module(&dir);
        registry.try_load_module(&dir.join("child"));
        assert!(registry.theme_independent(&dir.join("src/main.mesh")));
        assert!(!registry.theme_independent(&dir.join("child/src/card.mesh")));
        assert!(!registry.theme_independent(Path::new("/unrelated.mesh")));
        std::fs::remove_dir_all(dir).unwrap();
    }

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
                "state": { "percent": "float" },
                "methods": { "set_volume": "(percent: float) -> Result" },
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
}
