use super::*;
use std::path::{Path, PathBuf};

/// Durable decisions for installing one reviewed module. Source preparation,
/// approval UI and live runtime publication remain with the caller.
pub struct InstallPlan {
    pub root: RootModuleGraphManifest,
    pub profile: Option<(String, ShellProfile)>,
    pub activate_composition: bool,
}

impl InstallPlan {
    pub fn prepare_inventory(
        mut root: RootModuleGraphManifest,
        manifest: &ModuleManifest,
        destination: &Path,
        modules_dir: &Path,
    ) -> Result<RootModuleGraphManifest, ModuleManifestError> {
        let approvals = root
            .capability_approvals
            .entry(manifest.name.clone())
            .or_default();
        approvals.extend(manifest.mesh.capabilities.required.iter().cloned());
        approvals.sort();
        approvals.dedup();
        if !root.modules.is_empty() {
            let relative = destination
                .strip_prefix(modules_dir)
                .map_err(|_| {
                    ModuleManifestError::Validation(
                        "installed module escaped the modules directory".into(),
                    )
                })?
                .to_string_lossy()
                .replace('\\', "/");
            root.record_installed_module(
                manifest.name.clone(),
                InstalledModuleEntry {
                    kind: manifest.mesh.kind,
                    path: relative,
                    enabled: true,
                },
            );
        }
        Ok(root)
    }

    pub fn prepare(
        root: RootModuleGraphManifest,
        paths: &ProfilePaths,
        manifest: &ModuleManifest,
        manifests: &[ModuleManifest],
        profile_id: Option<&str>,
        available_only: bool,
    ) -> Result<Self, ModuleManifestError> {
        let mut plan = Self {
            root,
            profile: None,
            activate_composition: false,
        };
        if available_only {
            return Ok(plan);
        }
        let selected = profile_id
            .map(str::to_string)
            .or(paths.active_profile_id()?);
        let selected = match manifest.mesh.kind {
            ModuleKind::Composition => {
                plan.activate_composition = true;
                Some(selected.unwrap_or_else(|| DEFAULT_PROFILE_ID.to_string()))
            }
            ModuleKind::Ui if manifest.mesh.has_default_placement() => selected,
            _ => None,
        };
        if let Some(id) = selected {
            let mut profile = paths.load_or_default(&id)?;
            if plan.activate_composition {
                profile.from = Some(CompositionRef {
                    module: manifest.name.clone(),
                    version: Some(manifest.version.clone()),
                });
            } else {
                profile.add_root(manifest)?;
            }
            profile.active_module_ids(manifests)?;
            plan.profile = Some((id, profile));
        }
        Ok(plan)
    }

    pub fn persist_profile(
        &self,
        transaction: &mut PackageTransaction,
        paths: &ProfilePaths,
    ) -> Result<(), ModuleManifestError> {
        if let Some((id, profile)) = &self.profile {
            transaction.save_profile_if_revision(paths, id, profile, profile.revision)?;
        }
        Ok(())
    }
}

impl InstallPlan {
    /// Select a new profile for offline clients. A running shell publishes the
    /// pointer only after preparing its runtime candidate.
    pub fn select_if_missing(
        &self,
        transaction: &mut PackageTransaction,
        paths: &ProfilePaths,
    ) -> Result<(), ModuleManifestError> {
        if let Some((id, _)) = &self.profile
            && paths.active_profile_id()?.is_none()
        {
            transaction.set_active_profile(paths, id)?;
        }
        Ok(())
    }
}

/// A validated removal decision shared by CLI and live package management.
/// No files are changed until every dependent profile has been inspected.
pub struct UninstallPlan {
    pub root: RootModuleGraphManifest,
    pub installed_at: PathBuf,
    pub profiles: Vec<(String, ShellProfile)>,
    pub active_profile_changed: bool,
}

