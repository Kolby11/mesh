use super::component::FrontendCatalog;
use super::profile::PackageRuntimeRollback;
use super::*;
use mesh_core_capability::{CapabilityCatalog, PrivilegeLevel};
use mesh_core_module::package::{
    InstalledModuleEntry, MeshLock, ModuleId, ModuleKind, ModuleManifest, ModuleSource,
    PackageTransaction, ProfilePaths, RootModuleGraphManifest, ShellProfile, TrustTier,
    contained_path, load_installed_module_graph, load_module_signature, module_install_path,
    module_store_dir, module_tree_digest, validate_module_tree,
};
use std::collections::VecDeque;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The package service starts the on-disk transaction, while the activation
/// coordinator owns it until the candidate is live. The CLI uses the same
/// manifest, graph, profile, and lock types, but a running shell must be able
/// to perform these operations without shelling out to a privileged
/// management process.
impl Shell {
    pub(in crate::shell) fn apply_install_module(
        &mut self,
        source: &str,
        profile_id: Option<&str>,
        available_only: bool,
        allow_elevated: bool,
        allow_high: bool,
    ) -> Result<VecDeque<CoreRequest>, ShellRunError> {
        if source.trim().is_empty() {
            return Err(package_error("module source cannot be empty"));
        }

        let graph_path = self.installed_module_graph_path();
        let config_dir = graph_path
            .parent()
            .ok_or_else(|| package_error("root module graph has no parent directory"))?;
        let mut transaction = PackageTransaction::begin(config_dir, "install")
            .map_err(|error| package_error(error.to_string()))?;
        let package_rollback = PackageRuntimeRollback::capture(self);
        let root = RootModuleGraphManifest::from_path(&graph_path)
            .map_err(|error| package_error(error.to_string()))?;
        let modules_dir = config_dir.join(&root.modules_dir);
        transaction
            .protect_package_state(&graph_path, &modules_dir)
            .map_err(|error| package_error(error.to_string()))?;
        let mut staged = stage_module_source(source, &transaction.staging_dir())?;
        let manifest = ModuleManifest::from_path(&staged.path().join("module.json"))
            .map_err(|error| package_error(error.to_string()))?;
        check_install_capabilities(&manifest, allow_elevated, allow_high)?;
        let signature = load_module_signature(staged.path())
            .map_err(|error| package_error(error.to_string()))?;
        let digest =
            module_tree_digest(staged.path()).map_err(|error| package_error(error.to_string()))?;
        let trust = if signature.is_some() {
            TrustTier::Verified
        } else {
            TrustTier::for_source(
                &manifest.name,
                matches!(&staged, StagedModuleSource::Git { .. }),
            )
        };
        if let Err(error) = root.trust_policy.validate_candidate(
            &manifest.name,
            &manifest.version,
            &digest,
            trust,
            signature.as_ref(),
        ) {
            return Err(package_error(format!(
                "module {} provenance rejected: {error}",
                manifest.name
            )));
        }

        fs::create_dir_all(&modules_dir).map_err(|error| {
            package_error(format!(
                "failed to create {}: {error}",
                modules_dir.display()
            ))
        })?;
        let destination = module_install_path(&modules_dir, &manifest.name)
            .map_err(|error| package_error(error.to_string()))?;
        if destination.exists() {
            return Err(package_error(format!(
                "module {} is already installed at {}",
                manifest.name,
                destination.display()
            )));
        }
        transaction
            .protect(&destination)
            .map_err(|error| package_error(error.to_string()))?;
        staged.place_at(&destination)?;

        let previous_capability_approvals = root.capability_approvals.clone();
        let mut updated_root = root;
        approve_required_capabilities(&mut updated_root, &manifest);
        let explicit_inventory = !updated_root.modules.is_empty();
        if !updated_root.modules.is_empty() {
            let relative = destination
                .strip_prefix(&modules_dir)
                .map_err(|_| package_error("installed module escaped the modules directory"))?
                .to_string_lossy()
                .replace('\\', "/");
            updated_root.modules.insert(
                manifest.name.clone(),
                InstalledModuleEntry {
                    kind: manifest.mesh.kind,
                    path: relative,
                    enabled: true,
                },
            );
        }
        let root_changed = explicit_inventory
            || updated_root.capability_approvals != previous_capability_approvals;
        if root_changed {
            if let Err(error) = updated_root.save(&graph_path) {
                return Err(package_error(error.to_string()));
            }
        }

        let graph = match load_installed_module_graph(&graph_path) {
            Ok(graph) => graph,
            Err(error) => return Err(package_error(error.to_string())),
        };
        let installed = graph.module(&manifest.name).ok_or_else(|| {
            package_error(format!(
                "installed module {} was not discovered",
                manifest.name
            ))
        })?;
        if installed.kind != manifest.mesh.kind {
            return Err(package_error(format!(
                "installed module {} changed kind while being copied",
                manifest.name
            )));
        }

        let installed_manifests = graph
            .modules()
            .into_iter()
            .map(|module| module.manifest.clone())
            .collect::<Vec<_>>();
        record_lock_entry(
            config_dir,
            &manifest,
            &destination,
            &modules_dir,
            &staged,
            &installed_manifests,
            !available_only,
        )?;

        if !self
            .module_dirs
            .iter()
            .any(|directory| directory == &modules_dir)
        {
            self.module_dirs.push(modules_dir.clone());
        }
        if let Err(error) = self.commit_installed_module_graph(graph) {
            super::profile::abort_package_transaction(
                Some(transaction),
                Some(package_rollback),
                self,
            );
            return Err(error);
        }
        self.discover_modules();
        if let Err(error) = self.resolve_modules() {
            super::profile::abort_package_transaction(
                Some(transaction),
                Some(package_rollback),
                self,
            );
            return Err(error);
        }

        let requests = self.configure_install_activation(
            &manifest,
            profile_id,
            available_only,
            transaction,
            package_rollback,
        )?;
        tracing::info!(module_id = %manifest.name, "installed module through mesh.packages");
        Ok(requests)
    }

