use std::collections::{HashMap, HashSet, VecDeque};
use std::hash::{Hash, Hasher};
use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use mesh_core_frontend::{
    CompiledFrontendModule, compile_frontend_entrypoint, compile_frontend_module,
};
use mesh_core_module::Manifest;
use mesh_core_module::ModuleType;
use mesh_core_module::lifecycle::ModuleInstance;
use mesh_core_module::package::{InstalledModuleGraph, ModuleKind, NodeSlotOverride};
use rayon::prelude::*;

use super::memo;
use crate::shell::ShellRunError;

#[derive(Debug, Clone, Default)]
pub(in crate::shell) struct FrontendCatalog {
    pub(in crate::shell) modules: HashMap<String, FrontendCatalogEntry>,
    /// Source/indexing failures are kept with the catalog generation instead
    /// of aborting unrelated module compilation. The shell reports these as
    /// scoped diagnostics while graph construction remains authoritative.
    pub(in crate::shell) diagnostics: Vec<FrontendCatalogDiagnostic>,
    /// Contributions each host renders, keyed by
    /// [`extension_point_key`]`(host module id, point contract name)`.
    ///
    /// The core matches hosts to contributions by contract only; no module id
    /// appears in this file.
    pub(super) extension_point_contributions:
        HashMap<String, Vec<ResolvedExtensionPointContribution>>,
    /// Compiled contribution components, keyed by [`contribution_entry_key`].
    /// Each is an alternate root of its *contributing* module, so it runs with
    /// that module's VM, capabilities, and settings namespace.
    pub(super) extension_point_entries: HashMap<String, SharedCompiledFrontendModule>,
    /// Sparse effective placement overrides, keyed by root instance then slot.
    pub(super) node_slot_placements:
        std::collections::BTreeMap<String, std::collections::BTreeMap<String, NodeSlotOverride>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::shell) struct FrontendCatalogDiagnostic {
    pub(in crate::shell) module_id: String,
    pub(in crate::shell) contribution_id: Option<String>,
    pub(in crate::shell) source_path: PathBuf,
    pub(in crate::shell) message: String,
}

impl FrontendCatalogDiagnostic {
    fn module(module_id: &str, source_path: PathBuf, error: &ShellRunError) -> Self {
        Self {
            module_id: module_id.to_string(),
            contribution_id: None,
            source_path,
            message: error.to_string(),
        }
    }

    fn contribution(
        module_id: &str,
        contribution_id: &str,
        source_path: PathBuf,
        error: &ShellRunError,
    ) -> Self {
        Self {
            module_id: module_id.to_string(),
            contribution_id: Some(contribution_id.to_string()),
            source_path,
            message: error.to_string(),
        }
    }
}

/// Index key for the contributions one host renders at one extension point.
pub(super) fn extension_point_key(host_module_id: &str, point: &str) -> String {
    format!("{host_module_id}\u{1}{point}")
}

/// Index key for one contribution's compiled component.
pub(super) fn contribution_entry_key(source_module_id: &str, contribution_id: &str) -> String {
    format!("{source_module_id}\u{1}{contribution_id}")
}

fn validate_placement_props(
    reference: &str,
    props: &serde_json::Map<String, serde_json::Value>,
    compiled: &CompiledFrontendModule,
) -> Result<(), ShellRunError> {
    let declared = compiled.component.props.as_ref();
    for (name, value) in props {
        let Some(definition) = declared.and_then(|block| {
            block
                .props
                .iter()
                .find(|definition| definition.name == *name)
        }) else {
            return Err(ShellRunError::FrontendComposition {
                message: format!(
                    "invalid_node_props: contribution '{reference}' has no public prop '{name}'"
                ),
            });
        };
        if !definition.expose {
            return Err(ShellRunError::FrontendComposition {
                message: format!(
                    "invalid_node_props: contribution '{reference}' prop '{name}' is private"
                ),
            });
        }
        let value = mesh_core_component::json_to_prop_value_ref(value).map_err(|_| {
            ShellRunError::FrontendComposition {
                message: format!(
                    "invalid_node_props: contribution '{reference}' prop '{name}' must be scalar"
                ),
            }
        })?;
        mesh_core_component::validate_prop_value(definition, &value).map_err(|error| {
            ShellRunError::FrontendComposition {
                message: format!("invalid_node_props: {error}"),
            }
        })?;
    }
    Ok(())
}

#[derive(Debug, Clone)]
pub(in crate::shell) struct FrontendCatalogEntry {
    pub(in crate::shell) module_dir: PathBuf,
    /// The immutable compiled source is shared by every surface instance and
    /// catalog generation that still references it. A source reload replaces
    /// this pointer atomically with the next catalog snapshot.
    pub(in crate::shell) compiled: SharedCompiledFrontendModule,
}

fn manifest_fingerprint(manifest: &mesh_core_module::Manifest) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    // A manifest is normalized before it reaches the shell and always
    // serializable. Hash its canonical data instead of relying on pointer or
    // `Debug` identity, so edits to entrypoints/import declarations invalidate
    // the cached compilation.
    serde_json::to_vec(manifest)
        .expect("normalized module manifests serialize")
        .hash(&mut hasher);
    hasher.finish()
}

fn source_fingerprint(paths: &[PathBuf]) -> Option<u64> {
    let mut paths = paths.to_vec();
    paths.sort();
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for path in paths {
        path.hash(&mut hasher);
        let metadata = std::fs::symlink_metadata(&path).ok()?;
        if metadata.file_type().is_symlink() {
            return None;
        }
        std::fs::read(&path).ok()?.hash(&mut hasher);
    }
    Some(hasher.finish())
}

fn compile_catalog_entry_from_parts(
    module_id: &str,
    manifest: &Manifest,
    module_dir: &Path,
) -> Result<FrontendCatalogEntry, ShellRunError> {
    let mut compiled = compile_frontend_module(manifest, module_dir).map_err(|source| {
        ShellRunError::FrontendCompile {
            module_id: module_id.to_string(),
            source,
        }
    })?;
    // Contributions are indexed independently below, but their entry files
    // still belong to the host's watch set. This lets a later source edit
    // retry a contribution that was previously rejected without reparsing the
    // whole graph or silently losing hot reload coverage.
    for contributions in manifest.extension_point_contributions.values() {
        for contribution in contributions {
            if let Ok(path) = mesh_core_module::package::resolve_contained_module_file(
                module_dir,
                &contribution.entry,
                "frontend contribution entry",
            ) {
                if !compiled.watched_paths.contains(&path) {
                    compiled.watched_paths.push(path);
                }
            }
        }
    }
    Ok(FrontendCatalogEntry {
        module_dir: module_dir.to_path_buf(),
        compiled: compiled.into(),
    })
}

