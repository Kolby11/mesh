//! Version resolution over a module closure.
//!
//! **One version per module id.** A module id is also its settings namespace
//! ([`08-settings`](../../../../../docs/spec/08-settings.md)) and its surface
//! instance key, so two copies of `@mesh/audio` would fork the settings store
//! and the surface bookkeeping. Coexisting versions are therefore not a feature
//! MESH withholds — they are incoherent in this model, and the resolver says so
//! rather than silently picking one.

use super::{ModuleManifest, dependency_spec_to_string};
use mesh_core_service::{parse_contract_version, parse_version_req};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};

/// Where a module's source comes from when it is not already installed.
///
/// Without a registry, a version range needs somewhere to fetch from. A
/// registry later populates this same map from an index — the model does not
/// change, only who fills it in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", untagged)]
pub enum SourceSpec {
    Git {
        git: String,
        #[serde(default, rename = "ref", skip_serializing_if = "Option::is_none")]
        reference: Option<String>,
    },
    Path {
        path: String,
    },
}

/// One module's place in a resolved closure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedModule {
    pub module_id: String,
    pub version: String,
    /// Every required requirement that had to be satisfied, for diagnostics.
    pub requirements: BTreeMap<String, String>,
    /// Optional requirements are retained so callers can explain degraded
    /// activation without treating them as graph failures.
    pub optional_requirements: BTreeMap<String, String>,
    pub requested_by: BTreeSet<String>,
    pub source: Option<SourceSpec>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ResolutionOutcome {
    pub modules: BTreeMap<String, ResolvedModule>,
    pub conflicts: Vec<VersionConflict>,
    /// Required module ids with no installed candidate and no declared source.
    pub missing: BTreeMap<String, BTreeSet<String>>,
    /// Optional edges that could not be activated. These never block their
    /// requesting module, but remain explicit for diagnostics and tooling.
    pub optional: Vec<OptionalDependencyIssue>,
    /// Enabled modules whose required dependency closure cannot be activated.
    /// The reasons are stable, human-readable dependency ids/ranges so graph
    /// consumers can surface the same decision without re-resolving it.
    pub blocked: BTreeMap<String, BTreeSet<String>>,
    /// Modules reached through required edges from an enabled root. A required
    /// dependency may be installed but disabled as a direct root; it still
    /// belongs in this activation closure.
    pub required_closure: BTreeSet<String>,
}

impl ResolutionOutcome {
    pub fn is_satisfiable(&self) -> bool {
        self.conflicts.is_empty()
            && self.missing.is_empty()
            && !self
                .blocked
                .keys()
                .any(|module_id| self.required_closure.contains(module_id))
    }

