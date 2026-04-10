/// Host API injection — exposes MESH subsystems to Luau scripts.
///
/// This module injects a `mesh` global table into the Luau VM with sub-tables
/// for each subsystem the plugin has capability to access.
///
/// The injected API:
///
/// ```text
/// mesh.services.get(type)     → service proxy (requires service.*.read)
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
// ServiceRegistry will be used when the real Luau VM is integrated
// to create proxy tables for service access.

/// Describes what host APIs should be injected based on capabilities.
#[derive(Debug)]
pub struct HostApiManifest {
    pub has_theme_read: bool,
    pub has_locale_read: bool,
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

        Self {
            has_theme_read,
            has_locale_read,
            service_capabilities,
            has_events: true, // Events are always available.
        }
    }
}

/// Creates service proxy tables for Luau scripts.
///
/// A proxy wraps async service trait methods into callable functions.
/// The proxy checks capabilities before each call.
#[derive(Debug)]
pub struct ServiceProxy;

impl ServiceProxy {
    /// List which service types are available given the plugin's capabilities.
    ///
    /// E.g., if the plugin has `service.audio.read`, this returns `["audio"]`.
    pub fn available_services(caps: &CapabilitySet) -> Vec<String> {
        let mut services = Vec::new();
        for cap in caps.granted() {
            let id = cap.id();
            if let Some(rest) = id.strip_prefix("service.") {
                if let Some(service_name) = rest.split('.').next() {
                    if !services.contains(&service_name.to_string()) {
                        services.push(service_name.to_string());
                    }
                }
            }
        }
        services
    }

    /// Check if a specific service operation is allowed.
    pub fn can_read(caps: &CapabilitySet, service: &str) -> bool {
        caps.is_granted(&Capability::new(format!("service.{service}.read")))
    }

    /// Check if a specific service control operation is allowed.
    pub fn can_control(caps: &CapabilitySet, service: &str) -> bool {
        caps.is_granted(&Capability::new(format!("service.{service}.control")))
    }
}