fn compile_catalog_entry(
    module_id: &str,
    module: &ModuleInstance,
) -> Result<FrontendCatalogEntry, ShellRunError> {
    compile_catalog_entry_from_parts(module_id, &module.manifest, &module.path)
}

/// Compile one extension point contribution as an alternate root of its
/// contributing module.
fn compile_contribution_entry(
    module_id: &str,
    module: &ModuleInstance,
    entry: &str,
) -> Result<SharedCompiledFrontendModule, ShellRunError> {
    compile_contribution_entry_from_parts(module_id, &module.manifest, &module.path, entry)
}

fn compile_contribution_entry_from_parts(
    module_id: &str,
    manifest: &Manifest,
    module_dir: &Path,
    entry: &str,
) -> Result<SharedCompiledFrontendModule, ShellRunError> {
    let compiled = compile_frontend_entrypoint(manifest, module_dir, entry).map_err(|source| {
        ShellRunError::FrontendCompile {
            module_id: module_id.to_string(),
            source,
        }
    })?;
    Ok(compiled.into())
}

/// Copy-on-write handle for a compiled frontend module.
///
/// Production code treats compiled source as immutable and cloning this handle
/// only increments an `Arc` count. `DerefMut` is deliberately copy-on-write to
/// retain the concise fixture setup used by component tests.
#[derive(Debug, Clone)]
pub(in crate::shell) struct SharedCompiledFrontendModule {
    compiled: Arc<CompiledFrontendModule>,
    /// Captured when the immutable compilation was created. A graph rebuild
    /// compares it with the current files before retaining this snapshot.
    source_fingerprint: Option<u64>,
}

impl From<CompiledFrontendModule> for SharedCompiledFrontendModule {
    fn from(compiled: CompiledFrontendModule) -> Self {
        let source_fingerprint = source_fingerprint(&compiled.watched_paths);
        Self {
            compiled: Arc::new(compiled),
            source_fingerprint,
        }
    }
}

impl Deref for SharedCompiledFrontendModule {
    type Target = CompiledFrontendModule;

    fn deref(&self) -> &Self::Target {
        &self.compiled
    }
}

impl DerefMut for SharedCompiledFrontendModule {
    fn deref_mut(&mut self) -> &mut Self::Target {
        Arc::make_mut(&mut self.compiled)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(in crate::shell) struct ResolvedExtensionPointContribution {
    pub(in crate::shell) source_module_id: String,
    pub(in crate::shell) contribution_id: String,
    pub(in crate::shell) order: i64,
    pub(in crate::shell) props_fingerprint: u64,
    pub(in crate::shell) props: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone)]
pub(in crate::shell) struct FrontendCatalogHandle {
    state: Arc<RwLock<FrontendCatalogState>>,
}

#[derive(Debug, Clone)]
pub(in crate::shell) struct FrontendCatalogState {
    pub(in crate::shell) version: u64,
    pub(in crate::shell) catalog: Arc<FrontendCatalog>,
    pub(in crate::shell) changed_modules: Arc<HashSet<String>>,
    pub(in crate::shell) affected_modules: Arc<HashSet<String>>,
}

impl Default for FrontendCatalogHandle {
    fn default() -> Self {
        Self::from(FrontendCatalog::default())
    }
}

impl From<FrontendCatalog> for FrontendCatalogHandle {
    fn from(catalog: FrontendCatalog) -> Self {
        Self::from(Arc::new(catalog))
    }
}

impl From<Arc<FrontendCatalog>> for FrontendCatalogHandle {
    fn from(catalog: Arc<FrontendCatalog>) -> Self {
        Self {
            state: Arc::new(RwLock::new(FrontendCatalogState {
                version: 0,
                catalog,
                changed_modules: Arc::new(HashSet::new()),
                affected_modules: Arc::new(HashSet::new()),
            })),
        }
    }
}

impl FrontendCatalogHandle {
    pub(in crate::shell) fn snapshot(&self) -> FrontendCatalogState {
        self.state.read().unwrap().clone()
    }

    /// Atomically publish one catalog generation. The returned state can be
    /// restored if the caller fails before any component adopts the update.
    pub(in crate::shell) fn replace(
        &self,
        catalog: FrontendCatalog,
        changed_module: Option<&str>,
    ) -> FrontendCatalogState {
        let mut state = self.state.write().unwrap();
        let previous = state.clone();
        let catalog = Arc::new(catalog);
        let (changed_modules, affected_modules) =
            catalog_changes(&state.catalog, &catalog, changed_module);
        *state = FrontendCatalogState {
            version: state.version.wrapping_add(1),
            catalog,
            changed_modules: Arc::new(changed_modules),
            affected_modules: Arc::new(affected_modules),
        };
        previous
    }

    pub(in crate::shell) fn restore(&self, state: FrontendCatalogState) {
        *self.state.write().unwrap() = state;
    }

