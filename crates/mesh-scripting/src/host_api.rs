/// Host API injection — exposes MESH subsystems to Luau scripts.
///
/// This module injects a `mesh` global table into the Luau VM with sub-tables
/// for each subsystem the plugin has capability to access.
///
/// The injected API:
///
/// ```text
/// mesh.interfaces.get(name, version?) → interface proxy
/// mesh.services.get(type)     → legacy compatibility shim to mesh.interfaces.get("mesh.<type>")
/// mesh.theme.token(name)      → value          (requires theme.read)
/// mesh.theme.tokens(group)    → table          (requires theme.read)
/// mesh.theme.on_change(cb)    → subscription   (requires theme.read)
/// mesh.locale.current()       → string         (requires locale.read)
/// mesh.locale.translate(key)  → string         (requires locale.read)
/// mesh.config.get(key)        → value
/// mesh.config.get_all()       → table
/// mesh.events.subscribe(ch, cb) → subscription
/// mesh.events.publish(ch, payload)
/// mesh.ui.request_redraw()
/// mesh.log.info(msg)
/// mesh.log.warn(msg)
/// mesh.log.error(msg)
/// ```
use mesh_capability::{Capability, CapabilitySet};
// The runtime is interface-first now; mesh.services remains only as a
// compatibility alias for older scripts.

/// Describes what host APIs should be injected based on capabilities.
#[derive(Debug)]
pub struct HostApiManifest {
    pub has_theme_read: bool,
    pub has_locale_read: bool,
    pub interface_capabilities: Vec<String>,
    pub service_capabilities: Vec<String>,
    pub has_events: bool,
}

impl HostApiManifest {
    /// Build the manifest from a capability set.
    pub fn from_capabilities(caps: &CapabilitySet) -> Self {
        let has_theme_read = caps.is_granted(&Capability::new("theme.read"));
        let has_locale_read = caps.is_granted(&Capability::new("locale.read"));

        // Collect service capabilities (anything matching service.*.read or service.*.control).
        let service_capabilities: Vec<String> = caps
            .granted()
            .iter()
            .filter(|c| c.id().starts_with("service."))
            .map(|c| c.id().to_string())
            .collect();
        let interface_capabilities = service_capabilities
            .iter()
            .map(|capability| {
                capability
                    .strip_prefix("service.")
                    .and_then(|value| value.split('.').next())
                    .map(|name| format!("mesh.{name}"))
                    .unwrap_or_else(|| capability.clone())
            })
            .collect();

        Self {
            has_theme_read,
            has_locale_read,
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
    /// List which docs-era interfaces are available given the plugin's capabilities.
    ///
    /// E.g., if the plugin has `service.audio.read`, this returns `["mesh.audio"]`.
    pub fn available_interfaces(caps: &CapabilitySet) -> Vec<String> {
        let mut interfaces = Vec::new();
        for cap in caps.granted() {
            let id = cap.id();
            if let Some(rest) = id.strip_prefix("service.") {
                if let Some(service_name) = rest.split('.').next() {
                    let interface = format!("mesh.{service_name}");
                    if !interfaces.contains(&interface) {
                        interfaces.push(interface);
                    }
                }
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
                caps.is_granted(&Capability::new(format!("service.{short}.read")))
                    || caps.is_granted(&Capability::new(format!("service.{short}.control")))
                    || !other.starts_with("mesh.")
            }
        }
    }
}
