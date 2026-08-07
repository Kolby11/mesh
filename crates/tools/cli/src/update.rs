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

use mesh_core_capability::{Capability, PrivilegeLevel};
use mesh_core_module::package::{
    LockedModule, MeshLock, ModuleManifest, ModuleSource, has_local_edits, module_tree_digest,
};
use mesh_core_service::{CompatibilityClass, diff_contracts, parse_interface_contract};
use std::collections::BTreeMap;
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
    /// New elevated/high capabilities needing explicit re-approval.
    pub capability_additions: Vec<(String, Capability, PrivilegeLevel)>,
    /// Modules with local edits, under `EditPolicy::Refuse`.
    pub edited: Vec<String>,
}

impl UpdatePlan {
    pub fn is_refused(&self) -> bool {
        !self.breaking.is_empty()
            || !self.capability_additions.is_empty()
            || !self.edited.is_empty()
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
    // A module installed from a local path may be a git tree with no remote.
    // Only fetch when there is an origin to fetch from; a fetch that then fails
    // is reported rather than silently resolving stale refs.
    if has_origin(repository) {
        let fetch = std::process::Command::new("git")
            .arg("-C")
            .arg(repository)
            .arg("fetch")
            .arg("--quiet")
            .arg("origin")
            .output()
            .map_err(|error| format!("failed to run git fetch: {error}"))?;
        if !fetch.status.success() {
            return Err(format!(
                "git fetch failed: {}",
                String::from_utf8_lossy(&fetch.stderr).trim()
            ));
        }
    }
    let target = reference
        .map(|reference| format!("origin/{reference}"))
        .unwrap_or_else(|| "origin/HEAD".to_string());
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(repository)
        .arg("rev-parse")
        .arg(&target)
        .output()
        .map_err(|error| format!("failed to run git rev-parse: {error}"))?;
    if !output.status.success() {
        // A tag or a bare ref that is not a remote branch.
        let fallback = std::process::Command::new("git")
            .arg("-C")
            .arg(repository)
            .arg("rev-parse")
            .arg(reference.unwrap_or("HEAD"))
            .output()
            .map_err(|error| format!("failed to run git rev-parse: {error}"))?;
        if !fallback.status.success() {
            return Err(format!(
                "cannot resolve {target}: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
        return Ok(String::from_utf8_lossy(&fallback.stdout).trim().to_string());
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

/// Build the update plan without staging or committing anything.
pub fn plan_update(
    modules_dir: &Path,
    lock: &MeshLock,
    only: Option<&str>,
    policy: EditPolicy,
    installed: &BTreeMap<String, ModuleManifest>,
) -> Result<UpdatePlan, String> {
    let mut plan = UpdatePlan::default();

    for (module_id, locked) in &lock.modules {
        if only.is_some_and(|requested| requested != module_id) {
            continue;
        }
        let installed_at = modules_dir.join(module_id.trim_start_matches('@'));
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
    classify_capability_changes(&mut plan, installed);
    Ok(plan)
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

/// A candidate that asks for more privilege than the user approved must not
/// land silently — the install-time capability review applies to updates too.
fn classify_capability_changes(
    plan: &mut UpdatePlan,
    installed: &BTreeMap<String, ModuleManifest>,
) {
    for candidate in plan.candidates.iter().filter(|c| !c.is_unchanged()) {
        let approved: Vec<String> = installed
            .get(&candidate.module_id)
            .map(|manifest| declared_capabilities(manifest))
            .unwrap_or_default();
        for capability_id in declared_capabilities(&candidate.candidate_manifest) {
            if approved.contains(&capability_id) {
                continue;
            }
            let capability = Capability::new(capability_id);
            let level = capability.privilege_level();
            if matches!(level, PrivilegeLevel::Elevated | PrivilegeLevel::High) {
                plan.capability_additions
                    .push((candidate.module_id.clone(), capability, level));
            }
        }
    }
}

/// Normalization merges `mesh.uses.capabilities` into `mesh.capabilities`, so
/// the two lists overlap; deduplicate or one added capability reports twice.
fn declared_capabilities(manifest: &ModuleManifest) -> Vec<String> {
    let mut capabilities: Vec<String> = manifest
        .mesh
        .capabilities
        .required
        .iter()
        .chain(manifest.mesh.uses.capabilities.iter())
        .cloned()
        .collect();
    capabilities.sort();
    capabilities.dedup();
    capabilities
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
) -> Result<Vec<String>, String> {
    let mut updated = Vec::new();
    for candidate in plan.changed() {
        let installed_at = modules_dir.join(candidate.module_id.trim_start_matches('@'));
        let Some(revision) = &candidate.candidate_revision else {
            continue;
        };
        let checkout = std::process::Command::new("git")
            .arg("-C")
            .arg(&installed_at)
            .arg("checkout")
            .arg("--quiet")
            .arg(revision)
            .output()
            .map_err(|error| format!("failed to run git checkout: {error}"))?;
        if !checkout.status.success() {
            return Err(format!(
                "checking out {revision} for {} failed: {}",
                candidate.module_id,
                String::from_utf8_lossy(&checkout.stderr).trim()
            ));
        }
        let digest = module_tree_digest(&installed_at).map_err(|error| error.to_string())?;
        if let Some(entry) = lock.modules.get_mut(&candidate.module_id) {
            entry.version = candidate.candidate_version.clone();
            entry.revision = Some(revision.clone());
            entry.digest = digest;
        }
        updated.push(format!(
            "{} {} → {}",
            candidate.module_id, candidate.locked.version, candidate.candidate_version
        ));
    }

    let lock_path = config_dir.join("mesh.lock");
    let history = config_dir.join("lock-history");
    MeshLock::archive(&lock_path, &history).map_err(|error| error.to_string())?;
    lock.save(&lock_path).map_err(|error| error.to_string())?;
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
) -> Result<Vec<String>, String> {
    let lock_path = config_dir.join("mesh.lock");
    let history = config_dir.join("lock-history");
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
    for (module_id, entry) in &target.modules {
        let Some(revision) = &entry.revision else {
            continue;
        };
        let installed_at = modules_dir.join(module_id.trim_start_matches('@'));
        if !installed_at.exists() {
            continue;
        }
        let checkout = std::process::Command::new("git")
            .arg("-C")
            .arg(&installed_at)
            .arg("checkout")
            .arg("--quiet")
            .arg(revision)
            .output()
            .map_err(|error| format!("failed to run git checkout: {error}"))?;
        if !checkout.status.success() {
            return Err(format!(
                "restoring {module_id} to {revision} failed: {}",
                String::from_utf8_lossy(&checkout.stderr).trim()
            ));
        }
        restored.push(format!("{module_id} → {} ({revision})", entry.version));
    }

    let content = std::fs::read(&path).map_err(|error| error.to_string())?;
    std::fs::write(&lock_path, content).map_err(|error| error.to_string())?;
    restored.push(format!("lock restored to generation {target_generation}"));
    Ok(restored)
}

/// Recompute every digest and report which modules the user has edited.
pub fn verify(modules_dir: &Path, lock: &MeshLock) -> Vec<(String, bool)> {
    lock.modules
        .iter()
        .filter_map(|(module_id, entry)| {
            let installed_at = modules_dir.join(module_id.trim_start_matches('@'));
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
        classify_capability_changes(&mut plan, &installed);
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
        let installed = BTreeMap::from([(
            "@me/audio".to_string(),
            manifest("@me/audio", "1.0.0", &["exec.wpctl"], BASE_CONTRACT),
        )]);
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
        classify_capability_changes(&mut plan, &installed);
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

        let mut lock = MeshLock::new();
        lock.modules.insert(
            format!("@{installed_name}"),
            LockedModule {
                version: "1.0.0".into(),
                source: ModuleSource::Git {
                    url: repository.display().to_string(),
                    reference: Some("main".into()),
                },
                revision: Some("irrelevant".into()),
                // A digest that cannot match: the tree reads as edited.
                digest: "sha256:0000".into(),
                requested_by: BTreeSet::new(),
            },
        );

        let plan = plan_update(
            &modules_dir,
            &lock,
            None,
            EditPolicy::Refuse,
            &BTreeMap::new(),
        )
        .unwrap();
        assert!(plan.is_refused());
        assert_eq!(plan.edited, vec![format!("@{installed_name}")]);
        // --keep excludes it instead of refusing the whole update.
        let kept = plan_update(
            &modules_dir,
            &lock,
            None,
            EditPolicy::Keep,
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
                requested_by: BTreeSet::from(["@me/desk".to_string()]),
            },
        );
        assert_eq!(dependents("@me/shared", &lock), vec!["@me/desk"]);
        assert!(dependents("@me/absent", &lock).is_empty());
    }
}