    pub(in crate::shell) fn apply_uninstall_module(
        &mut self,
        module_id: &str,
        force: bool,
    ) -> Result<VecDeque<CoreRequest>, ShellRunError> {
        ModuleId::parse(module_id).map_err(|error| package_error(error.to_string()))?;

        let graph_path = self.installed_module_graph_path();
        let config_dir = graph_path
            .parent()
            .ok_or_else(|| package_error("root module graph has no parent directory"))?;
        let mut transaction = PackageTransaction::begin(config_dir, "uninstall")
            .map_err(|error| package_error(error.to_string()))?;
        let package_rollback = PackageRuntimeRollback::capture(self);
        let mut root = RootModuleGraphManifest::from_path(&graph_path)
            .map_err(|error| package_error(error.to_string()))?;
        let graph = load_installed_module_graph(&graph_path)
            .map_err(|error| package_error(error.to_string()))?;
        let node = graph
            .module(module_id)
            .ok_or_else(|| package_error(format!("module {module_id} is not installed")))?;
        transaction
            .protect_package_state(&graph_path, &config_dir.join(&root.modules_dir))
            .map_err(|error| package_error(error.to_string()))?;
        let installed_at = contained_path(
            &config_dir.join(&root.modules_dir),
            &node.path,
            "installed module path",
        )
        .map_err(|error| package_error(error.to_string()))?;

        let lock_path = config_dir.join("mesh.lock");
        let mut lock = MeshLock::load_or_default(&lock_path)
            .map_err(|error| package_error(error.to_string()))?;
        if !force
            && lock
                .modules
                .get(module_id)
                .is_some_and(|entry| !entry.requested_by.is_empty())
        {
            let requesters = lock
                .modules
                .get(module_id)
                .map(|entry| entry.requested_by.iter().cloned().collect::<Vec<_>>())
                .unwrap_or_default();
            return Err(package_error(format!(
                "{module_id} is still required by {}; remove those modules first or repeat with force",
                requesters.join(", ")
            )));
        }

        if !force {
            if graph
                .layout_entrypoint()
                .is_some_and(|layout| layout.module_id == module_id)
            {
                return Err(package_error(format!(
                    "{module_id} is the active layout module; select another layout or repeat with force"
                )));
            }
            if graph
                .backend_provider_contributions()
                .into_iter()
                .filter(|provider| provider.module_id == module_id)
                .any(|provider| {
                    graph
                        .active_provider(&provider.interface)
                        .is_some_and(|active| active.module_id == module_id)
                })
            {
                return Err(package_error(format!(
                    "{module_id} provides an active backend; select another provider or repeat with force"
                )));
            }
        }

        let paths = ProfilePaths::from_root_graph(&graph_path)
            .map_err(|error| package_error(error.to_string()))?;
        let active_profile = paths
            .active_profile_id()
            .map_err(|error| package_error(error.to_string()))?;
        let mut changed_profiles = Vec::new();
        for profile_id in paths
            .list()
            .map_err(|error| package_error(error.to_string()))?
        {
            let mut profile = paths
                .load(&profile_id)
                .map_err(|error| package_error(error.to_string()))?;
            let expected_revision = profile.revision;
            if !profile_references_module(&profile, module_id) {
                continue;
            }
            if !force {
                return Err(package_error(format!(
                    "{module_id} is referenced by profile {profile_id}; remove it from that profile or repeat with force"
                )));
            }
            remove_module_from_profile(&mut profile, module_id);
            paths
                .save_if_revision(&profile_id, &profile, expected_revision)
                .map_err(|error| package_error(error.to_string()))?;
            changed_profiles.push(profile_id);
        }

        if force {
            if root
                .layout
                .as_ref()
                .and_then(|layout| layout.entrypoint.split(':').next())
                == Some(module_id)
            {
                root.layout = None;
            }
            root.disabled.retain(|disabled| disabled != module_id);
            root.capability_approvals.remove(module_id);
            root.providers.retain(|_, provider| provider != module_id);
            if root
                .theme
                .as_ref()
                .is_some_and(|theme| theme.active == module_id)
            {
                root.theme = None;
            }
        }

        let explicit_inventory = !root.modules.is_empty();
        root.modules.remove(module_id);
        if explicit_inventory {
            root.save(&graph_path)
                .map_err(|error| package_error(error.to_string()))?;
        }

        transaction.remove(&installed_at).map_err(|error| {
            package_error(format!(
                "failed to remove {}: {error}",
                installed_at.display()
            ))
        })?;
        let active_profile_changed = active_profile
            .as_deref()
            .is_some_and(|profile_id| changed_profiles.iter().any(|id| id == profile_id));
        let mut requests = VecDeque::new();
        let mut transaction = Some(transaction);
        let mut package_rollback = Some(package_rollback);

        let new_graph = match load_installed_module_graph(&graph_path) {
            Ok(graph) => graph,
            Err(error) => {
                super::profile::abort_package_transaction(
                    transaction.take(),
                    package_rollback.take(),
                    self,
                );
                return Err(package_error(error.to_string()));
            }
        };
        // Do not remove the in-memory module or its live runtime until the
        // replacement graph has loaded successfully. A broken on-disk
        // candidate must leave the last-known-good activation coherent.
        if let Err(error) = self.commit_installed_module_graph(new_graph.clone()) {
            super::profile::abort_package_transaction(
                transaction.take(),
                package_rollback.take(),
                self,
            );
            return Err(error);
        }
        self.modules.remove(module_id);
        self.discover_modules();
        if let Err(error) = self.resolve_modules() {
            super::profile::abort_package_transaction(
                transaction.take(),
                package_rollback.take(),
                self,
            );
            return Err(error);
        }
        self.register_interfaces_from_graph(&new_graph);

        let lock_changed = lock.modules.remove(module_id).is_some()
            || lock
                .composition
                .as_ref()
                .is_some_and(|composition| composition.module == module_id);
        if lock
            .composition
            .as_ref()
            .is_some_and(|composition| composition.module == module_id)
        {
            lock.composition = None;
        }
        let remaining_manifests = new_graph
            .modules()
            .into_iter()
            .map(|module| module.manifest.clone())
            .collect::<Vec<_>>();
        lock.refresh_metadata(remaining_manifests.iter());
        if lock_changed || lock_path.exists() {
            if let Err(error) = MeshLock::archive(&lock_path, &config_dir.join("lock-history")) {
                super::profile::abort_package_transaction(
                    transaction.take(),
                    package_rollback.take(),
                    self,
                );
                return Err(package_error(error.to_string()));
            }
            if let Err(error) = lock.save_with_store(
                &lock_path,
                &config_dir.join(&root.modules_dir),
                &module_store_dir(config_dir),
            ) {
                super::profile::abort_package_transaction(
                    transaction.take(),
                    package_rollback.take(),
                    self,
                );
                return Err(package_error(error.to_string()));
            }
        }

        if active_profile_changed {
            let activation_generation = self.activation_generation;
            let package_transaction = transaction.take();
            let package_runtime_rollback = package_rollback.take();
            let package_transaction_requested = package_transaction.is_some();
            requests.extend(self.begin_profile_activation_with_package_transaction(
                active_profile.as_deref().unwrap_or_default(),
                None,
                package_transaction,
                package_runtime_rollback,
            ));
            if package_transaction_requested
                && !self.profile_transition_pending()
                && self.activation_generation == activation_generation
            {
                super::profile::abort_package_transaction(
                    transaction.take(),
                    package_rollback.take(),
                    self,
                );
                return Err(package_error(
                    "package activation was rejected before reaching the runtime commit",
                ));
            }
        } else if active_profile.is_none() {
            // Uninstall changes the resolved graph even when the removed
            // module was not a frontend. Reconcile the complete legacy graph
            // so backend providers, resources, contributions, and surfaces
            // all retire through the same activation coordinator.
            let activation_generation = self.activation_generation;
            let package_transaction = transaction.take();
            let package_runtime_rollback = package_rollback.take();
            let package_transaction_requested = package_transaction.is_some();
            requests.extend(self.activate_graph_candidate_with_package_transaction(
                new_graph.clone(),
                package_transaction,
                package_runtime_rollback,
            ));
            if package_transaction_requested
                && !self.profile_transition_pending()
                && self.activation_generation == activation_generation
            {
                super::profile::abort_package_transaction(
                    transaction.take(),
                    package_rollback.take(),
                    self,
                );
                return Err(package_error(
                    "package activation was rejected before reaching the runtime commit",
                ));
            }
        } else {
            let previous_catalog = self.frontend_catalog.snapshot().catalog;
            let catalog = match FrontendCatalog::from_modules_reusing(
                &self.modules,
                Some(&new_graph),
                Some(&previous_catalog),
            ) {
                Ok(catalog) => catalog,
                Err(error) => {
                    super::profile::abort_package_transaction(
                        transaction.take(),
                        package_rollback.take(),
                        self,
                    );
                    return Err(error);
                }
            };
            self.frontend_catalog.replace(catalog, None);
            self.sync_frontend_catalog_components();
        }

        tracing::info!(module_id, "uninstalled module through mesh.packages");
        if let Some(transaction) = transaction {
            if let Some(package_rollback) = package_rollback {
                self.commit_package_transaction(transaction, package_rollback)?;
            } else {
                transaction
                    .commit()
                    .map_err(|error| package_error(error.to_string()))?;
            }
        }
        Ok(requests)
    }

