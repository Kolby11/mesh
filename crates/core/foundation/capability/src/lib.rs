/// Capability-based permission model for MESH plugins.
///
/// Capabilities are named permissions that grant access to specific host APIs.
/// Plugins declare required and optional capabilities in their manifest.
/// The core grants or denies them at load time.
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fmt;

/// A single capability identifier.
///
/// Capabilities follow a dotted namespace convention:
/// `shell.widget`, `service.battery.read`, `exec.launch-app`, etc.
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
            "exec.command" | "shell.screenshot" | "dbus.system" | "net.socket" | "theme.write"
            | "locale.write" => PrivilegeLevel::High,

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

/// How sensitive a capability is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum PrivilegeLevel {
    /// Safe for most plugins. Read-only access to services, theme, locale.
    Standard,
    /// Grants meaningful system interaction. Requires user confirmation at install.
    Elevated,
    /// Powerful or sensitive access. Requires explicit user opt-in with a warning.
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

/// A handle proving that a capability has been granted.
///
/// Plugins receive handles at init for each granted capability.
/// APIs require the corresponding handle as a parameter, making
/// unauthorized access a compile-time error for Rust plugins.
#[derive(Debug, Clone)]
pub struct CapabilityHandle {
    capability: Capability,
}

impl CapabilityHandle {
    pub fn capability(&self) -> &Capability {
        &self.capability
    }
}

/// Manages capability grants for a plugin.
#[derive(Debug)]
pub struct CapabilitySet {
    granted: HashSet<Capability>,
}

impl CapabilitySet {
    pub fn new() -> Self {
        Self {
            granted: HashSet::new(),
        }
    }

    /// Grant a capability and return its handle.
    pub fn grant(&mut self, capability: Capability) -> CapabilityHandle {
        self.granted.insert(capability.clone());
        CapabilityHandle { capability }
    }

    /// Check if a capability has been granted.
    pub fn is_granted(&self, capability: &Capability) -> bool {
        self.granted.contains(capability)
    }

    /// Return all granted capabilities.
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