    /// Compile every active root owned by one module before publishing the
    /// replacement generation. Primary and contribution entries therefore
    /// cannot be observed from different source revisions.
    pub(in crate::shell) fn reload_module(
        &self,
        module_id: &str,
        manifest: &Manifest,
        module_dir: &Path,
    ) -> Result<(), ShellRunError> {
        let previous = self.snapshot();
        let catalog = previous
            .catalog
            .reload_module(module_id, manifest, module_dir)?;
        self.replace(catalog, Some(module_id));
        Ok(())
    }
}

fn catalog_changes(
    previous: &FrontendCatalog,
    next: &FrontendCatalog,
    changed_module: Option<&str>,
) -> (HashSet<String>, HashSet<String>) {
    let mut changed = HashSet::new();
    if let Some(module_id) = changed_module {
        changed.insert(module_id.to_string());
    }

    for module_id in previous.modules.keys().chain(next.modules.keys()) {
        if previous.modules.contains_key(module_id) != next.modules.contains_key(module_id) {
            changed.insert(module_id.clone());
        }
    }

    let mut changed_extension_points = HashSet::new();
    for key in previous
        .extension_point_contributions
        .keys()
        .chain(next.extension_point_contributions.keys())
    {
        if previous.extension_point_contributions.get(key)
            != next.extension_point_contributions.get(key)
        {
            changed_extension_points.insert(key.clone());
            for catalog in [previous, next] {
                if let Some(contributions) = catalog.extension_point_contributions.get(key) {
                    for contribution in contributions {
                        changed.insert(contribution.source_module_id.clone());
                    }
                }
            }
        }
    }

    // Use one graph spanning both catalog generations. Keeping removed edges
    // in the union is intentional: a root that was present in either
    // generation must be invalidated when a dependency or contribution is
    // removed, otherwise an instance can retain the old tree.
    let graph = FrontendDependencyGraph::from_catalogs(previous, next);
    let affected = graph.affected_modules(&changed, &changed_extension_points);

    (changed, affected)
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum FrontendRoot {
    Primary {
        module_id: String,
    },
    Contribution {
        source_module_id: String,
        contribution_id: String,
    },
}

/// Reverse ownership and import edges for every compiled frontend root.
///
/// Primary roots and extension-point contributions share this graph so a
/// dependency used only by a contribution can invalidate the host surface that
/// renders it. The graph deliberately records root consumers separately from
/// importers: a contribution is owned by its source module but consumed by one
/// or more host modules.
#[derive(Debug, Clone, Default)]
struct FrontendDependencyGraph {
    reverse_dependencies: HashMap<String, HashSet<FrontendRoot>>,
    owned_roots: HashMap<String, HashSet<FrontendRoot>>,
    root_consumers: HashMap<FrontendRoot, HashSet<String>>,
}

impl FrontendDependencyGraph {
    fn from_catalogs(previous: &FrontendCatalog, next: &FrontendCatalog) -> Self {
        let mut graph = Self::default();
        graph.extend_catalog(previous);
        graph.extend_catalog(next);
        graph
    }

    fn extend_catalog(&mut self, catalog: &FrontendCatalog) {
        for (module_id, entry) in &catalog.modules {
            self.register_root(
                module_id,
                FrontendRoot::Primary {
                    module_id: module_id.clone(),
                },
                &entry.compiled,
            );
        }

        for (entry_key, compiled) in &catalog.extension_point_entries {
            let Some((source_module_id, contribution_id)) = entry_key.split_once('\u{1}') else {
                continue;
            };
            self.register_root(
                source_module_id,
                FrontendRoot::Contribution {
                    source_module_id: source_module_id.to_string(),
                    contribution_id: contribution_id.to_string(),
                },
                compiled,
            );
        }

        for (point_key, contributions) in &catalog.extension_point_contributions {
            let Some((host_module_id, _)) = point_key.split_once('\u{1}') else {
                continue;
            };
            for contribution in contributions {
                let root = FrontendRoot::Contribution {
                    source_module_id: contribution.source_module_id.clone(),
                    contribution_id: contribution.contribution_id.clone(),
                };
                if self.root_consumers.contains_key(&root) {
                    self.root_consumers
                        .entry(root)
                        .or_default()
                        .insert(host_module_id.to_string());
                }
            }
        }
    }

    fn register_root(
        &mut self,
        owner_module_id: &str,
        root: FrontendRoot,
        compiled: &SharedCompiledFrontendModule,
    ) {
        self.owned_roots
            .entry(owner_module_id.to_string())
            .or_default()
            .insert(root.clone());
        self.root_consumers
            .entry(root.clone())
            .or_default()
            .insert(owner_module_id.to_string());
        for dependency in compiled.module_component_imports.values() {
            self.reverse_dependencies
                .entry(dependency.clone())
                .or_default()
                .insert(root.clone());
        }
    }

    fn affected_modules(
        &self,
        changed_modules: &HashSet<String>,
        changed_extension_points: &HashSet<String>,
    ) -> HashSet<String> {
        let mut affected = changed_modules.clone();
        let mut pending = VecDeque::from_iter(changed_modules.iter().cloned());

        for point_key in changed_extension_points {
            if let Some((host_module_id, _)) = point_key.split_once('\u{1}')
                && affected.insert(host_module_id.to_string())
            {
                pending.push_back(host_module_id.to_string());
            }
        }

        let mut visited_roots = HashSet::new();
        while let Some(module_id) = pending.pop_front() {
            let mut roots = HashSet::new();
            if let Some(owned) = self.owned_roots.get(&module_id) {
                roots.extend(owned.iter().cloned());
            }
            if let Some(importers) = self.reverse_dependencies.get(&module_id) {
                roots.extend(importers.iter().cloned());
            }

            for root in roots {
                if !visited_roots.insert(root.clone()) {
                    continue;
                }
                if let Some(consumers) = self.root_consumers.get(&root) {
                    for consumer in consumers {
                        if affected.insert(consumer.clone()) {
                            pending.push_back(consumer.clone());
                        }
                    }
                }
            }
        }

        affected
    }
}

impl FrontendCatalog {
    fn reload_module(
        &self,
        module_id: &str,
        manifest: &Manifest,
        module_dir: &Path,
    ) -> Result<Self, ShellRunError> {
        let mut next = self.clone();
        let primary = compile_catalog_entry_from_parts(module_id, manifest, module_dir)?;
        next.modules.insert(module_id.to_string(), primary);

        let contribution_keys = next
            .extension_point_entries
            .keys()
            .filter(|key| key.starts_with(&format!("{module_id}\u{1}")))
            .cloned()
            .collect::<Vec<_>>();
        for key in contribution_keys {
            let contribution_id = key
                .split_once('\u{1}')
                .map(|(_, contribution_id)| contribution_id)
                .ok_or_else(|| ShellRunError::FrontendComposition {
                    message: format!(
                        "invalid contribution catalog key '{key}' while reloading module '{module_id}'"
                    ),
                })?;
            let contribution = manifest
                .extension_point_contributions
                .values()
                .flat_map(Vec::as_slice)
                .find(|contribution| contribution.id == contribution_id)
                .ok_or_else(|| ShellRunError::FrontendComposition {
                    message: format!(
                        "contribution '{module_id}:{contribution_id}' is missing from the module manifest"
                    ),
                })?;
            let compiled = compile_contribution_entry_from_parts(
                module_id,
                manifest,
                module_dir,
                &contribution.entry,
            )?;
            next.extension_point_entries.insert(key, compiled);
        }

        Ok(next)
    }

    pub(in crate::shell) fn module(&self, module_id: &str) -> Option<&FrontendCatalogEntry> {
        self.modules.get(module_id)
    }

    pub(in crate::shell) fn diagnostics(&self) -> &[FrontendCatalogDiagnostic] {
        &self.diagnostics
    }

    pub(in crate::shell) fn from_modules(
        modules: &HashMap<String, ModuleInstance>,
        graph: Option<&InstalledModuleGraph>,
    ) -> Result<Self, ShellRunError> {
        Self::from_modules_reusing(modules, graph, None)
    }

    /// Rebuild graph-derived indexes while retaining compiled frontend sources
    /// whose normalized manifest and complete `.mesh` source set are unchanged.
    ///
    /// Module activation and deactivation change graph state far more often
    /// than authoring source. Revalidating the assembled catalog is necessary
    /// (enabled slot contributions and interface availability can change), but
    /// reparsing every independent frontend module is not.
    pub(in crate::shell) fn from_modules_reusing(
        modules: &HashMap<String, ModuleInstance>,
        graph: Option<&InstalledModuleGraph>,
        previous: Option<&FrontendCatalog>,
    ) -> Result<Self, ShellRunError> {
        let mut module_ids: Vec<String> = modules.keys().cloned().collect();
        module_ids.sort();

        let frontend_modules: Vec<_> = module_ids
            .iter()
            .filter_map(|module_id| {
                let module = modules.get(module_id)?;
                if !mesh_core_frontend::is_frontend_module(&module.manifest) {
                    return None;
                }
                // A graph-backed catalog is an activation boundary. Compile
                // only graph-enabled frontends (plus the shell-owned debug
                // inspector); an invalid or absent graph must never turn
                // discovery into an implicit allow-all filter.
                if let Some(graph) = graph
                    && *module_id != "@mesh/debug-inspector"
                    && !graph
                        .module(module_id)
                        .is_some_and(|entry| entry.enabled && entry.kind == ModuleKind::Frontend)
                {
                    return None;
                }
                Some((module_id, module))
            })
            .collect();
        let compiled_results = frontend_modules
            .par_iter()
            .map(|(module_id, module)| {
                let current_manifest_fingerprint = manifest_fingerprint(&module.manifest);
                if let Some(entry) = previous.and_then(|catalog| catalog.modules.get(*module_id))
                    && entry.module_dir == module.path
                    && manifest_fingerprint(&entry.compiled.manifest)
                        == current_manifest_fingerprint
                    && entry
                        .compiled
                        .source_fingerprint
                        .is_some_and(|fingerprint| {
                            source_fingerprint(&entry.compiled.watched_paths) == Some(fingerprint)
                        })
                {
                    return ((*module_id).clone(), module.path.clone(), Ok(entry.clone()));
                }

                (
                    (*module_id).clone(),
                    module.path.clone(),
                    compile_catalog_entry(module_id, module),
                )
            })
            .collect::<Vec<_>>();

        let mut diagnostics = Vec::new();
        let compiled_entries = compiled_results
            .into_iter()
            .filter_map(|(module_id, module_path, result)| match result {
                Ok(entry) => Some((module_id, entry)),
                Err(error) => {
                    diagnostics.push(FrontendCatalogDiagnostic::module(
                        &module_id,
                        module_path,
                        &error,
                    ));
                    None
                }
            })
            .collect::<Vec<_>>();

        let mut catalog = Self {
            modules: compiled_entries.into_iter().collect(),
            diagnostics,
            extension_point_contributions: HashMap::new(),
            extension_point_entries: HashMap::new(),
            node_slot_placements: graph
                .map(|graph| graph.node_slot_overrides().clone())
                .unwrap_or_default(),
        };

        // Host↔contribution matching is the graph's job: it owns the contract
        // declarations, the version check, and the enabled set. The catalog only
        // compiles what the graph resolved, so no module id is named here.
        let module_instances: HashMap<&str, &ModuleInstance> = frontend_modules
            .iter()
            .map(|(module_id, module)| (module_id.as_str(), *module))
            .collect();
        if let Some(graph) = graph {
            for ((host_module_id, point), contributions) in
                graph.all_extension_point_contributions()
            {
                if !catalog.modules.contains_key(host_module_id) {
                    continue;
                }
                let mut resolved = Vec::with_capacity(contributions.len());
                for contribution in contributions {
                    let Some(module) = module_instances.get(contribution.source_module_id.as_str())
                    else {
                        continue;
                    };
                    if !catalog.modules.contains_key(&contribution.source_module_id) {
                        continue;
                    }
                    let entry_key = contribution_entry_key(
                        &contribution.source_module_id,
                        &contribution.contribution_id,
                    );
                    if !catalog.extension_point_entries.contains_key(&entry_key) {
                        // Reuse an unchanged compilation across graph-only
                        // rebuilds, on the same terms as the primary entry.
                        let entry_path = mesh_core_module::package::resolve_contained_module_file(
                            &module.path,
                            &contribution.entry,
                            "frontend contribution entry",
                        )
                        .ok();
                        let reused = previous
                            .and_then(|catalog| catalog.extension_point_entries.get(&entry_key))
                            .filter(|compiled| {
                                entry_path
                                    .as_ref()
                                    .is_some_and(|path| compiled.source_path == *path)
                                    && compiled.source_fingerprint.is_some_and(|fingerprint| {
                                        source_fingerprint(&compiled.watched_paths)
                                            == Some(fingerprint)
                                    })
                            })
                            .cloned();
                        let compiled = match reused {
                            Some(compiled) => compiled,
                            None => match compile_contribution_entry(
                                &contribution.source_module_id,
                                module,
                                &contribution.entry,
                            ) {
                                Ok(compiled) => compiled,
                                Err(error) => {
                                    catalog.diagnostics.push(
                                        FrontendCatalogDiagnostic::contribution(
                                            &contribution.source_module_id,
                                            &contribution.contribution_id,
                                            entry_path.clone().unwrap_or_else(|| {
                                                module.path.join(&contribution.entry)
                                            }),
                                            &error,
                                        ),
                                    );
                                    continue;
                                }
                            },
                        };
                        if let Err(message) = catalog.validate_interface_imports(
                            &contribution.source_module_id,
                            &compiled,
                            graph,
                        ) {
                            let error = ShellRunError::FrontendComposition { message };
                            catalog
                                .diagnostics
                                .push(FrontendCatalogDiagnostic::contribution(
                                    &contribution.source_module_id,
                                    &contribution.contribution_id,
                                    entry_path
                                        .clone()
                                        .unwrap_or_else(|| module.path.join(&contribution.entry)),
                                    &error,
                                ));
                            continue;
                        }
                        catalog
                            .extension_point_entries
                            .insert(entry_key.clone(), compiled);
                    }
                    let props = contribution.props.clone();
                    resolved.push(ResolvedExtensionPointContribution {
                        source_module_id: contribution.source_module_id.clone(),
                        contribution_id: contribution.contribution_id.clone(),
                        order: contribution.order,
                        props_fingerprint: memo::slot_props_fingerprint(&props),
                        props,
                    });
                }
                if !resolved.is_empty() {
                    catalog
                        .extension_point_contributions
                        .insert(extension_point_key(host_module_id, point), resolved);
                }
            }
        }

        for (module_id, entry) in &catalog.modules {
            for (alias, target_module_id) in &entry.compiled.module_component_imports {
                catalog
                    .validate_component_module_import(&entry.compiled.manifest, target_module_id)
                    .map_err(|message| ShellRunError::FrontendComposition {
                        message: format!(
                            "module '{module_id}' cannot import {alias} from '{target_module_id}': {message}"
                        ),
                    })?;
            }
            for component_tag in entry.compiled.referenced_component_tags() {
                if entry.compiled.has_local_component(None, &component_tag) {
                    continue;
                }
                if entry
                    .compiled
                    .module_component_imports
                    .contains_key(&component_tag)
                {
                    continue;
                }
                return Err(ShellRunError::FrontendComposition {
                    message: format!(
                        "module '{module_id}' references <{component_tag}> but no explicit component import was compiled for that tag"
                    ),
                });
            }
            if let Some(graph) = graph {
                catalog
                    .validate_interface_imports(module_id, &entry.compiled, graph)
                    .map_err(|message| ShellRunError::FrontendComposition { message })?;
            }
        }

        catalog.diagnostics.sort_by(|left, right| {
            left.module_id
                .cmp(&right.module_id)
                .then_with(|| left.contribution_id.cmp(&right.contribution_id))
                .then_with(|| left.source_path.cmp(&right.source_path))
                .then_with(|| left.message.cmp(&right.message))
        });
        for diagnostic in &catalog.diagnostics {
            tracing::warn!(
                module_id = %diagnostic.module_id,
                contribution_id = ?diagnostic.contribution_id,
                source_path = %diagnostic.source_path.display(),
                error = %diagnostic.message,
                "frontend source indexing skipped an invalid entry"
            );
        }

        catalog.validate_node_slot_placements()?;

        Ok(catalog)
    }

    fn validate_node_slot_placements(&self) -> Result<(), ShellRunError> {
        fn find_slot<'a>(
            nodes: &'a [mesh_core_component::template::TemplateNode],
            name: &str,
        ) -> Option<&'a mesh_core_component::template::SlotNode> {
            use mesh_core_component::template::TemplateNode;
            for node in nodes {
                let found = match node {
                    TemplateNode::Slot(slot)
                        if slot.customizable && slot.name.as_deref() == Some(name) =>
                    {
                        Some(slot)
                    }
                    TemplateNode::Element(node) => find_slot(&node.children, name),
                    TemplateNode::Component(node) => find_slot(&node.children, name),
                    TemplateNode::If(node) => find_slot(&node.then_children, name)
                        .or_else(|| find_slot(&node.else_children, name)),
                    TemplateNode::For(node) => find_slot(&node.children, name),
                    _ => None,
                };
                if found.is_some() {
                    return found;
                }
            }
            None
        }

        for (root_instance, slots) in &self.node_slot_placements {
            let module_id = root_instance.split('#').next().unwrap_or(root_instance);
            let host = self.modules.get(module_id).ok_or_else(|| {
                ShellRunError::FrontendComposition {
                    message: format!(
                        "unknown_node_slot: root instance '{root_instance}' has no compiled module"
                    ),
                }
            })?;
            let roots = host
                .compiled
                .component
                .template
                .as_ref()
                .map(|template| template.root.as_slice())
                .unwrap_or_default();
            for (slot_name, over) in slots {
                let slot = find_slot(roots, slot_name).ok_or_else(|| {
                    ShellRunError::FrontendComposition {
                        message: format!(
                            "unknown_node_slot: '{root_instance}' has no customizable slot '{slot_name}'"
                        ),
                    }
                })?;
                let point = slot.extension_point.as_deref().ok_or_else(|| {
                    ShellRunError::FrontendComposition {
                        message: format!(
                            "node_slot_not_customizable: slot '{slot_name}' has no contract"
                        ),
                    }
                })?;
                let compatible = self.extension_point_contributions_for(module_id, point);
                if let Some(max) = host
                    .compiled
                    .manifest
                    .hosted_extension_points
                    .get(point)
                    .and_then(|hosted| hosted.max)
                    && over.nodes.len() > max as usize
                {
                    return Err(ShellRunError::FrontendComposition {
                        message: format!(
                            "node_slot_cardinality: slot '{slot_name}' accepts at most {max} nodes"
                        ),
                    });
                }
                for node in &over.nodes {
                    let Some((source, contribution_id)) = node.contribution.rsplit_once(':') else {
                        return Err(ShellRunError::FrontendComposition {
                            message: format!(
                                "node_contribution_incompatible: invalid reference '{}'",
                                node.contribution
                            ),
                        });
                    };
                    let contribution = compatible.iter().find(|entry| {
                        entry.source_module_id == source && entry.contribution_id == contribution_id
                    });
                    let Some(contribution) = contribution else {
                        return Err(ShellRunError::FrontendComposition {
                            message: format!(
                                "node_contribution_incompatible: '{}' does not satisfy '{point}'",
                                node.contribution
                            ),
                        });
                    };
                    let compiled = self
                        .contribution_entry(
                            &contribution.source_module_id,
                            &contribution.contribution_id,
                        )
                        .expect("resolved contributions are compiled");
                    validate_placement_props(&node.contribution, &node.props, compiled)?;
                }
            }
        }
        Ok(())
    }

