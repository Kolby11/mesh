//! `mesh update`, `mesh rollback`, `mesh uninstall`, `mesh lock verify`.
//!
//! An update is a transaction with a pre-commit refusal point, not a fetch:
//!
//! 1. resolve candidate revisions from each git source
//! 2. diff interface contracts as data — no module code runs
//! 3. diff capabilities, requiring re-approval for new elevated/high grants
//! 4. refuse to overwrite modules the user has edited
//! 5. stage, validate the candidate graph
//! 6. commit source, then the lock, then ask the shell to switch
//!
//! Everything before step 6 can refuse without touching the running shell.

use mesh_core_capability::{
    Capability, CapabilityCatalog, CapabilityPolicy, EffectiveCapabilities, PrivilegeLevel,
};
use mesh_core_module::package::{
    InstalledModuleGraph, LockedModule, MeshLock, ModuleGraphDiff, ModuleManifest, ModuleSource,
    PackageTransaction, RootModuleGraphManifest, has_local_edits, load_installed_module_graph,
    module_install_path, module_tree_digest, validate_module_tree,
};
use mesh_core_service::{CompatibilityClass, diff_contracts, parse_interface_contract};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

/// How a module whose working tree differs from its locked digest is handled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditPolicy {
    /// Refuse and report. The default: an update must never silently discard
    /// work, and installed source is directly editable by design.
    Refuse,
    /// Leave the module at its locked revision and continue with the rest.
    Keep,
    /// Discard local changes.
    Replace,
}

#[derive(Debug, Clone)]
pub struct CandidateModule {
    pub module_id: String,
    pub locked: LockedModule,
    pub candidate_version: String,
    pub candidate_revision: Option<String>,
    pub candidate_manifest: ModuleManifest,
}

impl CandidateModule {
    pub fn is_unchanged(&self) -> bool {
        self.candidate_revision == self.locked.revision
            && self.candidate_version == self.locked.version
    }
}

#[derive(Debug, Default)]
pub struct UpdatePlan {
    pub candidates: Vec<CandidateModule>,
    /// Contract changes that break an installed consumer.
    pub breaking: Vec<String>,
    /// New required capabilities needing explicit approval.
    pub capability_additions: Vec<(String, Capability, PrivilegeLevel)>,
    /// Modules with local edits, under `EditPolicy::Refuse`.
    pub edited: Vec<String>,
    /// Required dependency/provider failures in the staged candidate graph.
    pub graph_breaking: Vec<String>,
    /// The exact capability decisions made for the staged graph.
    pub capability_decisions: BTreeMap<String, EffectiveCapabilities>,
    /// Normalized activation changes between the installed and candidate
    /// graphs. This is the source for dry-run provider/profile output.
    pub graph_diff: Option<ModuleGraphDiff>,
    /// Candidate module trees materialized before the plan is classified. These
    /// paths remain inside the transaction workspace until commit or abort.
    pub staged_paths: BTreeMap<String, PathBuf>,
}

impl UpdatePlan {
    pub fn is_refused(&self) -> bool {
        !self.breaking.is_empty()
            || !self.capability_additions.is_empty()
            || !self.edited.is_empty()
            || !self.graph_breaking.is_empty()
    }

    pub fn changed(&self) -> impl Iterator<Item = &CandidateModule> {
        self.candidates
            .iter()
            .filter(|candidate| !candidate.is_unchanged())
    }
}

