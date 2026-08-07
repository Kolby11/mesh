use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::ops::{Deref, DerefMut};
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use mesh_core_frontend::{
    CompileFrontendError, CompiledFrontendModule, compile_frontend_entrypoint,
    compile_frontend_module,
};
use mesh_core_module::ModuleType;
use mesh_core_module::lifecycle::ModuleInstance;
use mesh_core_module::package::InstalledModuleGraph;
use rayon::prelude::*;

use super::memo;
use crate::shell::ShellRunError;

/// Compile a module's primary entry plus every extension point contribution it
/// declares.
///
/// A contribution is a module-owned component with the same import and
/// authoring rules as `main`, so it is compiled eagerly — an invalid page must
/// not lurk until a host happens to mount it. Its dependency paths join the
/// module's watch set, so editing one triggers catalog validation and reload.
fn compile_module_entrypoints(
    manifest: &mesh_core_module::Manifest,
    module_dir: &std::path::Path,
) -> Result<CompiledFrontendModule, CompileFrontendError> {
    let mut compiled = compile_frontend_module(manifest, module_dir)?;
    for contributions in manifest.extension_point_contributions.values() {
        for contribution in contributions {
            let contributed =
                compile_frontend_entrypoint(manifest, module_dir, &contribution.entry)?;
            for path in contributed.watched_paths {
                if !compiled.watched_paths.contains(&path) {
                    compiled.watched_paths.push(path);
                }
            }
        }
    }
    Ok(compiled)
}

#[derive(Debug, Clone, Default)]
pub(in crate::shell) struct FrontendCatalog {
    pub(super) modules: HashMap<String, FrontendCatalogEntry>,
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
}

/// Index key for the contributions one host renders at one extension point.
pub(super) fn extension_point_key(host_module_id: &str, point: &str) -> String {
    format!("{host_module_id}\u{1}{point}")
}

/// Index key for one contribution's compiled component.
pub(super) fn contribution_entry_key(source_module_id: &str, contribution_id: &str) -> String {
    format!("{source_module_id}\u{1}{contribution_id}")
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
        std::fs::read(path).ok()?.hash(&mut hasher);
    }
    Some(hasher.finish())
}

fn compile_catalog_entry(
    module_id: &str,
    module: &ModuleInstance,
) -> Result<FrontendCatalogEntry, ShellRunError> {
    let compiled =
        compile_module_entrypoints(&module.manifest, &module.path).map_err(|source| {
            ShellRunError::FrontendCompile {
                module_id: module_id.to_string(),
                source,
            }
        })?;
    Ok(FrontendCatalogEntry {
        module_dir: module.path.clone(),
        compiled: compiled.into(),
    })
}