    pub(in crate::shell) fn extension_point_contributions_for(
        &self,
        host_module_id: &str,
        point: &str,
    ) -> &[ResolvedExtensionPointContribution] {
        self.extension_point_contributions
            .get(&extension_point_key(host_module_id, point))
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    /// Compiled contribution roots belonging to `module_id`.
    ///
    /// A contribution's own local components and component imports live in its
    /// own compilation, not in the module's primary entry, so alias resolution
    /// inside a contribution must look here too.
    pub(super) fn contribution_entries_for(
        &self,
        module_id: &str,
    ) -> impl Iterator<Item = &SharedCompiledFrontendModule> {
        let prefix = format!("{module_id}\u{1}");
        self.extension_point_entries
            .iter()
            .filter(move |(key, _)| key.starts_with(&prefix))
            .map(|(_, compiled)| compiled)
    }

    /// Compiled contribution roots rendered by one host module. Contributions
    /// may come from a different module than the host, so this is distinct
    /// from [`Self::contribution_entries_for`], which indexes roots by their
    /// source module for alias resolution.
    pub(super) fn contribution_entries_for_host(
        &self,
        host_module_id: &str,
    ) -> Vec<&SharedCompiledFrontendModule> {
        let prefix = format!("{host_module_id}\u{1}");
        let mut seen = std::collections::HashSet::new();
        let mut entries = Vec::new();
        let mut point_keys: Vec<_> = self
            .extension_point_contributions
            .keys()
            .filter(|point_key| point_key.starts_with(&prefix))
            .collect();
        point_keys.sort_unstable();
        for point_key in point_keys {
            let contributions = &self.extension_point_contributions[point_key];
            for contribution in contributions {
                let key = contribution_entry_key(
                    &contribution.source_module_id,
                    &contribution.contribution_id,
                );
                if seen.insert(key.clone())
                    && let Some(compiled) = self.extension_point_entries.get(&key)
                {
                    entries.push(compiled);
                }
            }
        }
        entries
    }

    pub(in crate::shell) fn contribution_entry(
        &self,
        source_module_id: &str,
        contribution_id: &str,
    ) -> Option<&SharedCompiledFrontendModule> {
        self.extension_point_entries
            .get(&contribution_entry_key(source_module_id, contribution_id))
    }

    pub(in crate::shell) fn node_slot_placement(
        &self,
        root_instance: &str,
        slot_name: &str,
    ) -> Option<&NodeSlotOverride> {
        self.node_slot_placements
            .get(root_instance)
            .and_then(|slots| slots.get(slot_name))
    }

    pub(in crate::shell) fn top_level_surfaces(&self) -> Vec<FrontendCatalogEntry> {
        let mut entries: Vec<FrontendCatalogEntry> = self
            .modules
            .values()
            .filter(|entry| entry.compiled.manifest.package.module_type == ModuleType::Surface)
            .cloned()
            .collect();
        entries.sort_by(|left, right| {
            left.compiled
                .manifest
                .package
                .id
                .cmp(&right.compiled.manifest.package.id)
        });
        entries
    }

    pub(in crate::shell) fn top_level_surfaces_filtered(
        &self,
        enabled_frontends: Option<&std::collections::HashSet<String>>,
    ) -> Vec<FrontendCatalogEntry> {
        let mut entries = self.top_level_surfaces();
        if let Some(enabled_frontends) = enabled_frontends {
            entries.retain(|entry| enabled_frontends.contains(&entry.compiled.manifest.package.id));
        }
        entries
    }

    fn validate_component_module_import(
        &self,
        host: &mesh_core_module::Manifest,
        module_id: &str,
    ) -> Result<(), String> {
        if !host
            .required_module_dependencies()
            .iter()
            .any(|dependency_id| dependency_id == module_id)
        {
            return Err(format!(
                "target module '{module_id}' is not declared in mesh.uses.modules as a required module dependency"
            ));
        }
        let Some(entry) = self.modules.get(module_id) else {
            return Err("target module is not loaded".into());
        };
        match entry.compiled.manifest.package.module_type {
            ModuleType::Widget | ModuleType::Surface | ModuleType::Component => Ok(()),
            other => Err(format!(
                "target module must be a frontend widget, component, or surface, got {other}"
            )),
        }
    }

    fn validate_interface_imports(
        &self,
        module_id: &str,
        compiled: &CompiledFrontendModule,
        graph: &InstalledModuleGraph,
    ) -> Result<(), String> {
        let Some(requirements) = graph.requirements_for_frontend(module_id) else {
            return Ok(());
        };
        for (interface, requested_version) in compiled_interface_imports(compiled) {
            let required = requirements.backend.contains_key(&interface);
            let optional = requirements.optional_backend.contains_key(&interface);
            if !required && !optional {
                return Err(format!(
                    "module '{module_id}' imports interface '{interface}' but does not declare it in mesh.uses.interfaces or mesh.uses.optionalInterfaces"
                ));
            }
            let requested_range = requested_version
                .as_deref()
                .map(|version| {
                    mesh_core_service::parse_version_req(version).ok_or_else(|| {
                        format!(
                            "module '{module_id}' imports interface '{interface}' with invalid version range '{version}'"
                        )
                    })
                })
                .transpose()
                ?;

            let graph_knows_interface = graph.declared_interface(&interface).is_some()
                || !graph.backend_providers_for_interface(&interface).is_empty();
            if !graph_knows_interface {
                // Built-in interfaces are registered by the shell after the
                // installed graph is built. The graph cannot validate their
                // provider/version tuple yet, so leave those checks to the
                // runtime registry while still enforcing the manifest edge.
                continue;
            }

            let Some(provider) = graph.active_provider(&interface) else {
                // The installed graph keeps frontends with no provider active
                // so the runtime can expose an unavailable interface health
                // state. Optional imports also intentionally degrade here.
                continue;
            };

            let Some(request) = requested_range else {
                continue;
            };
            let contract_version = graph
                .interface_contract(&interface)
                .map(|contract| contract.version.clone());
            let provider_version = provider
                .version
                .as_deref()
                .and_then(mesh_core_service::parse_contract_version);
            let compatible = contract_version
                .as_ref()
                .is_none_or(|version| request.matches(version))
                && provider_version
                    .as_ref()
                    .is_none_or(|version| request.matches(version));
            if !compatible && required {
                return Err(format!(
                    "module '{module_id}' imports interface '{interface}@{}', but the available contract/provider versions are incompatible",
                    requested_version.as_deref().unwrap_or_default()
                ));
            }
        }

        Ok(())
    }

    pub(super) fn imported_component_module_id(
        &self,
        host: &mesh_core_module::Manifest,
        owner_source_path: Option<&std::path::Path>,
        alias: &str,
    ) -> Result<String, String> {
        let Some(entry) = self.modules.get(&host.package.id) else {
            return Err("host module is not loaded".into());
        };
        let module_id = entry
            .compiled
            .component_module_for(owner_source_path, alias)
            .or_else(|| {
                self.contribution_entries_for(&host.package.id)
                    .find_map(|compiled| compiled.component_module_for(owner_source_path, alias))
            });
        let Some(module_id) = module_id else {
            return Err(format!(
                "no explicit component import for alias '{alias}'; add a script import such as local {alias} = require(\"@scope/module\")"
            ));
        };
        self.validate_component_module_import(host, &module_id)?;
        Ok(module_id)
    }
}

#[cfg(test)]
mod performance_tests {
    use super::*;
    use mesh_core_module::lifecycle::ModuleInstance;