impl UninstallPlan {
    pub fn prepare(
        mut root: RootModuleGraphManifest,
        graph: &InstalledModuleGraph,
        lock: &MeshLock,
        paths: &ProfilePaths,
        modules_dir: &Path,
        module_id: &str,
        force: bool,
    ) -> Result<Self, ModuleManifestError> {
        ModuleId::parse(module_id)?;
        let fail = |message: String| ModuleManifestError::Validation(message);
        let node = graph
            .module(module_id)
            .ok_or_else(|| fail(format!("module {module_id} is not installed")))?;
        let installed_at = contained_path(modules_dir, &node.path, "installed module path")?;
        let manifests = graph
            .modules()
            .into_iter()
            .map(|node| &node.manifest)
            .collect::<Vec<_>>();
        let mut dependents = lock
            .modules
            .get(module_id)
            .map(|entry| entry.requested_by.clone())
            .unwrap_or_default();
        for manifest in &manifests {
            if manifest.name != module_id
                && (manifest.mesh.extends.as_deref() == Some(module_id)
                    || manifest
                        .mesh
                        .dependencies
                        .modules
                        .get(module_id)
                        .is_some_and(|dependency| !dependency.is_optional())
                    || manifest
                        .mesh
                        .uses
                        .modules
                        .get(module_id)
                        .is_some_and(|dependency| !dependency.is_optional()))
            {
                dependents.insert(manifest.name.clone());
            }
        }
        if !force && !dependents.is_empty() {
            return Err(fail(format!(
                "{module_id} is still required by {}; remove those first or repeat with --force",
                dependents.into_iter().collect::<Vec<_>>().join(", ")
            )));
        }
        if !force
            && graph
                .layout_entrypoint()
                .is_some_and(|layout| layout.module_id == module_id)
        {
            return Err(fail(format!(
                "{module_id} is the active layout module; select another layout or repeat with --force"
            )));
        }
        if !force
            && graph
                .backend_provider_contributions()
                .into_iter()
                .any(|provider| {
                    provider.module_id == module_id
                        && graph
                            .active_provider(&provider.interface)
                            .is_some_and(|active| active.module_id == module_id)
                })
        {
            return Err(fail(format!(
                "{module_id} provides an active backend; select another provider or repeat with --force"
            )));
        }
        let compositions = manifests
            .iter()
            .filter(|manifest| manifest.mesh.kind == ModuleKind::Composition)
            .map(|manifest| (manifest.name.as_str(), *manifest))
            .collect();
        let mut profiles = Vec::new();
        for id in paths.list()? {
            let mut profile = paths.load(&id)?;
            let resolved = resolve_composition(&profile, manifests.iter().copied())?;
            let effective = resolved.to_profile();
            let inherits_removed = match &profile.from {
                Some(from) => composition::composition_chain(&from.module, &compositions)?
                    .iter()
                    .any(|id| id == module_id),
                None => false,
            };
            if !inherits_removed
                && !effective.references_module(module_id)
                && !profile.references_module(module_id)
            {
                continue;
            }
            if !force {
                return Err(fail(format!(
                    "{module_id} is referenced by profile {id}; remove it from that profile or repeat with --force"
                )));
            }
            let revision = profile.revision;
            if inherits_removed || effective.resources.theme.as_deref() == Some(module_id) {
                // Flatten the resolved layer before removing provenance; otherwise
                // inherited roots, resources and slot overrides would disappear.
                profile = effective.clone();
                profile.roots = resolved.spec.roots.clone();
                profile.node_slots = resolved.spec.node_slots.clone();
                // Detached orphan overrides remain editable but must not gain
                // an active root merely because their composition disappeared.
                for orphan in &resolved.orphaned_overrides {
                    if let Some(root) = profile.roots.get_mut(orphan) {
                        root.active = Some(false);
                    }
                }
                profile.from = None;
                profile.revision = revision;
            }
            profile.remove_module_references(module_id);
            if profile.from.is_some() {
                for (id, instance) in &effective.roots {
                    if instance.module == module_id {
                        let mut inactive = instance.clone();
                        inactive.active = Some(false);
                        profile.roots.insert(id.clone(), inactive);
                    }
                }
                if effective.background_services.contains(module_id) {
                    profile
                        .remove_background_services
                        .insert(module_id.to_string());
                }
                profile.remove_providers.extend(
                    effective
                        .providers
                        .iter()
                        .filter(|(_, provider)| provider.as_str() == module_id)
                        .map(|(interface, _)| interface.clone()),
                );
                let mut resources = effective.resources.clone();
                for chain in [
                    &mut resources.icons,
                    &mut resources.fonts,
                    &mut resources.languages,
                ]
                .into_iter()
                .flatten()
                {
                    chain.retain(|id| id != module_id);
                }
                profile.resources = resources;
                profile.node_slots = resolved.spec.node_slots.clone();
                profile.slots = effective.slots.clone();
                profile.remove_module_references(module_id);
            }
            profile.validate()?;
            profiles.push((id, profile));
        }
        let active = paths.active_profile_id()?;
        let active_profile_changed =
            active.is_some_and(|active| profiles.iter().any(|(id, _)| *id == active));
        if force {
            root.remove_module_references(module_id);
        } else {
            root.forget_uninstalled_module(module_id);
        }
        Ok(Self {
            root,
            installed_at,
            profiles,
            active_profile_changed,
        })
    }