    fn commit_package_transaction(
        &mut self,
        transaction: PackageTransaction,
        package_rollback: PackageRuntimeRollback,
    ) -> Result<(), ShellRunError> {
        if let Err(error) = transaction.commit() {
            // `PackageTransaction` restores durable state from its Drop
            // guard when commit fails before the journal reaches the
            // committed phase. Restore the live package-derived mirrors as
            // well, so a failed finalization cannot leave the shell ahead of
            // the durable graph.
            super::profile::abort_package_transaction(None, Some(package_rollback), self);
            return Err(package_error(error.to_string()));
        }
        Ok(())
    }

    fn configure_install_activation(
        &mut self,
        manifest: &ModuleManifest,
        profile_id: Option<&str>,
        available_only: bool,
        transaction: PackageTransaction,
        package_rollback: PackageRuntimeRollback,
    ) -> Result<VecDeque<CoreRequest>, ShellRunError> {
        macro_rules! package_try {
            ($result:expr) => {
                match $result {
                    Ok(value) => value,
                    Err(error) => {
                        super::profile::abort_package_transaction(
                            Some(transaction),
                            Some(package_rollback),
                            self,
                        );
                        return Err(error);
                    }
                }
            };
        }
        if available_only {
            self.commit_package_transaction(transaction, package_rollback)?;
            return Ok(VecDeque::new());
        }

        let paths = package_try!(
            ProfilePaths::from_root_graph(&self.installed_module_graph_path())
                .map_err(|error| package_error(error.to_string()))
        );
        let selected_profile = profile_id.map(str::to_string).or(package_try!(
            paths
                .active_profile_id()
                .map_err(|error| package_error(error.to_string()))
        ));

        match manifest.mesh.kind {
            ModuleKind::Composition => {
                let profile_id = selected_profile
                    .unwrap_or_else(|| mesh_core_module::package::DEFAULT_PROFILE_ID.to_string());
                let mut profile = package_try!(
                    paths
                        .load_or_default(&profile_id)
                        .map_err(|error| package_error(error.to_string()))
                );
                let expected_revision = profile.revision;
                profile.from = Some(mesh_core_module::package::CompositionRef {
                    module: manifest.name.clone(),
                    version: Some(manifest.version.clone()),
                });
                package_try!(
                    paths
                        .save_if_revision(&profile_id, &profile, expected_revision)
                        .map_err(|error| package_error(error.to_string()))
                );
                if self.active_profile_id.as_deref() == Some(profile_id.as_str())
                    || self.active_profile_id.is_none()
                {
                    let activation_generation = self.activation_generation;
                    let requests = self.begin_profile_activation_with_package_transaction(
                        &profile_id,
                        None,
                        Some(transaction),
                        Some(package_rollback),
                    );
                    if !self.profile_transition_pending()
                        && self.activation_generation == activation_generation
                    {
                        return Err(package_error(
                            "package activation was rejected before reaching the runtime commit",
                        ));
                    }
                    Ok(requests)
                } else {
                    self.commit_package_transaction(transaction, package_rollback)?;
                    Ok(VecDeque::new())
                }
            }
            ModuleKind::Frontend => {
                let Some(profile_id) = selected_profile else {
                    let graph = package_try!(
                        self.load_installed_module_graph_cached()
                            .map_err(|error| package_error(error.to_string()))
                    )
                    .clone();
                    let activation_generation = self.activation_generation;
                    let requests = self.activate_graph_candidate_with_package_transaction(
                        graph,
                        Some(transaction),
                        Some(package_rollback),
                    );
                    if !self.profile_transition_pending()
                        && self.activation_generation == activation_generation
                    {
                        return Err(package_error(
                            "package activation was rejected before reaching the runtime commit",
                        ));
                    }
                    return Ok(requests);
                };
                let mut profile = package_try!(
                    paths
                        .load_or_default(&profile_id)
                        .map_err(|error| package_error(error.to_string()))
                );
                let expected_revision = profile.revision;
                package_try!(
                    profile
                        .add_frontend(manifest)
                        .map_err(|error| package_error(error.to_string()))
                );
                let manifests = package_try!(
                    self.load_installed_module_graph_cached()
                        .map_err(|error| package_error(error.to_string()))
                )
                .modules()
                .into_iter()
                .map(|module| module.manifest.clone())
                .collect::<Vec<_>>();
                package_try!(
                    profile
                        .active_module_ids(manifests.iter())
                        .map_err(|error| package_error(error.to_string()))
                );
                package_try!(
                    paths
                        .save_if_revision(&profile_id, &profile, expected_revision)
                        .map_err(|error| package_error(error.to_string()))
                );
                if self.active_profile_id.as_deref() == Some(profile_id.as_str())
                    || self.active_profile_id.is_none()
                {
                    let activation_generation = self.activation_generation;
                    let requests = self.begin_profile_activation_with_package_transaction(
                        &profile_id,
                        None,
                        Some(transaction),
                        Some(package_rollback),
                    );
                    if !self.profile_transition_pending()
                        && self.activation_generation == activation_generation
                    {
                        return Err(package_error(
                            "package activation was rejected before reaching the runtime commit",
                        ));
                    }
                    Ok(requests)
                } else {
                    self.commit_package_transaction(transaction, package_rollback)?;
                    Ok(VecDeque::new())
                }
            }
            _ if selected_profile.is_none() => {
                let graph = package_try!(
                    self.load_installed_module_graph_cached()
                        .map_err(|error| package_error(error.to_string()))
                )
                .clone();
                let activation_generation = self.activation_generation;
                let requests = self.activate_graph_candidate_with_package_transaction(
                    graph,
                    Some(transaction),
                    Some(package_rollback),
                );
                if !self.profile_transition_pending()
                    && self.activation_generation == activation_generation
                {
                    return Err(package_error(
                        "package activation was rejected before reaching the runtime commit",
                    ));
                }
                Ok(requests)
            }
            _ => {
                self.commit_package_transaction(transaction, package_rollback)?;
                Ok(VecDeque::new())
            }
        }
    }
}

