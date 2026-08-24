use super::component::{FrontendCatalog, FrontendCatalogHandle, FrontendSurfaceComponent};
use super::*;
use mesh_core_module::ModuleHealthRecord;
use rayon::prelude::*;
use std::collections::{BTreeMap, HashMap, HashSet};

const BUILTIN_DEBUG_INSPECTOR_ID: &str = "@mesh/debug-inspector";

pub(in crate::shell) fn installed_module_graph_path() -> PathBuf {
    if let Ok(path) = std::env::var("MESH_MODULE_GRAPH_PATH")
        && !path.trim().is_empty()
    {
        return PathBuf::from(path);
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .join("config/module.json")
}

fn load_installed_module_graph_candidate(
    root_module_graph_path: &Path,
) -> Result<InstalledModuleGraph, mesh_core_module::package::ModuleManifestError> {
    load_installed_module_graph(root_module_graph_path)
}

pub(in crate::shell) fn graph_i18n_catalog_sources(
    graph: &InstalledModuleGraph,
) -> Result<
    (
        Vec<mesh_core_locale::CatalogSource>,
        HashMap<String, String>,
    ),
    String,
> {
    graph.locale_catalog_sources()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::shell) struct ResourceAsset {
    pub module_id: String,
    pub id: String,
    pub handle: mesh_core_resources::ResourceAssetHandle,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(in crate::shell) struct ResourceSnapshot {
    pub revision: u64,
    pub icon_pack_chain: Vec<String>,
    pub font_pack_chain: Vec<String>,
    pub icon_assets: Vec<ResourceAsset>,
    pub font_assets: Vec<ResourceAsset>,
}

pub(in crate::shell) struct PreparedResourceSnapshot {
    pub(in crate::shell) generation: u64,
    pub(in crate::shell) resource_lease: Option<mesh_core_resources::ResourcePreparationLease>,
    pub snapshot: ResourceSnapshot,
    pub explanation: mesh_core_resources::ResourceExplanationSnapshot,
    pub icon_packs: Vec<mesh_core_icon::IconPackBindings>,
    pub font_registry: mesh_core_resources::FontRegistry,
    pub frontends: Vec<(String, mesh_core_icon::FrontendIconBindings)>,
}

fn resource_asset_explanation(
    id: impl Into<String>,
    path: &std::path::Path,
    prepared: bool,
) -> mesh_core_resources::ResourceAssetExplanation {
    resource_asset_explanation_with_fingerprint(
        id,
        path,
        mesh_core_resources::resource_fingerprint(path),
        prepared,
    )
}

fn resource_asset_explanation_with_fingerprint(
    id: impl Into<String>,
    path: &std::path::Path,
    fingerprint: Option<mesh_core_resources::ResourceFingerprint>,
    prepared: bool,
) -> mesh_core_resources::ResourceAssetExplanation {
    mesh_core_resources::ResourceAssetExplanation {
        id: id.into(),
        path: path.display().to_string(),
        fingerprint,
        prepared,
    }
}

fn resource_explanation_snapshot(
    revision: u64,
    host_catalog: &mesh_core_resources::SystemResourceCatalog,
    icon_chain: &[String],
    font_registry: &mesh_core_resources::FontRegistry,
    icon_packs: &[mesh_core_icon::IconPackBindings],
    frontends: &[(String, mesh_core_icon::FrontendIconBindings)],
    icon_requirements: &[(String, String, bool)],
    icon_assets: &[ResourceAsset],
    font_assets: &[ResourceAsset],
    shell_default_icon_pack: Option<&str>,
) -> mesh_core_resources::ResourceExplanationSnapshot {
    let mut explanation =
        mesh_core_resources::ResourceExplanationSnapshot::from_catalog(host_catalog);
    explanation.revision = revision;

    explanation
        .icons
        .available
        .extend(icon_chain.iter().cloned());
    explanation.icons.available.sort();
    explanation.icons.available.dedup();
    explanation.icons.contributions = icon_assets
        .iter()
        .map(|asset| resource_asset_explanation(&asset.id, &asset.handle.candidate_path(), true))
        .collect();
    explanation.icons.chain = icon_chain
        .iter()
        .enumerate()
        .filter_map(|(chain_position, module_id)| {
            let pack = icon_packs
                .iter()
                .find(|pack| &pack.module_id == module_id)?;
            let mut mappings = pack
                .mappings
                .iter()
                .map(
                    |(semantic_name, mapping)| mesh_core_resources::ResourceMappingExplanation {
                        semantic_name: semantic_name.clone(),
                        target: mapping.target.clone(),
                        multicolor: mapping.multicolor,
                        owner_module: pack.module_id.clone(),
                        fallback_stage: "pack-chain".into(),
                    },
                )
                .collect::<Vec<_>>();
            mappings.sort_by(|left, right| left.semantic_name.cmp(&right.semantic_name));

            let mut assets = Vec::new();
            for (alias, font) in &pack.font_aliases {
                if let Some(path) = font.resolved_font_path.as_ref() {
                    assets.push(resource_asset_explanation_with_fingerprint(
                        format!("font:{alias}"),
                        path,
                        font.font_fingerprint,
                        font.prepared_font.is_some(),
                    ));
                }
                if let Some(path) = font.glyph_map_path.as_ref() {
                    assets.push(resource_asset_explanation(
                        format!("glyph-map:{alias}"),
                        path,
                        font.prepared_glyphs.is_some(),
                    ));
                }
            }
            assets.sort_by(|left, right| left.id.cmp(&right.id));
            Some(mesh_core_resources::ResourcePackExplanation {
                module_id: pack.module_id.clone(),
                pack_id: pack.pack_id.clone(),
                chain_position,
                status: "selected".into(),
                assets,
                mappings,
                script_coverage: Vec::new(),
            })
        })
        .collect();

    let font_pack_bindings = font_registry.pack_bindings();
    let effective_font_chain = font_registry.effective_pack_chain();
    explanation.fonts.available = font_pack_bindings
        .iter()
        .flat_map(|pack| [pack.module_id.clone(), pack.pack_id.clone()])
        .collect();
    explanation.fonts.available.sort();
    explanation.fonts.available.dedup();
    explanation.fonts.contributions = font_assets
        .iter()
        .map(|asset| resource_asset_explanation(&asset.id, &asset.handle.candidate_path(), true))
        .collect();
    explanation.fonts.chain = effective_font_chain
        .iter()
        .enumerate()
        .filter_map(|(chain_position, pack_id)| {
            let pack = font_pack_bindings
                .iter()
                .find(|pack| &pack.pack_id == pack_id)?;
            let mut mappings = pack
                .mappings
                .iter()
                .map(
                    |(semantic_name, family)| mesh_core_resources::ResourceMappingExplanation {
                        semantic_name: semantic_name.clone(),
                        target: family.clone(),
                        multicolor: false,
                        owner_module: pack.module_id.clone(),
                        fallback_stage: "font-chain".into(),
                    },
                )
                .collect::<Vec<_>>();
            mappings.sort_by(|left, right| left.semantic_name.cmp(&right.semantic_name));
            let mut script_coverage = pack.covers.keys().cloned().collect::<Vec<_>>();
            script_coverage.extend(
                pack.faces
                    .iter()
                    .flat_map(|face| face.coverage.iter().cloned()),
            );
            script_coverage.sort();
            script_coverage.dedup();
            let mut assets = pack
                .faces
                .iter()
                .map(|face| {
                    resource_asset_explanation(
                        format!("face:{}", face.family),
                        &face.asset.candidate_path(),
                        true,
                    )
                })
                .collect::<Vec<_>>();
            assets.sort_by(|left, right| left.id.cmp(&right.id));
            Some(mesh_core_resources::ResourcePackExplanation {
                module_id: pack.module_id.clone(),
                pack_id: pack.pack_id.clone(),
                chain_position,
                status: "selected".into(),
                assets,
                mappings,
                script_coverage,
            })
        })
        .collect();

    let icon_frontend_chains = frontends
        .iter()
        .map(|(module_id, bindings)| {
            (
                module_id.clone(),
                bindings.effective_chain(shell_default_icon_pack),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let font_frontend_chains = font_registry
        .frontend_effective_pack_chains()
        .into_iter()
        .collect::<BTreeMap<_, _>>();
    let mut frontend_ids = icon_frontend_chains
        .keys()
        .chain(font_frontend_chains.keys())
        .cloned()
        .collect::<Vec<_>>();
    frontend_ids.sort();
    frontend_ids.dedup();
    explanation.frontends = frontend_ids
        .into_iter()
        .map(
            |module_id| mesh_core_resources::ResourceFrontendExplanation {
                icon_chain: icon_frontend_chains
                    .get(&module_id)
                    .cloned()
                    .unwrap_or_default(),
                font_chain: font_frontend_chains
                    .get(&module_id)
                    .cloned()
                    .unwrap_or_default(),
                module_id,
            },
        )
        .collect();

    if let Ok(config) = mesh_core_icon::IconConfig::builtin_xdg()
        && let Ok(mut registry) = mesh_core_icon::IconRegistry::from_config(config)
    {
        for pack in icon_packs {
            registry.set_icon_pack(pack.clone());
        }
        for (module_id, bindings) in frontends {
            registry.set_frontend_bindings(module_id.clone(), bindings.clone());
        }
        registry.set_shell_default_pack(shell_default_icon_pack.map(str::to_owned));
        for (module_id, semantic_name, required) in icon_requirements {
            let resolution = registry.resolve_for_module(module_id, semantic_name, 24);
            let resolution_explanation = match resolution {
                mesh_core_icon::IconResolution::Found {
                    provenance, target, ..
                } => {
                    let asset = match target {
                        mesh_core_icon::ResolvedTarget::File(path) => {
                            Some(resource_asset_explanation("resolved-icon", &path, true))
                        }
                        mesh_core_icon::ResolvedTarget::Glyph {
                            font_path,
                            font_fingerprint,
                            ..
                        } => Some(resource_asset_explanation_with_fingerprint(
                            "resolved-glyph",
                            &font_path,
                            font_fingerprint,
                            true,
                        )),
                    };
                    mesh_core_resources::ResourceResolutionExplanation {
                        module_id: module_id.clone(),
                        semantic_name: semantic_name.clone(),
                        required: *required,
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
                        module_id: module_id.clone(),
                        semantic_name: semantic_name.clone(),
                        required: *required,
                        status: "missing".into(),
                        owner_module: None,
                        pack_id: None,
                        candidate: None,
                        fallback_stage: None,
                        tried,
                        asset: None,
                    }
                }
            };
            if resolution_explanation.status == "missing" {
                explanation.diagnostics.push(
                    mesh_core_resources::ResourceExplanationDiagnostic {
                        severity: if *required { "error" } else { "warning" }.into(),
                        code: if *required {
                            "missing_required_icon"
                        } else {
                            "missing_optional_icon"
                        }
                        .into(),
                        module_id: Some(module_id.clone()),
                        pack_id: None,
                        message: format!(
                            "{} icon '{semantic_name}' did not resolve in the effective resource snapshot",
                            if *required { "required" } else { "optional" }
                        ),
                    },
                );
            }
            explanation.icons.resolutions.push(resolution_explanation);
        }
    }

    for (pack_id, family) in font_registry.missing_requirements() {
        let module_id = font_pack_bindings
            .iter()
            .find(|pack| pack.pack_id == pack_id)
            .map(|pack| pack.module_id.clone());
        explanation
            .diagnostics
            .push(mesh_core_resources::ResourceExplanationDiagnostic {
                severity: "warning".into(),
                code: "missing_host_font".into(),
                module_id,
                pack_id: Some(pack_id.clone()),
                message: format!(
                    "font pack '{pack_id}' requires host family '{family}'; resolver will use system fallback"
                ),
            });
    }
    explanation
}

/// A worker-owned resource candidate that can be polled by a non-blocking
/// profile transition. Dropping an unfinished job requests cancellation; the
/// worker remains responsible for observing the token between bounded units.
pub(in crate::shell) struct ResourcePreparationJob {
    worker: Option<std::thread::JoinHandle<Result<PreparedResourceSnapshot, ShellRunError>>>,
    token: mesh_core_resources::ResourcePreparationToken,
    lease: Option<mesh_core_resources::ResourcePreparationLease>,
}

impl ResourcePreparationJob {
    #[cfg(test)]
    pub(in crate::shell) fn from_test_worker(
        worker: std::thread::JoinHandle<Result<PreparedResourceSnapshot, ShellRunError>>,
        lease: mesh_core_resources::ResourcePreparationLease,
    ) -> Self {
        Self {
            worker: Some(worker),
            token: lease.token().clone(),
            lease: Some(lease),
        }
    }

    pub(in crate::shell) fn generation(&self) -> u64 {
        self.lease
            .as_ref()
            .map_or(0, mesh_core_resources::ResourcePreparationLease::generation)
    }

    pub(in crate::shell) fn is_finished(&self) -> bool {
        self.worker
            .as_ref()
            .is_some_and(std::thread::JoinHandle::is_finished)
    }

    pub(in crate::shell) fn cancel(&self) {
        self.token.cancel();
    }

    pub(in crate::shell) fn try_wait(
        &mut self,
    ) -> Option<Result<PreparedResourceSnapshot, ShellRunError>> {
        if !self.is_finished() {
            return None;
        }
        Some(self.join_worker())
    }

    pub(in crate::shell) fn wait(&mut self) -> Result<PreparedResourceSnapshot, ShellRunError> {
        if let Some(result) = self.try_wait() {
            result
        } else {
            self.join_worker()
        }
    }

    pub(in crate::shell) fn retire(&self) {
        if let Some(lease) = self.lease.as_ref() {
            lease.retire();
        }
    }

    fn join_worker(&mut self) -> Result<PreparedResourceSnapshot, ShellRunError> {
        let worker = self
            .worker
            .take()
            .ok_or_else(|| ShellRunError::FrontendComposition {
                message: "resource preparation job was already completed".into(),
            })?;
        let lease = self
            .lease
            .take()
            .ok_or_else(|| ShellRunError::FrontendComposition {
                message: "resource preparation generation was already released".into(),
            })?;
        let result = match worker.join() {
            Ok(result) => result,
            Err(_) => {
                lease.retire();
                return Err(ShellRunError::FrontendComposition {
                    message: "resource preparation worker panicked".into(),
                });
            }
        };
        match result {
            Ok(mut prepared) => {
                prepared.generation = lease.generation();
                prepared.resource_lease = Some(lease);
                Ok(prepared)
            }
            Err(error) => {
                lease.retire();
                Err(error)
            }
        }
    }
}

impl Drop for ResourcePreparationJob {
    fn drop(&mut self) {
        if self.worker.is_some() {
            self.cancel();
        }
        if let Some(lease) = self.lease.as_ref() {
            lease.retire();
        }
    }
}

impl Drop for PreparedResourceSnapshot {
    fn drop(&mut self) {
        if let Some(lease) = self.resource_lease.as_ref() {
            lease.retire();
        }
    }
}

impl Shell {
    pub(in crate::shell) fn prepare_resource_snapshot(
        &self,
        graph: &InstalledModuleGraph,
        settings_store: &SettingsStore,
    ) -> Result<PreparedResourceSnapshot, ShellRunError> {
        let mut job = self.start_resource_preparation_job(graph, settings_store)?;
        let generation = job.generation();
        let result = job.wait();
        match result {
            Ok(prepared)
                if prepared
                    .resource_lease
                    .as_ref()
                    .is_some_and(mesh_core_resources::ResourcePreparationLease::is_current) =>
            {
                Ok(prepared)
            }
            Ok(_) => {
                job.retire();
                Err(ShellRunError::FrontendComposition {
                    message: format!("resource preparation generation {generation} was superseded"),
                })
            }
            Err(error) => {
                job.retire();
                Err(error)
            }
        }
    }

    /// Start resource preparation without waiting for the worker. The job
    /// owns the active generation lease, so starting a newer job cancels this
    /// candidate and makes its eventual result ineligible for publication.
    pub(in crate::shell) fn start_resource_preparation_job(
        &self,
        graph: &InstalledModuleGraph,
        settings_store: &SettingsStore,
    ) -> Result<ResourcePreparationJob, ShellRunError> {
        let lease = self.resource_preparation.begin();
        let result =
            self.start_resource_preparation_job_with_lease(graph, settings_store, lease.clone());
        if result.is_err() {
            lease.retire();
        }
        result
    }

    fn start_resource_preparation_job_with_lease(
        &self,
        graph: &InstalledModuleGraph,
        settings_store: &SettingsStore,
        lease: mesh_core_resources::ResourcePreparationLease,
    ) -> Result<ResourcePreparationJob, ShellRunError> {
        let cancellation = lease.token().clone();
        let icon_chain = graph.icon_pack_chain().to_vec();
        let font_chain = graph.font_pack_chain().to_vec();
        let shell_font_chain = settings_store.shell().fonts.packs.clone();
        let shell_default_icon_pack = settings_store.shell().icons.default_pack.clone();
        let icon_ids = icon_chain
            .iter()
            .cloned()
            .collect::<std::collections::HashSet<_>>();
        let font_ids = font_chain
            .iter()
            .cloned()
            .collect::<std::collections::HashSet<_>>();
        let icon_contributions = graph.contributed_icons().to_vec();
        let font_contributions = graph.contributed_fonts().to_vec();
        let icon_requirements = graph
            .icon_requirements()
            .iter()
            .map(|requirement| {
                (
                    requirement.module_id.clone(),
                    requirement.name.clone(),
                    requirement.required,
                )
            })
            .collect::<Vec<_>>();

        let mut pack_inputs = Vec::new();
        for module_id in &icon_chain {
            let module =
                graph
                    .module(module_id)
                    .ok_or_else(|| ShellRunError::FrontendComposition {
                        message: format!("icon-pack {module_id} disappeared from the graph"),
                    })?;
            let root = module.manifest_path.parent().ok_or_else(|| {
                ShellRunError::FrontendComposition {
                    message: format!("icon-pack {module_id} has no module root"),
                }
            })?;
            let section = module.manifest.mesh.icon_pack.clone().ok_or_else(|| {
                ShellRunError::FrontendComposition {
                    message: format!(
                        "icon-pack {module_id} is selected but has no mesh.icon_pack declaration"
                    ),
                }
            })?;
            pack_inputs.push((module_id.clone(), root.to_path_buf(), section));
        }

        let mut font_pack_inputs = Vec::new();
        for module_id in &font_chain {
            let module =
                graph
                    .module(module_id)
                    .ok_or_else(|| ShellRunError::FrontendComposition {
                        message: format!("font-pack {module_id} disappeared from the graph"),
                    })?;
            let section = module.manifest.mesh.font_pack.clone().ok_or_else(|| {
                ShellRunError::FrontendComposition {
                    message: format!(
                        "font-pack {module_id} is selected but has no mesh.font_pack declaration"
                    ),
                }
            })?;
            let root = module.manifest_path.parent().ok_or_else(|| {
                ShellRunError::FrontendComposition {
                    message: format!("font-pack {module_id} has no module root"),
                }
            })?;
            font_pack_inputs.push((module_id.clone(), root.to_path_buf(), section));
        }

        let mut frontends = Vec::new();
        let mut font_frontends = Vec::new();
        for module in graph.enabled_modules() {
            if !matches!(module.kind, ModuleKind::Frontend | ModuleKind::Component) {
                continue;
            }
            let overrides =
                ModuleSettingsOverrides::from_namespace(&settings_store.namespace(&module.id));
            let author = module
                .manifest
                .mesh
                .icons
                .as_ref()
                .map(|icons| icons.overrides.clone())
                .unwrap_or_default();
            let ignore_default_frontend = module
                .manifest
                .mesh
                .icons
                .as_ref()
                .is_some_and(|icons| icons.ignore_shell_default);
            let user = overrides.icons.as_ref();
            frontends.push((
                module.id.clone(),
                mesh_core_icon::FrontendIconBindings {
                    declared_pack_chain: module.manifest.mesh.uses.resources.icons.clone(),
                    author_overrides: author,
                    user_pack_chain: user.and_then(|icons| icons.use_packs.clone()),
                    user_overrides: user
                        .map(|icons| icons.overrides.clone())
                        .unwrap_or_default(),
                    ignore_shell_default_frontend: ignore_default_frontend,
                    ignore_shell_default_user: user.is_some_and(|icons| icons.ignore_shell_default),
                },
            ));
            let font_author = module
                .manifest
                .mesh
                .fonts
                .as_ref()
                .map(|fonts| fonts.overrides.clone())
                .unwrap_or_default();
            let font_user = overrides.fonts.as_ref();
            font_frontends.push((
                module.id.clone(),
                mesh_core_resources::FontFrontendBindings {
                    declared_pack_chain: module.manifest.mesh.uses.resources.fonts.clone(),
                    author_overrides: font_author.into_iter().collect(),
                    user_pack_chain: font_user.and_then(|fonts| fonts.use_packs.clone()),
                    user_overrides: font_user
                        .map(|fonts| fonts.overrides.clone())
                        .unwrap_or_default()
                        .into_iter()
                        .collect(),
                },
            ));
        }

        let revision = self.resource_snapshot.revision.saturating_add(1);
        let worker_cancellation = cancellation.clone();
        let worker = std::thread::Builder::new()
            .name("mesh-resource-prepare".into())
            .spawn(move || {
                if worker_cancellation.is_cancelled() {
                    return Err(ShellRunError::FrontendComposition {
                        message: "resource preparation cancelled".into(),
                    });
                }
                // Refresh host roots before preparing any icon or font asset so
                // every consumer in this candidate reads the same catalog.
                let host_catalog = mesh_core_resources::refresh_system_resource_catalog();
                if worker_cancellation.is_cancelled() {
                    return Err(ShellRunError::FrontendComposition {
                        message: "resource preparation cancelled".into(),
                    });
                }
                let asset_paths =
                    |resources: Vec<mesh_core_module::package::ContributedPathResource>,
                     selected: std::collections::HashSet<String>| {
                        resources
                            .into_iter()
                            .filter(|resource| selected.contains(&resource.module_id))
                            .map(|resource| {
                                if worker_cancellation.is_cancelled() {
                                    return Err(ShellRunError::FrontendComposition {
                                        message: "resource preparation cancelled".into(),
                                    });
                                }
                                let root =
                                    resource.source.manifest_path.parent().ok_or_else(|| {
                                        ShellRunError::FrontendComposition {
                                            message: format!(
                                                "resource {} has no owning module root",
                                                resource.source.scoped_id
                                            ),
                                        }
                                    })?;
                                let handle = mesh_core_resources::ResourceAssetHandle::new(
                                    root,
                                    &resource.path,
                                )
                                .map_err(|error| {
                                    ShellRunError::FrontendComposition {
                                        message: format!(
                                            "resource {} is unsafe: {error}",
                                            resource.source.scoped_id
                                        ),
                                    }
                                })?;
                                handle
                                    .read_bounded_with_cancellation(
                                        mesh_core_resources::DEFAULT_MAX_RESOURCE_BYTES,
                                        &worker_cancellation,
                                    )
                                    .map_err(|error| ShellRunError::FrontendComposition {
                                        message: format!(
                                            "resource {} is unreadable: {error}",
                                            resource.source.scoped_id
                                        ),
                                    })?;
                                Ok(ResourceAsset {
                                    module_id: resource.module_id,
                                    id: resource.id,
                                    handle,
                                })
                            })
                            .collect::<Result<Vec<_>, ShellRunError>>()
                    };
                let icon_assets = asset_paths(icon_contributions, icon_ids)?;
                let font_assets = asset_paths(font_contributions, font_ids)?;
                let icon_packs = pack_inputs
                    .into_iter()
                    .map(|(module_id, root, section)| {
                        prepare_icon_pack_bindings_with_cancellation(
                            &module_id,
                            &root,
                            &section,
                            &worker_cancellation,
                        )
                    })
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|error| ShellRunError::FrontendComposition { message: error })?;
                let font_packs = font_pack_inputs
                    .into_iter()
                    .map(|(module_id, root, section)| {
                        let faces = section
                            .faces
                            .into_iter()
                            .map(|face| {
                                let asset = mesh_core_resources::ResourceAssetHandle::new(
                                    &root, &face.file,
                                )
                                .map_err(|error| ShellRunError::FrontendComposition {
                                    message: format!(
                                        "font-pack {module_id} face '{}' has an unsafe asset: {error}",
                                        face.family
                                    ),
                                })?;
                                mesh_core_resources::validate_font_face_with_cancellation(
                                    &asset,
                                    &face.family,
                                    &worker_cancellation,
                                )
                                    .map_err(|error| ShellRunError::FrontendComposition {
                                        message: format!(
                                            "font-pack {module_id} face '{}' is invalid: {error}",
                                            face.family
                                        ),
                                    })?;
                                Ok(mesh_core_resources::FontFaceBinding {
                                    family: face.family,
                                    asset,
                                    weight: face.weight,
                                    style: match face.style {
                                        mesh_core_module::manifest::FontPackFaceStyle::Normal => {
                                            "normal"
                                        }
                                        mesh_core_module::manifest::FontPackFaceStyle::Italic => {
                                            "italic"
                                        }
                                        mesh_core_module::manifest::FontPackFaceStyle::Oblique => {
                                            "oblique"
                                        }
                                    }
                                    .into(),
                                    stretch: face.stretch,
                                    coverage: face.coverage.into_iter().collect(),
                                })
                            })
                            .collect::<Result<Vec<_>, ShellRunError>>()?;
                        Ok(mesh_core_resources::FontPackBindings {
                            module_id,
                            pack_id: section.id,
                            required_families: section
                                .requires
                                .fonts
                                .into_iter()
                                .map(|requirement| requirement.family)
                                .collect(),
                            covers: section.covers.into_iter().collect(),
                            mappings: section.mappings.into_iter().collect(),
                            faces,
                        })
                    })
                    .collect::<Result<Vec<_>, ShellRunError>>()?;
                let mut font_registry =
                    mesh_core_resources::FontRegistry::from_catalog(&host_catalog);
                font_registry
                    .replace_with_cancellation(
                        font_packs,
                        font_chain.clone(),
                        &worker_cancellation,
                    )
                    .map_err(|error| ShellRunError::FrontendComposition {
                        message: format!("invalid font-pack resource snapshot: {error}"),
                    })?;
                font_registry
                    .set_shell_pack_chain(&shell_font_chain)
                    .map_err(|error| ShellRunError::FrontendComposition {
                        message: format!("invalid shell font-pack chain: {error}"),
                    })?;
                font_registry
                    .set_frontend_bindings(font_frontends)
                    .map_err(|error| ShellRunError::FrontendComposition {
                        message: format!("invalid frontend font-pack bindings: {error}"),
                    })?;
                for (pack_id, family) in font_registry.missing_requirements() {
                    tracing::warn!(
                        pack_id = %pack_id,
                        family = %family,
                        "font-pack requirement is not installed; role resolution will use fallback"
                    );
                }
                if worker_cancellation.is_cancelled() {
                    return Err(ShellRunError::FrontendComposition {
                        message: "resource preparation cancelled".into(),
                    });
                }

                let explanation = resource_explanation_snapshot(
                    revision,
                    &host_catalog,
                    &icon_chain,
                    &font_registry,
                    &icon_packs,
                    &frontends,
                    &icon_requirements,
                    &icon_assets,
                    &font_assets,
                    shell_default_icon_pack.as_deref(),
                );

                Ok(PreparedResourceSnapshot {
                    generation: 0,
                    resource_lease: None,
                    snapshot: ResourceSnapshot {
                        revision,
                        icon_pack_chain: icon_chain,
                        font_pack_chain: font_chain,
                        icon_assets,
                        font_assets,
                    },
                    explanation,
                    icon_packs,
                    font_registry,
                    frontends,
                })
            })
            .map_err(|error| ShellRunError::FrontendComposition {
                message: format!("failed to start resource preparation: {error}"),
            })?;
        Ok(ResourcePreparationJob {
            worker: Some(worker),
            token: cancellation,
            lease: Some(lease),
        })
    }

    pub(in crate::shell) fn commit_resource_snapshot(
        &mut self,
        prepared: &PreparedResourceSnapshot,
    ) -> Result<(), ShellRunError> {
        let current = prepared.resource_lease.as_ref().map_or_else(
            || self.resource_preparation.is_current(prepared.generation),
            mesh_core_resources::ResourcePreparationLease::is_current,
        );
        if !current {
            return Err(ShellRunError::FrontendComposition {
                message: format!(
                    "resource preparation generation {} is no longer current",
                    prepared.generation
                ),
            });
        }
        let font_revision_changed =
            self.font_registry.revision() != prepared.font_registry.revision();
        mesh_core_icon::replace_default_bindings(
            prepared.icon_packs.clone(),
            prepared.frontends.clone(),
            self.settings.icons.default_pack.clone(),
        )
        .map_err(|error| ShellRunError::FrontendComposition {
            message: format!("failed to publish resource snapshot: {error}"),
        })?;
        self.font_registry = prepared.font_registry.clone();
        self.resource_snapshot = Arc::new(prepared.snapshot.clone());
        self.resource_explanation = Arc::new(prepared.explanation.clone());
        let font_registry = &self.font_registry;
        self.theme.update_active(|theme| {
            apply_font_registry_tokens(theme, font_registry);
        });
        self.theme_watch.revision = self.theme.active_snapshot().revision;
        if font_revision_changed {
            self.mark_components_theme_changed()?;
        }
        if let Some(lease) = prepared.resource_lease.as_ref() {
            lease.retire();
        } else {
            self.resource_preparation.retire(prepared.generation);
        }
        Ok(())
    }

    /// Return the exact effective resource explanation last committed by the
    /// worker-built snapshot. Runtime diagnostics and external tooling should
    /// consume this model rather than rediscovering packs independently.
    pub fn resource_explanation_snapshot(
        &self,
    ) -> mesh_core_resources::ResourceExplanationSnapshot {
        (*self.resource_explanation).clone()
    }
}

/// Project the active font resource binding into the theme snapshot consumed
/// by every style resolver. `font.*` contains the standard role tokens and
/// `mesh.font.*` is an internal namespace for pack-qualified escape hatches.
pub(in crate::shell) fn apply_font_registry_tokens(
    theme: &mut mesh_core_theme::Theme,
    registry: &mesh_core_resources::FontRegistry,
) {
    theme.remove_tokens_with_prefix("mesh.font.");
    for (css_name, family) in registry.role_tokens() {
        let token_name = css_name
            .strip_prefix("--")
            .unwrap_or(&css_name)
            .replace('-', ".");
        theme.set_token(
            token_name,
            mesh_core_theme::TokenValue::String(family),
            mesh_core_theme::ThemeProvenance::BaseRecovery,
        );
    }
    for (token_name, family) in registry.qualified_role_tokens() {
        theme.set_token(
            token_name,
            mesh_core_theme::TokenValue::String(family),
            mesh_core_theme::ThemeProvenance::BaseRecovery,
        );
    }
}

fn log_locale_catalog_diagnostics(diagnostics: &[mesh_core_locale::CatalogSourceDiagnostics]) {
    for source in diagnostics {
        for diagnostic in &source.diagnostics {
            tracing::warn!(
                module_id = %source.module_id,
                locale = %source.locale,
                path = %source.path.display(),
                key = %diagnostic.key,
                "locale catalog entry rejected: {}",
                diagnostic.message,
            );
        }
    }
}

/// Layer a profile's sparse preferences and per-instance surface overrides on
/// top of shared user defaults. The shared document remains untouched.
pub(in crate::shell) fn effective_profile_settings(
    shared: SettingsStore,
    profile: Option<&mesh_core_module::package::ShellProfile>,
) -> Result<SettingsStore, mesh_core_config::ConfigError> {
    let Some(profile) = profile else {
        return Ok(shared);
    };
    let path = shared.path().to_path_buf();
    let mut document = shared.to_value();
    let root = document
        .as_object_mut()
        .expect("SettingsStore always serializes an object");
    if let Some(theme) = &profile.resources.theme {
        let target = root
            .entry("shell".to_string())
            .or_insert_with(|| serde_json::json!({}));
        mesh_core_config::merge_json(target, &serde_json::json!({ "theme": { "active": theme } }));
    }
    for (namespace, overrides) in &profile.settings {
        let target = root
            .entry(namespace.clone())
            .or_insert_with(|| serde_json::json!({}));
        mesh_core_config::merge_json(target, overrides);
    }
    for (instance_id, instance) in profile.roots.iter().filter(|(_, root)| root.active) {
        let Some(surface) = &instance.surface else {
            continue;
        };
        let target = root
            .entry(instance_id.clone())
            .or_insert_with(|| serde_json::json!({}));
        mesh_core_config::merge_json(target, &serde_json::json!({ "surface": surface }));
    }
    SettingsStore::from_value(path, document)
}

fn builtin_state_contract(
    interface: &str,
    fields: &[(&str, &str)],
) -> mesh_core_service::InterfaceContract {
    builtin_contract(interface, fields, &[])
}

/// A core-provided contract: state fields the shell emits, plus the methods it
/// answers itself.
///
/// The shell is the provider for these interfaces, so the methods here are the
/// public, capability-gated way for any module to change composition and
/// configuration. Declaring them as a contract rather than a reserved channel
/// is what makes the settings frontend replaceable: the caller needs
/// `service.<name>.control`, not a particular module id.
fn builtin_contract(
    interface: &str,
    fields: &[(&str, &str)],
    methods: &[(&str, &[(&str, &str)])],
) -> mesh_core_service::InterfaceContract {
    mesh_core_service::InterfaceContract {
        interface: interface.to_string(),
        version: mesh_core_service::parse_contract_version("1.0")
            .expect("built-in interface version must be valid"),
        state_fields: fields
            .iter()
            .map(|(name, field_type)| mesh_core_service::ContractStateField {
                name: (*name).to_string(),
                field_type: (*field_type).to_string(),
                description: None,
            })
            .collect(),
        methods: methods
            .iter()
            .map(|(name, args)| mesh_core_service::InterfaceMethod {
                name: (*name).to_string(),
                args: args
                    .iter()
                    .map(|(arg, arg_type)| mesh_core_service::InterfaceArgument {
                        name: (*arg).to_string(),
                        arg_type: (*arg_type).to_string(),
                    })
                    .collect(),
                returns: Some("Result".to_string()),
                // These land in the shell synchronously; there is no backend
                // queue to coalesce against and no optimistic state to bind.
                coalesce: false,
                state_binding: None,
            })
            .collect(),
        events: Vec::new(),
        types: HashMap::new(),
        capabilities: mesh_core_service::ContractCapabilities::default(),
    }
}

#[derive(Debug)]
pub(super) struct DiscoveredModuleManifest {
    pub(super) dir: PathBuf,
    pub(super) loaded:
        Result<mesh_core_module::LoadedManifest, mesh_core_module::manifest::ManifestError>,
}

pub(super) fn discover_shell_module_manifest_dirs(module_dirs: &[PathBuf]) -> Vec<PathBuf> {
    let mut discovered = Vec::new();
    for dir in module_dirs {
        if !dir.exists() {
            tracing::debug!("module directory does not exist: {}", dir.display());
            continue;
        }
        discovered.extend(discover_shell_module_manifest_dirs_under(dir));
    }
    discovered
}

fn discover_shell_module_manifest_dirs_under(dir: &Path) -> Vec<PathBuf> {
    if shell_module_manifest_exists(dir) {
        return vec![dir.to_path_buf()];
    }

    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) => {
            tracing::warn!("failed to read module directory {}: {e}", dir.display());
            return Vec::new();
        }
    };
    let mut child_dirs = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect::<Vec<_>>();
    child_dirs.sort();

    child_dirs
        .par_iter()
        .flat_map(|path| discover_shell_module_manifest_dirs_under(path))
        .collect()
}

fn shell_module_manifest_exists(dir: &Path) -> bool {
    dir.join("package.json").exists()
        || dir.join("module.json").exists()
        || dir.join("mesh.toml").exists()
}

pub(super) fn load_shell_module_manifests(
    module_dirs: &[PathBuf],
) -> Vec<DiscoveredModuleManifest> {
    module_dirs
        .par_iter()
        .map(|dir| DiscoveredModuleManifest {
            dir: dir.clone(),
            loaded: mesh_core_module::manifest::load_canonical_manifest(dir),
        })
        .collect()
}

/// Translate the graph's owner declarations into one complete schema snapshot
/// before any module receives settings. The config foundation keeps the raw
/// document; this graph-owned registration produces the validated projection.
pub(in crate::shell) fn register_graph_settings_schemas(
    store: &mut SettingsStore,
    graph: &InstalledModuleGraph,
) -> Result<(), mesh_core_config::SettingsSchemaError> {
    let mut properties_by_namespace: BTreeMap<String, serde_json::Map<String, serde_json::Value>> =
        graph
            .modules()
            .into_iter()
            .map(|module| (module.id.clone(), module_settings_properties()))
            .collect();

    for contribution in graph.settings_schemas() {
        let properties = properties_by_namespace
            .get_mut(&contribution.namespace)
            .ok_or_else(|| mesh_core_config::SettingsSchemaError::OwnerMismatch {
                namespace: contribution.namespace.clone(),
                owner: contribution.module_id.clone(),
            })?;
        let schema = if contribution.source.local_id == "props" {
            let prop_schema = normalize_object_schema(&contribution.schema);
            serde_json::json!({
                "type": "object",
                "properties": {
                    "global": prop_schema,
                    "instances": {
                        "type": "object",
                        "additionalProperties": prop_schema
                    }
                }
            })
        } else {
            normalize_object_schema(&contribution.schema)
        };
        let Some(fields) = schema
            .get("properties")
            .and_then(serde_json::Value::as_object)
        else {
            continue;
        };
        for (key, value) in fields {
            merge_schema_property(properties, key, value);
        }
    }

    let schemas = properties_by_namespace
        .into_iter()
        .map(|(namespace, properties)| {
            SettingsNamespaceSchema::new(
                namespace.clone(),
                namespace,
                serde_json::json!({
                    "type": "object",
                    "properties": properties
                }),
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    store.replace_namespace_schemas_transactionally(schemas)
}

fn normalize_object_schema(schema: &serde_json::Value) -> serde_json::Value {
    if schema.as_object().is_some_and(|schema| {
        schema.get("type").and_then(serde_json::Value::as_str) == Some("object")
    }) {
        return schema.clone();
    }
    serde_json::json!({
        "type": "object",
        "properties": schema
    })
}

fn merge_schema_property(
    properties: &mut serde_json::Map<String, serde_json::Value>,
    key: &str,
    value: &serde_json::Value,
) {
    let Some(existing) = properties.get_mut(key) else {
        properties.insert(key.to_string(), value.clone());
        return;
    };
    let Some(existing_properties) = existing
        .get_mut("properties")
        .and_then(serde_json::Value::as_object_mut)
    else {
        *existing = value.clone();
        return;
    };
    let Some(incoming_properties) = value
        .get("properties")
        .and_then(serde_json::Value::as_object)
    else {
        *existing = value.clone();
        return;
    };
    for (nested_key, nested_value) in incoming_properties {
        existing_properties.insert(nested_key.clone(), nested_value.clone());
    }
}

fn module_settings_properties() -> serde_json::Map<String, serde_json::Value> {
    serde_json::json!({
        "surface": {
            "type": "object",
            "properties": {
                "role": { "type": "string" },
                "promotable": { "type": "boolean" },
                "title": { "type": "string" },
                "app_id": { "type": "string" },
                "resizable": { "type": "boolean" },
                "decorations": { "type": "string" },
                "anchor": { "type": "string" },
                "layer": { "type": "string" },
                "exclusive_zone": { "type": "integer" },
                "keyboard_mode": { "type": "string" },
                "visible_on_start": { "type": "boolean" },
                "margin_top": { "type": "integer" },
                "margin_right": { "type": "integer" },
                "margin_bottom": { "type": "integer" },
                "margin_left": { "type": "integer" },
                "blur": { "type": "boolean" }
            }
        },
        "props": {
            "type": "object",
            "properties": {
                "global": { "type": "object" },
                "instances": {
                    "type": "object",
                    "additionalProperties": { "type": "object" }
                }
            }
        },
        "icons": {
            "type": "object",
            "properties": {
                "use_packs": { "type": "array", "items": { "type": "string" } },
                "overrides": {
                    "type": "object",
                    "additionalProperties": { "type": "string" }
                },
                "ignore_shell_default": { "type": "boolean" }
            }
        },
        "fonts": {
            "type": "object",
            "properties": {
                "use_packs": { "type": "array", "items": { "type": "string" } },
                "overrides": {
                    "type": "object",
                    "additionalProperties": { "type": "string" }
                }
            }
        },
        "i18n": {
            "type": "object",
            "properties": {
                "default_locale": { "type": "string" }
            }
        }
    })
    .as_object()
    .cloned()
    .expect("module settings schema is an object")
}

#[cfg(test)]
pub(super) fn load_shell_module_manifests_serial(
    module_dirs: &[PathBuf],
) -> Vec<DiscoveredModuleManifest> {
    module_dirs
        .iter()
        .map(|dir| DiscoveredModuleManifest {
            dir: dir.clone(),
            loaded: mesh_core_module::manifest::load_canonical_manifest(dir),
        })
        .collect()
}

impl Shell {
    pub fn new() -> Self {
        let config_path = mesh_core_config::default_config_path();
        let config = load_config(&config_path).unwrap_or_else(|e| {
            tracing::warn!("failed to load config, using defaults: {e}");
            ShellConfig {
                shell: Default::default(),
            }
        });
        let shared_settings = SettingsStore::load().unwrap_or_else(|e| {
            tracing::warn!("failed to load settings, using defaults: {e}");
            SettingsStore::default()
        });
        let graph_path = installed_module_graph_path();
        let capability_policy = RootModuleGraphManifest::from_path(&graph_path)
            .map(|root| CapabilityPolicy::from_approvals(root.capability_approvals))
            .unwrap_or_else(|error| {
                tracing::warn!(
                    "failed to load capability approvals from {}: {error}",
                    graph_path.display()
                );
                CapabilityPolicy::default()
            });
        let active_profile = mesh_core_module::package::ProfilePaths::from_root_graph(&graph_path)
            .and_then(|paths| paths.load_active())
            .unwrap_or_else(|error| {
                tracing::warn!("failed to load active shell profile: {error}");
                None
            });
        let active_profile_id = active_profile.as_ref().map(|(id, _)| id.clone());
        let settings_store = Arc::new(
            effective_profile_settings(
                shared_settings,
                active_profile.as_ref().map(|(_, profile)| profile),
            )
            .unwrap_or_else(|error| {
                tracing::warn!("failed to apply active profile settings: {error}");
                SettingsStore::default()
            }),
        );
        // A hand-edited file is the whole point of the store, so a bad value in
        // it is reported and skipped, never fatal: the shell starts on declared
        // defaults with the reason on stderr.
        mesh_core_config::log_settings_diagnostics("settings", settings_store.diagnostics());
        let settings = mesh_core_config::resolve_shell_locale_settings(settings_store.shell());

        // Discover and register XDG icon themes installed on the system.
        // Icon-pack binding modules reference them by name in their
        // mapping targets (`<theme>/<icon-name>`). Failures are logged
        // but non-fatal — hicolor fallback still works.
        for pack in mesh_core_icon::discover_xdg_packs() {
            let id = pack.id.clone();
            match mesh_core_icon::register_default_pack(pack) {
                Ok(true) => tracing::info!("registered XDG icon theme '{}'", id),
                Ok(false) => tracing::debug!("XDG icon theme '{}' already registered", id),
                Err(err) => {
                    tracing::warn!("failed to register XDG icon theme '{}': {err}", id)
                }
            }
        }
        mesh_core_icon::set_default_shell_pack(settings.icons.default_pack.clone());
        mesh_core_render::set_blur_quality(blur_quality_from_settings(&settings.render.blur));
        let (theme, theme_watch) = load_active_theme(&settings);
        let locale = LocaleEngine::with_fallback_locale(
            settings.i18n.locale.clone(),
            settings.i18n.fallback_locale.clone(),
        );
        let module_dirs = resolve_default_module_dirs(&config);
        let settings_watch = {
            let path = settings_store.path().to_path_buf();
            let modified_at = std::fs::metadata(&path)
                .ok()
                .and_then(|m| m.modified().ok());
            SettingsWatchState { path, modified_at }
        };

        let interfaces = InterfaceRegistry::new();
        let mut theme_contract = builtin_contract(
            "mesh.theme",
            &[
                ("current", "string"),
                ("theme_id", "string"),
                ("mode", "string?"),
                ("mode_policy", "object?"),
                ("color_scheme", "string?"),
                ("contrast", "string?"),
                ("tokens", "object?"),
                ("provenance", "object?"),
                ("revision", "string"),
                ("fingerprint", "integer?"),
                ("is_dark", "boolean"),
                ("themes", "object[]"),
                ("available", "string[]"),
                ("system_resources", "object"),
            ],
            &[
                ("set_theme", &[("theme_id", "string")]),
                ("set_icon_theme", &[("theme_id", "string")]),
                ("set_font_family", &[("family", "string")]),
            ],
        );
        theme_contract.events = vec![
            mesh_core_service::InterfaceEvent {
                name: "ThemeChanged".into(),
                payload: [
                    ("theme_id", "string"),
                    ("mode", "string"),
                    ("mode_policy", "object"),
                    ("color_scheme", "string"),
                    ("contrast", "string"),
                    ("revision", "string"),
                    ("tokens", "object"),
                    ("provenance", "object"),
                    ("changed_tokens", "object[]"),
                ]
                .into_iter()
                .map(|(name, arg_type)| mesh_core_service::InterfaceArgument {
                    name: name.into(),
                    arg_type: arg_type.into(),
                })
                .collect(),
            },
            mesh_core_service::InterfaceEvent {
                name: "TokenChanged".into(),
                payload: [
                    ("theme_id", "string"),
                    ("mode", "string"),
                    ("name", "string"),
                    ("value", "any?"),
                    ("provenance", "any?"),
                    ("revision", "string"),
                ]
                .into_iter()
                .map(|(name, arg_type)| mesh_core_service::InterfaceArgument {
                    name: name.into(),
                    arg_type: arg_type.into(),
                })
                .collect(),
            },
        ];
        interfaces.register_contract(theme_contract);
        // Locale writes stay on the `mesh.locale.set` host API, which already
        // enforces `locale.write`. A second, service-shaped way in would mean
        // two capability names for one operation.
        interfaces.register_contract(builtin_state_contract(
            "mesh.locale",
            &[
                ("current", "string"),
                ("locale", "string"),
                ("chain", "string[]"),
                ("direction", "string"),
                ("policy", "string"),
                ("revision", "string"),
            ],
        ));
        interfaces.register_contract(builtin_contract(
            "mesh.settings",
            &[("revision", "string"), ("namespaces", "object")],
            &[
                (
                    "set_prop",
                    &[
                        ("module_id", "string"),
                        ("instance_id", "string?"),
                        ("prop", "string"),
                        ("value", "any"),
                    ],
                ),
                (
                    "unset_prop",
                    &[
                        ("module_id", "string"),
                        ("instance_id", "string?"),
                        ("prop", "string"),
                    ],
                ),
            ],
        ));
        interfaces.register_contract(builtin_contract(
            "mesh.packages",
            &[
                ("modules", "object[]"),
                ("providers", "object"),
                ("profiles", "string[]"),
                ("active_profile", "string"),
            ],
            &[
                (
                    "set_module_enabled",
                    &[("module_id", "string"), ("enabled", "boolean")],
                ),
                (
                    "set_provider",
                    &[("interface", "string"), ("provider_id", "string")],
                ),
                ("switch_profile", &[("profile_id", "string")]),
                (
                    "install",
                    &[
                        ("source", "string"),
                        ("profile_id", "string?"),
                        ("available_only", "boolean?"),
                        ("allow_elevated", "boolean?"),
                        ("allow_high", "boolean?"),
                    ],
                ),
                (
                    "uninstall",
                    &[("module_id", "string"), ("force", "boolean?")],
                ),
            ],
        ));
        interfaces.register_contract(builtin_contract(
            "mesh.composition",
            &[
                ("profile_id", "string"),
                ("generation", "string"),
                ("roots", "object[]"),
                ("slots", "object[]"),
                ("palette", "object[]"),
            ],
            &[
                (
                    "apply_node_slot",
                    &[
                        ("profile_id", "string"),
                        ("root_instance", "string"),
                        ("slot", "string"),
                        ("nodes", "object[]"),
                        ("expected_generation", "string"),
                    ],
                ),
                (
                    "reset_node_slot",
                    &[
                        ("profile_id", "string"),
                        ("root_instance", "string"),
                        ("slot", "string"),
                        ("expected_generation", "string"),
                    ],
                ),
            ],
        ));
        interfaces.register(InterfaceProvider {
            interface: mesh_core_debug::DEBUG_INTERFACE.to_string(),
            version: Some("1.0".to_string()),
            base_module: Some("@mesh/debug".to_string()),
            provider_module: mesh_core_debug::DEBUG_SOURCE_MODULE_ID.to_string(),
            backend_name: "Shell".to_string(),
            priority: 100,
        });
        interfaces.register(InterfaceProvider {
            interface: "mesh.theme".to_string(),
            version: Some("1.0".to_string()),
            base_module: Some("@mesh/theme-interface".to_string()),
            provider_module: "@mesh/shell".to_string(),
            backend_name: "Shell Theme".to_string(),
            priority: 200,
        });
        interfaces.register(InterfaceProvider {
            interface: "mesh.locale".to_string(),
            version: Some("1.0".to_string()),
            base_module: Some("@mesh/locale-interface".to_string()),
            provider_module: "@mesh/shell".to_string(),
            backend_name: "Shell Locale".to_string(),
            priority: 200,
        });
        interfaces.register(InterfaceProvider {
            interface: "mesh.settings".to_string(),
            version: Some("1.0".to_string()),
            base_module: Some("@mesh/settings-interface".to_string()),
            provider_module: "@mesh/shell".to_string(),
            backend_name: "Shell Settings Store".to_string(),
            priority: 200,
        });
        interfaces.register(InterfaceProvider {
            interface: "mesh.packages".to_string(),
            version: Some("1.0".to_string()),
            base_module: Some("@mesh/packages-interface".to_string()),
            provider_module: "@mesh/shell".to_string(),
            backend_name: "Shell Package Graph".to_string(),
            priority: 200,
        });
        interfaces.register(InterfaceProvider {
            interface: "mesh.composition".to_string(),
            version: Some("1.0".to_string()),
            base_module: Some("@mesh/composition-interface".to_string()),
            provider_module: "@mesh/shell".to_string(),
            backend_name: "Shell Composition".to_string(),
            priority: 200,
        });

        let now = std::time::Instant::now();

        Self {
            config,
            settings,
            settings_store,
            theme,
            locale,
            diagnostics: DiagnosticsCollector::new(),
            interfaces,
            capability_policy,
            effective_capabilities: Arc::new(HashMap::new()),
            installed_module_graph: None,
            resource_snapshot: Arc::new(ResourceSnapshot::default()),
            resource_explanation: Arc::new(
                mesh_core_resources::ResourceExplanationSnapshot::default(),
            ),
            font_registry: mesh_core_resources::FontRegistry::default(),
            font_renderer_revision: 0,
            resource_preparation: mesh_core_resources::ResourcePreparationCoordinator::default(),
            active_profile_id,
            modules: HashMap::new(),
            frontend_catalog: FrontendCatalogHandle::default(),
            module_dirs,
            core: ShellCoreState::default(),
            last_published_theme_snapshot: None,
            components: Vec::new(),
            components_want_render: false,
            presented_last_frame: true,
            component_by_surface: HashMap::new(),
            service_delivery_index: ServiceDeliveryIndex::default(),
            surfaces: HashMap::new(),
            clipboard: Box::new(WaylandClipboard::default()),
            presentation_engine: PresentationEngine::select(),
            eventfd_fd: None,
            theme_watch,
            settings_watch,
            next_theme_reload_check: now,
            next_shell_settings_reload_check: now,
            next_frontend_reload_check: now,
            file_watcher_active: false,
            debug: DebugOverlayState::default(),
            debug_overlay: DebugOverlay::new(),
            active_key_modifiers: KeyModifiers::default(),
            keyboard_focus_surface: None,
            pending_wayland_events: VecDeque::new(),
            pending_popup_grabs: HashMap::new(),
            popup_grab_generation: 0,
            transfer_owned_keyboard_modes: HashMap::new(),
            service_handlers: HashMap::new(),
            backend_runtimes: HashMap::new(),
            pending_backend_runtimes: HashMap::new(),
            pending_resource_preparation: None,
            pending_profile_switch: None,
            deferred_requests: VecDeque::new(),
            backend_runtime_statuses: HashMap::new(),
            backend_supervision: HashMap::new(),
            backend_respawn: None,
            latest_service_state: HashMap::new(),
            latest_service_health: HashMap::new(),
            service_contract_validation: HashMap::new(),
            pending_bound_service_state: HashMap::new(),
            bound_service_state_transactions: HashMap::new(),
            command_throttle: HashMap::new(),
            pending_service_call_routes: HashMap::new(),
            pending_popover_hides: HashMap::new(),
            profiling: runtime::profiling::ProfilingRuntimeState::default(),
        }
    }

    pub fn discover_modules(&mut self) {
        let graph = match self.load_installed_module_graph_candidate() {
            Ok(graph) => graph,
            Err(error) => {
                tracing::error!(
                    "failed to load installed module graph; retaining last-known-good discovery: {error}"
                );
                return;
            }
        };
        if let Err(error) = self.commit_installed_module_graph(graph.clone()) {
            tracing::error!(
                "failed to prepare graph locale catalogs; retaining last-known-good discovery: {error}"
            );
            return;
        }
        self.modules.clear();
        for node in graph.modules() {
            let Some(module_dir) = node.manifest_path.parent() else {
                tracing::error!(module_id = %node.id, "graph module manifest has no parent directory");
                continue;
            };
            self.register_loaded_module(
                module_dir,
                mesh_core_module::LoadedManifest {
                    manifest: node.manifest.clone().into_runtime_manifest(),
                    path: node.manifest_path.clone(),
                    source: mesh_core_module::manifest::ManifestSource::CanonicalModuleJson,
                },
            );
        }
        self.register_interfaces_from_graph(&graph);
        tracing::info!("discovered {} graph-authorized modules", self.modules.len());
    }

    pub(in crate::shell) fn installed_module_graph_path(&self) -> PathBuf {
        installed_module_graph_path()
    }

    pub(in crate::shell) fn load_installed_module_graph_cached(
        &mut self,
    ) -> Result<&InstalledModuleGraph, mesh_core_module::package::ModuleManifestError> {
        if self.installed_module_graph.is_none() {
            let graph_path = self.installed_module_graph_path();
            let candidate = load_installed_module_graph_candidate(&graph_path)?;
            let locale = self.prepare_locale_for_graph(&candidate).map_err(|error| {
                mesh_core_module::package::ModuleManifestError::Validation(error.to_string())
            })?;
            self.commit_installed_module_graph_with_locale(candidate, locale);
        }
        Ok(self
            .installed_module_graph
            .as_ref()
            .expect("installed module graph was just loaded"))
    }

    /// Read a replacement graph without changing the active graph. Callers
    /// that prepare a live activation use this boundary so a malformed or
    /// incomplete on-disk candidate cannot discard the last-known-good graph.
    pub(in crate::shell) fn load_installed_module_graph_candidate(
        &self,
    ) -> Result<InstalledModuleGraph, mesh_core_module::package::ModuleManifestError> {
        load_installed_module_graph_candidate(&self.installed_module_graph_path())
    }

    #[cfg(test)]
    pub(in crate::shell) fn reload_installed_module_graph_at(
        &mut self,
        root_module_graph_path: &Path,
    ) -> Result<InstalledModuleGraph, mesh_core_module::package::ModuleManifestError> {
        let candidate = load_installed_module_graph_candidate(root_module_graph_path)?;
        self.commit_installed_module_graph(candidate.clone())
            .map_err(|error| {
                mesh_core_module::package::ModuleManifestError::Validation(error.to_string())
            })?;
        Ok(candidate)
    }

    pub(in crate::shell) fn prepare_locale_for_graph(
        &self,
        graph: &InstalledModuleGraph,
    ) -> Result<LocaleEngine, ShellRunError> {
        let mut candidate = self.locale.clone();
        let (sources, defaults) =
            graph_i18n_catalog_sources(graph).map_err(ShellRunError::LocaleCatalog)?;
        let prepared = candidate
            .prepare_catalog_snapshot_off_thread(sources, defaults)
            .map_err(|error| ShellRunError::LocaleCatalog(error.to_string()))?;
        log_locale_catalog_diagnostics(prepared.diagnostics());
        candidate.replace_catalog_snapshot(prepared.snapshot());
        Ok(candidate)
    }

    pub(in crate::shell) fn prepare_locale_for_settings(
        &self,
        settings: &ShellSettings,
        graph: &InstalledModuleGraph,
    ) -> Result<LocaleEngine, ShellRunError> {
        let mut candidate = self.prepare_locale_selection_for_settings(settings)?;
        let (sources, defaults) =
            graph_i18n_catalog_sources(graph).map_err(ShellRunError::LocaleCatalog)?;
        let prepared = candidate
            .prepare_catalog_snapshot_off_thread(sources, defaults)
            .map_err(|error| ShellRunError::LocaleCatalog(error.to_string()))?;
        log_locale_catalog_diagnostics(prepared.diagnostics());
        candidate.replace_catalog_snapshot(prepared.snapshot());
        Ok(candidate)
    }

    pub(in crate::shell) fn prepare_locale_selection_for_settings(
        &self,
        settings: &ShellSettings,
    ) -> Result<LocaleEngine, ShellRunError> {
        let settings = mesh_core_config::resolve_shell_locale_settings(settings);
        let requested = mesh_core_locale::LocaleSelection::try_new(
            settings.i18n.locale,
            settings.i18n.fallback_locale,
            1,
        )
        .map_err(|error| ShellRunError::LocaleCatalog(error.to_string()))?;
        let revision = if requested.active() == self.locale.current()
            && requested.fallback() == self.locale.fallback_locale()
        {
            self.locale.revision()
        } else {
            self.locale.revision().saturating_add(1)
        };
        let selection = mesh_core_locale::LocaleSelection::try_new(
            requested.active(),
            requested.fallback(),
            revision,
        )
        .map_err(|error| ShellRunError::LocaleCatalog(error.to_string()))?;
        let mut candidate = self.locale.clone();
        candidate.replace_selection(&selection);
        Ok(candidate)
    }

    /// Commit a graph and its prepared locale candidate as one in-memory
    /// activation boundary. Callers that need to prepare runtime objects first
    /// use `commit_installed_module_graph_with_locale` with the same candidate.
    pub(in crate::shell) fn commit_installed_module_graph(
        &mut self,
        graph: InstalledModuleGraph,
    ) -> Result<(), ShellRunError> {
        let locale = self.prepare_locale_for_graph(&graph)?;
        self.commit_installed_module_graph_with_locale(graph, locale);
        Ok(())
    }

    pub(in crate::shell) fn commit_installed_module_graph_with_locale(
        &mut self,
        graph: InstalledModuleGraph,
        locale: LocaleEngine,
    ) {
        self.sync_module_graph_health(&graph);
        self.locale = locale;
        self.installed_module_graph = Some(graph);
    }

    /// Project immutable graph diagnostics into the live module records. The
    /// graph remains static and reloadable; runtime failures are recorded on
    /// the corresponding ModuleInstance instead of mutating this snapshot.
    fn sync_module_graph_health(&mut self, graph: &InstalledModuleGraph) {
        for (module_id, module) in &mut self.modules {
            let Some(node) = graph.module(module_id) else {
                continue;
            };
            let diagnostics = graph
                .diagnostics()
                .iter()
                .filter(|diagnostic| diagnostic.module_id == *module_id)
                .map(|diagnostic| (diagnostic.status.as_str(), diagnostic.message.as_str()));
            let health_records = graph
                .health()
                .iter()
                .filter(|record| record.module_id == *module_id)
                .map(|record| (record.status.as_str(), record.message.as_str()));

            let mut unavailable_reason = None;
            let mut degraded_reason = None;
            for (status, message) in diagnostics.chain(health_records) {
                if status.contains("required")
                    || status.contains("missing")
                    || status.contains("blocked")
                    || status.contains("conflict")
                    || status == "provider_unavailable"
                    || status == "interface_unavailable"
                {
                    unavailable_reason.get_or_insert(message.to_string());
                } else if status.contains("optional")
                    || status == "interface_unconfigured"
                    || status == "provider_degraded"
                {
                    degraded_reason.get_or_insert(message.to_string());
                }
            }

            let health = if !node.enabled {
                ModuleHealthRecord::unavailable("module disabled by the installed graph", false)
            } else if let Some(reason) = unavailable_reason {
                ModuleHealthRecord::unavailable(reason, true)
            } else if let Some(reason) = degraded_reason {
                ModuleHealthRecord::degraded(reason)
            } else {
                ModuleHealthRecord::healthy()
            };
            module.set_static_health(health);
        }
    }

    pub(in crate::shell) fn register_interfaces_from_graph(
        &mut self,
        graph: &InstalledModuleGraph,
    ) {
        let mut settings = self.settings_store.as_ref().clone();
        match register_graph_settings_schemas(&mut settings, graph) {
            Ok(()) => {
                self.settings = mesh_core_config::resolve_shell_locale_settings(settings.shell());
                self.settings_store = Arc::new(settings);
                mesh_core_config::log_settings_diagnostics(
                    "registered settings schemas",
                    self.settings_store.diagnostics(),
                );
            }
            Err(error) => tracing::warn!(
                "failed to register graph-owned settings schemas transactionally: {error}"
            ),
        }
        for contract in graph.interface_contracts().values() {
            self.interfaces.register_contract(contract.clone());
        }

        for provider in graph.backend_provider_contributions() {
            self.interfaces.register(InterfaceProvider {
                interface: canonical_interface_name(&provider.interface),
                version: provider.version.clone(),
                base_module: provider.base_module.clone(),
                provider_module: provider.module_id.clone(),
                backend_name: provider
                    .provider
                    .clone()
                    .unwrap_or_else(|| provider.module_id.clone()),
                priority: provider.priority,
            });
        }
    }

    fn register_loaded_module(&mut self, dir: &Path, loaded: mesh_core_module::LoadedManifest) {
        let id = loaded.manifest.package.id.clone();
        tracing::info!(
            "discovered module: {} v{} ({}) from {}",
            id,
            loaded.manifest.package.version,
            loaded.manifest.package.module_type,
            loaded.source
        );
        self.modules.insert(
            id,
            ModuleInstance::new(
                loaded.manifest,
                dir.to_path_buf(),
                loaded.path,
                loaded.source,
            ),
        );
    }

    pub fn resolve_modules(&mut self) -> Result<(), ShellRunError> {
        let active_graph = self.load_installed_module_graph_cached()?.clone();
        let resources = self.prepare_resource_snapshot(&active_graph, &self.settings_store)?;
        self.commit_resource_snapshot(&resources)?;
        self.sync_module_graph_health(&active_graph);
        let mut effective_capabilities = HashMap::with_capacity(self.modules.len());
        for (module_id, module) in &self.modules {
            if !active_graph
                .module(module_id)
                .is_some_and(|node| node.enabled)
            {
                continue;
            }
            let effective = self.capability_policy.resolve(
                module_id,
                &module.manifest.capabilities.required,
                &module.manifest.capabilities.optional,
            )?;
            effective_capabilities.insert(module_id.clone(), effective);
        }
        self.effective_capabilities = Arc::new(effective_capabilities);
        let ids: Vec<String> = self.modules.keys().cloned().collect();
        for id in ids {
            if let Some(module) = self.modules.get_mut(&id) {
                if module.state == ModuleState::Discovered && active_graph.module(&id).is_some() {
                    if let Err(e) = module.transition(ModuleState::Resolved) {
                        tracing::warn!("failed to resolve module {id}: {e}");
                    }
                }
            }
        }
        Ok(())
    }

    pub fn module(&self, id: &str) -> Option<&ModuleInstance> {
        self.modules.get(id)
    }

    pub fn modules(&self) -> impl Iterator<Item = (&str, ModuleState)> {
        self.modules
            .iter()
            .map(|(id, inst)| (id.as_str(), inst.state))
    }

    pub(super) fn load_frontend_components(&mut self) -> Result<(), ShellRunError> {
        if !self.components.is_empty() {
            return Ok(());
        }

        let graph = self.load_installed_module_graph_cached()?.clone();
        let frontend_catalog = FrontendCatalog::from_modules(&self.modules, Some(&graph))?;
        self.frontend_catalog.replace(frontend_catalog, None);
        let frontend_catalog = self.frontend_catalog.snapshot().catalog;
        let enabled_frontends = self.installed_enabled_frontend_ids();
        let locale_catalog_snapshot = self.locale.catalog_snapshot();
        let interface_catalog = std::sync::Arc::new(self.interfaces.resolved_catalog());
        if let Some(profile_id) = self.active_profile_id.clone() {
            let paths = mesh_core_module::package::ProfilePaths::from_root_graph(
                &self.installed_module_graph_path(),
            )?;
            let profile = paths.load(&profile_id)?;
            let entries = frontend_catalog
                .top_level_surfaces()
                .into_iter()
                .map(|entry| (entry.compiled.manifest.package.id.clone(), entry))
                .collect::<HashMap<_, _>>();
            for (instance_id, root) in profile.roots.iter().filter(|(_, root)| root.active) {
                let entry = entries.get(&root.module).ok_or_else(|| {
                    ShellRunError::FrontendComposition {
                        message: format!(
                            "profile root {instance_id} has no mountable frontend entrypoint"
                        ),
                    }
                })?;
                self.register_component(Box::new(
                    FrontendSurfaceComponent::new(
                        entry.compiled.clone(),
                        entry.module_dir.clone(),
                        self.frontend_catalog.clone(),
                        interface_catalog.clone(),
                        self.settings_store.clone(),
                    )
                    .with_effective_capabilities(self.effective_capabilities.clone())
                    .with_instance_id(instance_id)
                    .with_locale_catalog_snapshot(locale_catalog_snapshot.clone()),
                ));
            }
            return Ok(());
        }
        for entry in frontend_catalog.top_level_surfaces_filtered(Some(&enabled_frontends)) {
            self.register_component(Box::new(
                FrontendSurfaceComponent::new(
                    entry.compiled,
                    entry.module_dir,
                    self.frontend_catalog.clone(),
                    interface_catalog.clone(),
                    self.settings_store.clone(),
                )
                .with_effective_capabilities(self.effective_capabilities.clone())
                .with_locale_catalog_snapshot(locale_catalog_snapshot.clone()),
            ));
        }

        Ok(())
    }

    pub(in crate::shell) fn activate_frontend_module(
        &mut self,
        module_id: &str,
        graph: &InstalledModuleGraph,
    ) -> Result<VecDeque<CoreRequest>, ShellRunError> {
        let locale = self.prepare_locale_for_graph(graph)?;
        self.activate_frontend_module_with_locale(module_id, graph, locale)
    }

    pub(in crate::shell) fn activate_frontend_module_with_locale(
        &mut self,
        module_id: &str,
        graph: &InstalledModuleGraph,
        locale: LocaleEngine,
    ) -> Result<VecDeque<CoreRequest>, ShellRunError> {
        let previous_catalog = self.frontend_catalog.snapshot().catalog;
        let catalog = FrontendCatalog::from_modules_reusing(
            &self.modules,
            Some(graph),
            Some(&previous_catalog),
        )?;
        let entry = catalog
            .top_level_surfaces()
            .into_iter()
            .find(|entry| entry.compiled.manifest.package.id == module_id);
        let previous_catalog = self.frontend_catalog.replace(catalog, None);

        let Some(entry) = entry else {
            // Widgets and component-only frontend packages own no surface.
            self.locale = locale;
            self.sync_frontend_catalog_components();
            return Ok(VecDeque::new());
        };
        let instance_ids = if let Some(profile_id) = &self.active_profile_id {
            let paths = mesh_core_module::package::ProfilePaths::from_root_graph(
                &self.installed_module_graph_path(),
            )?;
            paths
                .load(profile_id)?
                .roots
                .into_iter()
                .filter(|(_, root)| root.active && root.module == module_id)
                .map(|(instance_id, _)| instance_id)
                .collect::<Vec<_>>()
        } else {
            vec![entry.compiled.surface_id().to_string()]
        };
        let existing = self
            .components
            .iter()
            .map(|runtime| runtime.surface_id.clone())
            .collect::<HashSet<_>>();
        let mounted = (|| {
            let interface_catalog = std::sync::Arc::new(self.interfaces.resolved_catalog());
            let mut mounted = Vec::new();
            for instance_id in instance_ids {
                if existing.contains(&instance_id) {
                    continue;
                }
                if let Some(module) = self.modules.get_mut(module_id) {
                    if let Err(error) = module.mark_loaded() {
                        tracing::warn!(
                            module_id,
                            "frontend activation did not load module: {error}"
                        );
                    }
                }
                let mut component = FrontendSurfaceComponent::new(
                    entry.compiled.clone(),
                    entry.module_dir.clone(),
                    self.frontend_catalog.clone(),
                    interface_catalog.clone(),
                    self.settings_store.clone(),
                )
                .with_effective_capabilities(self.effective_capabilities.clone())
                .with_instance_id(&instance_id)
                .with_locale_catalog_snapshot(locale.catalog_snapshot());
                let diagnostics = self
                    .diagnostics
                    .register_instance(module_id.to_string(), instance_id.clone());
                let mut requests = VecDeque::from(
                    component
                        .mount(ComponentContext {
                            component_id: module_id.to_string(),
                            surface_id: instance_id,
                            diagnostics,
                        })
                        .map_err(ShellRunError::Component)?,
                );
                component
                    .locale_changed(&locale)
                    .map_err(ShellRunError::Component)?;
                for state in self.latest_service_state.values() {
                    let event = ServiceEvent::Updated {
                        service: state.interface.clone(),
                        source_module: state.provider_id.clone(),
                        payload: state.state.clone(),
                    };
                    if component.observes_service_event(&event) {
                        requests.extend(
                            component
                                .handle_service_event_with_generation(&event, state.generation)
                                .map_err(ShellRunError::Component)?,
                        );
                    }
                }
                mounted.push((component, requests));
            }
            Ok::<_, ShellRunError>(mounted)
        })();
        let mounted = match mounted {
            Ok(mounted) => mounted,
            Err(error) => {
                if let Some(module) = self.modules.get_mut(module_id) {
                    let _ = module.mark_failed(error.to_string());
                }
                let candidate_version = previous_catalog.version.wrapping_add(1);
                if !self
                    .frontend_catalog
                    .restore_if_current(candidate_version, previous_catalog)
                {
                    tracing::warn!(
                        module_id,
                        "skipping frontend catalog rollback because a newer generation is active"
                    );
                }
                return Err(error);
            }
        };
        let mut requests = VecDeque::new();
        for (component, component_requests) in mounted {
            let module_id = component.id().to_string();
            requests.extend(component_requests);
            self.register_component(Box::new(component));
            if let Some(module) = self.modules.get_mut(&module_id) {
                let _ = module.mark_initialized();
                if let Err(error) = module.mark_running() {
                    tracing::warn!(
                        module_id,
                        "frontend activation lifecycle transition failed: {error}"
                    );
                }
            }
        }
        self.locale = locale;
        self.sync_frontend_catalog_components();
        tracing::info!(module_id, "activated frontend module live");
        Ok(requests)
    }

    pub(in crate::shell) fn deactivate_frontend_module(
        &mut self,
        module_id: &str,
        graph: Option<&InstalledModuleGraph>,
    ) -> Result<VecDeque<CoreRequest>, ShellRunError> {
        let locale = match graph {
            Some(graph) => self.prepare_locale_for_graph(graph)?,
            None => self.locale.clone(),
        };
        let previous_catalog = self.frontend_catalog.snapshot().catalog;
        let catalog =
            FrontendCatalog::from_modules_reusing(&self.modules, graph, Some(&previous_catalog))?;
        self.frontend_catalog.replace(catalog, None);
        self.sync_frontend_catalog_components();

        let indices = self
            .components
            .iter()
            .enumerate()
            .filter(|(_, runtime)| runtime.component.id() == module_id)
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        if indices.is_empty() {
            self.locale = locale;
            return Ok(VecDeque::new());
        }
        let mut removed_surfaces = Vec::new();
        for index in indices.into_iter().rev() {
            let surface_id = self.components[index].surface_id.clone();
            let module_id = self.components[index].component.id().to_string();
            if let Err(error) = self.components[index].component.unmount() {
                tracing::warn!(
                    module_id,
                    error = %error,
                    "frontend deactivation unmount failed"
                );
            }
            if let Some(module) = self.modules.get_mut(&module_id)
                && let Err(error) = module.mark_unloaded()
            {
                tracing::warn!(
                    module_id,
                    "frontend unload lifecycle transition failed: {error}"
                );
            }
            self.destroy_all_child_surfaces(index);
            self.presentation_engine.destroy_surface(&surface_id);
            self.components.remove(index);
            self.diagnostics.unregister(&module_id, &surface_id);
            self.core.surfaces.remove(&surface_id);
            self.surfaces.remove(&surface_id);
            self.pending_popover_hides.remove(&surface_id);
            self.transfer_owned_keyboard_modes.remove(&surface_id);
            if self.keyboard_focus_surface.as_deref() == Some(surface_id.as_str()) {
                self.keyboard_focus_surface = None;
            }
            removed_surfaces.push(surface_id);
        }
        self.rebuild_component_surface_index();
        self.service_delivery_index.mark_dirty();
        self.locale = locale;
        tracing::info!(module_id, "deactivated frontend module live");
        let mut requests = VecDeque::new();
        for surface_id in removed_surfaces {
            match self.broadcast_core_event(CoreEvent::SurfaceVisibilityChanged {
                surface_id,
                visible: false,
            }) {
                Ok(next) => requests.extend(next),
                Err(error) => tracing::warn!(
                    module_id,
                    "frontend was disabled but its visibility notification failed: {error}"
                ),
            }
        }
        Ok(requests)
    }

    pub(super) fn unmount_components(&mut self) -> VecDeque<CoreRequest> {
        let mut requests = VecDeque::new();
        for runtime in &mut self.components {
            match runtime.component.unmount() {
                Ok(component_requests) => requests.extend(component_requests),
                Err(error) => tracing::warn!(
                    component_id = runtime.component.id(),
                    error = %error,
                    "frontend shutdown unmount failed"
                ),
            }
        }
        requests
    }

    pub(in crate::shell) fn sync_frontend_catalog_components(&mut self) -> bool {
        let mut invalidated = false;
        for runtime in &mut self.components {
            invalidated |= runtime.component.frontend_catalog_changed();
        }
        if invalidated {
            self.service_delivery_index.mark_dirty();
            self.components_want_render = true;
        }
        invalidated
    }

    fn installed_enabled_frontend_ids(&self) -> HashSet<String> {
        let graph = self
            .installed_module_graph
            .as_ref()
            .expect("frontend loading requires a validated installed module graph");
        let mut enabled = graph
            .frontend_modules()
            .into_iter()
            .map(|module| module.id.clone())
            .collect::<HashSet<_>>();
        enabled.insert(BUILTIN_DEBUG_INSPECTOR_ID.to_string());
        enabled
    }

    pub(super) fn register_component(&mut self, component: Box<dyn ShellComponent>) {
        let surface_id = component.surface_id().to_string();
        let initial_visibility = component
            .initial_visibility()
            .unwrap_or_else(default_surface_visibility);
        self.core
            .surfaces
            .entry(surface_id.clone())
            .or_insert_with(|| SurfaceState {
                visible: initial_visibility,
                closing_until: None,
            });
        self.surfaces.entry(surface_id.clone()).or_default();
        let component_index = self.components.len();
        self.components.push(ComponentRuntime::new(component));
        self.component_by_surface
            .insert(surface_id, component_index);
        self.service_delivery_index.mark_dirty();
    }

    pub(super) fn mount_components(&mut self) -> Result<VecDeque<CoreRequest>, ShellRunError> {
        let mut requests = VecDeque::new();
        for runtime in &mut self.components {
            let module_id = runtime.component.id().to_string();
            if let Some(module) = self.modules.get_mut(&module_id)
                && let Err(error) = module.mark_loaded()
            {
                tracing::warn!(module_id, "frontend mount did not load module: {error}");
            }
            let diagnostics = self
                .diagnostics
                .register_instance(module_id.clone(), runtime.surface_id.clone());
            let ctx = ComponentContext {
                component_id: module_id.clone(),
                surface_id: runtime.surface_id.clone(),
                diagnostics,
            };
            match runtime.component.mount(ctx) {
                Ok(component_requests) => {
                    requests.extend(component_requests);
                    if let Some(module) = self.modules.get_mut(&module_id) {
                        let _ = module.mark_initialized();
                        if let Err(error) = module.mark_running() {
                            tracing::warn!(
                                module_id,
                                "frontend mount lifecycle transition failed: {error}"
                            );
                        }
                    }
                }
                Err(error) => {
                    if let Some(module) = self.modules.get_mut(&module_id) {
                        let _ = module.mark_failed(error.to_string());
                    }
                    return Err(ShellRunError::Component(error));
                }
            }
        }
        // Mount first so module scripts can establish their service proxy;
        // then deliver the revisioned effective settings snapshot normally.
        requests.extend(self.sync_settings_service_state()?);
        requests.extend(self.sync_composition_service_state()?);
        self.service_delivery_index.mark_dirty();
        Ok(requests)
    }
}
