//! Capability-based permissions. Modules declare required and optional
//! capabilities in their manifest; core grants or denies them at load time.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fmt;

/// A dotted capability id: `shell.widget`, `service.battery.read`, …
#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct Capability(String);

impl Capability {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn id(&self) -> &str {
        &self.0
    }

    pub fn privilege_level(&self) -> PrivilegeLevel {
        match self.0.as_str() {
            // High privilege
            "exec.command" | "shell.screenshot" | "dbus.system" | "net.socket" | "locale.write" => {
                PrivilegeLevel::High
            }
            s if s.starts_with("exec.") && s != "exec.launch-app" => PrivilegeLevel::High,

            // Elevated privilege
            s if s.ends_with(".control") => PrivilegeLevel::Elevated,
            "exec.launch-app"
            | "net.http"
            | "shell.clipboard.write"
            | "shell.notification"
            | "fs.write"
            | "dbus.session"
            | "service.notifications.post"
            | "service.notifications.manage" => PrivilegeLevel::Elevated,

            // Standard (default)
            _ => PrivilegeLevel::Standard,
        }
    }

    /// Look up this capability in the closed host capability catalog.
    ///
    /// `privilege_level()` is retained for compatibility with callers that
    /// already hold a validated capability. New manifest and activation paths
    /// must use this catalog-backed lookup so an unclassified name cannot
    /// silently acquire the default `standard` level.
    pub fn catalog_privilege_level(&self) -> Option<PrivilegeLevel> {
        CapabilityCatalog::builtin().privilege_level(self.id())
    }
}

impl fmt::Display for Capability {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum PrivilegeLevel {
    /// Read-only access to services, theme, locale.
    Standard,
    /// Meaningful system interaction; confirmed at install.
    Elevated,
    /// Sensitive access; explicit opt-in with a warning.
    High,
}

impl fmt::Display for PrivilegeLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Standard => write!(f, "standard"),
            Self::Elevated => write!(f, "elevated"),
            Self::High => write!(f, "high"),
        }
    }
}

/// A capability definition in the host's closed capability vocabulary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityDefinition {
    pub id: String,
    pub privilege: PrivilegeLevel,
}

/// The host capabilities understood by this MESH build.
///
/// Service read/control capabilities are listed explicitly because interface
/// contracts are data and their consumer capabilities must be reviewed before
/// they become runnable. Provider host powers are also explicit; executable
/// access uses the structured `exec.argv:<program>:<json-args>` form rather
/// than basename-derived grants.
#[derive(Debug, Clone, Copy, Default)]
pub struct CapabilityCatalog;

impl CapabilityCatalog {
    pub const fn builtin() -> Self {
        Self
    }

    pub fn definition(&self, id: &str) -> Option<CapabilityDefinition> {
        let privilege = match id {
            "shell.surface"
            | "shell.widget"
            | "theme.read"
            | "locale.read"
            | "service.audio.read"
            | "service.brightness.read"
            | "service.bluetooth.read"
            | "service.composition.read"
            | "service.debug.read"
            | "service.device.read"
            | "service.media.read"
            | "service.network.read"
            | "service.packages.read"
            | "service.power.read"
            | "service.settings.read"
            | "service.wm.read" => PrivilegeLevel::Standard,
            "exec.launch-app"
            | "shell.clipboard.write"
            | "shell.notification"
            | "fs.write"
            | "dbus.session"
            | "net.http"
            | "service.audio.control"
            | "service.brightness.control"
            | "service.bluetooth.control"
            | "service.composition.control"
            | "service.media.control"
            | "service.network.control"
            | "service.packages.control"
            | "service.settings.control"
            | "service.theme.control"
            | "service.wm.control"
            | "service.notifications.post"
            | "service.notifications.manage" => PrivilegeLevel::Elevated,
            "exec.command" | "shell.screenshot" | "dbus.system" | "net.socket" | "locale.write" => {
                PrivilegeLevel::High
            }
            value if value.starts_with("exec.argv:") && valid_exec_argv_capability(value) => {
                PrivilegeLevel::High
            }
            _ => return None,
        };
        Some(CapabilityDefinition {
            id: id.to_string(),
            privilege,
        })
    }

    pub fn privilege_level(&self, id: &str) -> Option<PrivilegeLevel> {
        self.definition(id).map(|definition| definition.privilege)
    }

    pub fn validate(&self, id: &str) -> Result<PrivilegeLevel, CapabilityPolicyError> {
        self.privilege_level(id)
            .ok_or_else(|| CapabilityPolicyError::UnknownCapability {
                module_id: String::new(),
                capability: id.to_string(),
            })
    }
}