    fn shipped_frontend_modules() -> HashMap<String, ModuleInstance> {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
        std::fs::read_dir(root.join("modules/frontend"))
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
            .filter_map(|entry| {
                let module_dir = entry.path();
                let loaded =
                    mesh_core_module::manifest::load_canonical_manifest(&module_dir).ok()?;
                let module_id = loaded.manifest.package.id.clone();
                Some((
                    module_id,
                    ModuleInstance::new(loaded.manifest, module_dir, loaded.path, loaded.source),
                ))
            })
            .collect()
    }

    fn compile_sequentially(
        modules: &HashMap<String, ModuleInstance>,
    ) -> Result<Vec<(String, FrontendCatalogEntry)>, ShellRunError> {
        let mut module_ids: Vec<_> = modules.keys().cloned().collect();
        module_ids.sort();
        module_ids
            .into_iter()
            .filter_map(|module_id| {
                let module = modules.get(&module_id)?;
                mesh_core_frontend::is_frontend_module(&module.manifest)
                    .then_some((module_id, module))
            })
            .map(|(module_id, module)| {
                compile_catalog_entry(&module_id, module).map(|entry| (module_id, entry))
            })
            .collect()
    }

    #[test]
    fn parallel_catalog_compilation_matches_sequential_module_set() {
        let modules = shipped_frontend_modules();
        let sequential: std::collections::HashSet<_> = compile_sequentially(&modules)
            .unwrap()
            .into_iter()
            .map(|(id, _)| id)
            .collect();
        let parallel: std::collections::HashSet<_> = FrontendCatalog::from_modules(&modules, None)
            .unwrap()
            .modules
            .into_keys()
            .collect();
        assert_eq!(parallel, sequential);
    }

