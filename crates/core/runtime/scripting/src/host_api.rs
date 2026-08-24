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
/// import("mesh.i18n", "t")    → string         (requires locale.read)
/// mesh.locale.current()       → string         (requires locale.read)
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
use mesh_core_service::InterfaceContract;
use std::collections::HashSet;

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

/// Creates interface proxy tables for Luau scripts: async service methods
/// become callable functions, capability-checked before each call.
#[derive(Debug)]
pub struct InterfaceProxy;

impl InterfaceProxy {
    /// Interfaces the module's capabilities reach — either
    /// `service.audio.read` or `service.audio.control` yields
    /// `["mesh.audio"]`.
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

    /// Whether the capability set can read an interface.
    pub fn can_read(caps: &CapabilitySet, interface: &str) -> bool {
        if let Some(service_name) = interface.strip_prefix("mesh.") {
            return Self::can_read_service(caps, service_name);
        }
        true
    }

    /// Whether a context may receive the Rust-owned snapshot for a service.
    /// Control access is intentionally not a read grant: command-only modules
    /// must not receive provider state through the proxy or shell fan-out.
    pub fn can_read_service(caps: &CapabilitySet, service_name: &str) -> bool {
        match service_name {
            "theme" => caps.is_granted(&Capability::new("theme.read")),
            "locale" => caps.is_granted(&Capability::new("locale.read")),
            _ => has_service_capability(caps, service_name, "read"),
        }
    }

    /// Whether the capability set can control an interface. Control grants
    /// authorize mutations but never make the service state readable.
    pub fn can_control(caps: &CapabilitySet, interface: &str) -> bool {
        if let Some(service_name) = interface.strip_prefix("mesh.") {
            return match service_name {
                "theme" => caps.is_granted(&Capability::new("theme.control")),
                "locale" => caps.is_granted(&Capability::new("locale.write")),
                _ => has_service_capability(caps, service_name, "control"),
            };
        }
        true
    }

    /// Evaluate the contract-resolved read policy. A declared policy is
    /// authoritative even for third-party interface names; only contracts
    /// without any policy use the legacy service-name compatibility rule.
    pub fn can_read_contract(caps: &CapabilitySet, contract: &InterfaceContract) -> bool {
        capability_policy_granted(caps, contract.capabilities.read_policy().as_deref())
            .unwrap_or_else(|| Self::can_read(caps, &contract.interface))
    }

    pub fn can_subscribe_contract_event(
        caps: &CapabilitySet,
        contract: &InterfaceContract,
        event: &str,
    ) -> bool {
        capability_policy_granted(caps, contract.capabilities.event_policy(event).as_deref())
            .unwrap_or_else(|| Self::can_read(caps, &contract.interface))
    }

    /// Whether a consumer may create a proxy for a contract. State reads and
    /// event subscriptions are separate grants, so a consumer with only an
    /// explicit event grant may resolve the proxy without receiving state.
    pub fn can_access_contract(caps: &CapabilitySet, contract: &InterfaceContract) -> bool {
        Self::can_read_contract(caps, contract)
            || contract
                .events
                .iter()
                .any(|event| Self::can_subscribe_contract_event(caps, contract, &event.name))
            || Self::can_control_contract(caps, contract)
    }

    /// Whether a module has at least one effective control grant in a
    /// contract. A contract method policy is authoritative when present;
    /// otherwise the service control capability is the compatibility fallback.
    pub fn can_control_contract(caps: &CapabilitySet, contract: &InterfaceContract) -> bool {
        if contract.methods.is_empty() {
            return Self::can_control(caps, &contract.interface);
        }
        contract
            .methods
            .iter()
            .any(|method| Self::can_call_contract_method(caps, contract, &method.name))
    }

    pub fn can_call_contract_method(
        caps: &CapabilitySet,
        contract: &InterfaceContract,
        method: &str,
    ) -> bool {
        capability_policy_granted(caps, contract.capabilities.method_policy(method).as_deref())
            .unwrap_or_else(|| Self::can_control(caps, &contract.interface))
    }
}

fn capability_policy_granted(caps: &CapabilitySet, policy: Option<&[String]>) -> Option<bool> {
    policy.map(|requirements| {
        !requirements.is_empty()
            && requirements
                .iter()
                .all(|required| caps.is_granted(&Capability::new(required)))
    })
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

#[cfg(test)]
mod tests {
    use super::*;
    use mesh_core_service::parse_interface_contract;

    fn thermal_contract() -> InterfaceContract {
        parse_interface_contract(
            "alice.thermal",
            "1.0",
            &serde_json::json!({
                "state": [{ "name": "temperature", "type": "float" }],
                "methods": [{ "name": "calibrate" }],
                "events": [{ "name": "Changed" }],
                "capabilities": {
                    "read": ["alice.thermal.observe"],
                    "events": { "Changed": ["alice.thermal.subscribe"] },
                    "methods": { "calibrate": ["alice.thermal.calibrate"] }
                }
            }),
        )
        .unwrap()
    }

    #[test]
    fn custom_contract_policy_is_used_for_all_operations() {
        let contract = thermal_contract();
        let mut observe = CapabilitySet::new();
        observe.grant(Capability::new("alice.thermal.observe"));
        assert!(InterfaceProxy::can_read_contract(&observe, &contract));
        assert!(!InterfaceProxy::can_subscribe_contract_event(
            &observe, &contract, "Changed"
        ));
        assert!(!InterfaceProxy::can_call_contract_method(
            &observe,
            &contract,
            "calibrate"
        ));

        let mut control = CapabilitySet::new();
        control.grant(Capability::new("alice.thermal.calibrate"));
        assert!(InterfaceProxy::can_call_contract_method(
            &control,
            &contract,
            "calibrate"
        ));
        assert!(!InterfaceProxy::can_read_contract(&control, &contract));
        assert!(InterfaceProxy::can_control_contract(&control, &contract));
        assert!(InterfaceProxy::can_access_contract(&control, &contract));

        let mut subscribe = CapabilitySet::new();
        subscribe.grant(Capability::new("alice.thermal.subscribe"));
        assert!(InterfaceProxy::can_access_contract(&subscribe, &contract));
        assert!(!InterfaceProxy::can_read_contract(&subscribe, &contract));
    }

    #[test]
    fn service_control_grant_does_not_imply_state_read() {
        let mut control = CapabilitySet::new();
        control.grant(Capability::new("service.audio.control"));

        assert!(!InterfaceProxy::can_read(&control, "mesh.audio"));
        assert!(!InterfaceProxy::can_read_service(&control, "audio"));
        assert!(InterfaceProxy::can_control(&control, "mesh.audio"));
    }
}