    pub fn active_modules(
        &self,
        roots: impl IntoIterator<Item = impl AsRef<str>>,
    ) -> BTreeSet<String> {
        let mut active = roots
            .into_iter()
            .map(|root| root.as_ref().to_string())
            .filter(|root| !self.blocked.contains_key(root))
            .collect::<BTreeSet<_>>();
        active.extend(
            self.required_closure
                .iter()
                .filter(|module_id| {
                    self.modules.contains_key(*module_id) && !self.blocked.contains_key(*module_id)
                })
                .cloned(),
        );
        active
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OptionalDependencyIssue {
    pub module_id: String,
    pub dependency_id: String,
    pub requirement: String,
    pub status: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionConflict {
    pub module_id: String,
    pub available: String,
    /// Requirer → range, for every requirement in play.
    pub requirements: BTreeMap<String, String>,
}

impl VersionConflict {
    pub fn message(&self) -> String {
        let demands = self
            .requirements
            .iter()
            .map(|(requirer, range)| format!("{requirer} needs {range}"))
            .collect::<Vec<_>>()
            .join("; ");
        format!(
            "no single version of {} satisfies every requirement ({demands}); installed is {}. \
             MESH resolves one version per module id because the id is also the settings namespace \
             and surface instance key",
            self.module_id, self.available
        )
    }
}

/// Resolve the closure reachable from `roots`.
///
/// `available` is the installed set. A module required but not installed is
/// reported in `missing` together with the declared source, if any, so the
/// caller can fetch it.
pub fn resolve_closure<'a>(
    roots: impl IntoIterator<Item = &'a str>,
    available: impl IntoIterator<Item = &'a ModuleManifest>,
) -> ResolutionOutcome {
    let available: HashMap<&str, &ModuleManifest> = available
        .into_iter()
        .map(|manifest| (manifest.name.as_str(), manifest))
        .collect();

    let mut requirements: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
    let mut optional_requirements: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
    let mut requested_by: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut required_requested_by: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut required_edges: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
    let mut optional_edges: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
    let mut sources: BTreeMap<String, SourceSpec> = BTreeMap::new();
    let mut reached: BTreeSet<String> = BTreeSet::new();
    let mut queue: VecDeque<(String, bool)> = VecDeque::new();
    let mut processed_required = BTreeSet::new();
    let mut processed_optional = BTreeSet::new();
    let mut required_closure = BTreeSet::new();

    for root in roots {
        queue.push_back((root.to_string(), true));
        requested_by.entry(root.to_string()).or_default();
        required_closure.insert(root.to_string());
    }

    while let Some((module_id, required_path)) = queue.pop_front() {
        let processed = if required_path {
            required_closure.insert(module_id.clone());
            processed_required.insert(module_id.clone())
        } else {
            processed_optional.insert(module_id.clone())
        };
        if !processed || !reached.insert(module_id.clone()) && !required_path {
            continue;
        }
        let Some(manifest) = available.get(module_id.as_str()) else {
            continue;
        };
        for (dependency_id, spec) in &manifest.mesh.dependencies.modules {
            let requirement = dependency_spec_to_string(spec);
            requested_by
                .entry(dependency_id.clone())
                .or_default()
                .insert(module_id.clone());
            if spec.is_optional() {
                optional_requirements
                    .entry(dependency_id.clone())
                    .or_default()
                    .insert(module_id.clone(), requirement.clone());
                optional_edges
                    .entry(module_id.clone())
                    .or_default()
                    .insert(dependency_id.clone(), requirement);
                if available.contains_key(dependency_id.as_str()) {
                    queue.push_back((dependency_id.clone(), false));
                }
            } else {
                requirements
                    .entry(dependency_id.clone())
                    .or_default()
                    .insert(module_id.clone(), requirement.clone());
                required_requested_by
                    .entry(dependency_id.clone())
                    .or_default()
                    .insert(module_id.clone());
                required_edges
                    .entry(module_id.clone())
                    .or_default()
                    .insert(dependency_id.clone(), requirement);
                if required_path {
                    required_closure.insert(dependency_id.clone());
                }
                queue.push_back((dependency_id.clone(), required_path));
            }
        }
        for (dependency_id, source) in &manifest.mesh.uses.sources {
            sources
                .entry(dependency_id.clone())
                .or_insert_with(|| source.clone());
        }
    }

    let mut outcome = ResolutionOutcome::default();
    for module_id in &reached {
        let module_requirements = requirements.get(module_id).cloned().unwrap_or_default();
        let Some(manifest) = available.get(module_id.as_str()) else {
            continue;
        };

        outcome.modules.insert(
            module_id.clone(),
            ResolvedModule {
                module_id: module_id.clone(),
                version: manifest.version.clone(),
                requirements: module_requirements,
                optional_requirements: optional_requirements
                    .get(module_id)
                    .cloned()
                    .unwrap_or_default(),
                requested_by: requested_by.get(module_id).cloned().unwrap_or_default(),
                source: sources.get(module_id).cloned(),
            },
        );
    }

    for (dependency_id, requirers) in &required_requested_by {
        let active_requirers = requirers
            .iter()
            .filter(|requester| required_closure.contains(*requester))
            .cloned()
            .collect::<BTreeSet<_>>();
        if !active_requirers.is_empty() && !available.contains_key(dependency_id.as_str()) {
            outcome
                .missing
                .insert(dependency_id.clone(), active_requirers);
        }
    }

    // A dependency conflict belongs to the requesting edge, not the target
    // module. The installed target may still be a valid independent root, so
    // only modules whose required ranges fail are blocked.
    for (dependency_id, dependency_requirements) in &requirements {
        let Some(available_manifest) = available.get(dependency_id.as_str()) else {
            continue;
        };
        let unsatisfied = dependency_requirements
            .iter()
            .filter(|(requester, _)| required_closure.contains(*requester))
            .filter(|(_, requirement)| !version_matches(requirement, &available_manifest.version))
            .map(|(requester, _)| requester.clone())
            .collect::<BTreeSet<_>>();
        if !unsatisfied.is_empty() {
            outcome.conflicts.push(VersionConflict {
                module_id: dependency_id.clone(),
                available: available_manifest.version.clone(),
                requirements: dependency_requirements.clone(),
            });
        }
    }

    // Propagate required-edge failures through the reachable graph. Optional
    // edges are intentionally absent from this pass: an unavailable optional
    // service/module degrades only the requesting module's feature set.
    let mut changed = true;
    while changed {
        changed = false;
        for (requester, dependencies) in &required_edges {
            if !available.contains_key(requester.as_str())
                || outcome.blocked.contains_key(requester)
            {
                continue;
            }
            for (dependency_id, requirement) in dependencies {
                let reason = if !available.contains_key(dependency_id.as_str()) {
                    Some(format!("missing required module {dependency_id}"))
                } else if available
                    .get(dependency_id.as_str())
                    .is_none_or(|manifest| !version_matches(requirement, &manifest.version))
                {
                    Some(format!(
                        "required module {dependency_id} does not satisfy {requirement}"
                    ))
                } else if outcome.blocked.contains_key(dependency_id) {
                    Some(format!("required module {dependency_id} is blocked"))
                } else {
                    None
                };
                if let Some(reason) = reason {
                    outcome
                        .blocked
                        .entry(requester.clone())
                        .or_default()
                        .insert(reason);
                    changed = true;
                    break;
                }
            }
        }
    }

    for (requester, dependencies) in &optional_edges {
        for (dependency_id, requirement) in dependencies {
            let issue = if !available.contains_key(dependency_id.as_str()) {
                Some((
                    "optional_module_dependency_missing",
                    format!("optional module {dependency_id} is not installed"),
                ))
            } else if available
                .get(dependency_id.as_str())
                .is_none_or(|manifest| !version_matches(requirement, &manifest.version))
            {
                Some((
                    "optional_module_dependency_version_mismatch",
                    format!("optional module {dependency_id} does not satisfy {requirement}"),
                ))
            } else if outcome.blocked.contains_key(dependency_id) {
                Some((
                    "optional_module_dependency_blocked",
                    format!(
                        "optional module {dependency_id} is blocked by its required dependencies"
                    ),
                ))
            } else {
                None
            };
            if let Some((status, message)) = issue {
                outcome.optional.push(OptionalDependencyIssue {
                    module_id: requester.clone(),
                    dependency_id: dependency_id.clone(),
                    requirement: requirement.clone(),
                    status: status.into(),
                    message,
                });
            }
        }
    }
    outcome.required_closure = required_closure;
    outcome
        .conflicts
        .sort_by(|left, right| left.module_id.cmp(&right.module_id));
    outcome.optional.sort_by(|left, right| {
        left.module_id
            .cmp(&right.module_id)
            .then_with(|| left.dependency_id.cmp(&right.dependency_id))
    });
    outcome
}

fn version_matches(requirement: &str, version: &str) -> bool {
    parse_contract_version(version)
        .zip(parse_version_req(requirement))
        .is_some_and(|(version, request)| request.matches(&version))
}

/// Where to fetch `module_id` from, most specific first.
pub fn source_for<'a>(
    module_id: &str,
    profile_sources: &'a BTreeMap<String, SourceSpec>,
    composition: Option<&'a ModuleManifest>,
) -> Option<&'a SourceSpec> {
    profile_sources
        .get(module_id)
        .or_else(|| composition.and_then(|manifest| manifest.mesh.uses.sources.get(module_id)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(name: &str, version: &str, dependencies: &[(&str, &str)]) -> ModuleManifest {
        let deps = dependencies
            .iter()
            .map(|(id, range)| format!(r#""{id}":"{range}""#))
            .collect::<Vec<_>>()
            .join(",");
        ModuleManifest::from_json_str(&format!(
            r#"{{"name":"{name}","version":"{version}","mesh":{{"apiVersion":"0.1",
               "kind":"library","uses":{{"modules":{{{deps}}}}}}}}}"#
        ))
        .unwrap()
    }