    #[test]
    fn top_level_surface_entries_share_the_compiled_module() {
        let modules = shipped_frontend_modules();
        let catalog = FrontendCatalog::from_modules(&modules, None).unwrap();
        let surfaces = catalog.top_level_surfaces();
        let surface = surfaces.first().expect("shipped catalog has a surface");
        let module_id = &surface.compiled.manifest.package.id;
        let catalog_entry = catalog.modules.get(module_id).unwrap();

        assert!(std::ptr::eq::<CompiledFrontendModule>(
            &*surface.compiled,
            &*catalog_entry.compiled,
        ));
    }

    #[cfg(unix)]
    #[test]
    fn watched_source_fingerprint_does_not_follow_symlinks() {
        let temp = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let target = outside.path().join("outside.mesh");
        let link = temp.path().join("watched.mesh");
        std::fs::write(&target, "outside").unwrap();
        std::os::unix::fs::symlink(&target, &link).unwrap();

        assert_eq!(source_fingerprint(&[link]), None);
    }

    /// The Stage 1 gate at the catalog boundary: a module's contributed page is
    /// compiled and mounted with no module id hardcoded in the shell.
    #[test]
    fn contributed_settings_pages_mount_through_the_extension_point() {
        let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
        let graph = mesh_core_module::package::load_installed_module_graph(
            &workspace_root.join("config/module.json"),
        )
        .expect("shipped graph loads");
        let modules = shipped_frontend_modules();
        let catalog = FrontendCatalog::from_modules(&modules, Some(&graph)).unwrap();

        let contributions =
            catalog.extension_point_contributions_for("@mesh/settings", "mesh.settings.page");
        assert!(
            contributions.iter().any(|contribution| {
                contribution.source_module_id == "@mesh/navigation-bar"
                    && contribution.contribution_id == "navigation-bar"
            }),
            "the settings host should receive the contributed page"
        );

        let compiled = catalog
            .contribution_entry("@mesh/navigation-bar", "navigation-bar")
            .expect("the contribution compiles as its own root");
        assert_eq!(
            compiled.source_path,
            workspace_root.join("modules/frontend/navigation-bar/src/settings.mesh")
        );
    }