fn package_error(message: impl Into<String>) -> ShellRunError {
    ShellRunError::Package(message.into())
}

fn check_install_capabilities(
    manifest: &ModuleManifest,
    allow_elevated: bool,
    allow_high: bool,
) -> Result<(), ShellRunError> {
    let catalog = CapabilityCatalog::builtin();
    let requested = manifest
        .mesh
        .capabilities
        .required
        .iter()
        .chain(manifest.mesh.capabilities.optional.iter())
        .map(|id| mesh_core_capability::Capability::new(id.clone()));
    for capability in requested {
        let level = catalog
            .validate(capability.id())
            .map_err(|error| package_error(error.to_string()))?;
        match level {
            PrivilegeLevel::High if !allow_high => {
                return Err(package_error(format!(
                    "{} requests high capability {}; repeat with allow_high",
                    manifest.name, capability
                )));
            }
            PrivilegeLevel::Elevated if !allow_elevated && !allow_high => {
                return Err(package_error(format!(
                    "{} requests elevated capability {}; repeat with allow_elevated",
                    manifest.name, capability
                )));
            }
            _ => {}
        }
    }
    Ok(())
}

fn approve_required_capabilities(root: &mut RootModuleGraphManifest, manifest: &ModuleManifest) {
    let approvals = root
        .capability_approvals
        .entry(manifest.name.clone())
        .or_default();
    for capability in &manifest.mesh.capabilities.required {
        if !approvals.contains(capability) {
            approvals.push(capability.clone());
        }
    }
    approvals.sort();
    approvals.dedup();
}