    #[test]
    fn a_diamond_resolves_to_one_shared_version() {
        let root = manifest(
            "@me/root",
            "1.0.0",
            &[("@me/a", ">=1.0.0"), ("@me/b", ">=1.0.0")],
        );
        let a = manifest("@me/a", "1.0.0", &[("@me/shared", ">=1.0.0")]);
        let b = manifest("@me/b", "1.0.0", &[("@me/shared", ">=1.1.0")]);
        let shared = manifest("@me/shared", "1.2.0", &[]);

        let outcome = resolve_closure(["@me/root"], [&root, &a, &b, &shared]);
        assert!(outcome.is_satisfiable(), "{:?}", outcome.conflicts);
        assert_eq!(outcome.modules.len(), 4);
        assert_eq!(outcome.modules["@me/shared"].version, "1.2.0");
        assert_eq!(
            outcome.modules["@me/shared"].requested_by,
            BTreeSet::from(["@me/a".to_string(), "@me/b".to_string()])
        );
    }

    #[test]
    fn requirements_no_single_version_satisfies_are_a_named_conflict() {
        let root = manifest(
            "@me/root",
            "1.0.0",
            &[("@me/a", ">=1.0.0"), ("@me/b", ">=1.0.0")],
        );
        let a = manifest("@me/a", "1.0.0", &[("@me/shared", "^1.0.0")]);
        let b = manifest("@me/b", "1.0.0", &[("@me/shared", "^2.0.0")]);
        let shared = manifest("@me/shared", "1.2.0", &[]);

        let outcome = resolve_closure(["@me/root"], [&root, &a, &b, &shared]);
        assert!(!outcome.is_satisfiable());
        let conflict = &outcome.conflicts[0];
        assert_eq!(conflict.module_id, "@me/shared");
        let message = conflict.message();
        // The diagnostic must name both requirers and both ranges.
        assert!(message.contains("@me/a needs ^1.0.0"), "{message}");
        assert!(message.contains("@me/b needs ^2.0.0"), "{message}");
        assert!(message.contains("1.2.0"), "{message}");
    }