    fn catalog_with_navigation_contribution_source(
        settings_source: &str,
    ) -> (tempfile::TempDir, FrontendCatalog) {
        let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
        let graph = mesh_core_module::package::load_installed_module_graph(
            &workspace_root.join("config/module.json"),
        )
        .expect("shipped graph loads");
        let mut modules = shipped_frontend_modules();
        let navigation = modules
            .remove("@mesh/navigation-bar")
            .expect("shipped navigation module");
        let temp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(temp.path().join("src")).unwrap();
        std::fs::write(
            temp.path().join("src/main.mesh"),
            "<template><box /></template>",
        )
        .unwrap();
        for entries in graph.all_extension_point_contributions().values() {
            for entry in entries {
                if entry.source_module_id != "@mesh/navigation-bar" {
                    continue;
                }
                let path = temp.path().join(&entry.entry);
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent).unwrap();
                }
                let source = if entry.entry == "src/settings.mesh" {
                    settings_source
                } else {
                    "<template><text /></template>"
                };
                std::fs::write(path, source).unwrap();
            }
        }
        modules.insert(
            "@mesh/navigation-bar".into(),
            ModuleInstance::new(
                navigation.manifest,
                temp.path().to_path_buf(),
                temp.path().join("module.json"),
                navigation.manifest_source,
            ),
        );

