use super::component::FrontendCatalog;
use super::profile::PackageRuntimeRollback;
use super::*;
use mesh_core_capability::{CapabilityCatalog, PrivilegeLevel};
use mesh_core_module::package::{
    InstalledModuleEntry, MeshLock, ModuleId, ModuleKind, ModuleManifest, ModuleSource,
    PackageOperation, PackageOwner, PackageTransaction, ProfilePaths, RootModuleGraphManifest,
    TrustTier, contained_path, load_authoring_snapshot, load_module_signature, module_install_path,
    module_tree_digest,
};
use std::collections::VecDeque;
use std::fs;

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
        let mut transaction =
            PackageTransaction::begin(config_dir, PackageOwner::Shell, PackageOperation::Install)
                .map_err(|error| package_error(error.to_string()))?;
        let package_rollback = PackageRuntimeRollback::capture(self);
        let root = RootModuleGraphManifest::from_path(&graph_path)
            .map_err(|error| package_error(error.to_string()))?;
        let modules_dir = config_dir.join(&root.modules_dir);
        transaction
            .protect_package_state(&graph_path, &modules_dir)
            .map_err(|error| package_error(error.to_string()))?;
        let staged = transaction
            .stage_module_source(source)
            .map_err(|error| package_error(error.to_string()))?;
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
                matches!(staged.source(), ModuleSource::Git { .. }),
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
        transaction
            .place_staged_module(&staged, &destination)
            .map_err(|error| package_error(error.to_string()))?;

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
            updated_root.record_installed_module(
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
            if let Err(error) = transaction.save_root(&graph_path, &updated_root) {
                return Err(package_error(error.to_string()));
            }
        }

        let graph = match load_authoring_snapshot(&graph_path) {
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
        transaction
            .record_module_lock(
                &manifest,
                &destination,
                &modules_dir,
                staged.source(),
                staged.revision(),
                &installed_manifests,
                !available_only,
            )
            .map_err(|error| package_error(error.to_string()))?;

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
        let mut transaction =
            PackageTransaction::begin(config_dir, PackageOwner::Shell, PackageOperation::Uninstall)
                .map_err(|error| package_error(error.to_string()))?;
        let package_rollback = PackageRuntimeRollback::capture(self);
        let mut root = RootModuleGraphManifest::from_path(&graph_path)
            .map_err(|error| package_error(error.to_string()))?;
        let graph = load_authoring_snapshot(&graph_path)
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
            if !profile.references_module(module_id) {
                continue;
            }
            if !force {
                return Err(package_error(format!(
                    "{module_id} is referenced by profile {profile_id}; remove it from that profile or repeat with force"
                )));
            }
            profile.remove_module_references(module_id);
            transaction
                .save_profile_if_revision(&paths, &profile_id, &profile, expected_revision)
                .map_err(|error| package_error(error.to_string()))?;
            changed_profiles.push(profile_id);
        }

        let explicit_inventory = !root.modules.is_empty();
        if force {
            root.remove_module_references(module_id);
        }

        if !force {
            root.modules.remove(module_id);
        }
        if explicit_inventory {
            transaction
                .save_root(&graph_path, &root)
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

        let new_graph = match load_authoring_snapshot(&graph_path) {
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
            if let Err(error) = transaction
                .as_mut()
                .expect("package transaction is retained until lock persistence")
                .save_lock(&mut lock, &config_dir.join(&root.modules_dir))
            {
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
            self.frontend_catalog
                .replace_with_graph(catalog, None, &new_graph);
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
        mut transaction: PackageTransaction,
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
                    transaction
                        .save_profile_if_revision(&paths, &profile_id, &profile, expected_revision)
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
                    transaction
                        .save_profile_if_revision(&paths, &profile_id, &profile, expected_revision)
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