/// Read a module's manifest at a git revision without checking it out.
///
/// One `git show` is enough to learn the candidate version, which is what turns
/// a bare commit into something with update semantics.
#[cfg(test)]
pub fn manifest_at_revision(repository: &Path, revision: &str) -> Result<ModuleManifest, String> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(repository)
        .arg("show")
        .arg(format!("{revision}:module.json"))
        .output()
        .map_err(|error| format!("failed to run git show: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "git show {revision}:module.json failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let content = String::from_utf8_lossy(&output.stdout).to_string();
    ModuleManifest::from_json_str(&content).map_err(|error| error.to_string())
}

/// Resolve the revision a git ref currently points at.
pub fn resolve_revision(repository: &Path, reference: Option<&str>) -> Result<String, String> {
    // Resolve a configured origin without fetching into the installed checkout.
    // A planning pass must not change the live repository's refs or index.
    if has_origin(repository) {
        let remote = std::process::Command::new("git")
            .arg("-C")
            .arg(repository)
            .args(["remote", "get-url", "origin"])
            .output()
            .map_err(|error| format!("failed to read git origin: {error}"))?;
        if !remote.status.success() {
            return Err(format!(
                "git remote get-url origin failed: {}",
                String::from_utf8_lossy(&remote.stderr).trim()
            ));
        }
        return resolve_remote_revision(String::from_utf8_lossy(&remote.stdout).trim(), reference);
    }
    let target = reference.unwrap_or("HEAD");
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(repository)
        .arg("rev-parse")
        .arg(target)
        .output()
        .map_err(|error| format!("failed to run git rev-parse: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "cannot resolve {target}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn has_origin(repository: &Path) -> bool {
    std::process::Command::new("git")
        .arg("-C")
        .arg(repository)
        .args(["remote", "get-url", "origin"])
        .output()
        .is_ok_and(|output| output.status.success())
}

/// Resolve a remote ref without fetching into the installed checkout.
fn resolve_remote_revision(remote: &str, reference: Option<&str>) -> Result<String, String> {
    let mut queries = Vec::new();
    if let Some(reference) = reference {
        if reference.starts_with("refs/") {
            queries.push(reference.to_string());
        } else {
            queries.push(format!("refs/heads/{reference}"));
            queries.push(format!("refs/tags/{reference}"));
            queries.push(reference.to_string());
        }
    } else {
        queries.push("HEAD".into());
    }

    let mut last_error = String::new();
    for query in queries {
        let output = std::process::Command::new("git")
            .args(["ls-remote", "--exit-code"])
            .arg(remote)
            .arg(&query)
            .output()
            .map_err(|error| format!("failed to run git ls-remote: {error}"))?;
        if !output.status.success() {
            last_error = String::from_utf8_lossy(&output.stderr).trim().to_string();
            continue;
        }
        let revision = String::from_utf8_lossy(&output.stdout)
            .lines()
            .find_map(|line| line.split_whitespace().next())
            .unwrap_or_default()
            .to_string();
        if !revision.is_empty() {
            return Ok(revision);
        }
    }

    Err(format!(
        "cannot resolve {} from {remote}: {}",
        reference.unwrap_or("default branch"),
        if last_error.is_empty() {
            "no matching ref"
        } else {
            &last_error
        }
    ))
}

fn resolve_candidate_revision(repository: &Path, source: &ModuleSource) -> Result<String, String> {
    let ModuleSource::Git { url, reference } = source else {
        return Err("candidate revision requested for a non-Git module".into());
    };
    resolve_remote_revision(url, reference.as_deref()).or_else(|error| {
        // A repository created locally may not have a remote, while its lock
        // still records the checkout path. In that case rev-parse is read-only
        // and remains a safe fallback; never fetch into the live checkout.
        if url == &repository.display().to_string() && !has_origin(repository) {
            resolve_revision(repository, reference.as_deref())
        } else {
            Err(error)
        }
    })
}

/// Build the update plan without staging or committing anything.
#[cfg(test)]
pub fn plan_update(
    modules_dir: &Path,
    lock: &MeshLock,
    only: Option<&str>,
    policy: EditPolicy,
    installed: &BTreeMap<String, ModuleManifest>,
    approvals: &BTreeMap<String, Vec<String>>,
) -> Result<UpdatePlan, String> {
    let mut plan = UpdatePlan::default();

    for (module_id, locked) in &lock.modules {
        if only.is_some_and(|requested| requested != module_id) {
            continue;
        }
        let installed_at =
            module_install_path(modules_dir, module_id).map_err(|error| error.to_string())?;
        if !installed_at.exists() {
            continue;
        }
        let edited = has_local_edits(&installed_at, locked).map_err(|error| error.to_string())?;
        if edited && policy == EditPolicy::Refuse {
            plan.edited.push(module_id.clone());
            continue;
        }
        if edited && policy == EditPolicy::Keep {
            continue;
        }

        let ModuleSource::Git { reference, .. } = &locked.source else {
            // Path installs have no upstream to poll; they update by reinstall.
            continue;
        };
        let revision = resolve_revision(&installed_at, reference.as_deref())?;
        let candidate_manifest = manifest_at_revision(&installed_at, &revision)?;
        plan.candidates.push(CandidateModule {
            module_id: module_id.clone(),
            locked: locked.clone(),
            candidate_version: candidate_manifest.version.clone(),
            candidate_revision: Some(revision),
            candidate_manifest,
        });
    }

    classify_contract_changes(&mut plan, installed);
    classify_capability_changes(&mut plan, approvals)?;
    Ok(plan)
}

/// Materialize and validate the complete update graph before any live module
/// tree is checked out or replaced.
pub fn plan_update_from_staged_graph(
    root_path: &Path,
    modules_dir: &Path,
    lock: &MeshLock,
    only: Option<&str>,
    policy: EditPolicy,
    installed: &BTreeMap<String, ModuleManifest>,
    approvals: &BTreeMap<String, Vec<String>>,
    transaction: &mut PackageTransaction,
) -> Result<UpdatePlan, String> {
    let installed_graph = load_installed_module_graph(root_path)
        .map_err(|error| format!("installed graph validation failed: {error}"))?;
    let mut plan = collect_update_candidates(modules_dir, lock, only, policy)?;
    let candidate_root =
        stage_candidate_graph(root_path, modules_dir, installed, &mut plan, transaction)?;
    let candidate_graph = load_installed_module_graph(&candidate_root)
        .map_err(|error| format!("candidate graph validation failed: {error}"))?;
    plan.graph_diff = Some(installed_graph.diff(&candidate_graph));

    for candidate in &mut plan.candidates {
        let module = candidate_graph
            .module(&candidate.module_id)
            .ok_or_else(|| {
                format!(
                    "candidate module {} is not present in the staged graph",
                    candidate.module_id
                )
            })?;
        candidate.candidate_version = module.manifest.version.clone();
        candidate.candidate_manifest = module.manifest.clone();
    }

    classify_contract_changes(&mut plan, installed);
    classify_capability_changes(&mut plan, approvals)?;
    resolve_candidate_capabilities(&mut plan, &candidate_graph, approvals)?;
    // Trust-policy and other graph diagnostics can change even when every
    // locked revision is unchanged, so candidate graph review is never gated
    // on a source update.
    classify_candidate_graph(&mut plan, &candidate_graph);
    Ok(plan)
}

fn collect_update_candidates(
    modules_dir: &Path,
    lock: &MeshLock,
    only: Option<&str>,
    policy: EditPolicy,
) -> Result<UpdatePlan, String> {
    let mut plan = UpdatePlan::default();

    for (module_id, locked) in &lock.modules {
        if only.is_some_and(|requested| requested != module_id) {
            continue;
        }
        let installed_at =
            module_install_path(modules_dir, module_id).map_err(|error| error.to_string())?;
        if !installed_at.exists() {
            continue;
        }
        let edited = has_local_edits(&installed_at, locked).map_err(|error| error.to_string())?;
        if edited && policy == EditPolicy::Refuse {
            plan.edited.push(module_id.clone());
            continue;
        }
        if edited && policy == EditPolicy::Keep {
            continue;
        }

        let ModuleSource::Git { .. } = &locked.source else {
            continue;
        };
        let revision = resolve_candidate_revision(&installed_at, &locked.source)?;
        plan.candidates.push(CandidateModule {
            module_id: module_id.clone(),
            locked: locked.clone(),
            candidate_version: String::new(),
            candidate_revision: Some(revision),
            candidate_manifest: ModuleManifest::from_path(&installed_at.join("module.json"))
                .map_err(|error| error.to_string())?,
        });
    }
    Ok(plan)
}

fn stage_candidate_graph(
    root_path: &Path,
    modules_dir: &Path,
    installed: &BTreeMap<String, ModuleManifest>,
    plan: &mut UpdatePlan,
    transaction: &PackageTransaction,
) -> Result<PathBuf, String> {
    let current_root = RootModuleGraphManifest::from_path(root_path)
        .map_err(|error| format!("failed to read root graph for update planning: {error}"))?;
    let candidate_root_dir = transaction.staging_dir().join("candidate-graph");
    let candidate_modules_dir = candidate_root_dir.join("modules");
    fs::create_dir_all(&candidate_modules_dir)
        .map_err(|error| format!("failed to create candidate graph directory: {error}"))?;

    let candidate_by_id = plan
        .candidates
        .iter()
        .map(|candidate| {
            (
                candidate.module_id.clone(),
                (
                    candidate.locked.source.clone(),
                    candidate.candidate_revision.clone(),
                    candidate.is_unchanged(),
                ),
            )
        })
        .collect::<HashMap<_, _>>();
    let module_ids = if current_root.modules.is_empty() {
        installed.keys().cloned().collect::<Vec<_>>()
    } else {
        current_root.modules.keys().cloned().collect::<Vec<_>>()
    };

    for module_id in module_ids {
        let live =
            module_install_path(modules_dir, &module_id).map_err(|error| error.to_string())?;
        if !live.exists() {
            return Err(format!(
                "candidate graph references missing installed module {module_id}"
            ));
        }
        let relative = current_root
            .modules
            .get(&module_id)
            .map(|entry| entry.path.clone())
            .unwrap_or_else(|| module_id.trim_start_matches('@').to_string());
        let destination = mesh_core_module::package::contained_path(
            &candidate_modules_dir,
            &relative,
            "candidate module path",
        )
        .map_err(|error| error.to_string())?;
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("failed to create candidate module parent: {error}"))?;
        }

        if let Some((source, revision, unchanged)) = candidate_by_id.get(&module_id)
            && !unchanged
        {
            let Some(revision) = revision.as_deref() else {
                return Err(format!("candidate {module_id} has no resolved revision"));
            };
            let ModuleSource::Git { url, .. } = source else {
                return Err(format!("candidate {module_id} is not a Git source"));
            };
            stage_git_revision(url, revision, &destination)?;
            plan.staged_paths
                .insert(module_id.clone(), destination.clone());
        } else {
            let metadata = fs::symlink_metadata(&live).map_err(|error| {
                format!("failed to inspect installed module {module_id}: {error}")
            })?;
            copy_module_tree(&live, &destination, &metadata)?;
            validate_module_tree(&destination).map_err(|error| error.to_string())?;
        }
    }

    let mut candidate_root = current_root;
    candidate_root.modules_dir = "modules".into();
    fs::create_dir_all(&candidate_root_dir)
        .map_err(|error| format!("failed to create candidate root directory: {error}"))?;
    let candidate_root_path = candidate_root_dir.join("module.json");
    candidate_root
        .save(&candidate_root_path)
        .map_err(|error| format!("failed to write candidate root graph: {error}"))?;

    let config_dir = root_path.parent().ok_or_else(|| {
        format!(
            "root module graph has no parent directory: {}",
            root_path.display()
        )
    })?;
    for name in ["profiles", "active-profile"] {
        let source = config_dir.join(name);
        if !source.exists() {
            continue;
        }
        let destination = candidate_root_dir.join(name);
        let metadata = fs::symlink_metadata(&source)
            .map_err(|error| format!("failed to inspect {name}: {error}"))?;
        if metadata.is_dir() {
            copy_module_tree(&source, &destination, &metadata)?;
        } else if metadata.is_file() {
            copy_module_tree(&source, &destination, &metadata)?;
        } else {
            return Err(format!(
                "candidate graph support path {name} is not a file or directory"
            ));
        }
    }
    Ok(candidate_root_path)
}

fn resolve_candidate_capabilities(
    plan: &mut UpdatePlan,
    graph: &InstalledModuleGraph,
    approvals: &BTreeMap<String, Vec<String>>,
) -> Result<(), String> {
    let policy = CapabilityPolicy::from_approvals(
        approvals
            .iter()
            .map(|(module_id, capabilities)| (module_id.clone(), capabilities.clone())),
    );
    for module in graph.enabled_modules() {
        match policy.resolve(
            &module.id,
            &module.manifest.mesh.capabilities.required,
            &module.manifest.mesh.capabilities.optional,
        ) {
            Ok(effective) => {
                plan.capability_decisions
                    .insert(module.id.clone(), effective);
            }
            Err(mesh_core_capability::CapabilityPolicyError::MissingRequiredApproval {
                ..
            }) => {
                // The user-facing refusal is kept in capability_additions so
                // the CLI can print each newly required grant.
            }
            Err(error) => return Err(format!("candidate capability resolution failed: {error}")),
        }
    }
    Ok(())
}

fn classify_candidate_graph(plan: &mut UpdatePlan, graph: &InstalledModuleGraph) {
    let resolution = graph.resolution();
    for (dependency, requirers) in &resolution.missing {
        for requester in requirers {
            plan.graph_breaking.push(format!(
                "candidate graph: {requester} requires missing module {dependency}"
            ));
        }
    }
    for conflict in &resolution.conflicts {
        plan.graph_breaking
            .push(format!("candidate graph: {}", conflict.message()));
    }
    for (module_id, reasons) in &resolution.blocked {
        plan.graph_breaking.push(format!(
            "candidate graph: {module_id} is blocked because {}",
            reasons.iter().cloned().collect::<Vec<_>>().join("; ")
        ));
    }

    let required_interface_statuses = BTreeSet::from([
        "required_interface_unavailable",
        "required_interface_version_mismatch",
        "missing_interface_contract",
        "missing_interface_required_capability",
        "trust_policy_blocked",
    ]);
    for diagnostic in graph.diagnostics() {
        if required_interface_statuses.contains(diagnostic.status.as_str()) {
            plan.graph_breaking.push(format!(
                "candidate graph: {} ({})",
                diagnostic.message, diagnostic.status
            ));
        }
    }
    plan.graph_breaking.sort();
    plan.graph_breaking.dedup();
}

/// Compare each candidate's interface contracts against what is installed.
///
/// Contracts are data, so this decides breakage without executing anything.
fn classify_contract_changes(plan: &mut UpdatePlan, installed: &BTreeMap<String, ModuleManifest>) {
    for candidate in plan.candidates.iter().filter(|c| !c.is_unchanged()) {
        let Some(current) = installed.get(&candidate.module_id) else {
            continue;
        };
        for (name, locked_contract) in declared_contracts(current) {
            let Some(candidate_contract) =
                declared_contracts(&candidate.candidate_manifest).remove(&name)
            else {
                plan.breaking.push(format!(
                    "{} no longer declares interface {name}; its consumers break",
                    candidate.module_id
                ));
                continue;
            };
            let diff = diff_contracts(&locked_contract, &candidate_contract);
            if diff.class() == CompatibilityClass::Breaking {
                for change in diff.breaking_changes() {
                    plan.breaking.push(format!(
                        "{} {name}: {} ({})",
                        candidate.module_id, change.detail, change.path
                    ));
                }
            }
        }
    }
}

fn declared_contracts(
    manifest: &ModuleManifest,
) -> BTreeMap<String, mesh_core_service::InterfaceContract> {
    let mut contracts = BTreeMap::new();
    let declarations = manifest
        .mesh
        .interface
        .iter()
        .chain(manifest.mesh.interfaces.iter());
    for declaration in declarations {
        let Some(value) = &declaration.contract else {
            continue;
        };
        let version = declaration.version.as_deref().unwrap_or("1.0");
        if let Ok(contract) = parse_interface_contract(&declaration.name, version, value) {
            contracts.insert(declaration.name.clone(), contract);
        }
    }
    contracts
}

/// A candidate that asks for a required capability not present in the persisted
/// approval set must not land silently. Optional capabilities remain denied
/// unless explicitly approved, but do not block an update by themselves.
fn classify_capability_changes(
    plan: &mut UpdatePlan,
    approvals: &BTreeMap<String, Vec<String>>,
) -> Result<(), String> {
    let catalog = CapabilityCatalog::builtin();
    for candidate in plan.candidates.iter().filter(|c| !c.is_unchanged()) {
        let approved = approvals
            .get(&candidate.module_id)
            .map(|ids| ids.iter().collect::<std::collections::BTreeSet<_>>())
            .unwrap_or_default();
        let required = candidate
            .candidate_manifest
            .mesh
            .capabilities
            .required
            .iter()
            .collect::<std::collections::BTreeSet<_>>();
        for capability_id in candidate
            .candidate_manifest
            .mesh
            .capabilities
            .required
            .iter()
            .chain(
                candidate
                    .candidate_manifest
                    .mesh
                    .capabilities
                    .optional
                    .iter(),
            )
        {
            let level = catalog
                .validate(capability_id)
                .map_err(|error| error.to_string())?;
            if required.contains(capability_id) && !approved.contains(capability_id) {
                let capability = Capability::new(capability_id.clone());
                plan.capability_additions
                    .push((candidate.module_id.clone(), capability, level));
            }
        }
    }
    Ok(())
}

/// Check the installed tree out at the candidate revision and refresh the lock.
///
/// Source lands first, then the lock, then the caller asks the running shell to
/// switch. A lock write failure fails the transaction: the lock is the rollback
/// record.
pub fn commit_update(
    modules_dir: &Path,
    config_dir: &Path,
    lock: &mut MeshLock,
    plan: &UpdatePlan,
    transaction: &mut PackageTransaction,
) -> Result<Vec<String>, String> {
    let mut updated = Vec::new();
    for candidate in plan.changed() {
        let installed_at = module_install_path(modules_dir, &candidate.module_id)
            .map_err(|error| error.to_string())?;
        let Some(revision) = &candidate.candidate_revision else {
            continue;
        };
        let staged = plan.staged_paths.get(&candidate.module_id).ok_or_else(|| {
            format!(
                "update plan has no staged tree for candidate {}",
                candidate.module_id
            )
        })?;
        transaction
            .replace_with(&installed_at, staged)
            .map_err(|error| error.to_string())?;
        let digest = module_tree_digest(&installed_at).map_err(|error| error.to_string())?;
        if let Some(entry) = lock.modules.get_mut(&candidate.module_id) {
            entry.version = candidate.candidate_version.clone();
            entry.revision = Some(revision.clone());
            entry.digest = digest;
            entry.dependencies = candidate
                .candidate_manifest
                .mesh
                .dependencies
                .modules
                .iter()
                .map(|(module_id, spec)| {
                    (
                        module_id.clone(),
                        mesh_core_module::package::dependency_spec_to_string(spec),
                    )
                })
                .collect();
        }
        if lock
            .composition
            .as_ref()
            .is_some_and(|composition| composition.module == candidate.module_id)
        {
            if let Some(composition) = lock.composition.as_mut() {
                composition.version = candidate.candidate_version.clone();
            }
        }
        updated.push(format!(
            "{} {} → {}",
            candidate.module_id, candidate.locked.version, candidate.candidate_version
        ));
    }

    let manifests = lock
        .modules
        .keys()
        .filter_map(|module_id| module_install_path(modules_dir, module_id).ok())
        .filter(|path| path.exists())
        .map(|path| ModuleManifest::from_path(&path).map_err(|error| error.to_string()))
        .collect::<Result<Vec<_>, _>>()?;
    lock.refresh_metadata(manifests.iter());

    let lock_path = config_dir.join("mesh.lock");
    let history = config_dir.join("lock-history");
    MeshLock::archive(&lock_path, &history).map_err(|error| error.to_string())?;
    lock.save_with_store(
        &lock_path,
        modules_dir,
        &mesh_core_module::package::module_store_dir(config_dir),
    )
    .map_err(|error| error.to_string())?;
    Ok(updated)
}

/// Restore a previous lock generation and re-materialize its revisions.
///
/// Trees are re-fetched by revision rather than archived: git is
/// content-addressed, so the revision is an exact and cheap way back.
pub fn rollback(
    modules_dir: &Path,
    config_dir: &Path,
    generation: Option<u64>,
    transaction: &mut PackageTransaction,
) -> Result<Vec<String>, String> {
    let lock_path = config_dir.join("mesh.lock");
    let history = config_dir.join("lock-history");
    let current = MeshLock::load_or_default(&lock_path).map_err(|error| error.to_string())?;
    let generations = MeshLock::history(&history);
    if generations.is_empty() {
        return Err("no previous lock generation to roll back to".into());
    }
    let (target_generation, path) = match generation {
        Some(requested) => generations
            .into_iter()
            .find(|(generation, _)| *generation == requested)
            .ok_or_else(|| format!("lock generation {requested} is not archived"))?,
        None => generations.into_iter().next().expect("checked non-empty"),
    };

    let target = MeshLock::from_path(&path).map_err(|error| error.to_string())?;
    let mut restored = Vec::new();
    fs::create_dir_all(modules_dir)
        .map_err(|error| format!("failed to create modules directory: {error}"))?;
    for (index, (module_id, entry)) in target.modules.iter().enumerate() {
        let installed_at =
            module_install_path(modules_dir, module_id).map_err(|error| error.to_string())?;
        let staged = stage_lock_entry(
            entry,
            &installed_at,
            config_dir,
            &transaction.staging_dir().join(format!("rollback-{index}")),
        )?;
        transaction
            .replace_with(&installed_at, &staged)
            .map_err(|error| error.to_string())?;
        restored.push(format!("{module_id} → {}", entry.version));
    }
    for module_id in current.modules.keys() {
        if target.modules.contains_key(module_id) {
            continue;
        }
        let installed_at =
            module_install_path(modules_dir, module_id).map_err(|error| error.to_string())?;
        if installed_at.exists() {
            transaction
                .remove(&installed_at)
                .map_err(|error| error.to_string())?;
            restored.push(format!("removed {module_id}"));
        }
    }

    MeshLock::archive(&lock_path, &history).map_err(|error| error.to_string())?;
    target
        .save_exact_with_store(
            &lock_path,
            modules_dir,
            &mesh_core_module::package::module_store_dir(config_dir),
        )
        .map_err(|error| error.to_string())?;
    restored.push(format!("lock restored to generation {target_generation}"));
    Ok(restored)
}

fn stage_git_revision(source: &str, revision: &str, destination: &Path) -> Result<PathBuf, String> {
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create update staging directory: {error}"))?;
    }
    let clone = std::process::Command::new("git")
        .args(["clone", "--quiet", "--no-hardlinks"])
        .arg(source)
        .arg(destination)
        .output()
        .map_err(|error| format!("failed to stage Git revision: {error}"))?;
    if !clone.status.success() {
        return Err(format!(
            "staging {source} failed: {}",
            String::from_utf8_lossy(&clone.stderr).trim()
        ));
    }
    let checkout = std::process::Command::new("git")
        .args(["-C"])
        .arg(destination)
        .args(["checkout", "--quiet", revision])
        .output()
        .map_err(|error| format!("failed to stage Git checkout: {error}"))?;
    if !checkout.status.success() {
        return Err(format!(
            "checking out {revision} failed: {}",
            String::from_utf8_lossy(&checkout.stderr).trim()
        ));
    }
    validate_module_tree(destination).map_err(|error| error.to_string())?;
    Ok(destination.to_path_buf())
}

fn stage_lock_entry(
    entry: &LockedModule,
    installed_at: &Path,
    config_dir: &Path,
    destination: &Path,
) -> Result<PathBuf, String> {
    if let Some(revision) = &entry.revision {
        if installed_at.exists() {
            return stage_git_revision(&installed_at.display().to_string(), revision, destination);
        }
        if let ModuleSource::Git { url, .. } = &entry.source {
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent).map_err(|error| {
                    format!("failed to create rollback staging directory: {error}")
                })?;
            }
            let clone = std::process::Command::new("git")
                .args(["clone", "--quiet", url])
                .arg(destination)
                .output()
                .map_err(|error| format!("failed to fetch rollback source: {error}"))?;
            if !clone.status.success() {
                return Err(format!(
                    "fetching rollback source failed: {}",
                    String::from_utf8_lossy(&clone.stderr).trim()
                ));
            }
            let checkout = std::process::Command::new("git")
                .args(["-C"])
                .arg(destination)
                .args(["checkout", "--quiet", revision])
                .output()
                .map_err(|error| format!("failed to restore rollback revision: {error}"))?;
            if !checkout.status.success() {
                return Err(format!(
                    "restoring {revision} failed: {}",
                    String::from_utf8_lossy(&checkout.stderr).trim()
                ));
            }
            validate_module_tree(destination).map_err(|error| error.to_string())?;
            return Ok(destination.to_path_buf());
        }
    }
    let ModuleSource::Path { path } = &entry.source else {
        return Err(format!(
            "cannot materialize rollback entry without a revision: {}",
            installed_at.display()
        ));
    };
    let source = Path::new(path);
    let source = if source.is_absolute() {
        source.to_path_buf()
    } else {
        config_dir.join(source)
    };
    let metadata = fs::symlink_metadata(&source).map_err(|error| {
        format!(
            "failed to read rollback source {}: {error}",
            source.display()
        )
    })?;
    copy_module_tree(&source, destination, &metadata)?;
    validate_module_tree(destination).map_err(|error| error.to_string())?;
    Ok(destination.to_path_buf())
}

