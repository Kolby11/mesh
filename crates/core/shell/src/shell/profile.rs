use super::backend::{
    BackendLaunchCandidate, BackendRuntimeStatus,
    backend_launch_candidates_from_graph_with_capabilities,
};
use super::component::{
    FrontendCatalog, FrontendCatalogHandle, FrontendCatalogState, FrontendSurfaceComponent,
};
use super::*;
use mesh_core_module::package::{
    ComponentPlacement, InstalledModuleGraph, NodeSlotOverride, PackageTransaction, ProfilePaths,
    ProfileRootInstance, ShellProfile, load_authoring_snapshot_for_profile,
};
use std::collections::{HashMap, HashSet, VecDeque};
use std::hash::{Hash, Hasher};

pub(super) struct PreparedProfileFrontend {
    component: FrontendSurfaceComponent,
    requests: VecDeque<CoreRequest>,
}

#[cfg(test)]
mod tests {
    use super::super::discovery::effective_profile_settings;
    use super::{CandidatePreview, CandidatePreviewSurface, Shell, update_module_prop_override};
    use mesh_core_config::SettingsStore;
    use mesh_core_module::package::ShellProfile;
    use std::sync::Arc;

    #[test]
    fn profile_preferences_layer_over_shared_settings_without_mutating_them() {
        let shared = SettingsStore::from_value(
            "/tmp/shared-settings.json",
            serde_json::json!({
                "shell": { "theme": { "active": "mesh-default-light" } },
                "@test/panel": { "props": { "global": { "density": "comfortable" } } }
            }),
        )
        .unwrap();
        let profile = ShellProfile::from_json_str(
            r#"{
                "schemaVersion": 3,
                "roots": {},
                "settings": {
                    "shell": { "theme": { "active": "mesh-default-dark" } },
                    "@test/panel": { "props": { "global": { "density": "compact" } } }
                }
            }"#,
        )
        .unwrap();

        let effective = effective_profile_settings(shared.clone(), Some(&profile)).unwrap();
        assert_eq!(effective.shell().theme.active, "mesh-default-dark");
        assert_eq!(
            effective.namespace("@test/panel")["props"]["global"]["density"],
            "compact"
        );
        assert_eq!(shared.shell().theme.active, "mesh-default-light");
    }

    #[test]
    fn failed_profile_recovery_restores_only_the_candidate_active_pointer() {
        let directory = tempfile::tempdir().unwrap();
        let graph_path = directory.path().join("module.json");
        std::fs::write(&graph_path, "{}").unwrap();
        let paths = mesh_core_module::package::ProfilePaths::from_root_graph(&graph_path).unwrap();
        paths
            .save("old", &ShellProfile::new())
            .expect("save old profile");
        paths
            .save("new", &ShellProfile::new())
            .expect("save new profile");
        paths.set_active("new").unwrap();

        Shell::restore_active_profile_if_current(&paths, "new", Some("old")).unwrap();
        assert_eq!(paths.active_profile_id().unwrap().as_deref(), Some("old"));

        paths.set_active("old").unwrap();
        let error = Shell::restore_active_profile_if_current(&paths, "new", None).unwrap_err();
        assert!(error.contains("changed to 'old'"));
        assert_eq!(paths.active_profile_id().unwrap().as_deref(), Some("old"));
    }

    #[test]
    fn module_prop_updates_preserve_siblings_and_reset_sparsely() {
        let mut namespace = serde_json::json!({
            "surface": { "anchor": "top" },
            "props": { "global": { "density": "compact" } }
        });

        update_module_prop_override(
            &mut namespace,
            None,
            "blur_enabled",
            Some(serde_json::json!(false)),
        );
        assert_eq!(
            namespace["props"]["global"],
            serde_json::json!({ "density": "compact", "blur_enabled": false })
        );

        update_module_prop_override(&mut namespace, None, "blur_enabled", None);
        update_module_prop_override(&mut namespace, None, "density", None);
        assert!(namespace.get("props").is_none());
        assert_eq!(namespace["surface"]["anchor"], serde_json::json!("top"));
    }

    #[test]
    fn instance_prop_updates_preserve_global_values_and_prune_empty_scopes() {
        let mut namespace = serde_json::json!({
            "props": { "global": { "density": "compact" } }
        });
        update_module_prop_override(
            &mut namespace,
            Some("@test/panel#bottom"),
            "density",
            Some(serde_json::json!("comfortable")),
        );
        assert_eq!(
            namespace["props"]["instances"]["@test/panel#bottom"]["density"],
            "comfortable"
        );

        update_module_prop_override(&mut namespace, Some("@test/panel#bottom"), "density", None);
        assert!(namespace["props"].get("instances").is_none());
        assert_eq!(namespace["props"]["global"]["density"], "compact");
    }

    #[test]
    fn instance_surface_override_is_scoped_to_the_profile_root_key() {
        let shared = SettingsStore::from_value(
            "/tmp/shared-settings.json",
            serde_json::json!({ "@test/panel": { "surface": { "anchor": "top" } } }),
        )
        .unwrap();
        let profile = ShellProfile::from_json_str(
            r#"{
                "schemaVersion": 3,
                "roots": {
                    "@test/panel#bottom": {
                        "module": "@test/panel",
                        "surface": { "anchor": "bottom" }
                    }
                }
            }"#,
        )
        .unwrap();

        let effective = effective_profile_settings(shared, Some(&profile)).unwrap();
        assert_eq!(
            effective.namespace("@test/panel#bottom")["surface"]["anchor"],
            "bottom"
        );
        assert_eq!(
            effective.namespace("@test/panel#other")["surface"]["anchor"],
            "top"
        );
    }

    #[test]
    fn candidate_preview_keeps_surfaces_hidden_until_all_backends_are_ready() {
        let preview = CandidatePreview {
            generation: 7,
            profile_id: "work".into(),
            surfaces: Arc::new(vec![CandidatePreviewSurface {
                surface_id: "@test/panel".into(),
                component_id: "@test/panel".into(),
                hidden: true,
            }]),
            required_backends: Arc::new(vec!["mesh.audio".into(), "mesh.network".into()]),
            ready_backends: Arc::new(Vec::new()),
            diagnostics: Arc::new(Vec::new()),
        };

        assert!(!preview.ready());
        assert!(
            preview
                .surfaces()
                .iter()
                .all(CandidatePreviewSurface::hidden)
        );
        let preview = preview.with_backend_ready("mesh.audio");
        assert!(!preview.ready());
        let preview = preview.with_backend_ready("mesh.network");
        assert!(preview.ready());
    }
}

/// The complete, immutable input to one live activation.
///
/// Runtime objects are deliberately not part of this value: they are
/// prepared against these snapshots and become visible only when the plan is
/// committed. Keeping the plan behind an `Arc` prevents the readiness path
/// from mutating the candidate that the commit path will publish.
pub(super) struct ActivationPlan {
    pub(super) generation: u64,
    pub(super) profile_id: String,
    pub(super) profile_revision: Option<u64>,
    pub(super) graph: InstalledModuleGraph,
    pub(super) interface_catalog: Arc<mesh_core_service::ResolvedServiceCatalog>,
    pub(super) locale: LocaleEngine,
    pub(super) settings: Arc<SettingsStore>,
    pub(super) resources: super::discovery::PreparedResourceSnapshot,
    pub(super) prepared_theme: Option<PreparedThemeState>,
    pub(super) catalog: FrontendCatalog,
    pub(super) effective_capabilities: Arc<HashMap<String, EffectiveCapabilities>>,
    pub(super) desired_surfaces: HashSet<String>,
    pub(super) root_modules: HashMap<String, String>,
    pub(super) desired_providers: HashMap<String, String>,
    pub(super) provider_candidates: Vec<BackendLaunchCandidate>,
}

/// A frontend surface prepared for an activation candidate. Candidate
/// components are mounted and can be inspected by the coordinator, but are
/// deliberately not registered in the live component/presentation maps until
/// the candidate commits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidatePreviewSurface {
    surface_id: String,
    component_id: String,
    hidden: bool,
}

impl CandidatePreviewSurface {
    pub fn surface_id(&self) -> &str {
        &self.surface_id
    }

    pub fn component_id(&self) -> &str {
        &self.component_id
    }

    pub fn hidden(&self) -> bool {
        self.hidden
    }
}

/// Read-only health/identity view of the prepared activation. It represents
/// shell-side hidden surfaces: no candidate surface is mapped or exposed to
/// the compositor before the active snapshot swap.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidatePreview {
    generation: u64,
    profile_id: String,
    surfaces: Arc<Vec<CandidatePreviewSurface>>,
    required_backends: Arc<Vec<String>>,
    ready_backends: Arc<Vec<String>>,
    diagnostics: Arc<Vec<String>>,
}

impl CandidatePreview {
    fn from_plan(plan: &ActivationPlan, required_backends: Vec<String>) -> Self {
        let mut surfaces = plan
            .root_modules
            .iter()
            .map(|(surface_id, component_id)| CandidatePreviewSurface {
                surface_id: surface_id.clone(),
                component_id: component_id.clone(),
                hidden: true,
            })
            .collect::<Vec<_>>();
        surfaces.sort_by(|left, right| left.surface_id.cmp(&right.surface_id));
        Self {
            generation: plan.generation,
            profile_id: plan.profile_id.clone(),
            surfaces: Arc::new(surfaces),
            required_backends: Arc::new(required_backends),
            ready_backends: Arc::new(Vec::new()),
            diagnostics: Arc::new(Vec::new()),
        }
    }