/// Compile one extension point contribution as an alternate root of its
/// contributing module.
fn compile_contribution_entry(
    module_id: &str,
    module: &ModuleInstance,
    entry: &str,
) -> Result<SharedCompiledFrontendModule, ShellRunError> {
    let compiled =
        compile_frontend_entrypoint(&module.manifest, &module.path, entry).map_err(|source| {
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
pub(super) struct ResolvedExtensionPointContribution {
    pub(super) source_module_id: String,
    pub(super) contribution_id: String,
    pub(super) order: i64,
    pub(super) props_fingerprint: u64,
    pub(super) props: serde_json::Map<String, serde_json::Value>,
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

    pub(in crate::shell) fn update_compiled_module(
        &self,
        module_id: &str,
        compiled: SharedCompiledFrontendModule,
    ) {
        let mut catalog = (*self.snapshot().catalog).clone();
        if let Some(entry) = catalog.modules.get_mut(module_id) {
            entry.compiled = compiled;
        }
        self.replace(catalog, Some(module_id));
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

    // Walk the reverse composition graph. A module is affected when it imports
    // an affected component, or when one of the extension points it hosts
    // changed.
    let mut affected = changed.clone();
    loop {
        let mut discovered = Vec::new();
        for catalog in [previous, next] {
            for (module_id, entry) in &catalog.modules {
                if affected.contains(module_id) {
                    continue;
                }
                let imports_affected = entry
                    .compiled
                    .module_component_imports
                    .values()
                    .any(|dependency| affected.contains(dependency));
                let hosted_point_affected = entry
                    .compiled
                    .manifest
                    .hosted_extension_points
                    .keys()
                    .any(|point| {
                        changed_extension_points.contains(&extension_point_key(module_id, point))
                    });
                if imports_affected || hosted_point_affected {
                    discovered.push(module_id.clone());
                }
            }
        }
        if discovered.is_empty() {
            break;
        }
        affected.extend(discovered);
    }

    (changed, affected)
}

impl FrontendCatalog {
    pub(in crate::shell) fn module(&self, module_id: &str) -> Option<&FrontendCatalogEntry> {
        self.modules.get(module_id)
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
                mesh_core_frontend::is_frontend_module(&module.manifest)
                    .then_some((module_id, module))
            })
            .collect();
        let compiled_entries = frontend_modules
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
                    return Ok(((*module_id).clone(), entry.clone()));
                }

                compile_catalog_entry(module_id, module).map(|entry| ((*module_id).clone(), entry))
            })
            .collect::<Result<Vec<_>, _>>()?;

        let mut catalog = Self {
            modules: compiled_entries.into_iter().collect(),
            extension_point_contributions: HashMap::new(),
            extension_point_entries: HashMap::new(),
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
                    let entry_key = contribution_entry_key(
                        &contribution.source_module_id,
                        &contribution.contribution_id,
                    );
                    if !catalog.extension_point_entries.contains_key(&entry_key) {
                        // Reuse an unchanged compilation across graph-only
                        // rebuilds, on the same terms as the primary entry.
                        let reused = previous
                            .and_then(|catalog| catalog.extension_point_entries.get(&entry_key))
                            .filter(|compiled| {
                                compiled.source_path == module.path.join(&contribution.entry)
                                    && compiled.source_fingerprint.is_some_and(|fingerprint| {
                                        source_fingerprint(&compiled.watched_paths)
                                            == Some(fingerprint)
                                    })
                            })
                            .cloned();
                        let compiled = match reused {
                            Some(compiled) => compiled,
                            None => compile_contribution_entry(
                                &contribution.source_module_id,
                                module,
                                &contribution.entry,
                            )?,
                        };
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
                if entry.compiled.local_components.contains_key(&component_tag) {
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

        Ok(catalog)
    }

    pub(super) fn extension_point_contributions_for(
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

    pub(super) fn contribution_entry(
        &self,
        source_module_id: &str,
        contribution_id: &str,
    ) -> Option<&SharedCompiledFrontendModule> {
        self.extension_point_entries
            .get(&contribution_entry_key(source_module_id, contribution_id))
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
        let declared = requirements
            .backend
            .keys()
            .chain(requirements.optional_backend.keys())
            .collect::<std::collections::HashSet<_>>();

        for interface in compiled_interface_imports(compiled) {
            if !declared.contains(&interface) {
                return Err(format!(
                    "module '{module_id}' imports interface '{interface}' but does not declare it in mesh.uses.interfaces or mesh.uses.optionalInterfaces"
                ));
            }
        }

        Ok(())
    }

    pub(super) fn imported_component_module_id(
        &self,
        host: &mesh_core_module::Manifest,
        alias: &str,
    ) -> Result<String, String> {
        let Some(entry) = self.modules.get(&host.package.id) else {
            return Err("host module is not loaded".into());
        };
        let module_id = entry
            .compiled
            .module_component_imports
            .get(alias)
            .or_else(|| {
                self.contribution_entries_for(&host.package.id)
                    .find_map(|compiled| compiled.module_component_imports.get(alias))
            });
        let Some(module_id) = module_id else {
            return Err(format!(
                "no explicit component import for alias '{alias}'; add a script import such as local {alias} = require(\"@scope/module\")"
            ));
        };
        self.validate_component_module_import(host, module_id)?;
        Ok(module_id.clone())
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
                let loaded = mesh_core_module::manifest::load_manifest(&module_dir).ok()?;
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

fn compiled_interface_imports(
    compiled: &CompiledFrontendModule,
) -> std::collections::HashSet<String> {
    compiled
        .local_components
        .values()
        .chain(std::iter::once(&compiled.component))
        .flat_map(|component| {
            component
                .imports
                .iter()
                .filter_map(|import| match &import.target {
                    mesh_core_component::ComponentImportTarget::InterfaceApi {
                        interface, ..
                    } => Some(interface.clone()),
                    _ => None,
                })
        })
        .collect()
}
