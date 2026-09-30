//! The one install review shared by the shell and the CLI.
//!
//! Before a staged module is placed, core decides two things: whether its
//! provenance satisfies the root graph's trust policy, and which privilege
//! level each capability it requests carries. Front ends only render the
//! typed result and their own approval hints; they never re-derive either
//! decision.

use super::{
    ModuleManifest, ModuleManifestError, TrustPolicy, TrustTier, load_module_signature,
    module_tree_digest,
};
use mesh_core_capability::{
    CapabilityCatalog, CapabilityPolicyError, PrivilegeLevel, ServicePermissionDeclaration,
};
use std::path::Path;

/// Privilege approvals the user supplied for this install.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct InstallApproval {
    pub allow_elevated: bool,
    pub allow_high: bool,
}

impl InstallApproval {
    fn permits(self, level: PrivilegeLevel) -> bool {
        match level {
            PrivilegeLevel::Standard => true,
            PrivilegeLevel::Elevated => self.allow_elevated || self.allow_high,
            PrivilegeLevel::High => self.allow_high,
        }
    }
}

/// One requested capability with its catalog privilege level.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewedCapability {
    pub id: String,
    pub level: PrivilegeLevel,
    pub required: bool,
}

/// An accepted install: its provenance tier and classified capabilities.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallReview {
    pub trust: TrustTier,
    pub digest: String,
    pub capabilities: Vec<ReviewedCapability>,
}