fn copy_module_tree(
    source: &Path,
    destination: &Path,
    metadata: &fs::Metadata,
) -> Result<(), String> {
    if metadata.file_type().is_symlink() {
        return Err(format!(
            "rollback source {} contains a symlink",
            source.display()
        ));
    }
    if metadata.is_dir() {
        fs::create_dir_all(destination).map_err(|error| error.to_string())?;
        for entry in fs::read_dir(source).map_err(|error| error.to_string())? {
            let entry = entry.map_err(|error| error.to_string())?;
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path).map_err(|error| error.to_string())?;
            copy_module_tree(&path, &destination.join(entry.file_name()), &metadata)?;
        }
    } else if metadata.is_file() {
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        fs::copy(source, destination).map_err(|error| error.to_string())?;
    } else {
        return Err(format!(
            "rollback source {} is not a file or directory",
            source.display()
        ));
    }
    fs::set_permissions(destination, metadata.permissions()).map_err(|error| error.to_string())?;
    Ok(())
}

/// Recompute every digest and report which modules the user has edited.
pub fn verify(modules_dir: &Path, lock: &MeshLock) -> Vec<(String, bool)> {
    lock.modules
        .iter()
        .filter_map(|(module_id, entry)| {
            let Ok(installed_at) = module_install_path(modules_dir, module_id) else {
                return Some((module_id.clone(), true));
            };
            if !installed_at.exists() {
                return Some((module_id.clone(), true));
            }
            has_local_edits(&installed_at, entry)
                .ok()
                .map(|edited| (module_id.clone(), edited))
        })
        .collect()
}