fn record_lock_entry(
    config_dir: &Path,
    manifest: &ModuleManifest,
    installed_at: &Path,
    modules_dir: &Path,
    source: &StagedModuleSource,
    installed_manifests: &[ModuleManifest],
    activate_composition: bool,
) -> Result<(), ShellRunError> {
    let lock_path = config_dir.join("mesh.lock");
    let mut lock =
        MeshLock::load_or_default(&lock_path).map_err(|error| package_error(error.to_string()))?;
    let digest =
        module_tree_digest(installed_at).map_err(|error| package_error(error.to_string()))?;
    let (module_source, revision) = match source {
        StagedModuleSource::Local(path) => (
            ModuleSource::Path {
                path: path.display().to_string(),
            },
            None,
        ),
        StagedModuleSource::Git {
            url,
            reference,
            revision,
            ..
        } => (
            ModuleSource::Git {
                url: url.clone(),
                reference: reference.clone(),
            },
            Some(revision.clone()),
        ),
    };
    let signature =
        load_module_signature(installed_at).map_err(|error| package_error(error.to_string()))?;
    let trust = if signature.is_some() {
        TrustTier::Verified
    } else {
        TrustTier::for_source(
            &manifest.name,
            matches!(&module_source, ModuleSource::Git { .. }),
        )
    };
    lock.modules.insert(
        manifest.name.clone(),
        mesh_core_module::package::LockedModule {
            version: manifest.version.clone(),
            source: module_source,
            revision,
            digest,
            trust,
            signature,
            dependencies: Default::default(),
            requested_by: Default::default(),
        },
    );
    if activate_composition && manifest.mesh.kind == ModuleKind::Composition {
        lock.composition = Some(mesh_core_module::package::LockedComposition {
            module: manifest.name.clone(),
            version: manifest.version.clone(),
        });
    }
    lock.refresh_metadata(installed_manifests.iter());
    MeshLock::archive(&lock_path, &config_dir.join("lock-history"))
        .map_err(|error| package_error(error.to_string()))?;
    lock.save_with_store(&lock_path, modules_dir, &module_store_dir(config_dir))
        .map_err(|error| package_error(error.to_string()))
}