    pub(super) fn with_backend_ready(&self, interface: &str) -> Self {
        if self.ready_backends.iter().any(|ready| ready == interface) {
            return self.clone();
        }
        let mut ready_backends = (*self.ready_backends).clone();
        ready_backends.push(interface.to_string());
        Self {
            ready_backends: Arc::new(ready_backends),
            ..self.clone()
        }
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn profile_id(&self) -> &str {
        &self.profile_id
    }

    pub fn surfaces(&self) -> &[CandidatePreviewSurface] {
        self.surfaces.as_slice()
    }

    pub fn required_backends(&self) -> &[String] {
        self.required_backends.as_slice()
    }

    pub fn ready_backends(&self) -> &[String] {
        self.ready_backends.as_slice()
    }

    pub fn diagnostics(&self) -> &[String] {
        self.diagnostics.as_slice()
    }

    pub fn ready(&self) -> bool {
        self.required_backends
            .iter()
            .all(|interface| self.ready_backends.iter().any(|ready| ready == interface))
    }
}

/// Immutable runtime identity for a committed activation. All generation
/// owned mirrors are retained together so consumers cannot combine a graph
/// from one activation with providers, settings, or presentation metadata from
/// another.
pub struct ActiveSnapshot {
    generation: u64,
    profile_id: Option<String>,
    profile_revision: Option<u64>,
    plan: Arc<ActivationPlan>,
    initial_states: Arc<HashMap<String, serde_json::Value>>,
    settings: Arc<SettingsStore>,
    roots: Arc<HashMap<String, String>>,
    providers: Arc<HashMap<String, mesh_core_backend::BackendIdentity>>,
    frontend_catalog_revision: u64,
    settings_revision: u64,
    theme_revision: u64,
    locale_revision: u64,
    resource_generation: u64,
    watch_generation: u64,
    presentation_generations: Arc<HashMap<String, u64>>,
}

impl ActiveSnapshot {
    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn profile_id(&self) -> Option<&str> {
        self.profile_id.as_deref()
    }

    pub fn profile_revision(&self) -> Option<u64> {
        self.profile_revision
    }

    pub fn roots(&self) -> &HashMap<String, String> {
        &self.roots
    }

    pub fn graph(&self) -> &InstalledModuleGraph {
        &self.plan.graph
    }

    pub fn interface_catalog(&self) -> Arc<mesh_core_service::ResolvedServiceCatalog> {
        self.plan.interface_catalog.clone()
    }

    pub fn settings(&self) -> &SettingsStore {
        &self.settings
    }

    pub fn providers(&self) -> &HashMap<String, mesh_core_backend::BackendIdentity> {
        &self.providers
    }

    pub fn initial_state(&self, interface: &str) -> Option<&serde_json::Value> {
        self.initial_states.get(interface)
    }

    pub fn frontend_catalog_revision(&self) -> u64 {
        self.frontend_catalog_revision
    }

    pub fn settings_revision(&self) -> u64 {
        self.settings_revision
    }

    pub fn theme_revision(&self) -> u64 {
        self.theme_revision
    }

    pub fn locale_revision(&self) -> u64 {
        self.locale_revision
    }

    pub fn resource_generation(&self) -> u64 {
        self.resource_generation
    }

    pub fn watch_generation(&self) -> u64 {
        self.watch_generation
    }

    pub fn presentation_generation(&self, surface_id: &str) -> Option<u64> {
        self.presentation_generations.get(surface_id).copied()
    }
}

pub(super) struct PendingProfileSwitch {
    plan: Arc<ActivationPlan>,
    prepared_frontends: Vec<PreparedProfileFrontend>,
    pub(in crate::shell) candidate_backends: HashMap<String, BackendRuntimeSlot>,
    waiting_backends: HashSet<String>,
    candidate_started: HashSet<String>,
    candidate_initial_states: HashMap<String, serde_json::Value>,
    persist_active_profile: bool,
    rollback: Option<crate::shell::module_config::ModuleConfigRollback>,
    /// A package mutation stays journaled until this prepared runtime is
    /// committed. Dropping it before that point restores the package state.
    pub(in crate::shell) package_transaction: Option<PackageTransaction>,
    pub(in crate::shell) package_rollback: Option<PackageRuntimeRollback>,
    profile_switch_ack: Option<super::types::IpcProfileSwitchResponseSender>,
}

pub(super) struct PendingResourcePreparation {
    profile_id: String,
    profile: ShellProfile,
    graph: InstalledModuleGraph,
    locale: LocaleEngine,
    settings: Arc<SettingsStore>,
    pub(in crate::shell) resource_job: super::discovery::ResourcePreparationJob,
    /// Legacy graph activation has no durable profile pointer to update.
    /// Keeping this bit with the pending work makes the same coordinator safe
    /// for both profile and no-profile graph deltas.
    persist_active_profile: bool,
    pub(in crate::shell) rollback: Option<crate::shell::module_config::ModuleConfigRollback>,
    /// The package journal spans asynchronous resource preparation as well as
    /// the later runtime activation commit.
    pub(in crate::shell) package_transaction: Option<PackageTransaction>,
    pub(in crate::shell) package_rollback: Option<PackageRuntimeRollback>,
    profile_switch_ack: Option<super::types::IpcProfileSwitchResponseSender>,
}

const LEGACY_ACTIVATION_ID: &str = "@mesh/legacy-graph";

/// In-memory mirrors that package service operations update while their
/// journal is still open. Runtime candidates are prepared from those mirrors,
/// so aborting the journal must restore them along with the protected files.
pub(super) struct PackageRuntimeRollback {
    installed_module_graph: Option<InstalledModuleGraph>,
    resource_snapshot: Arc<super::discovery::ResourceSnapshot>,
    resource_explanation: Arc<mesh_core_resources::ResourceExplanationSnapshot>,
    font_registry: mesh_core_resources::FontRegistry,
    font_renderer_revision: u64,
    settings: ShellSettings,
    settings_store: Arc<SettingsStore>,
    control_plane_revision: DurableControlPlaneRevision,
    theme: ThemeEngine,
    locale: LocaleEngine,
    interfaces: mesh_core_service::ResolvedServiceCatalog,
    effective_capabilities: Arc<HashMap<String, EffectiveCapabilities>>,
    composition_mode: ShellCompositionMode,
    active_profile_id: Option<String>,
    frontend_catalog: FrontendCatalogState,
    module_dirs: Vec<PathBuf>,
    last_published_theme_snapshot: Option<mesh_core_theme::ThemeSnapshot>,
    theme_watch: ThemeWatchState,
}

impl PackageRuntimeRollback {
    pub(in crate::shell) fn capture(shell: &Shell) -> Self {
        Self {
            installed_module_graph: shell.installed_module_graph.clone(),
            resource_snapshot: shell.resource_snapshot.clone(),
            resource_explanation: shell.resource_explanation.clone(),
            font_registry: shell.font_registry.clone(),
            font_renderer_revision: shell.font_renderer_revision,
            settings: shell.settings.clone(),
            settings_store: shell.settings_store.clone(),
            control_plane_revision: shell.control_plane_revision,
            theme: shell.theme.clone(),
            locale: shell.locale.clone(),
            interfaces: shell.interfaces.resolved_catalog(),
            effective_capabilities: shell.effective_capabilities.clone(),
            composition_mode: shell.composition_mode.clone(),
            active_profile_id: shell.active_profile_id.clone(),
            frontend_catalog: shell.frontend_catalog.snapshot(),
            module_dirs: shell.module_dirs.clone(),
            last_published_theme_snapshot: shell.last_published_theme_snapshot.clone(),
            theme_watch: shell.theme_watch.clone(),
        }
    }

