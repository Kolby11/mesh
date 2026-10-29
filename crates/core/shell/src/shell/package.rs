use super::component::FrontendCatalog;
use super::profile::PackageRuntimeRollback;
use super::*;
use mesh_core_module::package::{
    InstallApproval, ModuleId, PackageOperation, PackageOwner, PackageTransaction,
};
use std::collections::VecDeque;

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
    ) -> Result<VecDeque<SourcedRequest>, ShellRunError> {
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
        let installed = transaction
            .install_package(
                &graph_path,
                source,
                profile_id,
                available_only,
                InstallApproval {
                    allow_elevated,
                    allow_high,
                },
            )
            .map_err(|error| package_error(error.to_string()))?;
        let mesh_core_module::package::InstalledPackage {
            manifest,
            modules_dir,
            graph,
            plan,
            ..
        } = installed;

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
            plan.profile.as_ref().map(|(id, _)| id.as_str()),
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
    ) -> Result<VecDeque<SourcedRequest>, ShellRunError> {
        ModuleId::parse(module_id).map_err(|error| package_error(error.to_string()))?;

        let graph_path = self.installed_module_graph_path();
        let config_dir = graph_path
            .parent()
            .ok_or_else(|| package_error("root module graph has no parent directory"))?;
        let mut transaction =
            PackageTransaction::begin(config_dir, PackageOwner::Shell, PackageOperation::Uninstall)
                .map_err(|error| package_error(error.to_string()))?;
        let package_rollback = PackageRuntimeRollback::capture(self);
        let removed = transaction
            .uninstall_package(&graph_path, module_id, force)
            .map_err(|error| package_error(error.to_string()))?;
        let active_profile = removed.active_profile_id;
        let active_profile_changed = removed.active_profile_changed;
        let new_graph = removed.graph;
        let mut requests = VecDeque::new();
        let mut transaction = Some(transaction);
        let mut package_rollback = Some(package_rollback);
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
                    return Err(error.into());
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
        target_profile: Option<&str>,
        available_only: bool,
        transaction: PackageTransaction,
        package_rollback: PackageRuntimeRollback,
    ) -> Result<VecDeque<SourcedRequest>, ShellRunError> {
        if available_only {
            self.commit_package_transaction(transaction, package_rollback)?;
            return Ok(VecDeque::new());
        }
        let generation = self.activation_generation;
        let requests = if let Some(id) = target_profile {
            if self
                .active_profile_id
                .as_deref()
                .is_some_and(|active| active != id)
            {
                self.commit_package_transaction(transaction, package_rollback)?;
                return Ok(VecDeque::new());
            }
            self.begin_profile_activation_with_package_transaction(
                id,
                None,
                Some(transaction),
                Some(package_rollback),
            )
        } else if self.active_profile_id.is_none() {
            let graph = match self.load_installed_module_graph_cached() {
                Ok(graph) => graph.clone(),
                Err(error) => {
                    super::profile::abort_package_transaction(
                        Some(transaction),
                        Some(package_rollback),
                        self,
                    );
                    return Err(package_error(error.to_string()));
                }
            };
            self.activate_graph_candidate_with_package_transaction(
                graph,
                Some(transaction),
                Some(package_rollback),
            )
        } else {
            self.commit_package_transaction(transaction, package_rollback)?;
            return Ok(VecDeque::new());
        };
        if !self.profile_transition_pending() && self.activation_generation == generation {
            return Err(package_error(
                "package activation was rejected before reaching the runtime commit",
            ));
        }
        Ok(requests)
    }
}

fn package_error(message: impl Into<String>) -> ShellRunError {
    ShellRunError::Package(message.into())
}