fn valid_exec_argv_capability(value: &str) -> bool {
    let Some(specification) = value.strip_prefix("exec.argv:") else {
        return false;
    };
    let Some((program, arguments)) = specification.split_once(':') else {
        return false;
    };
    if program.is_empty() || program.contains('\0') || arguments.is_empty() {
        return false;
    }
    arguments == "*" || serde_json::from_str::<Vec<String>>(arguments).is_ok()
}

/// The immutable result of resolving a module's declarations against user
/// approvals. Only this value should cross the activation boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectiveCapabilities {
    module_id: String,
    required: BTreeSet<String>,
    optional: BTreeSet<String>,
    granted: BTreeSet<String>,
}

impl EffectiveCapabilities {
    pub fn module_id(&self) -> &str {
        &self.module_id
    }

    pub fn is_granted(&self, capability: &Capability) -> bool {
        self.granted.contains(capability.id())
    }

    pub fn is_granted_id(&self, capability: &str) -> bool {
        self.granted.contains(capability)
    }

    pub fn granted_ids(&self) -> impl Iterator<Item = &str> {
        self.granted.iter().map(String::as_str)
    }

    pub fn required_ids(&self) -> impl Iterator<Item = &str> {
        self.required.iter().map(String::as_str)
    }

    pub fn optional_ids(&self) -> impl Iterator<Item = &str> {
        self.optional.iter().map(String::as_str)
    }

    /// Adapt the immutable policy result to the runtime's capability proof
    /// container. The runtime receives only the resolved grant set.
    pub fn into_capability_set(&self) -> CapabilitySet {
        CapabilitySet::from_ids(self.granted.iter().cloned())
    }
}

/// Persisted decisions and the catalog-backed resolver used at activation.
#[derive(Debug, Clone, Default)]
pub struct CapabilityPolicy {
    catalog: CapabilityCatalog,
    approvals: BTreeMap<String, BTreeSet<String>>,
}

impl CapabilityPolicy {
    pub fn from_approvals<I>(approvals: I) -> Self
    where
        I: IntoIterator<Item = (String, Vec<String>)>,
    {
        let approvals = approvals
            .into_iter()
            .map(|(module_id, capabilities)| {
                (module_id, capabilities.into_iter().collect::<BTreeSet<_>>())
            })
            .collect();
        Self {
            catalog: CapabilityCatalog::builtin(),
            approvals,
        }
    }

    pub fn approvals(&self) -> &BTreeMap<String, BTreeSet<String>> {
        &self.approvals
    }

    pub fn resolve(
        &self,
        module_id: &str,
        required: &[String],
        optional: &[String],
    ) -> Result<EffectiveCapabilities, CapabilityPolicyError> {
        let required = normalize_declarations(&self.catalog, module_id, required)?;
        let optional = normalize_declarations(&self.catalog, module_id, optional)?;
        let approved = self.approvals.get(module_id);

        let missing_required = required
            .iter()
            .filter(|capability| !approved.is_some_and(|set| set.contains(*capability)))
            .cloned()
            .collect::<Vec<_>>();
        if !missing_required.is_empty() {
            return Err(CapabilityPolicyError::MissingRequiredApproval {
                module_id: module_id.to_string(),
                capabilities: missing_required,
            });
        }

        let mut granted = required.clone();
        granted.extend(
            optional
                .iter()
                .filter(|capability| approved.is_some_and(|set| set.contains(*capability)))
                .cloned(),
        );
        Ok(EffectiveCapabilities {
            module_id: module_id.to_string(),
            required,
            optional,
            granted,
        })
    }
}