#[derive(Debug, thiserror::Error)]
pub enum InstallReviewError {
    #[error("module {module_id} provenance rejected: {reason}")]
    Provenance { module_id: String, reason: String },
    #[error(transparent)]
    Manifest(#[from] ModuleManifestError),
    #[error(transparent)]
    UnknownCapability(#[from] CapabilityPolicyError),
    /// A capability above the supplied approval. Front ends add the hint
    /// for their own approval flag.
    #[error("{module_id} requests {level} capability {capability}")]
    NeedsApproval {
        module_id: String,
        capability: String,
        level: PrivilegeLevel,
    },
}

/// Review a staged module before it is placed.
///
/// `installed` is the capability catalog of the currently installed graph;
/// the candidate's own interface declarations add their permissions, so a
/// package that ships an interface and its consumer reviews as one unit.
pub fn review_install(
    trust_policy: &TrustPolicy,
    staged: &Path,
    manifest: &ModuleManifest,
    from_git: bool,
    installed: &CapabilityCatalog,
    approval: InstallApproval,
) -> Result<InstallReview, InstallReviewError> {
    let signature = load_module_signature(staged)?;
    let digest = module_tree_digest(staged)?;
    let trust = if signature.is_some() {
        TrustTier::Verified
    } else {
        TrustTier::for_source(&manifest.name, from_git)
    };
    trust_policy
        .validate_candidate(
            &manifest.name,
            &manifest.version,
            &digest,
            trust,
            signature.as_ref(),
        )
        .map_err(|reason| InstallReviewError::Provenance {
            module_id: manifest.name.clone(),
            reason,
        })?;

    let catalog = candidate_catalog(installed, manifest);
    let capabilities = review_capabilities(&catalog, manifest)?;
    if let Some(needs) = capabilities
        .iter()
        .find(|capability| !approval.permits(capability.level))
    {
        return Err(InstallReviewError::NeedsApproval {
            module_id: manifest.name.clone(),
            capability: needs.id.clone(),
            level: needs.level,
        });
    }
    Ok(InstallReview {
        trust,
        digest,
        capabilities,
    })
}

/// Classify every capability a manifest requests, required first.
pub fn review_capabilities(
    catalog: &CapabilityCatalog,
    manifest: &ModuleManifest,
) -> Result<Vec<ReviewedCapability>, CapabilityPolicyError> {
    let requested = &manifest.mesh.capabilities;
    requested
        .required
        .iter()
        .map(|id| (id, true))
        .chain(requested.optional.iter().map(|id| (id, false)))
        .map(|(id, required)| {
            let level = catalog.validate(id).map_err(|error| match error {
                CapabilityPolicyError::UnknownCapability { capability, .. } => {
                    CapabilityPolicyError::UnknownCapability {
                        module_id: manifest.name.clone(),
                        capability,
                    }
                }
                other => other,
            })?;
            Ok(ReviewedCapability {
                id: id.clone(),
                level,
                required,
            })
        })
        .collect()
}

/// The installed catalog plus the permissions the candidate's own interface
/// declarations carry. Declarations that fail validation are left out, so a
/// request for one reviews as unknown.
pub fn candidate_catalog(installed: &CapabilityCatalog, manifest: &ModuleManifest) -> CapabilityCatalog {
    let declarations = manifest
        .mesh
        .interface
        .iter()
        .chain(manifest.mesh.interfaces.iter())
        .filter_map(|declaration| {
            let contract = declaration.contract.as_ref()?;
            let version = declaration.version.as_deref().unwrap_or("1.0");
            mesh_core_service::parse_interface_contract(&declaration.name, version, contract).ok()
        })
        .flat_map(|contract| {
            contract
                .capabilities
                .permissions
                .into_iter()
                .map(move |(permission, level)| ServicePermissionDeclaration {
                    interface: contract.interface.clone(),
                    permission,
                    level,
                })
        })
        .collect::<Vec<_>>();
    installed.clone().with_service_permissions(declarations).0
}

#[cfg(test)]
mod tests {
    use super::*;

    struct StagedDir(std::path::PathBuf);

    impl StagedDir {
        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for StagedDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn staged(manifest: &str) -> (StagedDir, ModuleManifest) {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let dir = StagedDir(std::env::temp_dir().join(format!(
            "mesh-install-review-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        )));
        std::fs::create_dir_all(dir.path()).unwrap();
        std::fs::write(dir.path().join("module.json"), manifest).unwrap();
        let manifest = ModuleManifest::from_path(&dir.path().join("module.json")).unwrap();
        (dir, manifest)
    }

    fn review(
        manifest_json: &str,
        installed: &CapabilityCatalog,
        approval: InstallApproval,
    ) -> Result<InstallReview, InstallReviewError> {
        let (dir, manifest) = staged(manifest_json);
        review_install(
            &TrustPolicy::default(),
            dir.path(),
            &manifest,
            false,
            installed,
            approval,
        )
    }

    const CONSUMER: &str = r#"{"name":"@me/forecast","version":"1.0.0","mesh":{"apiVersion":"0.1","kind":"frontend","entry":"main.mesh",
        "capabilities":{"required":["service.weather.read"],"optional":["service.weather.refresh"]}}}"#;

    fn weather_catalog() -> CapabilityCatalog {
        CapabilityCatalog::builtin()
            .with_service_permissions([
                ServicePermissionDeclaration {
                    interface: "mesh.weather".into(),
                    permission: "service.weather.read".into(),
                    level: "standard".into(),
                },
                ServicePermissionDeclaration {
                    interface: "mesh.weather".into(),
                    permission: "service.weather.refresh".into(),
                    level: "elevated".into(),
                },
            ])
            .0
    }

    #[test]
    fn interface_permissions_review_at_their_declared_level() {
        let error = review(CONSUMER, &weather_catalog(), InstallApproval::default()).unwrap_err();
        assert!(
            matches!(
                &error,
                InstallReviewError::NeedsApproval { capability, level: PrivilegeLevel::Elevated, .. }
                    if capability == "service.weather.refresh"
            ),
            "{error}"
        );

        let accepted = review(
            CONSUMER,
            &weather_catalog(),
            InstallApproval {
                allow_elevated: true,
                allow_high: false,
            },
        )
        .unwrap();
        assert_eq!(accepted.trust, TrustTier::Local);
        assert_eq!(
            accepted.capabilities,
            vec![
                ReviewedCapability {
                    id: "service.weather.read".into(),
                    level: PrivilegeLevel::Standard,
                    required: true,
                },
                ReviewedCapability {
                    id: "service.weather.refresh".into(),
                    level: PrivilegeLevel::Elevated,
                    required: false,
                },
            ]
        );
    }

    #[test]
    fn an_undeclared_permission_is_unknown_without_its_interface() {
        let error = review(
            CONSUMER,
            &CapabilityCatalog::builtin(),
            InstallApproval {
                allow_elevated: true,
                allow_high: true,
            },
        )
        .unwrap_err();
        assert!(
            error.to_string().contains("'@me/forecast' requests unknown capability"),
            "{error}"
        );
    }

    #[test]
    fn a_candidate_that_ships_its_interface_reviews_its_own_permissions() {
        let provider = r#"{"name":"@acme/weather","version":"1.0.0","mesh":{"apiVersion":"0.1","kind":"backend",
            "entrypoints":{"main":"src/main.luau"},
            "capabilities":{"required":["service.weather.read"]},
            "interfaces":[{"name":"mesh.weather","version":"1.0","contract":{
                "capabilities":{"permissions":{"service.weather.read":"standard"}}}}],
            "implements":[{"interface":"mesh.weather","provider":"openmeteo"}]}}"#;
        let accepted = review(
            provider,
            &CapabilityCatalog::builtin(),
            InstallApproval::default(),
        )
        .unwrap();
        assert_eq!(accepted.capabilities[0].level, PrivilegeLevel::Standard);
    }
}