/// Module ids that still require `module_id`, so uninstall can refuse.
pub fn dependents(module_id: &str, lock: &MeshLock) -> Vec<String> {
    lock.modules
        .get(module_id)
        .map(|entry| entry.requested_by.iter().cloned().collect())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use mesh_core_module::package::{LockedModule, MeshLock, ModuleSource};
    use std::collections::BTreeSet;

    fn manifest(
        name: &str,
        version: &str,
        capabilities: &[&str],
        contract: &str,
    ) -> ModuleManifest {
        let capabilities = capabilities
            .iter()
            .map(|capability| format!(r#""{capability}""#))
            .collect::<Vec<_>>()
            .join(",");
        ModuleManifest::from_json_str(&format!(
            r#"{{"name":"{name}","version":"{version}","mesh":{{"apiVersion":"0.1",
               "kind":"backend","entry":"main.luau",
               "uses":{{"capabilities":[{capabilities}]}},
               "implements":[{{"interface":"mesh.audio","version":"1.0"}}],
               "interfaces":[{{"name":"mesh.audio","version":"1.0","contract":{contract}}}]}}}}"#
        ))
        .unwrap()
    }

    const BASE_CONTRACT: &str = r#"{"state":[{"name":"percent","type":"float"}],
        "methods":[{"name":"set_volume","args":[{"name":"percent","type":"float"}],
        "returns":"Result"}],"events":[],"capabilities":{"required":["service.audio.read"]}}"#;

    fn candidate(module_id: &str, version: &str, manifest: ModuleManifest) -> CandidateModule {
        CandidateModule {
            module_id: module_id.into(),
            locked: LockedModule {
                version: "1.0.0".into(),
                source: ModuleSource::Git {
                    url: "https://example.invalid/x".into(),
                    reference: None,
                },
                revision: Some("old".into()),
                digest: "sha256:0".into(),
                trust: Default::default(),
                signature: None,
                dependencies: BTreeMap::new(),
                requested_by: BTreeSet::new(),
            },
            candidate_version: version.into(),
            candidate_revision: Some("new".into()),
            candidate_manifest: manifest,
        }
    }

    #[test]
    fn a_compatible_contract_change_does_not_refuse_the_update() {
        let installed = BTreeMap::from([(
            "@me/audio".to_string(),
            manifest("@me/audio", "1.0.0", &[], BASE_CONTRACT),
        )]);
        let widened = r#"{"state":[{"name":"percent","type":"float"},
            {"name":"muted","type":"boolean"}],
            "methods":[{"name":"set_volume","args":[{"name":"percent","type":"float"}],
            "returns":"Result"}],"events":[],
            "capabilities":{"required":["service.audio.read"]}}"#;
        let mut plan = UpdatePlan {
            candidates: vec![candidate(
                "@me/audio",
                "1.1.0",
                manifest("@me/audio", "1.1.0", &[], widened),
            )],
            ..UpdatePlan::default()
        };
        classify_contract_changes(&mut plan, &installed);
        let approvals = BTreeMap::from([(
            "@me/audio".to_string(),
            vec!["service.audio.read".to_string()],
        )]);
        classify_capability_changes(&mut plan, &approvals).unwrap();
        assert!(!plan.is_refused(), "{:?}", plan.breaking);
    }

    #[test]
    fn a_removed_method_refuses_the_update_and_names_the_change() {
        let installed = BTreeMap::from([(
            "@me/audio".to_string(),
            manifest("@me/audio", "1.0.0", &[], BASE_CONTRACT),
        )]);
        let narrowed = r#"{"state":[{"name":"percent","type":"float"}],"methods":[],
            "events":[],"capabilities":{"required":["service.audio.read"]}}"#;
        let mut plan = UpdatePlan {
            candidates: vec![candidate(
                "@me/audio",
                "2.0.0",
                manifest("@me/audio", "2.0.0", &[], narrowed),
            )],
            ..UpdatePlan::default()
        };
        classify_contract_changes(&mut plan, &installed);
        assert!(plan.is_refused());
        assert!(
            plan.breaking
                .iter()
                .any(|entry| entry.contains("set_volume")),
            "{:?}",
            plan.breaking
        );
    }

    #[test]
    fn a_new_high_capability_requires_re_approval() {
        let mut plan = UpdatePlan {
            candidates: vec![candidate(
                "@me/audio",
                "1.1.0",
                manifest(
                    "@me/audio",
                    "1.1.0",
                    &["exec.wpctl", "exec.command"],
                    BASE_CONTRACT,
                ),
            )],
            ..UpdatePlan::default()
        };
        let approvals = BTreeMap::from([("@me/audio".to_string(), vec!["exec.wpctl".to_string()])]);
        classify_capability_changes(&mut plan, &approvals).unwrap();
        assert!(plan.is_refused());
        assert_eq!(plan.capability_additions.len(), 1);
        assert_eq!(
            plan.capability_additions[0].2,
            mesh_core_capability::PrivilegeLevel::High
        );
    }

    #[test]
    fn an_unchanged_candidate_is_not_part_of_the_update() {
        let mut unchanged = candidate(
            "@me/audio",
            "1.0.0",
            manifest("@me/audio", "1.0.0", &[], BASE_CONTRACT),
        );
        unchanged.candidate_revision = Some("old".into());
        let plan = UpdatePlan {
            candidates: vec![unchanged],
            ..UpdatePlan::default()
        };
        assert_eq!(plan.changed().count(), 0);
    }

    fn git(repository: &Path, args: &[&str]) {
        let output = std::process::Command::new("git")
            .arg("-C")
            .arg(repository)
            .args(args)
            .output()
            .expect("git runs");
        assert!(
            output.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    /// A real repository with two revisions, so the update path is exercised
    /// end to end rather than only its classification helpers.
    fn fixture_repository(name: &str) -> PathBuf {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("mesh-update-{name}-{nonce}"));
        std::fs::create_dir_all(&root).unwrap();
        git(&root, &["init", "--quiet", "--initial-branch=main"]);
        git(&root, &["config", "user.email", "test@example.invalid"]);
        git(&root, &["config", "user.name", "Test"]);

        let write_manifest = |version: &str, contract: &str| {
            std::fs::write(
                root.join("module.json"),
                format!(
                    r#"{{"name":"@me/audio","version":"{version}","mesh":{{"apiVersion":"0.1",
                       "kind":"backend","entry":"main.luau",
                       "implements":[{{"interface":"mesh.audio","version":"1.0"}}],
                       "interfaces":[{{"name":"mesh.audio","version":"1.0",
                         "contract":{contract}}}]}}}}"#
                ),
            )
            .unwrap();
        };
        write_manifest("1.0.0", BASE_CONTRACT);
        std::fs::write(root.join("main.luau"), "return {}").unwrap();
        git(&root, &["add", "."]);
        git(&root, &["commit", "--quiet", "-m", "v1"]);

        let widened = r#"{"state":[{"name":"percent","type":"float"},
            {"name":"muted","type":"boolean"}],
            "methods":[{"name":"set_volume","args":[{"name":"percent","type":"float"}],
            "returns":"Result"}],"events":[],
            "capabilities":{"required":["service.audio.read"]}}"#;
        write_manifest("1.1.0", widened);
        git(&root, &["add", "."]);
        git(&root, &["commit", "--quiet", "-m", "v2"]);
        root
    }

    fn external_contract_repository(name: &str) -> PathBuf {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("mesh-update-contract-{name}-{nonce}"));
        std::fs::create_dir_all(root.join("contracts")).unwrap();
        git(&root, &["init", "--quiet", "--initial-branch=main"]);
        git(&root, &["config", "user.email", "test@example.invalid"]);
        git(&root, &["config", "user.name", "Test"]);

        let write_revision = |version: &str, contract: &str| {
            std::fs::write(
                root.join("module.json"),
                format!(
                    r#"{{"name":"@me/audio","version":"{version}","mesh":{{"apiVersion":"0.1",
                       "kind":"backend","entry":"main.luau",
                       "uses":{{"capabilities":["service.audio.read"]}},
                       "implements":[{{"interface":"mesh.audio","version":"1.0"}}],
                       "interfaces":[{{"name":"mesh.audio","version":"1.0",
                         "contract":"contracts/audio.json"}}]}}}}"#
                ),
            )
            .unwrap();
            std::fs::write(root.join("contracts/audio.json"), contract).unwrap();
            std::fs::write(root.join("main.luau"), "return {}").unwrap();
        };
        write_revision("1.0.0", BASE_CONTRACT);
        git(&root, &["add", "."]);
        git(&root, &["commit", "--quiet", "-m", "v1"]);

        let narrowed = r#"{"state":[{"name":"percent","type":"float"}],"methods":[],"events":[],"capabilities":{"required":["service.audio.read"]}}"#;
        write_revision("2.0.0", narrowed);
        git(&root, &["add", "."]);
        git(&root, &["commit", "--quiet", "-m", "v2"]);
        root
    }

    fn staged_graph_fixture(
        name: &str,
    ) -> (
        PathBuf,
        PathBuf,
        PathBuf,
        MeshLock,
        BTreeMap<String, ModuleManifest>,
    ) {
        let source = external_contract_repository(name);
        let workspace = source.parent().unwrap().to_path_buf();
        let config = workspace.join("config");
        let installed = config.join("modules/me/audio");
        std::fs::create_dir_all(installed.parent().unwrap()).unwrap();
        std::fs::rename(&source, &installed).unwrap();

        let old_revision = std::process::Command::new("git")
            .arg("-C")
            .arg(&installed)
            .args(["rev-parse", "HEAD~1"])
            .output()
            .unwrap();
        let old_revision = String::from_utf8_lossy(&old_revision.stdout)
            .trim()
            .to_string();
        git(&installed, &["checkout", "--quiet", &old_revision]);

        let root_path = config.join("module.json");
        std::fs::create_dir_all(&config).unwrap();
        std::fs::write(
            &root_path,
            r#"{"mesh":{"schemaVersion":1,"modulesDir":"modules","modules":{"@me/audio":{"kind":"backend","path":"me/audio"}}}}"#,
        )
        .unwrap();
        let digest = module_tree_digest(&installed).unwrap();
        let mut lock = MeshLock::new();
        lock.modules.insert(
            "@me/audio".into(),
            LockedModule {
                version: "1.0.0".into(),
                source: ModuleSource::Git {
                    url: installed.display().to_string(),
                    reference: Some("main".into()),
                },
                revision: Some(old_revision),
                digest,
                trust: Default::default(),
                signature: None,
                dependencies: BTreeMap::new(),
                requested_by: BTreeSet::new(),
            },
        );
        let installed_graph = load_installed_module_graph(&root_path).unwrap();
        let installed_manifests = installed_graph
            .modules()
            .into_iter()
            .map(|module| (module.id.clone(), module.manifest.clone()))
            .collect();
        (workspace, root_path, installed, lock, installed_manifests)
    }

    #[test]
    fn staged_graph_reads_external_contracts_before_live_replacement() {
        let (workspace, root_path, installed, lock, installed_manifests) =
            staged_graph_fixture("external");
        let config = root_path.parent().unwrap();
        let modules_dir = config.join("modules");
        let approvals = BTreeMap::from([(
            "@me/audio".to_string(),
            vec!["service.audio.read".to_string()],
        )]);
        let mut transaction = PackageTransaction::begin(config, "test-update").unwrap();
        transaction
            .protect_package_state(&root_path, &modules_dir)
            .unwrap();

        let plan = plan_update_from_staged_graph(
            &root_path,
            &modules_dir,
            &lock,
            None,
            EditPolicy::Replace,
            &installed_manifests,
            &approvals,
            &mut transaction,
        )
        .unwrap();

        assert!(
            plan.breaking
                .iter()
                .any(|change| change.contains("set_volume")),
            "external contract change was not classified: {:?}",
            plan.breaking
        );
        assert!(plan.staged_paths.contains_key("@me/audio"));
        assert_eq!(
            ModuleManifest::from_path(&installed.join("module.json"))
                .unwrap()
                .version,
            "1.0.0"
        );
        assert_eq!(
            plan.capability_decisions["@me/audio"]
                .granted_ids()
                .collect::<Vec<_>>(),
            vec!["service.audio.read"]
        );
        transaction.abort().unwrap();
        std::fs::remove_dir_all(workspace).unwrap();
    }

    #[test]
    fn a_candidate_version_is_read_from_a_revision_without_checking_it_out() {
        let repository = fixture_repository("read");
        let head = resolve_revision(&repository, Some("main")).unwrap();
        let previous = {
            let output = std::process::Command::new("git")
                .arg("-C")
                .arg(&repository)
                .args(["rev-parse", "HEAD~1"])
                .output()
                .unwrap();
            String::from_utf8_lossy(&output.stdout).trim().to_string()
        };

        assert_eq!(
            manifest_at_revision(&repository, &head).unwrap().version,
            "1.1.0"
        );
        // Reading the older revision must not move the working tree.
        assert_eq!(
            manifest_at_revision(&repository, &previous)
                .unwrap()
                .version,
            "1.0.0"
        );
        assert_eq!(
            ModuleManifest::from_path(&repository.join("module.json"))
                .unwrap()
                .version,
            "1.1.0"
        );

        std::fs::remove_dir_all(&repository).ok();
    }

    #[test]
    fn a_module_edited_since_install_is_refused_before_anything_is_fetched() {
        let repository = fixture_repository("edits");
        let modules_dir = repository.parent().unwrap().to_path_buf();
        let installed_name = repository
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string();
        let module_id = format!("@test/{installed_name}");
        let installed_at = modules_dir.join("test").join(&installed_name);
        std::fs::create_dir_all(installed_at.parent().unwrap()).unwrap();
        std::fs::rename(&repository, &installed_at).unwrap();
        let repository = installed_at;

        let mut lock = MeshLock::new();
        lock.modules.insert(
            module_id.clone(),
            LockedModule {
                version: "1.0.0".into(),
                source: ModuleSource::Git {
                    url: repository.display().to_string(),
                    reference: Some("main".into()),
                },
                revision: Some("irrelevant".into()),
                // A digest that cannot match: the tree reads as edited.
                digest: "sha256:0000".into(),
                trust: Default::default(),
                signature: None,
                dependencies: BTreeMap::new(),
                requested_by: BTreeSet::new(),
            },
        );

        let plan = plan_update(
            &modules_dir,
            &lock,
            None,
            EditPolicy::Refuse,
            &BTreeMap::new(),
            &BTreeMap::new(),
        )
        .unwrap();
        assert!(plan.is_refused());
        assert_eq!(plan.edited, vec![module_id]);
        // --keep excludes it instead of refusing the whole update.
        let kept = plan_update(
            &modules_dir,
            &lock,
            None,
            EditPolicy::Keep,
            &BTreeMap::new(),
            &BTreeMap::new(),
        )
        .unwrap();
        assert!(!kept.is_refused());
        assert_eq!(kept.changed().count(), 0);

        std::fs::remove_dir_all(&repository).ok();
    }

    #[test]
    fn uninstall_refuses_while_something_still_requires_the_module() {
        let mut lock = MeshLock::new();
        lock.modules.insert(
            "@me/shared".into(),
            LockedModule {
                version: "1.0.0".into(),
                source: ModuleSource::Path {
                    path: "shared".into(),
                },
                revision: None,
                digest: "sha256:0".into(),
                trust: Default::default(),
                signature: None,
                dependencies: BTreeMap::new(),
                requested_by: BTreeSet::from(["@me/desk".to_string()]),
            },
        );
        assert_eq!(dependents("@me/shared", &lock), vec!["@me/desk"]);
        assert!(dependents("@me/absent", &lock).is_empty());
    }
}