enum StagedModuleSource {
    Local(PathBuf),
    Git {
        checkout: PathBuf,
        url: String,
        reference: Option<String>,
        revision: String,
    },
}

impl StagedModuleSource {
    fn path(&self) -> &Path {
        match self {
            Self::Local(path) => path,
            Self::Git { checkout, .. } => checkout,
        }
    }

    fn place_at(&mut self, destination: &Path) -> Result<(), ShellRunError> {
        validate_module_tree(self.path()).map_err(|error| package_error(error.to_string()))?;
        match self {
            Self::Local(path) => copy_module_tree(path, destination)
                .map_err(|error| package_error(error.to_string())),
            Self::Git { checkout, .. } => fs::rename(&*checkout, destination).map_err(|error| {
                package_error(format!("failed to move staged Git checkout: {error}"))
            }),
        }
    }
}

impl Drop for StagedModuleSource {
    fn drop(&mut self) {
        if let Self::Git { checkout, .. } = self {
            let _ = fs::remove_dir_all(checkout);
        }
    }
}

fn stage_module_source(
    source: &str,
    modules_dir: &Path,
) -> Result<StagedModuleSource, ShellRunError> {
    let local = PathBuf::from(source);
    if fs::symlink_metadata(&local)
        .map(|metadata| metadata.is_dir())
        .unwrap_or(false)
    {
        validate_module_tree(&local).map_err(|error| package_error(error.to_string()))?;
        return Ok(StagedModuleSource::Local(local));
    }

    let (url, reference) = parse_git_source(source)?;
    fs::create_dir_all(modules_dir).map_err(|error| {
        package_error(format!(
            "failed to create {}: {error}",
            modules_dir.display()
        ))
    })?;
    let checkout_name = format!(
        ".mesh-install-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    );
    let checkout = contained_path(modules_dir, &checkout_name, "staged module path")
        .map_err(|error| package_error(error.to_string()))?;
    let clone = Command::new("git")
        .args(["clone", "--quiet", &url])
        .arg(&checkout)
        .output()
        .map_err(|error| package_error(format!("failed to run git clone: {error}")))?;
    if !clone.status.success() {
        let _ = fs::remove_dir_all(&checkout);
        return Err(package_error(format!(
            "git clone failed: {}",
            command_error(&clone)
        )));
    }
    if let Some(reference) = &reference {
        let checkout_result = Command::new("git")
            .args(["-C"])
            .arg(&checkout)
            .args(["checkout", "--quiet", reference])
            .output()
            .map_err(|error| package_error(format!("failed to run git checkout: {error}")))?;
        if !checkout_result.status.success() {
            let _ = fs::remove_dir_all(&checkout);
            return Err(package_error(format!(
                "git checkout of {reference:?} failed: {}",
                command_error(&checkout_result)
            )));
        }
    }
    let revision = Command::new("git")
        .args(["-C"])
        .arg(&checkout)
        .args(["rev-parse", "HEAD"])
        .output()
        .map_err(|error| package_error(format!("failed to read cloned revision: {error}")))?;
    if !revision.status.success() {
        let _ = fs::remove_dir_all(&checkout);
        return Err(package_error(format!(
            "git rev-parse failed: {}",
            command_error(&revision)
        )));
    }
    Ok(StagedModuleSource::Git {
        checkout,
        url,
        reference,
        revision: String::from_utf8_lossy(&revision.stdout).trim().to_string(),
    })
}

fn parse_git_source(source: &str) -> Result<(String, Option<String>), ShellRunError> {
    let (url, reference) = match source.rsplit_once('#') {
        Some((url, reference)) if !reference.is_empty() => (url, Some(reference.to_string())),
        Some(_) => {
            return Err(package_error(
                "Git source has an empty ref after '#'; omit '#' or provide a ref",
            ));
        }
        None => (source, None),
    };
    if url.trim().is_empty() {
        return Err(package_error("Git source URL cannot be empty"));
    }
    Ok((url.to_string(), reference))
}

fn command_error(output: &std::process::Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if stderr.is_empty() {
        format!("exit status {}", output.status)
    } else {
        stderr
    }
}

fn copy_module_tree(source: &Path, destination: &Path) -> std::io::Result<()> {
    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let target = destination.join(entry.file_name());
        if file_type.is_symlink() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!(
                    "module contains unsupported symlink {}",
                    entry.path().display()
                ),
            ));
        }
        if file_type.is_dir() {
            copy_module_tree(&entry.path(), &target)?;
        } else if file_type.is_file() {
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

fn profile_references_module(profile: &ShellProfile, module_id: &str) -> bool {
    profile
        .from
        .as_ref()
        .is_some_and(|from| from.module == module_id)
        || profile.roots.values().any(|root| root.module == module_id)
        || profile.background_services.contains(module_id)
        || profile
            .providers
            .values()
            .any(|provider| provider == module_id)
        || profile.resources.theme.as_deref() == Some(module_id)
        || profile.resources.icons.iter().any(|id| id == module_id)
        || profile.resources.fonts.iter().any(|id| id == module_id)
        || profile.resources.languages.iter().any(|id| id == module_id)
}

fn remove_module_from_profile(profile: &mut ShellProfile, module_id: &str) {
    if profile
        .from
        .as_ref()
        .is_some_and(|from| from.module == module_id)
    {
        profile.from = None;
    }
    profile.roots.retain(|_, root| root.module != module_id);
    profile.background_services.remove(module_id);
    profile
        .providers
        .retain(|_, provider| provider != module_id);
    if profile.resources.theme.as_deref() == Some(module_id) {
        profile.resources.theme = None;
    }
    profile.resources.icons.retain(|id| id != module_id);
    profile.resources.fonts.retain(|id| id != module_id);
    profile.resources.languages.retain(|id| id != module_id);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_reference_cleanup_removes_all_module_uses() {
        let mut profile = ShellProfile::new();
        profile.from = Some(mesh_core_module::package::CompositionRef {
            module: "@mesh/desktop".into(),
            version: None,
        });
        profile.roots.insert(
            "@mesh/panel#default".into(),
            mesh_core_module::package::ProfileRootInstance {
                module: "@mesh/panel".into(),
                ..Default::default()
            },
        );
        profile.background_services.insert("@mesh/audio".into());
        profile
            .providers
            .insert("mesh.audio".into(), "@mesh/audio".into());
        profile.resources.theme = Some("@mesh/theme".into());
        profile.resources.icons.push("@mesh/icons".into());

        assert!(profile_references_module(&profile, "@mesh/audio"));
        remove_module_from_profile(&mut profile, "@mesh/audio");
        assert!(!profile_references_module(&profile, "@mesh/audio"));
        assert!(profile_references_module(&profile, "@mesh/desktop"));
    }

    #[test]
    fn parse_git_source_keeps_optional_ref() {
        assert_eq!(
            parse_git_source("https://example.test/mesh.git#v1").unwrap(),
            ("https://example.test/mesh.git".into(), Some("v1".into()))
        );
        assert!(parse_git_source("https://example.test/mesh.git#").is_err());
    }
}
