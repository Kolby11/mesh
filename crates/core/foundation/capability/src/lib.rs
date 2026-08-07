//! Capability-based permissions. Modules declare required and optional
//! capabilities in their manifest; core grants or denies them at load time.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
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
            Capability::new("exec.wpctl").privilege_level(),
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
}
