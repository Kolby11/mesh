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
    /// Every requirement that had to be satisfied, for diagnostics.
    pub requirements: BTreeMap<String, String>,
    pub requested_by: BTreeSet<String>,
    pub source: Option<SourceSpec>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ResolutionOutcome {
    pub modules: BTreeMap<String, ResolvedModule>,
    pub conflicts: Vec<VersionConflict>,
    /// Required module ids with no installed candidate and no declared source.
    pub missing: BTreeMap<String, BTreeSet<String>>,
}

impl ResolutionOutcome {
    pub fn is_satisfiable(&self) -> bool {
        self.conflicts.is_empty() && self.missing.is_empty()
    }
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
    let mut requested_by: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut sources: BTreeMap<String, SourceSpec> = BTreeMap::new();
    let mut reached: BTreeSet<String> = BTreeSet::new();
    let mut queue: VecDeque<String> = VecDeque::new();

    for root in roots {
        queue.push_back(root.to_string());
        requested_by.entry(root.to_string()).or_default();
    }

    while let Some(module_id) = queue.pop_front() {
        if !reached.insert(module_id.clone()) {
            continue;
        }
        let Some(manifest) = available.get(module_id.as_str()) else {
            continue;
        };
        for (dependency_id, spec) in &manifest.mesh.uses.modules {
            requirements
                .entry(dependency_id.clone())
                .or_default()
                .insert(module_id.clone(), dependency_spec_to_string(spec));
            requested_by
                .entry(dependency_id.clone())
                .or_default()
                .insert(module_id.clone());
            queue.push_back(dependency_id.clone());
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
            outcome.missing.insert(
                module_id.clone(),
                requested_by.get(module_id).cloned().unwrap_or_default(),
            );
            continue;
        };

        if let Some(conflict) = version_conflict(module_id, &manifest.version, &module_requirements)
        {
            outcome.conflicts.push(conflict);
            continue;
        }
        outcome.modules.insert(
            module_id.clone(),
            ResolvedModule {
                module_id: module_id.clone(),
                version: manifest.version.clone(),
                requirements: module_requirements,
                requested_by: requested_by.get(module_id).cloned().unwrap_or_default(),
                source: sources.get(module_id).cloned(),
            },
        );
    }
    outcome
}

/// The installed version must satisfy *every* requirement, not just one.
///
/// A range MESH cannot parse is not treated as a conflict: an unparseable
/// requirement is an authoring bug reported elsewhere, and failing the whole
/// closure over it would be worse than proceeding.
fn version_conflict(
    module_id: &str,
    available: &str,
    requirements: &BTreeMap<String, String>,
) -> Option<VersionConflict> {
    let version = parse_contract_version(available)?;
    let unsatisfied = requirements.iter().any(|(_, range)| {
        parse_version_req(range).is_some_and(|request| !request.matches(&version))
    });
    unsatisfied.then(|| VersionConflict {
        module_id: module_id.to_string(),
        available: available.to_string(),
        requirements: requirements.clone(),
    })
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

    #[test]
    fn an_unparseable_range_does_not_fail_the_closure() {
        let root = manifest("@me/root", "1.0.0", &[("@me/a", "not-a-range")]);
        let a = manifest("@me/a", "1.0.0", &[]);
        let outcome = resolve_closure(["@me/root"], [&root, &a]);
        assert!(outcome.is_satisfiable());
    }
}