    fn restore(self, shell: &mut Shell) {
        let Self {
            installed_module_graph,
            resource_snapshot,
            resource_explanation,
            font_registry,
            font_renderer_revision,
            settings,
            settings_store,
            control_plane_revision,
            theme,
            locale,
            interfaces,
            effective_capabilities,
            composition_mode,
            active_profile_id,
            frontend_catalog,
            module_dirs,
            last_published_theme_snapshot,
            theme_watch,
        } = self;

        // Rebuild module instances from the journal-restored graph before
        // putting back the exact snapshots captured at package-operation
        // entry. This also returns resolved module state after an uninstall
        // removed an instance from the live map.
        shell.composition_mode = composition_mode.clone();
        shell.active_profile_id = active_profile_id.clone();
        shell.installed_module_graph = installed_module_graph.clone();
        shell.module_dirs = module_dirs;
        shell.discover_modules();
        if let Err(error) = shell.resolve_modules() {
            tracing::warn!("failed to re-resolve restored package state: {error}");
        }

        shell.installed_module_graph = installed_module_graph;
        let icon_registry = resource_snapshot.icon_registry.clone();
        shell.resource_snapshot = resource_snapshot;
        mesh_core_icon::replace_default_registry(icon_registry);
        shell.resource_explanation = resource_explanation;
        shell.font_registry = font_registry;
        shell.font_renderer_revision = font_renderer_revision;
        shell.settings = settings;
        shell.settings_store = settings_store;
        shell.control_plane_revision = control_plane_revision;
        shell.theme = theme;
        shell.locale = locale;
        shell.interfaces.replace_catalog(interfaces);
        shell.effective_capabilities = effective_capabilities;
        shell.composition_mode = composition_mode;
        shell.active_profile_id = active_profile_id;
        let current_catalog_version = shell.frontend_catalog.snapshot().version;
        shell
            .frontend_catalog
            .restore_if_current(current_catalog_version, frontend_catalog);
        shell.last_published_theme_snapshot = last_published_theme_snapshot;
        shell.theme_watch = theme_watch;
    }
}

fn restore_module_config(rollback: crate::shell::module_config::ModuleConfigRollback) {
    if let Err(error) = rollback.restore() {
        tracing::error!("failed to restore graph activation decision: {error}");
    }
}

fn restore_module_config_if_present(
    rollback: Option<crate::shell::module_config::ModuleConfigRollback>,
) {
    if let Some(rollback) = rollback {
        restore_module_config(rollback);
    }
}

pub(in crate::shell) fn abort_package_transaction(
    package_transaction: Option<PackageTransaction>,
    package_rollback: Option<PackageRuntimeRollback>,
    shell: &mut Shell,
) {
    if let Some(package_transaction) = package_transaction
        && let Err(error) = package_transaction.abort()
    {
        tracing::error!("failed to abort package transaction: {error}");
    }
    if let Some(package_rollback) = package_rollback {
        package_rollback.restore(shell);
    }
}

pub(super) fn interface_catalog_for_graph(
    base: &mesh_core_service::ResolvedServiceCatalog,
    graph: &InstalledModuleGraph,
) -> Result<mesh_core_service::ResolvedServiceCatalog, ShellRunError> {
    // Start from the immutable core catalog instead of the live
    // published snapshot. The latter may contain providers or contracts from an older
    // graph generation, which would make a rejected or disabled module
    // reachable after activation.
    let mut catalog = base.to_builder();
    for (interface, contract) in graph.interface_contracts() {
        let compiled = contract
            .compile(mesh_core_service::DeclarationProvenance::new(
                "graph", interface,
            ))
            .map_err(|error| ShellRunError::FrontendComposition {
                message: format!("candidate interface '{interface}' is invalid: {error}"),
            })?;
        catalog.replace_contract(compiled);
    }

    let mut graph_provider_modules = HashMap::<String, Vec<String>>::new();
    for provider in graph.backend_provider_contributions() {
        let interface = mesh_core_service::canonical_interface_name(&provider.interface);
        graph_provider_modules
            .entry(interface.clone())
            .or_default()
            .push(provider.module_id.clone());
        catalog.register_provider(mesh_core_service::InterfaceProvider {
            interface,
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
    for (interface, provider_modules) in graph_provider_modules {
        catalog.set_graph_provider_modules(&interface, provider_modules);
        catalog.set_active_provider(
            &interface,
            graph
                .active_provider(&interface)
                .map(|provider| provider.module_id.clone()),
        );
    }
    Ok(catalog.build())
}

fn candidate_capabilities(
    policy: &CapabilityPolicy,
    graph: &InstalledModuleGraph,
    modules: &HashMap<String, ModuleInstance>,
) -> Result<Arc<HashMap<String, EffectiveCapabilities>>, ShellRunError> {
    let mut effective = HashMap::with_capacity(modules.len());
    for (module_id, module) in modules {
        if !graph.module(module_id).is_some_and(|node| node.enabled) {
            continue;
        }
        effective.insert(
            module_id.clone(),
            policy.resolve(
                module_id,
                &module.manifest.capabilities.required,
                &module.manifest.capabilities.optional,
            )?,
        );
    }
    Ok(Arc::new(effective))
}

impl Shell {
    /// Route a validated installed-graph replacement through the activation
    /// coordinator. Profile mode reloads the active composition, while the
    /// migration-era no-profile mode derives the legacy root set from the
    /// candidate graph. Both modes prepare resources, frontends, interfaces,
    /// and backend providers before one commit point.
    pub(in crate::shell) fn activate_graph_candidate(
        &mut self,
        graph: InstalledModuleGraph,
    ) -> VecDeque<CoreRequest> {
        self.activate_graph_candidate_with_package_transaction(graph, None, None)
    }

    pub(in crate::shell) fn activate_graph_candidate_with_package_transaction(
        &mut self,
        graph: InstalledModuleGraph,
        package_transaction: Option<PackageTransaction>,
        package_rollback: Option<PackageRuntimeRollback>,
    ) -> VecDeque<CoreRequest> {
        if self.profile_transition_pending() {
            tracing::warn!("graph activation rejected while another activation is pending");
            abort_package_transaction(package_transaction, package_rollback, self);
            return VecDeque::new();
        }
        if !self.pending_backend_runtimes.is_empty() {
            tracing::warn!("graph activation rejected while a provider switch is pending");
            abort_package_transaction(package_transaction, package_rollback, self);
            return VecDeque::new();
        }

        let selection =
            super::discovery::load_configured_profile(&self.installed_module_graph_path());
        match selection {
            Ok(Some((profile_id, _))) => self.begin_profile_activation_with_package_transaction(
                &profile_id,
                None,
                package_transaction,
                package_rollback,
            ),
            Ok(None) => self.begin_legacy_graph_activation_with_package_transaction(
                graph,
                None,
                package_transaction,
                package_rollback,
            ),
            Err(error) => {
                abort_package_transaction(package_transaction, package_rollback, self);
                let message =
                    format!("configured shell graph/profile could not be activated: {error}");
                if self.installed_module_graph.is_none() {
                    self.enter_composition_recovery(message);
                } else {
                    self.diagnostics.record_lifecycle_error(
                        "@mesh/shell",
                        "configured_composition_recovery",
                        message,
                    );
                }
                VecDeque::new()
            }
        }
    }

    /// Re-read the installed graph and activate it only when its normalized
    /// graph state changed. Source-only changes remain on the frontend reload
    /// path; module, provider, contribution, and resource deltas all enter the
    /// same activation coordinator.
    pub(in crate::shell) fn reconcile_installed_graph(&mut self) -> VecDeque<CoreRequest> {
        let candidate = match self.load_installed_module_graph_candidate() {
            Ok(candidate) => candidate,
            Err(error) => {
                let message = format!(
                    "configured shell graph/profile change was rejected; retaining active graph: {error}"
                );
                tracing::warn!("{message}");
                if self.installed_module_graph.is_none() {
                    self.enter_composition_recovery(message);
                } else {
                    self.diagnostics.record_lifecycle_error(
                        "@mesh/shell",
                        "configured_composition_recovery",
                        message,
                    );
                }
                return VecDeque::new();
            }
        };
        if self
            .installed_module_graph
            .as_ref()
            .is_some_and(|active| active.diff(&candidate).is_empty())
        {
            return VecDeque::new();
        }
        self.activate_graph_candidate(candidate)
    }

    pub(in crate::shell) fn begin_legacy_graph_activation(
        &mut self,
        graph: InstalledModuleGraph,
        rollback: Option<crate::shell::module_config::ModuleConfigRollback>,
    ) -> VecDeque<CoreRequest> {
        self.begin_legacy_graph_activation_with_package_transaction(graph, rollback, None, None)
    }

    pub(in crate::shell) fn begin_legacy_graph_activation_with_package_transaction(
        &mut self,
        graph: InstalledModuleGraph,
        mut rollback: Option<crate::shell::module_config::ModuleConfigRollback>,
        package_transaction: Option<PackageTransaction>,
        package_rollback: Option<PackageRuntimeRollback>,
    ) -> VecDeque<CoreRequest> {
        if !self.pending_backend_runtimes.is_empty() {
            abort_package_transaction(package_transaction, package_rollback, self);
            if let Some(rollback) = rollback {
                restore_module_config(rollback);
            }
            self.reject_profile_switch(
                LEGACY_ACTIVATION_ID,
                "a provider switch is already being prepared".into(),
            );
            return VecDeque::new();
        }
        let settings = self.settings_store.clone();
        let locale = match self.prepare_locale_for_graph(&graph) {
            Ok(locale) => locale,
            Err(error) => {
                abort_package_transaction(package_transaction, package_rollback, self);
                if let Some(rollback) = rollback.take() {
                    restore_module_config(rollback);
                }
                self.reject_profile_switch(LEGACY_ACTIVATION_ID, error.to_string());
                return VecDeque::new();
            }
        };
        let resource_job = match self.start_resource_preparation_job(&graph, &settings) {
            Ok(job) => job,
            Err(error) => {
                abort_package_transaction(package_transaction, package_rollback, self);
                if let Some(rollback) = rollback {
                    restore_module_config(rollback);
                }
                self.reject_profile_switch(LEGACY_ACTIVATION_ID, error.to_string());
                return VecDeque::new();
            }
        };
        self.pending_resource_preparation = Some(PendingResourcePreparation {
            profile_id: LEGACY_ACTIVATION_ID.to_string(),
            profile: ShellProfile::new(),
            graph,
            locale,
            settings,
            resource_job,
            persist_active_profile: false,
            rollback: rollback.take(),
            package_transaction,
            package_rollback,
            profile_switch_ack: None,
        });
        self.poll_pending_resource_preparation()
    }

    pub(in crate::shell) fn begin_profile_activation(
        &mut self,
        profile_id: &str,
        rollback: Option<crate::shell::module_config::ModuleConfigRollback>,
    ) -> VecDeque<CoreRequest> {
        self.begin_profile_activation_with_package_transaction(profile_id, rollback, None, None)
    }

    pub(in crate::shell) fn begin_profile_activation_with_package_transaction(
        &mut self,
        profile_id: &str,
        rollback: Option<crate::shell::module_config::ModuleConfigRollback>,
        package_transaction: Option<PackageTransaction>,
        package_rollback: Option<PackageRuntimeRollback>,
    ) -> VecDeque<CoreRequest> {
        self.begin_profile_activation_with_package_transaction_and_ack(
            profile_id,
            rollback,
            package_transaction,
            package_rollback,
            None,
        )
    }

    fn begin_profile_activation_with_package_transaction_and_ack(
        &mut self,
        profile_id: &str,
        rollback: Option<crate::shell::module_config::ModuleConfigRollback>,
        package_transaction: Option<PackageTransaction>,
        package_rollback: Option<PackageRuntimeRollback>,
        mut profile_switch_ack: Option<super::types::IpcProfileSwitchResponseSender>,
    ) -> VecDeque<CoreRequest> {
        if !self.pending_backend_runtimes.is_empty() {
            abort_package_transaction(package_transaction, package_rollback, self);
            if let Some(rollback) = rollback {
                restore_module_config(rollback);
            }
            self.reject_profile_switch_with_ack(
                profile_id,
                "a provider switch is already being prepared".into(),
                profile_switch_ack.take(),
            );
            return VecDeque::new();
        }
        let graph_path = self.installed_module_graph_path();
        let paths = match ProfilePaths::from_root_graph(&graph_path) {
            Ok(paths) => paths,
            Err(error) => {
                abort_package_transaction(package_transaction, package_rollback, self);
                if let Some(rollback) = rollback {
                    restore_module_config(rollback);
                }
                self.reject_profile_switch_with_ack(
                    profile_id,
                    error.to_string(),
                    profile_switch_ack.take(),
                );
                return VecDeque::new();
            }
        };
        let profile = match paths.load(profile_id) {
            Ok(profile) => profile,
            Err(error) => {
                abort_package_transaction(package_transaction, package_rollback, self);
                if let Some(rollback) = rollback {
                    restore_module_config(rollback);
                }
                self.reject_profile_switch_with_ack(
                    profile_id,
                    error.to_string(),
                    profile_switch_ack.take(),
                );
                return VecDeque::new();
            }
        };
        let graph = match load_authoring_snapshot_for_profile(&graph_path, &profile) {
            Ok(graph) => graph,
            Err(error) => {
                abort_package_transaction(package_transaction, package_rollback, self);
                if let Some(rollback) = rollback {
                    restore_module_config(rollback);
                }
                self.reject_profile_switch_with_ack(
                    profile_id,
                    error.to_string(),
                    profile_switch_ack.take(),
                );
                return VecDeque::new();
            }
        };
        let shared = match SettingsStore::load() {
            Ok(settings) => settings,
            Err(error) => {
                abort_package_transaction(package_transaction, package_rollback, self);
                if let Some(rollback) = rollback {
                    restore_module_config(rollback);
                }
                self.reject_profile_switch_with_ack(
                    profile_id,
                    error.to_string(),
                    profile_switch_ack.take(),
                );
                return VecDeque::new();
            }
        };
        let settings = match super::discovery::effective_profile_settings(shared, Some(&profile)) {
            Ok(mut settings) => {
                if let Err(error) =
                    super::discovery::register_graph_settings_schemas(&mut settings, &graph)
                {
                    abort_package_transaction(package_transaction, package_rollback, self);
                    if let Some(rollback) = rollback {
                        restore_module_config(rollback);
                    }
                    self.reject_profile_switch_with_ack(
                        profile_id,
                        error.to_string(),
                        profile_switch_ack.take(),
                    );
                    return VecDeque::new();
                }
                Arc::new(settings)
            }
            Err(error) => {
                abort_package_transaction(package_transaction, package_rollback, self);
                if let Some(rollback) = rollback {
                    restore_module_config(rollback);
                }
                self.reject_profile_switch_with_ack(
                    profile_id,
                    error.to_string(),
                    profile_switch_ack.take(),
                );
                return VecDeque::new();
            }
        };
        let locale = match self.prepare_locale_for_settings(settings.shell(), &graph) {
            Ok(locale) => locale,
            Err(error) => {
                abort_package_transaction(package_transaction, package_rollback, self);
                if let Some(rollback) = rollback {
                    restore_module_config(rollback);
                }
                self.reject_profile_switch_with_ack(
                    profile_id,
                    error.to_string(),
                    profile_switch_ack.take(),
                );
                return VecDeque::new();
            }
        };
        let resource_job = match self.start_resource_preparation_job(&graph, &settings) {
            Ok(job) => job,
            Err(error) => {
                abort_package_transaction(package_transaction, package_rollback, self);
                if let Some(rollback) = rollback {
                    restore_module_config(rollback);
                }
                self.reject_profile_switch_with_ack(
                    profile_id,
                    error.to_string(),
                    profile_switch_ack.take(),
                );
                return VecDeque::new();
            }
        };
        self.pending_resource_preparation = Some(PendingResourcePreparation {
            profile_id: profile_id.to_string(),
            profile,
            graph,
            locale,
            settings,
            resource_job,
            persist_active_profile: true,
            rollback,
            package_transaction,
            package_rollback,
            profile_switch_ack,
        });
        self.poll_pending_resource_preparation()
    }

    pub(in crate::shell) fn profile_transition_pending(&self) -> bool {
        self.pending_resource_preparation.is_some() || self.pending_profile_switch.is_some()
    }

    pub(in crate::shell) fn profile_candidate_is_pending(
        &self,
        interface: &str,
        provider_id: &str,
    ) -> bool {
        self.profile_candidate_is_pending_at_identity(
            interface,
            provider_id,
            mesh_core_backend::BackendIdentity::default(),
        )
    }

    pub(in crate::shell) fn profile_candidate_is_pending_at_identity(
        &self,
        interface: &str,
        provider_id: &str,
        identity: mesh_core_backend::BackendIdentity,
    ) -> bool {
        self.pending_profile_switch
            .as_ref()
            .and_then(|pending| pending.candidate_backends.get(interface))
            .is_some_and(|slot| {
                *slot
                    .event_provider_id
                    .read()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    == provider_id
                    && (identity == mesh_core_backend::BackendIdentity::default()
                        || *slot
                            .identity
                            .read()
                            .unwrap_or_else(|poisoned| poisoned.into_inner())
                            == identity)
            })
    }

    pub(in crate::shell) fn apply_node_slot_edit(
        &mut self,
        profile_id: &str,
        root_instance: &str,
        slot: &str,
        nodes: Option<serde_json::Value>,
        expected_generation: &str,
    ) -> Result<VecDeque<CoreRequest>, ShellRunError> {
        if self.profile_transition_pending() {
            return Err(ShellRunError::Package(
                "node edit rejected while a profile switch is pending".into(),
            ));
        }
        let graph_path = self.installed_module_graph_path();
        let paths = ProfilePaths::from_root_graph(&graph_path)?;
        let mut profile = paths.load(profile_id)?;
        let expected_revision = profile.revision;
        let generation = profile_generation(&profile)?;
        if generation != expected_generation {
            return Err(ShellRunError::Package(format!(
                "node_edit_generation_conflict: expected {expected_generation}, current {generation}"
            )));
        }

        match nodes {
            Some(nodes) => {
                let nodes: Vec<ComponentPlacement> =
                    serde_json::from_value(nodes).map_err(|error| {
                        ShellRunError::Package(format!("invalid_node_props: {error}"))
                    })?;
                profile
                    .node_slots
                    .entry(root_instance.to_string())
                    .or_default()
                    .insert(slot.to_string(), NodeSlotOverride { nodes });
            }
            None => {
                if let Some(slots) = profile.node_slots.get_mut(root_instance) {
                    slots.remove(slot);
                    if slots.is_empty() {
                        profile.node_slots.remove(root_instance);
                    }
                }
            }
        }
        profile.validate()?;
        let candidate = load_authoring_snapshot_for_profile(&graph_path, &profile)?;
        let previous_catalog = self.frontend_catalog.snapshot().catalog;
        FrontendCatalog::from_modules_reusing(
            &self.modules,
            Some(&candidate),
            Some(&previous_catalog),
        )?;

        paths.save_if_revision(profile_id, &profile, expected_revision)?;
        if self.active_profile_id.as_deref() == Some(profile_id) {
            return Ok(self.apply_switch_profile(profile_id));
        }
        Ok(VecDeque::new())
    }

    pub(in crate::shell) fn sync_composition_service_state(
        &mut self,
    ) -> Result<VecDeque<CoreRequest>, ShellRunError> {
        let mode = self.composition_mode.service_name();
        let recovery_reason = self.composition_mode.recovery_reason().map(str::to_owned);
        let Some(profile_id) = self.active_profile_id.clone() else {
            return self.broadcast_service_event(ServiceEvent::Updated {
                service: "mesh.composition".into(),
                source_module: "@mesh/shell".into(),
                payload: serde_json::json!({
                    "mode": mode,
                    "recovery_reason": recovery_reason,
                    "profile_id": "",
                    "generation": "",
                    "roots": [],
                    "slots": [],
                    "palette": [],
                }),
            });
        };
        let paths = ProfilePaths::from_root_graph(&self.installed_module_graph_path())?;
        let profile_delta = paths.load(&profile_id)?;
        let generation = profile_generation(&profile_delta)?;
        let graph = self.load_installed_module_graph_cached()?.clone();
        let manifests = graph
            .modules()
            .into_iter()
            .map(|module| &module.manifest)
            .collect::<Vec<_>>();
        let profile =
            mesh_core_module::package::resolve_composition(&profile_delta, manifests)?.to_profile();
        let catalog = self.frontend_catalog.snapshot().catalog;
        let mut roots = Vec::new();
        let mut slots = Vec::new();
        let mut palette = Vec::new();

        for (root_instance, root) in &profile.roots {
            roots.push(serde_json::json!({
                "instance": root_instance,
                "module": root.module,
                "active": root.active,
            }));
            if !root.active {
                continue;
            }
            let Some(host) = catalog.modules.get(&root.module) else {
                continue;
            };
            for descriptor in customizable_slots(&host.compiled.component) {
                let Some(point) = descriptor.extension_point.as_deref() else {
                    continue;
                };
                let slot_name = descriptor.name.as_deref().unwrap_or_default();
                let hosted = host.compiled.manifest.hosted_extension_points.get(point);
                let defaults = hosted
                    .and_then(|hosted| hosted.slots.get(slot_name))
                    .map(|slot| slot.defaults.clone())
                    .unwrap_or_default();
                let effective = catalog
                    .node_slot_placement(root_instance, slot_name)
                    .map(|slot| serde_json::to_value(&slot.nodes))
                    .transpose()
                    .map_err(|error| ShellRunError::Package(error.to_string()))?
                    .unwrap_or_else(|| {
                        serde_json::Value::Array(
                            defaults
                                .iter()
                                .enumerate()
                                .map(|(index, reference)| {
                                    serde_json::json!({
                                        "id": format!("default-{index}"),
                                        "use": reference,
                                        "props": {},
                                    })
                                })
                                .collect(),
                        )
                    });
                slots.push(serde_json::json!({
                    "root_instance": root_instance,
                    "name": slot_name,
                    "contract": point,
                    "layout": hosted.and_then(|hosted| hosted.layout.clone()).unwrap_or_else(|| "row".into()),
                    "max": hosted.and_then(|hosted| hosted.max),
                    "defaults": defaults,
                    "nodes": effective,
                    "overridden": catalog.node_slot_placement(root_instance, slot_name).is_some(),
                }));
                for contribution in catalog.extension_point_contributions_for(&root.module, point) {
                    let compiled = catalog
                        .contribution_entry(
                            &contribution.source_module_id,
                            &contribution.contribution_id,
                        )
                        .expect("resolved contribution is compiled");
                    let props_schema = mesh_core_component::props_settings_schema(
                        compiled.component.props.as_ref(),
                    )
                    .map(|schema| {
                        let (schema, resolutions) = {
                            let translator = self
                                .locale
                                .module_translator(&contribution.source_module_id);
                            translator.resolve_json(
                                &schema,
                                &format!(
                                    "mesh.composition.palette.{}.props_schema",
                                    contribution.contribution_id
                                ),
                            )
                        };
                        for resolution in resolutions {
                            super::record_localized_miss(&mut self.diagnostics, &resolution, None);
                        }
                        schema
                    });
                    palette.push(serde_json::json!({
                        "contract": point,
                        "use": format!("{}:{}", contribution.source_module_id, contribution.contribution_id),
                        "module": contribution.source_module_id,
                        "id": contribution.contribution_id,
                        "props_schema": props_schema,
                    }));
                }
            }
        }
        palette.sort_by(|left, right| left["use"].as_str().cmp(&right["use"].as_str()));
        palette.dedup_by(|left, right| left["use"] == right["use"]);

        self.broadcast_service_event(ServiceEvent::Updated {
            service: "mesh.composition".into(),
            source_module: "@mesh/shell".into(),
            payload: serde_json::json!({
                "mode": mode,
                "recovery_reason": recovery_reason,
                "profile_id": profile_id,
                "generation": generation,
                "roots": roots,
                "slots": slots,
                "palette": palette,
            }),
        })
    }

    pub(in crate::shell) fn apply_set_module_prop(
        &mut self,
        module_id: &str,
        instance_id: Option<&str>,
        prop: &str,
        value: Option<serde_json::Value>,
    ) -> Result<VecDeque<CoreRequest>, ShellRunError> {
        let catalog = self.frontend_catalog.snapshot();
        let definition = catalog
            .catalog
            .module(module_id)
            .and_then(|entry| entry.compiled.component.props.as_ref())
            .and_then(|block| {
                block
                    .props
                    .iter()
                    .find(|definition| definition.name == prop)
            })
            .filter(|definition| definition.expose)
            .cloned()
            .ok_or_else(|| ShellRunError::FrontendComposition {
                message: format!(
                    "module '{module_id}' does not expose a configurable prop named '{prop}'"
                ),
            })?;

        if let Some(instance_id) = instance_id
            && !self.components.iter().any(|runtime| {
                runtime.surface_id == instance_id && runtime.component.id() == module_id
            })
        {
            return Err(ShellRunError::FrontendComposition {
                message: format!("'{instance_id}' is not a live instance of module '{module_id}'"),
            });
        }

        let settings_path = instance_id
            .map(|id| format!("props.instances.{id}"))
            .unwrap_or_else(|| "props.global".to_string());

        if let Some(value) = value.as_ref() {
            let parsed = mesh_core_component::json_to_prop_value_ref(value).map_err(|error| {
                ShellRunError::FrontendComposition {
                    message: format!(
                        "setting {module_id}.{settings_path}.{prop} must be a string, number, or boolean: {error}"
                    ),
                }
            })?;
            mesh_core_component::validate_prop_value(&definition, &parsed).map_err(|error| {
                ShellRunError::FrontendComposition {
                    message: format!("invalid setting {module_id}.{settings_path}.{prop}: {error}"),
                }
            })?;
        }

        let candidate = self.prepare_control_plane_settings(|shared, profile| {
            if let Some(profile) = profile {
                let namespace = profile
                    .settings
                    .entry(module_id.to_string())
                    .or_insert_with(|| serde_json::json!({}));
                update_module_prop_override(namespace, instance_id, prop, value);
                if namespace.as_object().is_some_and(serde_json::Map::is_empty) {
                    profile.settings.remove(module_id);
                }
            } else {
                let mut namespace = shared.namespace(module_id);
                update_module_prop_override(&mut namespace, instance_id, prop, value);
                shared.set_namespace(module_id, namespace);
            }
            Ok(())
        })?;
        let commit = self.commit_control_plane_settings(candidate)?;
        self.components_want_render = true;
        self.commit_control_plane_batch(commit, None, None, false, false)
    }

    pub(in crate::shell) fn apply_switch_profile(
        &mut self,
        profile_id: &str,
    ) -> VecDeque<CoreRequest> {
        self.apply_switch_profile_with_ack(profile_id, None)
    }

    pub(in crate::shell) fn apply_switch_profile_with_ack(
        &mut self,
        profile_id: &str,
        profile_switch_ack: Option<super::types::IpcProfileSwitchResponseSender>,
    ) -> VecDeque<CoreRequest> {
        if self.pending_profile_switch.is_some() {
            self.reject_profile_switch_with_ack(
                profile_id,
                "another profile switch is already being prepared".into(),
                profile_switch_ack,
            );
            return VecDeque::new();
        }
        if let Some(pending) = self.pending_resource_preparation.take() {
            pending.resource_job.cancel();
            restore_module_config_if_present(pending.rollback);
            abort_package_transaction(pending.package_transaction, pending.package_rollback, self);
            self.reject_profile_switch_with_ack(
                &pending.profile_id,
                format!("profile switch was superseded by '{profile_id}'"),
                pending.profile_switch_ack,
            );
        }
        if !self.pending_backend_runtimes.is_empty() {
            self.reject_profile_switch_with_ack(
                profile_id,
                "a provider switch is already being prepared".into(),
                profile_switch_ack,
            );
            return VecDeque::new();
        }
        self.begin_profile_activation_with_package_transaction_and_ack(
            profile_id,
            None,
            None,
            None,
            profile_switch_ack,
        )
    }

    pub(in crate::shell) fn poll_pending_resource_preparation(&mut self) -> VecDeque<CoreRequest> {
        let Some(pending) = self.pending_resource_preparation.take() else {
            return VecDeque::new();
        };
        let PendingResourcePreparation {
            profile_id,
            profile,
            graph,
            locale,
            settings,
            mut resource_job,
            persist_active_profile,
            rollback,
            package_transaction,
            package_rollback,
            profile_switch_ack,
        } = pending;
        let Some(result) = resource_job.try_wait() else {
            self.pending_resource_preparation = Some(PendingResourcePreparation {
                profile_id,
                profile,
                graph,
                locale,
                settings,
                resource_job,
                persist_active_profile,
                rollback,
                package_transaction,
                package_rollback,
                profile_switch_ack,
            });
            return VecDeque::new();
        };
        match result {
            Ok(resources)
                if resources
                    .resource_lease
                    .as_ref()
                    .is_some_and(mesh_core_resources::ResourcePreparationLease::is_current) =>
            {
                self.continue_profile_switch(
                    profile_id,
                    profile,
                    graph,
                    locale,
                    settings,
                    resources,
                    persist_active_profile,
                    rollback,
                    package_transaction,
                    package_rollback,
                    profile_switch_ack,
                )
            }
            Ok(_) => {
                resource_job.retire();
                abort_package_transaction(package_transaction, package_rollback, self);
                if let Some(rollback) = rollback {
                    let _ = rollback.restore();
                }
                self.reject_profile_switch_with_ack(
                    &profile_id,
                    "candidate resources were superseded".into(),
                    profile_switch_ack,
                );
                VecDeque::new()
            }
            Err(error) => {
                resource_job.retire();
                abort_package_transaction(package_transaction, package_rollback, self);
                if let Some(rollback) = rollback {
                    let _ = rollback.restore();
                }
                self.reject_profile_switch_with_ack(
                    &profile_id,
                    format!("candidate resources are invalid: {error}"),
                    profile_switch_ack,
                );
                VecDeque::new()
            }
        }
    }

    fn continue_profile_switch(
        &mut self,
        profile_id: String,
        profile: ShellProfile,
        graph: InstalledModuleGraph,
        locale: LocaleEngine,
        settings: Arc<SettingsStore>,
        resources: super::discovery::PreparedResourceSnapshot,
        persist_active_profile: bool,
        mut rollback: Option<crate::shell::module_config::ModuleConfigRollback>,
        mut package_transaction: Option<PackageTransaction>,
        mut package_rollback: Option<PackageRuntimeRollback>,
        mut profile_switch_ack: Option<super::types::IpcProfileSwitchResponseSender>,
    ) -> VecDeque<CoreRequest> {
        macro_rules! reject_candidate {
            ($message:expr) => {{
                if let Some(rollback) = rollback.take() {
                    let _ = rollback.restore();
                }
                abort_package_transaction(
                    package_transaction.take(),
                    package_rollback.take(),
                    self,
                );
                self.reject_profile_switch_with_ack(
                    &profile_id,
                    $message.into(),
                    profile_switch_ack.take(),
                );
                return VecDeque::new();
            }};
        }
        let interface_catalog =
            match interface_catalog_for_graph(&self.builtin_interface_catalog, &graph) {
                Ok(catalog) => Arc::new(catalog),
                Err(error) => {
                    reject_candidate!(error.to_string());
                }
            };
        let candidate_interfaces = interface_catalog.as_ref();
        let effective_capabilities =
            match candidate_capabilities(&self.capability_policy, &graph, &self.modules) {
                Ok(capabilities) => capabilities,
                Err(error) => {
                    reject_candidate!(error.to_string());
                }
            };
        let resolved_shell_settings =
            mesh_core_config::resolve_shell_locale_settings(settings.shell());
        let previous_catalog = self.frontend_catalog.snapshot().catalog;
        let catalog = match FrontendCatalog::from_modules_reusing(
            &self.modules,
            Some(&graph),
            Some(&previous_catalog),
        ) {
            Ok(catalog) => catalog,
            Err(error) => {
                reject_candidate!(error.to_string());
            }
        };
        let temporary_catalog = FrontendCatalogHandle::from(catalog.clone());
        let entries = catalog
            .top_level_surfaces()
            .into_iter()
            .map(|entry| (entry.compiled.manifest.package.id.clone(), entry))
            .collect::<HashMap<_, _>>();
        let mut profile = profile;
        if !persist_active_profile {
            for entry in catalog.top_level_surfaces() {
                let module_id = entry.compiled.manifest.package.id.clone();
                profile.roots.insert(
                    entry.compiled.surface_id().to_string(),
                    ProfileRootInstance {
                        module: module_id,
                        entrypoint: "main".into(),
                        active: true,
                        surface: None,
                    },
                );
            }
        }
        let desired_surfaces = profile
            .roots
            .iter()
            .filter(|(_, root)| root.active)
            .map(|(instance_id, _)| instance_id.clone())
            .collect::<HashSet<_>>();
        let root_modules = profile
            .roots
            .iter()
            .filter(|(_, root)| root.active)
            .map(|(instance_id, root)| (instance_id.clone(), root.module.clone()))
            .collect::<HashMap<_, _>>();
        let existing_surface_modules = self
            .components
            .iter()
            .map(|runtime| {
                (
                    runtime.surface_id.clone(),
                    runtime.component.id().to_string(),
                )
            })
            .collect::<HashMap<_, _>>();
        let mut prepared_frontends = Vec::new();
        for (instance_id, root) in profile.roots.iter().filter(|(_, root)| root.active) {
            if existing_surface_modules
                .get(instance_id)
                .is_some_and(|module_id| module_id == &root.module)
            {
                continue;
            }
            let Some(entry) = entries.get(&root.module) else {
                reject_candidate!(format!(
                    "profile root {instance_id} has no mountable frontend entrypoint"
                ));
            };
            let mut component = FrontendSurfaceComponent::new(
                entry.compiled.clone(),
                entry.module_dir.clone(),
                temporary_catalog.clone(),
                interface_catalog.clone(),
                settings.clone(),
            )
            .with_effective_capabilities(effective_capabilities.clone())
            .with_instance_id(instance_id)
            .with_locale_snapshot(locale.snapshot());
            let diagnostics = self
                .diagnostics
                .register_instance(root.module.clone(), instance_id.clone());
            let mut requests = match component.mount(ComponentContext {
                component_id: root.module.clone(),
                surface_id: instance_id.clone(),
                diagnostics,
            }) {
                Ok(requests) => VecDeque::from(requests),
                Err(error) => {
                    reject_candidate!(error.to_string());
                }
            };
            if let Err(error) = component.locale_changed(&locale) {
                reject_candidate!(error.to_string());
            }
            for state in self.latest_service_state.values() {
                let event = ServiceEvent::Updated {
                    service: state.interface.clone(),
                    source_module: state.provider_id.clone(),
                    payload: state.state.clone(),
                };
                if component.observes_service_event(&event) {
                    match component.handle_service_event_with_generation(&event, state.generation) {
                        Ok(next) => requests.extend(next),
                        Err(error) => {
                            reject_candidate!(error.to_string());
                        }
                    }
                }
            }
            prepared_frontends.push(PreparedProfileFrontend {
                component,
                requests,
            });
        }

        let (mut candidates, statuses) = backend_launch_candidates_from_graph_with_capabilities(
            &graph,
            &self.modules,
            &settings,
            candidate_interfaces,
            Some(&effective_capabilities),
        );
        if let Some(status) = statuses.iter().find(|status| {
            !matches!(
                status.status,
                "optional_backend_unavailable" | "optional_backend_inactive"
            )
        }) {
            reject_candidate!(status.message.clone());
        }
        // Theme state is part of every activation candidate, not only when a
        // settings field changed. A graph can replace a descriptor's source,
        // modes, label, or catalog while the selected id stays the same.
        let mut prepared_theme = match prepare_theme_state_for_graph(settings.shell(), &graph) {
            Ok(Some(prepared)) => Some(prepared),
            Ok(None) => {
                let (engine, watch) = default_theme_state(settings.shell());
                Some(PreparedThemeState { engine, watch })
            }
            Err(error) => {
                reject_candidate!(format!("candidate theme is invalid: {error}"));
            }
        };
        if let Some(prepared) = prepared_theme.as_mut() {
            prepared.engine.update_active(|active| {
                super::discovery::apply_font_registry_tokens(active, &resources.font_registry);
            });
        }
        let candidate_theme_id = prepared_theme
            .as_ref()
            .map(|prepared| prepared.engine.active().id.clone())
            .unwrap_or_else(|| self.theme.active().id.clone());
        for candidate in &mut candidates {
            Self::apply_runtime_settings(
                candidate,
                &candidate_theme_id,
                &resolved_shell_settings.i18n.locale,
            );
        }
        let desired_providers = candidates
            .iter()
            .map(|candidate| (candidate.interface.clone(), candidate.module_id.clone()))
            .collect::<HashMap<_, _>>();
        let current_configs = self
            .installed_module_graph
            .as_ref()
            .map(|current_graph| {
                let current_interface_catalog = self.interfaces.snapshot();
                let (mut current, _) = backend_launch_candidates_from_graph_with_capabilities(
                    current_graph,
                    &self.modules,
                    &self.settings_store,
                    current_interface_catalog.as_ref(),
                    Some(&self.effective_capabilities),
                );
                for candidate in &mut current {
                    self.apply_shell_runtime_settings(candidate);
                }
                current
                    .into_iter()
                    .map(|candidate| {
                        (
                            candidate.interface,
                            (candidate.module_id, candidate.settings),
                        )
                    })
                    .collect::<HashMap<_, _>>()
            })
            .unwrap_or_default();
        let plan = Arc::new(ActivationPlan {
            generation: self.activation_generation.saturating_add(1),
            profile_id: profile_id.clone(),
            profile_revision: persist_active_profile.then_some(profile.revision),
            graph,
            interface_catalog,
            locale,
            settings,
            resources,
            prepared_theme,
            catalog,
            effective_capabilities,
            desired_surfaces,
            root_modules,
            desired_providers,
            provider_candidates: candidates,
        });
        let changed = plan
            .provider_candidates
            .iter()
            .filter(|candidate| {
                let running_matches = self
                    .backend_runtimes
                    .get(&candidate.interface)
                    .is_some_and(|running| running.provider_id == candidate.module_id);
                let config_matches = current_configs.get(&candidate.interface).is_some_and(
                    |(provider_id, settings)| {
                        provider_id == &candidate.module_id && settings == &candidate.settings
                    },
                );
                !(running_matches && config_matches)
            })
            .cloned()
            .collect::<Vec<_>>();
        let required_backends = changed
            .iter()
            .map(|candidate| candidate.interface.clone())
            .collect::<Vec<_>>();
        let candidate_preview = CandidatePreview::from_plan(&plan, required_backends);
        let mut pending = PendingProfileSwitch {
            plan: plan.clone(),
            prepared_frontends,
            candidate_backends: HashMap::new(),
            waiting_backends: HashSet::new(),
            candidate_started: HashSet::new(),
            candidate_initial_states: HashMap::new(),
            persist_active_profile,
            rollback: rollback.take(),
            package_transaction: package_transaction.take(),
            package_rollback: package_rollback.take(),
            profile_switch_ack: profile_switch_ack.take(),
        };
        if !changed.is_empty() {
            let Some(ctx) = self.backend_respawn.clone() else {
                if let Some(rollback) = pending.rollback.take() {
                    let _ = rollback.restore();
                }
                abort_package_transaction(
                    pending.package_transaction.take(),
                    pending.package_rollback.take(),
                    self,
                );
                self.reject_profile_switch_with_ack(
                    &profile_id,
                    "backend runtime is unavailable while candidate providers need preparation"
                        .into(),
                    pending.profile_switch_ack.take(),
                );
                return VecDeque::new();
            };
            for candidate in changed {
                let interface = candidate.interface.clone();
                let event_provider_id = format!(
                    "@mesh/profile-candidate/{}/{}/{}",
                    profile_id, interface, candidate.module_id
                );
                let identity = self.next_backend_identity(&interface, plan.generation);
                let slot = self.start_backend_candidate_with_event_id(
                    &ctx.handle,
                    ctx.tx.clone(),
                    candidate,
                    ctx.wake.clone(),
                    event_provider_id,
                    identity,
                );
                pending.waiting_backends.insert(interface.clone());
                pending.candidate_backends.insert(interface, slot);
            }
        }

        self.candidate_preview = Some(Arc::new(candidate_preview));
        self.pending_profile_switch = Some(pending);
        if self
            .pending_profile_switch
            .as_ref()
            .is_some_and(|pending| pending.waiting_backends.is_empty())
        {
            return self.commit_pending_profile_switch();
        }
        tracing::info!(
            profile_id,
            "profile candidate prepared; waiting for backend readiness"
        );
        VecDeque::new()
    }

    pub(in crate::shell) fn handle_profile_backend_lifecycle(
        &mut self,
        interface: &str,
        provider_id: &str,
        status: BackendRuntimeStatus,
    ) -> bool {
        self.handle_profile_backend_lifecycle_at_identity(
            interface,
            provider_id,
            mesh_core_backend::BackendIdentity::default(),
            status,
        )
    }

    pub(in crate::shell) fn handle_profile_backend_lifecycle_at_identity(
        &mut self,
        interface: &str,
        provider_id: &str,
        identity: mesh_core_backend::BackendIdentity,
        status: BackendRuntimeStatus,
    ) -> bool {
        let needs_initial_state = self.service_requires_initial_state(interface);
        let Some(pending) = self.pending_profile_switch.as_mut() else {
            return false;
        };
        let is_candidate = pending
            .candidate_backends
            .get(interface)
            .is_some_and(|slot| {
                *slot
                    .event_provider_id
                    .read()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    == provider_id
                    && (identity == mesh_core_backend::BackendIdentity::default()
                        || *slot
                            .identity
                            .read()
                            .unwrap_or_else(|poisoned| poisoned.into_inner())
                            == identity)
            });
        if !is_candidate {
            return false;
        }
        let candidate_module_id = pending
            .candidate_backends
            .get(interface)
            .map(|slot| slot.provider_id.clone())
            .expect("profile candidate was checked above");
        let has_current_runtime = self.backend_runtimes.contains_key(interface);
        let should_commit = if status == BackendRuntimeStatus::Running {
            pending.candidate_started.insert(interface.to_string());
            if !needs_initial_state || pending.candidate_initial_states.contains_key(interface) {
                pending.waiting_backends.remove(interface);
            }
            pending.waiting_backends.is_empty()
        } else {
            false
        };
        if status == BackendRuntimeStatus::Running {
            self.mark_candidate_backend_ready(interface);
            if should_commit {
                let requests = self.commit_pending_profile_switch();
                self.enqueue_effects(requests);
            }
        } else if matches!(
            status,
            BackendRuntimeStatus::InitFailed
                | BackendRuntimeStatus::Failed
                | BackendRuntimeStatus::Stopped
        ) {
            self.abort_pending_profile_switch(format!(
                "provider {provider_id} failed to initialize for {interface}"
            ));
            if !has_current_runtime {
                self.update_module_runtime_lifecycle(
                    &candidate_module_id,
                    BackendRuntimeStatus::Failed,
                    "profile candidate failed to initialize",
                );
            }
        }
        true
    }

    pub(in crate::shell) fn capture_profile_backend_update(
        &mut self,
        interface: &str,
        provider_id: &str,
        event: ServiceEvent,
    ) -> bool {
        self.capture_profile_backend_update_at_identity(
            interface,
            provider_id,
            mesh_core_backend::BackendIdentity::default(),
            event,
        )
    }

    pub(in crate::shell) fn capture_profile_backend_update_at_identity(
        &mut self,
        interface: &str,
        provider_id: &str,
        identity: mesh_core_backend::BackendIdentity,
        event: ServiceEvent,
    ) -> bool {
        let Some(pending) = self.pending_profile_switch.as_ref() else {
            return false;
        };
        let Some(candidate) = pending.candidate_backends.get(interface) else {
            return false;
        };
        let is_candidate = candidate
            .event_provider_id
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .as_str()
            == provider_id;
        let identity_matches = identity == mesh_core_backend::BackendIdentity::default()
            || *candidate
                .identity
                .read()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                == identity;
        if !is_candidate || !identity_matches {
            return false;
        }
        let actual_provider_id = candidate.provider_id.clone();
        let ServiceEvent::Updated { payload, .. } = self.normalize_service_event(event) else {
            return true;
        };
        if !self.validate_service_state_shape(interface, &actual_provider_id, &payload) {
            let has_current_runtime = self.backend_runtimes.contains_key(interface);
            self.abort_pending_profile_switch(format!(
                "provider {actual_provider_id} emitted an invalid initial service snapshot for {interface}"
            ));
            if !has_current_runtime {
                self.update_module_runtime_lifecycle(
                    &actual_provider_id,
                    BackendRuntimeStatus::Failed,
                    "provider emitted an invalid initial service snapshot",
                );
            }
            return true;
        }
        let should_commit = if let Some(pending) = self.pending_profile_switch.as_mut() {
            pending
                .candidate_initial_states
                .insert(interface.to_string(), payload);
            if pending.candidate_started.contains(interface) {
                pending.waiting_backends.remove(interface);
            }
            pending.waiting_backends.is_empty()
        } else {
            false
        };
        if should_commit {
            let requests = self.commit_pending_profile_switch();
            self.enqueue_effects(requests);
        }
        tracing::debug!(interface, provider_id, "buffered prepared profile snapshot");
        true
    }

    fn commit_pending_profile_switch(&mut self) -> VecDeque<CoreRequest> {
        let Some(mut pending) = self.pending_profile_switch.take() else {
            return VecDeque::new();
        };
        let plan = pending.plan.clone();
        let paths = match ProfilePaths::from_root_graph(&self.installed_module_graph_path()) {
            Ok(paths) => paths,
            Err(error) => {
                self.abort_profile_candidate(pending, error.to_string());
                return VecDeque::new();
            }
        };
        // Capture and update the durable active-profile pointer only for a
        // profile activation. Legacy graph activation uses this same commit
        // boundary but has no profile pointer to mutate.
        let previous_active = if pending.persist_active_profile {
            match paths.active_profile_id() {
                Ok(previous) => previous,
                Err(error) => {
                    self.abort_profile_candidate(pending, error.to_string());
                    return VecDeque::new();
                }
            }
        } else {
            None
        };
        if pending.persist_active_profile
            && let Err(error) = paths.set_active(&plan.profile_id)
        {
            self.abort_profile_candidate(pending, error.to_string());
            return VecDeque::new();
        }
        if let Err(error) = self.commit_resource_snapshot_for_settings(
            &plan.resources,
            plan.settings.shell().icons.default_pack.clone(),
            false,
        ) {
            if pending.persist_active_profile
                && let Err(restore_error) = Self::restore_active_profile_if_current(
                    &paths,
                    &plan.profile_id,
                    previous_active.as_deref(),
                )
            {
                tracing::error!(
                    "failed to restore previous active-profile pointer after an aborted \
                     profile switch; the durable pointer may now name a profile the running \
                     shell never adopted: {restore_error}"
                );
            }
            self.abort_profile_candidate(pending, error.to_string());
            return VecDeque::new();
        }

        // The journal is the durable half of this activation. Commit it only
        // after every candidate is prepared and the active-profile pointer
        // has been updated, but before publishing any replacement runtime
        // objects. A crash after this point reopens the same committed graph;
        // every failure before it drops the transaction and restores disk.
        if let Some(package_transaction) = pending.package_transaction.take() {
            if let Err(error) = package_transaction.commit() {
                if pending.persist_active_profile
                    && let Err(restore_error) = Self::restore_active_profile_if_current(
                        &paths,
                        &plan.profile_id,
                        previous_active.as_deref(),
                    )
                {
                    tracing::error!(
                        "failed to restore previous active-profile pointer after package \
                         transaction abort; the durable pointer may now name a profile the \
                         running shell never adopted: {restore_error}"
                    );
                }
                self.abort_profile_candidate(pending, error.to_string());
                return VecDeque::new();
            }
        }

        self.frontend_catalog.replace(plan.catalog.clone(), None);
        for prepared in &mut pending.prepared_frontends {
            prepared
                .component
                .adopt_frontend_catalog(self.frontend_catalog.clone());
        }
        let initial_states = Arc::new(std::mem::take(&mut pending.candidate_initial_states));
        // All fallible candidate work is complete. From this point on the
        // coordinator only swaps prepared values and retires old runtime
        // objects. The immutable ActiveSnapshot is published after those
        // swaps, before any newly committed state can emit follow-up work.
        self.interfaces
            .publish_snapshot(Arc::clone(&plan.interface_catalog));
        self.clear_candidate_preview(plan.generation);
        let prepared_surfaces = pending
            .prepared_frontends
            .iter()
            .map(|prepared| prepared.component.surface_id().to_string())
            .collect::<HashSet<_>>();
        for index in (0..self.components.len()).rev() {
            if !plan
                .desired_surfaces
                .contains(&self.components[index].surface_id)
                || prepared_surfaces.contains(&self.components[index].surface_id)
            {
                self.remove_profile_component(index);
            }
        }
        let mut requests = VecDeque::new();
        for prepared in pending.prepared_frontends {
            requests.extend(prepared.requests);
            self.register_component(Box::new(prepared.component));
        }
        let mut prepared_states = Vec::new();

        let obsolete = self
            .backend_runtimes
            .keys()
            .filter(|interface| !plan.desired_providers.contains_key(*interface))
            .cloned()
            .collect::<Vec<_>>();
        for interface in obsolete {
            self.stop_backend_runtime(&interface);
        }
        for (interface, slot) in pending.candidate_backends {
            let initial_state = initial_states.get(&interface).cloned();
            let provider_id = slot.provider_id.clone();
            let identity = *slot
                .identity
                .read()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            *slot
                .event_provider_id
                .write()
                .unwrap_or_else(|poisoned| poisoned.into_inner()) = slot.provider_id.clone();
            self.backend_supervision.remove(&interface);
            self.replace_backend_runtime(interface.clone(), slot);
            self.note_backend_running(&interface);
            if let Some(module) = self.modules.get_mut(&provider_id) {
                module.clear_quarantine();
            }
            self.record_backend_runtime_status_at_identity(
                interface.clone(),
                provider_id.clone(),
                identity,
                BackendRuntimeStatus::Running,
                "backend runtime ready".to_string(),
            );
            if let Some(payload) = initial_state {
                prepared_states.push((interface, provider_id, payload));
            }
        }
        self.retag_backend_runtimes_for_activation(plan.generation);

        let old_locale = self.settings.i18n.clone();
        let old_locale_catalog_revision = self.locale.catalog_snapshot().revision();
        let settings = mesh_core_config::resolve_shell_locale_settings(plan.settings.shell());
        // Every activation carries a prepared theme state. This is intentional
        // even when the selected id is unchanged: graph replacement can alter
        // descriptor modes, source content, ownership, or the catalog itself.
        let theme_changed = plan.prepared_theme.is_some();
        let prepared_theme = if let Some(prepared) = plan.prepared_theme.clone() {
            Some((prepared.engine, prepared.watch))
        } else if theme_changed {
            let (mut theme, mut watch) = default_theme_state(&settings);
            theme.update_active(|active| {
                super::discovery::apply_font_registry_tokens(active, &self.font_registry);
            });
            watch.revision = theme.active_snapshot().revision;
            Some((theme, watch))
        } else {
            None
        };
        let locale_changed = old_locale.locale != settings.i18n.locale
            || old_locale.fallback_locale != settings.i18n.fallback_locale
            || old_locale.policy != settings.i18n.policy
            || old_locale_catalog_revision != plan.locale.catalog_snapshot().revision();
        let control_plane = super::runtime::ControlPlaneSettingsCommit {
            store: (*plan.settings).clone(),
            revision: DurableControlPlaneRevision::new(
                plan.settings.revision(),
                plan.profile_revision,
            ),
        };
        match self.commit_control_plane_batch(
            control_plane,
            prepared_theme,
            Some(plan.locale.clone()),
            theme_changed,
            locale_changed,
        ) {
            Ok(effects) => requests.extend(effects),
            Err(error) => {
                tracing::warn!("profile control-plane refresh failed after commit: {error}");
                self.diagnostics.record_lifecycle_error(
                    "@mesh/shell",
                    "profile_post_commit_control_plane_refresh_failed",
                    format!(
                        "profile '{}' committed but its settings/theme/locale effect batch \
                         failed: {error}",
                        plan.profile_id
                    ),
                );
            }
        }

        self.installed_module_graph = Some(plan.graph.clone());
        if pending.persist_active_profile {
            self.active_profile_id = Some(plan.profile_id.clone());
            self.composition_mode = ShellCompositionMode::ConfiguredProfile {
                id: plan.profile_id.clone(),
            };
        } else {
            self.active_profile_id = None;
            self.composition_mode = ShellCompositionMode::LegacyNoProfile;
        }

        // Reconcile the active watch inputs after the graph and mounted
        // surfaces have been swapped. The resulting generation is part of the
        // same immutable snapshot exposed to observers.
        self.reconcile_file_watcher();
        let providers = self
            .backend_runtimes
            .iter()
            .map(|(interface, slot)| {
                (
                    interface.clone(),
                    *slot
                        .identity
                        .read()
                        .unwrap_or_else(|poisoned| poisoned.into_inner()),
                )
            })
            .collect::<HashMap<_, _>>();
        let presentation_generations = plan
            .desired_surfaces
            .iter()
            .map(|surface_id| (surface_id.clone(), plan.generation))
            .collect::<HashMap<_, _>>();
        self.activation_generation = plan.generation;
        self.active_snapshot = Some(Arc::new(ActiveSnapshot {
            generation: plan.generation,
            profile_id: pending
                .persist_active_profile
                .then(|| plan.profile_id.clone()),
            profile_revision: plan.profile_revision,
            plan: plan.clone(),
            initial_states: initial_states.clone(),
            settings: plan.settings.clone(),
            roots: Arc::new(plan.root_modules.clone()),
            providers: Arc::new(providers),
            frontend_catalog_revision: self.frontend_catalog.snapshot().version,
            settings_revision: plan.settings.revision(),
            theme_revision: self.theme.active_snapshot().revision,
            locale_revision: self.locale.revision(),
            resource_generation: plan.resources.generation,
            watch_generation: self.file_watch_set.generation,
            presentation_generations: Arc::new(presentation_generations),
        }));
        if let Some(response) = pending.profile_switch_ack.take() {
            let _ = response.send(super::types::IpcProfileSwitchResponse::Committed {
                profile_id: plan.profile_id.clone(),
                generation: plan.generation,
            });
        }
        for (interface, provider_id, payload) in prepared_states {
            let event = ServiceEvent::Updated {
                service: interface,
                source_module: provider_id,
                payload,
            };
            if self.record_latest_service_state(&event) {
                match self.deliver_service_event(&event) {
                    Ok(next) => requests.extend(next),
                    Err(error) => {
                        tracing::warn!("failed to deliver prepared profile service state: {error}")
                    }
                }
            }
        }
        if let Ok(next) = self.sync_composition_service_state() {
            requests.extend(next);
        }
        self.sync_frontend_catalog_components();
        self.components_want_render = true;
        tracing::info!(profile_id = plan.profile_id, "switched shell profile live");
        requests
    }

    fn remove_profile_component(&mut self, index: usize) {
        let surface_id = self.components[index].surface_id.clone();
        let module_id = self.components[index].component.id().to_string();
        if let Err(error) = self.components[index].unmount() {
            tracing::warn!(
                module_id,
                error = %error,
                "profile replacement unmount failed"
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
        self.rebuild_component_surface_index();
        self.service_delivery_index.mark_dirty();
    }

    fn abort_pending_profile_switch(&mut self, message: String) {
        if let Some(pending) = self.pending_profile_switch.take() {
            self.abort_profile_candidate(pending, message);
        }
    }

    pub(in crate::shell) fn abort_profile_candidate(
        &mut self,
        pending: PendingProfileSwitch,
        message: String,
    ) {
        let profile_id = pending.plan.profile_id.clone();
        let response = pending.profile_switch_ack;
        self.clear_candidate_preview(pending.plan.generation);
        if let Some(lease) = pending.plan.resources.resource_lease.as_ref() {
            lease.retire();
        }
        for slot in pending.candidate_backends.into_values() {
            self.retire_backend_runtime_slot(slot);
        }
        restore_module_config_if_present(pending.rollback);
        abort_package_transaction(pending.package_transaction, pending.package_rollback, self);
        self.reject_profile_switch_with_ack(&profile_id, message, response);
    }

    fn restore_active_profile_if_current(
        paths: &ProfilePaths,
        candidate_profile_id: &str,
        previous_profile_id: Option<&str>,
    ) -> Result<(), String> {
        let current = paths
            .active_profile_id()
            .map_err(|error| error.to_string())?;
        if current.as_deref() == previous_profile_id {
            return Ok(());
        }
        match current {
            Some(current) if current == candidate_profile_id => paths
                .restore_active(previous_profile_id)
                .map_err(|error| error.to_string()),
            Some(current) => Err(format!(
                "active profile pointer changed to '{current}' during recovery; refusing to overwrite it"
            )),
            None => Err(
                "active profile pointer disappeared during recovery; refusing to overwrite it"
                    .into(),
            ),
        }
    }

    fn reject_profile_switch(&mut self, profile_id: &str, message: String) {
        tracing::warn!(profile_id, "profile switch rejected: {message}");
        self.diagnostics.record_lifecycle_error(
            "@mesh/shell",
            "profile_switch_rejected",
            format!("profile {profile_id}: {message}"),
        );
    }

    fn reject_profile_switch_with_ack(
        &mut self,
        profile_id: &str,
        message: String,
        response: Option<super::types::IpcProfileSwitchResponseSender>,
    ) {
        self.reject_profile_switch(profile_id, message.clone());
        if let Some(response) = response {
            let _ = response.send(super::types::IpcProfileSwitchResponse::Rejected {
                profile_id: profile_id.to_string(),
                generation: self.activation_generation,
                reason: message,
            });
        }
    }
}

pub(super) fn profile_generation(
    profile: &mesh_core_module::package::ShellProfile,
) -> Result<String, ShellRunError> {
    let bytes = serde_json::to_vec(profile).map_err(|error| {
        ShellRunError::Package(format!("profile serialization failed: {error}"))
    })?;
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut hasher);
    Ok(format!("{:016x}", hasher.finish()))
}

fn customizable_slots(
    component: &mesh_core_component::ComponentFile,
) -> Vec<&mesh_core_component::template::SlotNode> {
    fn visit<'a>(
        nodes: &'a [mesh_core_component::template::TemplateNode],
        out: &mut Vec<&'a mesh_core_component::template::SlotNode>,
    ) {
        use mesh_core_component::template::TemplateNode;
        for node in nodes {
            match node {
                TemplateNode::Slot(slot) if slot.customizable => out.push(slot),
                TemplateNode::Element(node) => visit(&node.children, out),
                TemplateNode::Component(node) => visit(&node.children, out),
                TemplateNode::If(node) => {
                    visit(&node.then_children, out);
                    visit(&node.else_children, out);
                }
                TemplateNode::For(node) => visit(&node.children, out),
                _ => {}
            }
        }
    }
    let mut slots = Vec::new();
    if let Some(template) = &component.template {
        visit(&template.root, &mut slots);
    }
    slots
}

fn update_module_prop_override(
    namespace: &mut serde_json::Value,
    instance_id: Option<&str>,
    prop: &str,
    value: Option<serde_json::Value>,
) {
    if !namespace.is_object() {
        *namespace = serde_json::json!({});
    }
    let namespace_object = namespace.as_object_mut().expect("object established above");
    let props = namespace_object
        .entry("props".to_string())
        .or_insert_with(|| serde_json::json!({}));
    if !props.is_object() {
        *props = serde_json::json!({});
    }
    let props_object = props.as_object_mut().expect("object established above");
    if let Some(instance_id) = instance_id {
        let instances = props_object
            .entry("instances".to_string())
            .or_insert_with(|| serde_json::json!({}));
        if !instances.is_object() {
            *instances = serde_json::json!({});
        }
        let instances_object = instances.as_object_mut().expect("object established above");
        let instance = instances_object
            .entry(instance_id.to_string())
            .or_insert_with(|| serde_json::json!({}));
        if !instance.is_object() {
            *instance = serde_json::json!({});
        }
        let instance_object = instance.as_object_mut().expect("object established above");
        match value {
            Some(value) => {
                instance_object.insert(prop.to_string(), value);
            }
            None => {
                instance_object.remove(prop);
            }
        }
        if instance_object.is_empty() {
            instances_object.remove(instance_id);
        }
        if instances_object.is_empty() {
            props_object.remove("instances");
        }
    } else {
        let global = props_object
            .entry("global".to_string())
            .or_insert_with(|| serde_json::json!({}));
        if !global.is_object() {
            *global = serde_json::json!({});
        }
        let global_object = global.as_object_mut().expect("object established above");
        match value {
            Some(value) => {
                global_object.insert(prop.to_string(), value);
            }
            None => {
                global_object.remove(prop);
            }
        }
        if global_object.is_empty() {
            props_object.remove("global");
        }
    }
    if props_object.is_empty() {
        namespace_object.remove("props");
    }
}