fn normalize_declarations(
    catalog: &CapabilityCatalog,
    module_id: &str,
    declarations: &[String],
) -> Result<BTreeSet<String>, CapabilityPolicyError> {
    let mut normalized = BTreeSet::new();
    for capability in declarations {
        if capability.trim() != capability || capability.is_empty() {
            return Err(CapabilityPolicyError::InvalidCapability {
                module_id: module_id.to_string(),
                capability: capability.clone(),
            });
        }
        catalog
            .validate(capability)
            .map_err(|error| error.with_module(module_id))?;
        normalized.insert(capability.clone());
    }
    Ok(normalized)
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CapabilityPolicyError {
    #[error("module '{module_id}' requests unknown capability '{capability}'")]
    UnknownCapability {
        module_id: String,
        capability: String,
    },
    #[error("module '{module_id}' declares malformed capability '{capability}'")]
    InvalidCapability {
        module_id: String,
        capability: String,
    },
    #[error("module '{module_id}' is missing approval for required capabilities: {capabilities:?}")]
    MissingRequiredApproval {
        module_id: String,
        capabilities: Vec<String>,
    },
}

impl CapabilityPolicyError {
    fn with_module(self, module_id: &str) -> Self {
        match self {
            Self::UnknownCapability { capability, .. } => Self::UnknownCapability {
                module_id: module_id.to_string(),
                capability,
            },
            other => other,
        }
    }
}

/// Proof that a capability was granted. APIs take the handle as a parameter,
/// making unauthorized access a compile-time error for Rust modules.
#[derive(Debug, Clone)]
pub struct CapabilityHandle {
    capability: Capability,
}

impl CapabilityHandle {
    pub fn capability(&self) -> &Capability {
        &self.capability
    }
}

#[derive(Debug, Clone)]
pub struct CapabilitySet {
    granted: HashSet<Capability>,
}

impl CapabilitySet {
    pub fn new() -> Self {
        Self {
            granted: HashSet::new(),
        }
    }

    pub fn from_ids<I>(ids: I) -> Self
    where
        I: IntoIterator<Item = String>,
    {
        Self {
            granted: ids.into_iter().map(Capability::new).collect(),
        }
    }

    pub fn grant(&mut self, capability: Capability) -> CapabilityHandle {
        self.granted.insert(capability.clone());
        CapabilityHandle { capability }
    }

    pub fn is_granted(&self, capability: &Capability) -> bool {
        self.granted.contains(capability)
    }

    pub fn granted(&self) -> &HashSet<Capability> {
        &self.granted
    }
}

impl Default for CapabilitySet {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capability_privilege_levels() {
        assert_eq!(
            Capability::new("theme.read").privilege_level(),
            PrivilegeLevel::Standard
        );
        assert_eq!(
            Capability::new("service.network.control").privilege_level(),
            PrivilegeLevel::Elevated
        );
        assert_eq!(
            Capability::new("exec.command").privilege_level(),
            PrivilegeLevel::High
        );
        assert_eq!(
            Capability::new("exec.argv:wpctl:[\"get-volume\"]").privilege_level(),
            PrivilegeLevel::High
        );
        assert_eq!(
            Capability::new("exec.launch-app").privilege_level(),
            PrivilegeLevel::Elevated
        );
    }

    #[test]
    fn capability_set_grant_and_check() {
        let mut set = CapabilitySet::new();
        let cap = Capability::new("theme.read");
        let _handle = set.grant(cap.clone());
        assert!(set.is_granted(&cap));
        assert!(!set.is_granted(&Capability::new("exec.command")));
    }

    #[test]
    fn unknown_capabilities_fail_closed() {
        assert_eq!(
            Capability::new("service.unknown.read").catalog_privilege_level(),
            None
        );
        assert_eq!(
            CapabilityCatalog::builtin().validate("exec.wpctl"),
            Err(CapabilityPolicyError::UnknownCapability {
                module_id: String::new(),
                capability: "exec.wpctl".into(),
            })
        );
        assert_eq!(
            CapabilityCatalog::builtin().validate("exec.argv:wpctl:[\"get-volume\"]"),
            Ok(PrivilegeLevel::High)
        );
        assert_eq!(
            CapabilityCatalog::builtin().validate("service.unknown.read"),
            Err(CapabilityPolicyError::UnknownCapability {
                module_id: String::new(),
                capability: "service.unknown.read".into(),
            })
        );
    }

    #[test]
    fn required_capabilities_need_approval_and_optional_default_to_denied() {
        let policy =
            CapabilityPolicy::from_approvals([("@mesh/test".into(), vec!["theme.read".into()])]);
        let required = vec!["theme.read".into()];
        let optional = vec!["locale.read".into()];
        let effective = policy.resolve("@mesh/test", &required, &optional).unwrap();
        assert!(effective.is_granted_id("theme.read"));
        assert!(!effective.is_granted_id("locale.read"));
        assert_eq!(
            policy.resolve("@mesh/missing", &required, &[]),
            Err(CapabilityPolicyError::MissingRequiredApproval {
                module_id: "@mesh/missing".into(),
                capabilities: vec!["theme.read".into()],
            })
        );
    }

    #[test]
    fn optional_approval_is_included_in_effective_grants() {
        let policy = CapabilityPolicy::from_approvals([(
            "@mesh/test".into(),
            vec!["theme.read".into(), "locale.read".into()],
        )]);
        let effective = policy
            .resolve(
                "@mesh/test",
                &["theme.read".into()],
                &["locale.read".into()],
            )
            .unwrap();
        assert!(effective.is_granted_id("locale.read"));
        assert_eq!(effective.into_capability_set().granted().len(), 2);
    }
}
