/// Host API injection — exposes MESH subsystems to Luau scripts.
///
/// This module injects a `mesh` global table into the Luau VM with sub-tables
/// for each subsystem the module has capability to access.
///
/// The injected API:
///
/// ```text
/// require("mesh.<service>") → interface proxy
/// require("mesh.<service>").state → active provider latest state table
/// mesh.theme.token(name)      → value          (requires theme.read)
/// mesh.theme.tokens(group)    → table          (requires theme.read)
/// mesh.theme.on_change(cb)    → subscription   (requires theme.read)
/// mesh.locale.current()       → string         (requires locale.read)
/// mesh.locale.translate(key)  → string         (requires locale.read)
/// mesh.locale.set(locale)     → publishes locale change request (requires locale.write)
/// mesh.config()               → table          (backend helper; full module settings)
/// mesh.exec(program, args)    → table          (backend helper)
/// mesh.service.set_poll_interval(ms)           (backend helper)
/// mesh.events.subscribe(ch, cb) → subscription
/// mesh.events.publish(ch, payload)
/// mesh.ui.request_redraw()
/// mesh.log(level, msg)
/// mesh.log.debug(msg)
/// mesh.log.info(msg)
/// mesh.log.warn(msg)
/// mesh.log.error(msg)
/// ```
use mesh_core_capability::{Capability, CapabilitySet};
use std::collections::HashSet;
// The runtime is interface-first now; mesh.services remains only as a
// compatibility alias for older scripts.

/// Describes what host APIs should be injected based on capabilities.
#[derive(Debug)]
pub struct HostApiManifest {
    pub has_theme_read: bool,
    pub has_locale_read: bool,
    pub has_locale_write: bool,
    pub interface_capabilities: Vec<String>,
    pub service_capabilities: Vec<String>,
    pub has_events: bool,
}

impl HostApiManifest {
    /// Build the manifest from a capability set.
    pub fn from_capabilities(caps: &CapabilitySet) -> Self {
        let has_theme_read = caps.is_granted(&Capability::new("theme.read"));
        let has_locale_read = caps.is_granted(&Capability::new("locale.read"));
        let has_locale_write = caps.is_granted(&Capability::new("locale.write"));

        // Collect service capabilities and their docs-era interface aliases in one pass.
        let mut service_capabilities = Vec::new();
        let mut interface_capabilities = Vec::new();
        let mut seen_interfaces = HashSet::new();
        for cap in caps.granted() {
            let id = cap.id();
            let Some(service_name) = service_name_from_capability(id) else {
                continue;
            };
            service_capabilities.push(id.to_string());
            if seen_interfaces.insert(service_name.to_string()) {
                interface_capabilities.push(format!("mesh.{service_name}"));
            }
        }

        Self {
            has_theme_read,
            has_locale_read,
            has_locale_write,
            interface_capabilities,
            service_capabilities,
            has_events: true, // Events are always available.
        }
    }
}

/// Creates interface proxy tables for Luau scripts.
///
/// A proxy wraps async service trait methods into callable functions.
/// The proxy checks capabilities before each call.
#[derive(Debug)]
pub struct InterfaceProxy;

impl InterfaceProxy {
    /// List which docs-era interfaces are available given the module's capabilities.
    ///
    /// E.g., if the module has `service.audio.read`, this returns `["mesh.audio"]`.
    pub fn available_interfaces(caps: &CapabilitySet) -> Vec<String> {
        let mut interfaces = Vec::new();
        let mut seen = HashSet::new();
        for cap in caps.granted() {
            if let Some(service_name) = service_name_from_capability(cap.id())
                && seen.insert(service_name.to_string())
            {
                interfaces.push(format!("mesh.{service_name}"));
            }
        }
        interfaces
    }

    /// Normalize a short service name or fully-qualified interface name.
    pub fn canonical_name(name: &str) -> String {
        if name.contains('.') {
            name.to_string()
        } else {
            format!("mesh.{name}")
        }
    }

    /// Check if a specific interface is readable with the current capability set.
    ///
    /// This remains permissive for non-core interfaces during the transition;
    /// richer contract-level capability enforcement will come once interface
    /// contracts are loaded dynamically.
    pub fn can_read(caps: &CapabilitySet, interface: &str) -> bool {
        match interface {
            "mesh.theme" => caps.is_granted(&Capability::new("theme.read")),
            "mesh.locale" => caps.is_granted(&Capability::new("locale.read")),
            other => {
                let short = other
                    .strip_prefix("mesh.")
                    .unwrap_or(other)
                    .split('.')
                    .next_back()
                    .unwrap_or(other);
                has_service_capability(caps, short, "read")
                    || has_service_capability(caps, short, "control")
                    || !other.starts_with("mesh.")
            }
        }
    }
}

fn service_name_from_capability(capability: &str) -> Option<&str> {
    let rest = capability.strip_prefix("service.")?;
    let (service_name, action) = rest.rsplit_once('.')?;
    matches!(action, "read" | "control").then_some(service_name)
}

fn has_service_capability(caps: &CapabilitySet, service_name: &str, action: &str) -> bool {
    caps.granted()
        .iter()
        .any(|cap| service_capability_matches(cap.id(), service_name, action))
}

fn service_capability_matches(capability: &str, service_name: &str, action: &str) -> bool {
    let Some(rest) = capability.strip_prefix("service.") else {
        return false;
    };
    let Some((candidate_service, candidate_action)) = rest.rsplit_once('.') else {
        return false;
    };
    candidate_service == service_name && candidate_action == action
}