        let catalog = FrontendCatalog::from_modules(&modules, Some(&graph)).unwrap();
        (temp, catalog)
    }

    #[test]
    fn contribution_roots_must_declare_imported_interfaces() {
        let (_temp, catalog) = catalog_with_navigation_contribution_source(
            r#"
<template><text /></template>
<script lang="luau">
local unknown = require("mesh.unknown")
</script>
"#,
        );

        assert!(
            catalog
                .contribution_entry("@mesh/navigation-bar", "navigation-bar")
                .is_none()
        );
        assert!(catalog.diagnostics().iter().any(|diagnostic| {
            diagnostic.module_id == "@mesh/navigation-bar"
                && diagnostic.contribution_id.as_deref() == Some("navigation-bar")
                && diagnostic.message.contains("does not declare it")
        }));
    }

    #[test]
    fn contribution_roots_reject_incompatible_required_interface_ranges() {
        let (_temp, catalog) = catalog_with_navigation_contribution_source(
            r#"
<template><text /></template>
<script lang="luau">
local audio = require("mesh.audio@>=2.0")
</script>
"#,
        );

        assert!(
            catalog
                .contribution_entry("@mesh/navigation-bar", "navigation-bar")
                .is_none()
        );
        assert!(catalog.diagnostics().iter().any(|diagnostic| {
            diagnostic.module_id == "@mesh/navigation-bar"
                && diagnostic.contribution_id.as_deref() == Some("navigation-bar")
                && diagnostic.message.contains("versions are incompatible")
        }));
    }

    #[test]
    fn graph_only_catalog_rebuild_reuses_unchanged_compilations() {
        let modules = shipped_frontend_modules();
        let initial = FrontendCatalog::from_modules(&modules, None).unwrap();
        let rebuilt =
            FrontendCatalog::from_modules_reusing(&modules, None, Some(&initial)).unwrap();

        assert_eq!(initial.modules.len(), rebuilt.modules.len());
        for (module_id, initial_entry) in &initial.modules {
            let rebuilt_entry = rebuilt.modules.get(module_id).unwrap();
            assert!(std::ptr::eq::<CompiledFrontendModule>(
                &*initial_entry.compiled,
                &*rebuilt_entry.compiled,
            ));
        }
    }

    #[test]
    fn invalid_source_is_scoped_to_one_catalog_entry() {
        let mut modules = shipped_frontend_modules();
        let temp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(temp.path().join("src")).unwrap();

        let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
        let manifest = std::fs::read_to_string(
            workspace_root.join("modules/frontend/debug-inspector/module.json"),
        )
        .unwrap()
        .replace("@mesh/debug-inspector", "@test/broken");
        std::fs::write(temp.path().join("module.json"), manifest).unwrap();
        std::fs::write(temp.path().join("src/main.mesh"), "<template><").unwrap();

        let loaded = mesh_core_module::manifest::load_canonical_manifest(temp.path()).unwrap();
        let module_id = loaded.manifest.package.id.clone();
        modules.insert(
            module_id.clone(),
            ModuleInstance::new(
                loaded.manifest,
                temp.path().to_path_buf(),
                loaded.path,
                loaded.source,
            ),
        );

        let catalog = FrontendCatalog::from_modules(&modules, None).unwrap();

        assert!(catalog.module("@mesh/debug-inspector").is_some());
        assert!(catalog.module(&module_id).is_none());
        assert!(catalog.diagnostics().iter().any(|diagnostic| {
            diagnostic.module_id == module_id
                && diagnostic.contribution_id.is_none()
                && diagnostic.source_path == temp.path()
                && diagnostic
                    .message
                    .contains("failed to compile frontend module")
        }));
    }

    // cargo test -p mesh-core-shell --release --lib shared_compiled_handle_clone_beats_deep_clone -- --ignored --nocapture
    #[test]
    #[ignore = "release-only compiled frontend ownership benchmark"]
    fn shared_compiled_handle_clone_beats_deep_clone() {
        use std::hint::black_box;
        use std::time::Instant;

        let modules = shipped_frontend_modules();
        let catalog = FrontendCatalog::from_modules(&modules, None).unwrap();
        let surface = catalog
            .top_level_surfaces()
            .into_iter()
            .next()
            .expect("shipped catalog has a surface");
        let iterations = 1_000;

        let started = Instant::now();
        for _ in 0..iterations {
            black_box((*surface.compiled).clone());
        }
        let deep_clone = started.elapsed();

        let started = Instant::now();
        for _ in 0..iterations {
            black_box(surface.compiled.clone());
        }
        let shared_clone = started.elapsed();

        eprintln!(
            "compiled frontend clone over {iterations} iterations: deep {deep_clone:?}, shared {shared_clone:?}"
        );
        assert!(shared_clone < deep_clone);
    }

    #[test]
    #[ignore = "release-only frontend compilation benchmark"]
    fn parallel_frontend_compilation_beats_sequential_startup() {
        use std::hint::black_box;
        use std::time::Instant;

        let modules = shipped_frontend_modules();
        let iterations = 20;

        let started = Instant::now();
        for _ in 0..iterations {
            black_box(compile_sequentially(&modules).unwrap());
        }
        let sequential = started.elapsed();

        let started = Instant::now();
        for _ in 0..iterations {
            black_box(FrontendCatalog::from_modules(&modules, None).unwrap());
        }
        let parallel = started.elapsed();

        eprintln!(
            "frontend compilation over {iterations} shipped-catalog builds: sequential {sequential:?}, parallel {parallel:?}"
        );
    }
}

fn compiled_interface_imports(compiled: &CompiledFrontendModule) -> Vec<(String, Option<String>)> {
    let mut imports = std::collections::BTreeSet::new();
    compiled
        .all_local_components()
        .into_iter()
        .chain(std::iter::once(&compiled.component))
        .flat_map(|component| {
            component
                .imports
                .iter()
                .filter_map(|import| match &import.target {
                    mesh_core_component::ComponentImportTarget::InterfaceApi {
                        interface,
                        version,
                    } => Some((
                        mesh_core_service::canonical_interface_name(interface),
                        version.clone(),
                    )),
                    _ => None,
                })
        })
        .for_each(|import| {
            imports.insert(import);
        });
    imports.into_iter().collect()
}