    pub fn persist(
        &self,
        transaction: &mut PackageTransaction,
        root_path: &Path,
        paths: &ProfilePaths,
    ) -> Result<(), ModuleManifestError> {
        for (id, profile) in &self.profiles {
            transaction.save_profile_if_revision(paths, id, profile, profile.revision)?;
        }
        transaction.save_root(root_path, &self.root)?;
        transaction.remove(&self.installed_at)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!(
                "mesh-management-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            fs::create_dir_all(root.join("modules")).unwrap();
            fs::write(root.join("module.json"), r#"{"name":"@me/config","version":"1","mesh":{"modulesDir":"modules","modules":{}}}"#).unwrap();
            Self(root)
        }
        fn module(&self, name: &str, mut mesh: serde_json::Value) -> ModuleManifest {
            mesh["apiVersion"] = "0.1".into();
            let directory = self.0.join("modules").join(name);
            fs::create_dir_all(&directory).unwrap();
            let value =
                serde_json::json!({"name":format!("@me/{name}"),"version":"1.0.0","mesh":mesh});
            fs::write(directory.join("module.json"), value.to_string()).unwrap();
            fs::write(directory.join("main.mesh"), "<template><box/></template>").unwrap();
            fs::write(directory.join("main.luau"), "function start(self) end").unwrap();
            ModuleManifest::from_path(&directory.join("module.json")).unwrap()
        }
        fn paths(&self) -> ProfilePaths {
            ProfilePaths::from_root_graph(&self.0.join("module.json")).unwrap()
        }
        fn graph(&self) -> InstalledModuleGraph {
            load_authoring_snapshot(&self.0.join("module.json")).unwrap()
        }
        fn plan(&self, module: &str, force: bool) -> Result<UninstallPlan, ModuleManifestError> {
            UninstallPlan::prepare(
                RootModuleGraphManifest::from_path(&self.0.join("module.json"))?,
                &self.graph(),
                &MeshLock::new(),
                &self.paths(),
                &self.0.join("modules"),
                module,
                force,
            )
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            fn make_writable(path: &Path) {
                use std::os::unix::fs::PermissionsExt;
                let Ok(metadata) = fs::symlink_metadata(path) else {
                    return;
                };
                if metadata.file_type().is_symlink() {
                    return;
                }
                let mut permissions = metadata.permissions();
                permissions
                    .set_mode(permissions.mode() | if metadata.is_dir() { 0o700 } else { 0o600 });
                let _ = fs::set_permissions(path, permissions);
                if metadata.is_dir()
                    && let Ok(entries) = fs::read_dir(path)
                {
                    for entry in entries.flatten() {
                        make_writable(&entry.path());
                    }
                }
            }
            make_writable(&self.0);
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn forced_composition_removal_preserves_resolved_contents_and_revision() {
        let fixture = Fixture::new();
        fixture.module(
            "panel",
            serde_json::json!({"kind":"ui","entry":"main.mesh","surface":{"anchor":"top"}}),
        );
        fixture.module(
            "base",
            serde_json::json!({"kind":"composition","compose":{
                "roots":{"@me/panel#top":{"module":"@me/panel","surface":{"anchor":"top"}}},
                "settings":{"@me/panel":{"density":"compact"}},
                "resources":{"icons":[]},
                "slots":{"me.settings":{"order":["@me/panel"]}},
                "nodeSlots":{"@me/panel#top":{"start":{"nodes":[]}}}
            }}),
        );
        fixture.module(
            "desk",
            serde_json::json!({"kind":"composition","extends":"@me/base","compose":{}}),
        );
        let profile = ShellProfile::from_json_str(r#"{"schemaVersion":3,"from":{"module":"@me/desk"},"roots":{"@me/panel#top":{"surface":{"anchor":"bottom"}},"@me/panel#orphan":{"surface":{"anchor":"left"}}}}"#).unwrap();
        let committed = fixture
            .paths()
            .save_if_revision("work", &profile, 0)
            .unwrap();
        fixture.paths().set_active("work").unwrap();
        assert!(fixture.plan("@me/base", false).is_err());
        let plan = fixture.plan("@me/base", true).unwrap();
        assert!(plan.active_profile_changed);
        let candidate = &plan.profiles[0].1;
        assert_eq!(candidate.revision, committed.revision);
        assert!(candidate.from.is_none());
        assert!(!candidate.roots["@me/panel#orphan"].is_active());
        assert_eq!(
            candidate.roots["@me/panel#orphan"]
                .surface
                .as_ref()
                .unwrap()
                .anchor
                .as_deref(),
            Some("left")
        );
        assert_eq!(
            candidate.roots["@me/panel#top"]
                .surface
                .as_ref()
                .unwrap()
                .anchor
                .as_deref(),
            Some("bottom")
        );
        assert_eq!(candidate.settings["@me/panel"]["density"], "compact");
        assert_eq!(candidate.slots["me.settings"].order, ["@me/panel"]);
        assert!(candidate.node_slots.contains_key("@me/panel#top"));
        assert_eq!(candidate.resources.icons, Some(vec![]));
        let mut transaction =
            PackageTransaction::begin(&fixture.0, PackageOwner::Cli, PackageOperation::Uninstall)
                .unwrap();
        plan.persist(
            &mut transaction,
            &fixture.0.join("module.json"),
            &fixture.paths(),
        )
        .unwrap();
        transaction.commit().unwrap();
        assert_eq!(fixture.paths().load("work").unwrap().revision, 2);
        assert!(fixture.graph().module("@me/panel").unwrap().enabled);
    }

    #[test]
    fn forced_service_removal_subtracts_inherited_bindings_without_detaching_the_profile() {
        let fixture = Fixture::new();
        fixture.module(
            "panel",
            serde_json::json!({"kind":"ui","entry":"main.mesh","surface":{"anchor":"top"}}),
        );
        fixture.module(
            "audio",
            serde_json::json!({"kind":"backend","entry":"main.luau",
            "interfaces":[{"name":"me.audio","version":"1","contract":{}}],
            "implements":[{"interface":"me.audio","version":"1"}]}),
        );
        fixture.module(
            "desk",
            serde_json::json!({"kind":"composition","compose":{
                "roots":{"@me/panel#top":{"module":"@me/panel"}},
                "backgroundServices":["@me/audio"],"providers":{"me.audio":"@me/audio"}
            }}),
        );
        let profile =
            ShellProfile::from_json_str(r#"{"schemaVersion":3,"from":{"module":"@me/desk"}}"#)
                .unwrap();
        fixture.paths().save("work", &profile).unwrap();
        fixture.paths().set_active("work").unwrap();
        let plan = fixture.plan("@me/audio", true).unwrap();
        let profile = &plan.profiles[0].1;
        assert_eq!(profile.from.as_ref().unwrap().module, "@me/desk");
        assert!(profile.remove_background_services.contains("@me/audio"));
        assert!(profile.remove_providers.contains("me.audio"));
        let mut transaction =
            PackageTransaction::begin(&fixture.0, PackageOwner::Cli, PackageOperation::Uninstall)
                .unwrap();
        plan.persist(
            &mut transaction,
            &fixture.0.join("module.json"),
            &fixture.paths(),
        )
        .unwrap();
        transaction.commit().unwrap();
        let graph = fixture.graph();
        assert!(graph.module("@me/panel").unwrap().enabled);
        assert!(graph.module("@me/audio").is_none());
    }

    #[test]
    fn a_provider_with_a_different_contract_version_is_not_selected_without_consumers() {
        let fixture = Fixture::new();
        fixture.module(
            "audio",
            serde_json::json!({"kind":"backend","entry":"main.luau",
            "interfaces":[{"name":"me.audio","version":"1.0","contract":{}}],
            "implements":[{"interface":"me.audio","version":"1.1"}]}),
        );
        assert!(fixture.graph().active_provider("me.audio").is_none());
    }

    #[test]
    fn uninstall_refuses_required_installed_dependencies_even_without_a_lock_entry() {
        let fixture = Fixture::new();
        fixture.module("shared", serde_json::json!({"kind":"library"}));
        fixture.module(
            "consumer",
            serde_json::json!({"kind":"library","uses":{"modules":{"@me/shared":"*"}}}),
        );
        let error = fixture.plan("@me/shared", false).err().unwrap().to_string();
        assert!(error.contains("required by @me/consumer"), "{error}");
        assert!(fixture.0.join("modules/shared/module.json").exists());
    }

    #[test]
    fn package_engines_preserve_unlocked_sources_and_return_the_published_generation() {
        let fixture = Fixture::new();
        fixture.module(
            "desk",
            serde_json::json!({"kind":"composition","compose":{}}),
        );
        let paths = fixture.paths();
        let profile =
            ShellProfile::from_json_str(r#"{"schemaVersion":3,"from":{"module":"@me/desk"}}"#)
                .unwrap();
        paths.save("work", &profile).unwrap();
        paths.set_active("work").unwrap();
        let root_path = fixture.0.join("module.json");
        let source = fixture.0.join("source");
        fs::create_dir_all(&source).unwrap();
        fs::write(
            source.join("module.json"),
            r#"{"name":"@me/helpers","version":"1","mesh":{"apiVersion":"0.1","kind":"library"}}"#,
        )
        .unwrap();
        let mut transaction =
            PackageTransaction::begin(&fixture.0, PackageOwner::Cli, PackageOperation::Install)
                .unwrap();
        let installed = transaction
            .install_package(
                &root_path,
                source.to_str().unwrap(),
                None,
                true,
                InstallApproval {
                    allow_elevated: false,
                    allow_high: false,
                },
            )
            .unwrap();
        assert!(installed.graph.module("@me/desk").is_some());
        assert!(
            installed
                .graph
                .module("@me/helpers")
                .unwrap()
                .manifest_path
                .starts_with(fixture.0.join(".mesh-store"))
        );
        transaction.commit().unwrap();
        let mut transaction =
            PackageTransaction::begin(&fixture.0, PackageOwner::Shell, PackageOperation::Uninstall)
                .unwrap();
        let removed = transaction
            .uninstall_package(&root_path, "@me/helpers", false)
            .unwrap();
        assert!(removed.graph.module("@me/desk").is_some());
        assert!(removed.graph.module("@me/helpers").is_none());
        transaction.commit().unwrap();
        assert!(!fixture.0.join("modules/me/helpers").exists());
    }

    #[test]
    fn failed_install_activation_restores_source_profiles_and_inventory() {
        let fixture = Fixture::new();
        let paths = fixture.paths();
        paths.save("work", &ShellProfile::new()).unwrap();
        paths.set_active("work").unwrap();
        let root_path = fixture.0.join("module.json");
        let root_before = fs::read(&root_path).unwrap();
        let profile_before = fs::read(paths.profile_path("work").unwrap()).unwrap();
        let source = fixture.0.join("source");
        fs::create_dir_all(&source).unwrap();
        fs::write(source.join("module.json"), serde_json::json!({"name":"@me/new-panel","version":"1","mesh":{
            "apiVersion":"0.1","kind":"ui","entry":"main.mesh","surface":{"anchor":"top"},"uses":{"modules":{"@me/missing":"*"}}
        }}).to_string()).unwrap();
        fs::write(source.join("main.mesh"), "<template><box/></template>").unwrap();
        let mut transaction =
            PackageTransaction::begin(&fixture.0, PackageOwner::Cli, PackageOperation::Install)
                .unwrap();
        assert!(
            transaction
                .install_package(
                    &root_path,
                    source.to_str().unwrap(),
                    None,
                    false,
                    InstallApproval {
                        allow_elevated: false,
                        allow_high: false
                    }
                )
                .is_err()
        );
        transaction.abort().unwrap();
        assert_eq!(fs::read(&root_path).unwrap(), root_before);
        assert_eq!(
            fs::read(paths.profile_path("work").unwrap()).unwrap(),
            profile_before
        );
        assert!(!fixture.0.join("modules/me/new-panel").exists());
        assert!(!fixture.0.join(".mesh-package-transaction.json").exists());
    }

    #[test]
    fn uninstall_refuses_an_inferred_active_provider_without_an_explicit_binding() {
        let fixture = Fixture::new();
        fixture.module(
            "audio",
            serde_json::json!({"kind":"backend","entry":"main.luau",
            "interfaces":[{"name":"me.audio","version":"1","contract":{}}],
            "implements":[{"interface":"me.audio","version":"1"}]}),
        );
        let error = fixture.plan("@me/audio", false).err().unwrap().to_string();
        assert!(error.contains("active backend"), "{error}");
        assert!(fixture.0.join("modules/audio/module.json").exists());
    }

    #[test]
    fn install_profile_plans_are_revision_checked_and_placement_driven() {
        let fixture = Fixture::new();
        let panel = fixture.module(
            "panel",
            serde_json::json!({"kind":"ui","entry":"main.mesh","surface":{"anchor":"top"}}),
        );
        fixture.paths().save("work", &ShellProfile::new()).unwrap();
        fixture.paths().set_active("work").unwrap();
        let graph = fixture.graph();
        let manifests = graph
            .modules()
            .into_iter()
            .map(|node| node.manifest.clone())
            .collect::<Vec<_>>();
        let root = RootModuleGraphManifest::from_path(&fixture.0.join("module.json")).unwrap();
        let plan = InstallPlan::prepare(
            root.clone(),
            &fixture.paths(),
            &panel,
            &manifests,
            None,
            false,
        )
        .unwrap();
        assert_eq!(plan.profile.as_ref().unwrap().0, "work");
        assert!(
            plan.profile
                .as_ref()
                .unwrap()
                .1
                .roots
                .contains_key("@me/panel#default")
        );
        let available =
            InstallPlan::prepare(root, &fixture.paths(), &panel, &manifests, None, true).unwrap();
        assert!(available.profile.is_none());
        fixture
            .paths()
            .save_if_revision("work", &ShellProfile::new(), 0)
            .unwrap();
        let mut transaction =
            PackageTransaction::begin(&fixture.0, PackageOwner::Cli, PackageOperation::Install)
                .unwrap();
        assert!(
            plan.persist_profile(&mut transaction, &fixture.paths())
                .is_err()
        );
        transaction.abort().unwrap();
        assert_eq!(fixture.paths().load("work").unwrap().revision, 1);
    }
}

#[cfg(test)]
mod inherited_removal_tests {
    use super::*;
    #[test]
    fn invalid_removal_module_ids_are_rejected_when_loading_profiles() {
        assert!(
            ShellProfile::from_json_str(
                r#"{"schemaVersion":3,"removeBackgroundServices":["../audio"]}"#
            )
            .is_err()
        );
        assert!(
            ShellProfile::from_json_str(r#"{"schemaVersion":3,"removeProviders":[""]}"#).is_err()
        );
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PackageInstallError {
    #[error(transparent)]
    Manifest(#[from] ModuleManifestError),
    #[error(transparent)]
    Review(#[from] InstallReviewError),
}

pub struct InstalledPackage {
    pub manifest: ModuleManifest,
    pub destination: PathBuf,
    pub modules_dir: PathBuf,
    pub graph: InstalledModuleGraph,
    pub plan: InstallPlan,
}

impl PackageTransaction {
    /// Stage, review and persist an installation under this transaction's
    /// journal. The caller commits after offline selection or live preparation;
    /// an error leaves rollback to the transaction's abort/Drop boundary.
    pub fn install_package(
        &mut self,
        root_path: &Path,
        source: &str,
        profile_id: Option<&str>,
        available_only: bool,
        approval: InstallApproval,
    ) -> Result<InstalledPackage, PackageInstallError> {
        let root = RootModuleGraphManifest::from_path(root_path)?;
        let config_dir = root_path.parent().ok_or_else(|| {
            ModuleManifestError::Validation("root module graph has no parent directory".into())
        })?;
        let modules_dir = config_dir.join(&root.modules_dir);
        self.protect_package_state(root_path, &modules_dir)?;
        let staged = self.stage_module_source(source)?;
        let manifest = ModuleManifest::from_path(&staged.path().join("module.json"))?;
        let catalog = load_authoring_snapshot(root_path)
            .map(|graph| graph.capability_catalog().clone())
            .unwrap_or_default();
        review_install(
            &root.trust_policy,
            staged.path(),
            &manifest,
            matches!(staged.source(), ModuleSource::Git { .. }),
            &catalog,
            approval,
        )?;
        let destination = module_install_path(&modules_dir, &manifest.name)?;
        if destination.exists() {
            return Err(ModuleManifestError::Validation(format!(
                "module {} is already installed at {}",
                manifest.name,
                destination.display()
            ))
            .into());
        }
        self.place_staged_module(&staged, &destination)?;
        let root = InstallPlan::prepare_inventory(root, &manifest, &destination, &modules_dir)?;
        self.save_root(root_path, &root)?;
        let graph = load_authoring_snapshot(root_path)?;
        if !graph
            .module(&manifest.name)
            .is_some_and(|node| node.kind == manifest.mesh.kind)
        {
            return Err(ModuleManifestError::Validation(format!(
                "installed module {} was not discovered with the expected kind",
                manifest.name
            ))
            .into());
        }
        let manifests = graph
            .modules()
            .into_iter()
            .map(|node| node.manifest.clone())
            .collect::<Vec<_>>();
        let paths = ProfilePaths::from_root_graph(root_path)?;
        let plan = InstallPlan::prepare(
            root,
            &paths,
            &manifest,
            &manifests,
            profile_id,
            available_only,
        )?;
        plan.persist_profile(self, &paths)?;
        self.record_module_lock(
            &manifest,
            &destination,
            &modules_dir,
            staged.source(),
            staged.revision(),
            &manifests,
            plan.activate_composition,
        )?;
        let graph = load_authoring_snapshot(root_path)?;
        Ok(InstalledPackage {
            manifest,
            destination,
            modules_dir,
            graph,
            plan,
        })
    }
}

pub struct UninstalledPackage {
    pub graph: InstalledModuleGraph,
    pub active_profile_changed: bool,
    pub active_profile_id: Option<String>,
}

impl PackageTransaction {
    /// Validate every removal decision, then journal source/profile/lock changes.
    /// The returned graph reflects the newly published durable generation.
    pub fn uninstall_package(
        &mut self,
        root_path: &Path,
        module_id: &str,
        force: bool,
    ) -> Result<UninstalledPackage, ModuleManifestError> {
        let root = RootModuleGraphManifest::from_path(root_path)?;
        let config_dir = root_path.parent().ok_or_else(|| {
            ModuleManifestError::Validation("root module graph has no parent directory".into())
        })?;
        let modules_dir = config_dir.join(&root.modules_dir);
        let graph = load_authoring_snapshot(root_path)?;
        let paths = ProfilePaths::from_root_graph(root_path)?;
        let active_profile_id = paths.active_profile_id()?;
        let lock_path = config_dir.join("mesh.lock");
        let mut lock = MeshLock::load_or_default(&lock_path)?;
        let plan =
            UninstallPlan::prepare(root, &graph, &lock, &paths, &modules_dir, module_id, force)?;
        self.protect_package_state(root_path, &modules_dir)?;
        plan.persist(self, root_path, &paths)?;
        let changed = lock.modules.remove(module_id).is_some()
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
        lock.refresh_metadata(
            graph
                .modules()
                .into_iter()
                .filter(|node| node.id != module_id)
                .map(|node| &node.manifest),
        );
        if changed || lock_path.exists() {
            self.save_lock(&mut lock, &modules_dir)?;
        }
        let graph = load_authoring_snapshot(root_path)?;
        Ok(UninstalledPackage {
            graph,
            active_profile_changed: plan.active_profile_changed,
            active_profile_id,
        })
    }
}