    #[test]
    fn a_required_module_that_is_not_installed_is_reported_with_its_requirers() {
        let root = manifest("@me/root", "1.0.0", &[("@me/absent", ">=1.0.0")]);
        let outcome = resolve_closure(["@me/root"], [&root]);
        assert!(!outcome.is_satisfiable());
        assert_eq!(
            outcome.missing["@me/absent"],
            BTreeSet::from(["@me/root".to_string()])
        );
    }

    #[test]
    fn an_optional_module_that_is_not_installed_degrades_without_blocking_its_root() {
        let root = ModuleManifest::from_json_str(
            r#"{"name":"@me/root","version":"1.0.0","mesh":{"apiVersion":"0.1","kind":"library","uses":{"modules":{"@me/optional":{"version":">=1.0.0","optional":true}}}}}"#,
        )
        .unwrap();
        let outcome = resolve_closure(["@me/root"], [&root]);

        assert!(outcome.is_satisfiable(), "{outcome:?}");
        assert!(outcome.missing.is_empty());
        assert_eq!(outcome.optional.len(), 1);
        assert_eq!(outcome.optional[0].module_id, "@me/root");
        assert_eq!(
            outcome.optional[0].status,
            "optional_module_dependency_missing"
        );
    }

    #[test]
    fn a_required_dependency_blocks_only_the_requesting_module() {
        let root = manifest("@me/root", "1.0.0", &[("@me/absent", ">=1.0.0")]);
        let independent = manifest("@me/independent", "1.0.0", &[]);
        let outcome = resolve_closure(["@me/root", "@me/independent"], [&root, &independent]);

        assert!(!outcome.is_satisfiable());
        assert!(outcome.blocked.contains_key("@me/root"));
        assert!(!outcome.blocked.contains_key("@me/independent"));
        assert_eq!(
            outcome.active_modules(["@me/root", "@me/independent"]),
            BTreeSet::from(["@me/independent".to_string()])
        );
    }

    #[test]
    fn module_version_ranges_are_validated_before_resolution() {
        let error = ModuleManifest::from_json_str(
            r#"{"name":"@me/root","version":"1.0.0","mesh":{"apiVersion":"0.1","kind":"library","uses":{"modules":{"@me/other":"not-a-range"}}}}"#,
        )
        .unwrap_err();

        assert!(error.to_string().contains("invalid version range"));
    }

    #[test]
    fn a_dependency_cycle_terminates() {
        let a = manifest("@me/a", "1.0.0", &[("@me/b", ">=1.0.0")]);
        let b = manifest("@me/b", "1.0.0", &[("@me/a", ">=1.0.0")]);
        let outcome = resolve_closure(["@me/a"], [&a, &b]);
        assert_eq!(outcome.modules.len(), 2);
    }

    #[test]
    fn sources_are_carried_from_the_requiring_manifest() {
        let root = ModuleManifest::from_json_str(
            r#"{"name":"@me/root","version":"1.0.0","mesh":{"apiVersion":"0.1","kind":"composition",
                "uses":{"modules":{"@me/a":">=1.0.0"},
                "sources":{"@me/a":{"git":"https://example.invalid/a","ref":"v1"}}}}}"#,
        )
        .unwrap();
        let a = manifest("@me/a", "1.0.0", &[]);
        let outcome = resolve_closure(["@me/root"], [&root, &a]);
        assert_eq!(
            outcome.modules["@me/a"].source,
            Some(SourceSpec::Git {
                git: "https://example.invalid/a".into(),
                reference: Some("v1".into()),
            })
        );
    }
}
