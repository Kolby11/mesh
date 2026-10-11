//! The host-owned registry for operations that cross from Luau into the shell.
//!
//! Raw event channels are still useful for service commands, but shell
//! operations have a finite ABI. Keeping their operation identity, caller
//! policy, capability requirement, and payload shape together prevents a
//! dedicated helper and `mesh.events.publish` from acquiring different
//! authorization rules.

use crate::policy::ResourceBudget;
use mesh_core_capability::{Capability, CapabilityCatalog, CapabilitySet};
use mesh_core_service::{InterfaceContract, InterfaceMethod, ResolvedServiceCatalog};
use serde_json::{Map, Value};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellOperation {
    ToggleDebugOverlay,
    ToggleDebugLayoutBounds,
    ToggleDebugElementPicker,
    OpenDebugSource,
    ToggleDebugProfiling,
    RunDebugBenchmark,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OperationRejection {
    UnknownChannel {
        channel: String,
    },
    Removed {
        channel: String,
        replacement: &'static str,
    },
    /// A service method published as an event. Service calls go through the
    /// typed proxy returned by `require(interface)`.
    ServiceCallEvent {
        channel: String,
        interface: String,
        method: String,
    },
    MalformedPayload {
        channel: String,
        reason: String,
    },
    Unauthorized {
        channel: String,
        module_id: String,
        capability: String,
    },
    CallerNotAllowed {
        channel: String,
        module_id: String,
    },
    Dropped {
        channel: String,
        reason: String,
    },
}

impl fmt::Display for OperationRejection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownChannel { channel } => {
                write!(f, "Unknown shell channel '{channel}'")
            }
            Self::Removed {
                channel,
                replacement,
            } => write!(
                f,
                "Shell channel '{channel}' was removed; use {replacement}"
            ),
            Self::ServiceCallEvent {
                channel,
                interface,
                method,
            } => write!(
                f,
                "Publishing service call '{channel}' was removed; call `require(\"{interface}\").{method}(...)` instead"
            ),
            Self::MalformedPayload { channel, reason } => {
                write!(f, "Malformed payload for '{channel}': {reason}")
            }
            Self::Unauthorized {
                channel,
                module_id,
                capability,
            } => write!(
                f,
                "Denied shell operation '{channel}' from '{module_id}' without {capability}"
            ),
            Self::CallerNotAllowed { channel, module_id } => write!(
                f,
                "Caller '{module_id}' is not allowed to invoke shell operation '{channel}'"
            ),
            Self::Dropped { channel, reason } => {
                write!(f, "Dropped shell operation '{channel}': {reason}")
            }
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct OperationRegistry;

impl OperationRegistry {
    pub const fn builtin() -> Self {
        Self
    }

    /// Authorize a channel if it belongs to the shell operation ABI.
    ///
    /// `Ok(None)` deliberately leaves `mesh.<interface>.<method>` service
    /// commands to the interface-contract router. Every `shell.*` channel,
    /// including unknown ones, is decided here.
    pub fn authorize_event(
        &self,
        channel: &str,
        payload: &Value,
        module_id: &str,
        capabilities: &CapabilitySet,
    ) -> Result<Option<ShellOperation>, OperationRejection> {
        if !channel.starts_with("shell.") {
            return Ok(None);
        }
        if let Some(replacement) = removed_channel_replacement(channel) {
            return Err(OperationRejection::Removed {
                channel: channel.to_string(),
                replacement,
            });
        }
        let Some((operation, capability, caller)) = definition(channel) else {
            return Err(OperationRejection::UnknownChannel {
                channel: channel.to_string(),
            });
        };
        if let Some(caller) = caller
            && caller != module_id
        {
            return Err(OperationRejection::CallerNotAllowed {
                channel: channel.to_string(),
                module_id: module_id.to_string(),
            });
        }
        if let Some(capability) = capability {
            let capability = CapabilityCatalog::builtin()
                .capability(capability)
                .expect("shell operation capability must be in the closed catalog");
            if !capabilities.is_granted(&capability) {
                return Err(OperationRejection::Unauthorized {
                    channel: channel.to_string(),
                    module_id: module_id.to_string(),
                    capability: capability.to_string(),
                });
            }
        }
        validate_payload(channel, operation, payload)?;
        Ok(Some(operation))
    }

    /// Authorize an event against the immutable interface catalog visible to a
    /// frontend context. The legacy shell registry only knows about `shell.*`
    /// operations; this companion decision keeps service commands on the same
    /// typed boundary instead of allowing `mesh.events.publish` to forge an
    /// arbitrary routable channel.
    pub fn authorize_event_with_catalog(
        &self,
        channel: &str,
        payload: &Value,
        module_id: &str,
        capabilities: &CapabilitySet,
        catalog: &ResolvedServiceCatalog,
    ) -> Result<Option<ShellOperation>, OperationRejection> {
        if channel.starts_with("shell.") {
            return self.authorize_event(channel, payload, module_id, capabilities);
        }

        if channel == "mesh.service.cancel" {
            let object = payload
                .as_object()
                .ok_or_else(|| malformed(channel, "payload must be an object"))?;
            expect_keys(object, &["interface", "call_id"])
                .map_err(|reason| malformed(channel, &reason))?;
            let interface = require_string(object, "interface")
                .map_err(|reason| malformed(channel, &reason))?;
            object
                .get("call_id")
                .and_then(Value::as_u64)
                .filter(|id| *id != 0)
                .ok_or_else(|| malformed(channel, "field 'call_id' must be a positive integer"))?;
            if !interface.starts_with("mesh.")
                || catalog.resolve(interface, None).contract.is_none()
            {
                return Err(OperationRejection::UnknownChannel {
                    channel: channel.to_string(),
                });
            }
            let service_name = interface.strip_prefix("mesh.").unwrap_or(interface);
            let capability = Capability::new(format!("service.{service_name}.control"));
            if !capabilities.is_granted(&capability) {
                return Err(OperationRejection::Unauthorized {
                    channel: channel.to_string(),
                    module_id: module_id.to_string(),
                    capability: capability.id().to_string(),
                });
            }
            return Ok(None);
        }

        let Some((interface, method)) = channel.rsplit_once('.') else {
            return Err(OperationRejection::UnknownChannel {
                channel: channel.to_string(),
            });
        };
        let declared = interface.starts_with("mesh.")
            && catalog
                .resolve(interface, None)
                .contract
                .is_some_and(|contract| contract.methods.iter().any(|m| m.name == method));
        if declared {
            return Err(OperationRejection::ServiceCallEvent {
                channel: channel.to_string(),
                interface: interface.to_string(),
                method: method.to_string(),
            });
        }
        Err(OperationRejection::UnknownChannel {
            channel: channel.to_string(),
        })
    }

    /// Validate an imperative element side effect before it enters the shell
    /// queue. Element references are already scoped to a component, so this
    /// boundary validates operation identity and payload shape rather than a
    /// second capability name.
    pub(crate) fn authorize_element_action(
        &self,
        target: &str,
        action: &str,
        args: &Value,
        options: &Value,
    ) -> Result<(), OperationRejection> {
        if target.trim().is_empty() {
            return Err(malformed("element.action", "target must not be empty"));
        }
        if !options.is_null() && !options.is_object() {
            return Err(malformed("element.action", "options must be an object"));
        }
        let Some(arguments) = args.as_array() else {
            return Err(malformed("element.action", "args must be an array"));
        };
        let valid = match action {
            "focus" | "blur" | "scroll_into_view" | "click" => arguments.is_empty(),
            "scroll_to" => {
                (1..=2).contains(&arguments.len())
                    && arguments.iter().all(|value| value.is_number())
            }
            "set_value" => arguments.len() == 1 && arguments[0].is_string(),
            "set_attribute" => arguments.len() == 2 && arguments[0].is_string(),
            _ => false,
        };
        if valid {
            Ok(())
        } else {
            Err(malformed(
                "element.action",
                &format!("invalid arguments for element operation '{action}'"),
            ))
        }
    }
}

fn malformed(channel: &str, reason: &str) -> OperationRejection {
    OperationRejection::MalformedPayload {
        channel: channel.to_string(),
        reason: reason.to_string(),
    }
}

/// Whether `capabilities` may call `method`: every capability the method
/// declares, or `service.<name>.control` when it declares none. Service
/// proxies decide this once, when they are acquired.
pub(crate) fn method_capabilities_granted(
    capabilities: &CapabilitySet,
    contract: &InterfaceContract,
    method: &InterfaceMethod,
) -> bool {
    match contract
        .capabilities
        .methods
        .get(&method.name)
        .filter(|required| !required.is_empty())
    {
        Some(required) => required
            .iter()
            .all(|capability| capabilities.is_granted(&Capability::new(capability))),
        None => {
            let service_name = contract
                .interface
                .strip_prefix("mesh.")
                .unwrap_or(&contract.interface);
            capabilities.is_granted(&Capability::new(format!("service.{service_name}.control")))
        }
    }
}

/// Reserve both the item and its retained serialized bytes as one operation.
/// If the byte budget rejects the item, the queue reservation is rolled back so
/// rejected work cannot permanently consume the realm's queue capacity.
pub(crate) fn reserve_side_effect(resources: &ResourceBudget, bytes: usize) -> Result<(), String> {
    resources
        .reserve_queue()
        .map_err(|error| error.to_string())?;
    if let Err(error) = resources.reserve_queued_output(bytes) {
        resources.release_queue(1);
        return Err(error.to_string());
    }
    Ok(())
}

pub(crate) fn release_side_effect(resources: &ResourceBudget, count: usize, bytes: usize) {
    resources.release_queue(count);
    resources.release_queued_output(bytes);
}

fn definition(
    channel: &str,
) -> Option<(ShellOperation, Option<&'static str>, Option<&'static str>)> {
    let definition = match channel {
        "shell.toggle-debug-overlay" => (
            ShellOperation::ToggleDebugOverlay,
            Some("service.debug.control"),
            Some("@mesh/debug-inspector"),
        ),
        "shell.toggle-debug-layout-bounds" => (
            ShellOperation::ToggleDebugLayoutBounds,
            Some("service.debug.control"),
            Some("@mesh/debug-inspector"),
        ),
        "shell.toggle-debug-element-picker" => (
            ShellOperation::ToggleDebugElementPicker,
            Some("service.debug.control"),
            Some("@mesh/debug-inspector"),
        ),
        "shell.open-debug-source" => (
            ShellOperation::OpenDebugSource,
            Some("service.debug.control"),
            Some("@mesh/debug-inspector"),
        ),
        "shell.toggle-debug-profiling" => (
            ShellOperation::ToggleDebugProfiling,
            Some("service.debug.control"),
            Some("@mesh/debug-inspector"),
        ),
        "shell.run-debug-benchmark" => (
            ShellOperation::RunDebugBenchmark,
            Some("service.debug.control"),
            Some("@mesh/debug-inspector"),
        ),
        _ => return None,
    };
    Some(definition)
}

fn removed_channel_replacement(channel: &str) -> Option<&'static str> {
    match channel {
        "shell.show-surface" => Some("`mesh.shell.show_surface(surface_id)`"),
        "shell.hide-surface" => Some("`mesh.shell.hide_surface(surface_id)`"),
        "shell.toggle-surface" => Some("`mesh.shell.toggle_surface(surface_id)`"),
        "shell.hide-popover" => Some("`mesh.shell.hide_popover(surface_id, { defer = bool })`"),
        "shell.set-surface-role" => Some("`mesh.shell.set_surface_role(surface_id, role)`"),
        "shell.toggle-surface-role" => Some("`mesh.shell.toggle_surface_role(surface_id)`"),
        "shell.promote-widget" => Some("`mesh.shell.promote_widget(surface_id, node_key)`"),
        "shell.demote-widget" => Some("`mesh.shell.demote_widget(surface_id, node_key)`"),
        "shell.set-widget-role" => Some("`mesh.shell.set_widget_role(surface_id, node_key, role)`"),
        "shell.position-surface" => {
            Some("`mesh.shell.position_surface(surface_id, margin_top, margin_left)`")
        }
        "shell.activate-popover" => {
            Some("`mesh.shell.activate_popover(surface_id, event, options)`")
        }
        "shell.set-locale" => Some("`mesh.locale.set(locale)`"),
        "shell.schedule-handler" => Some("`task.delay(ms, fn)`"),
        "shell.cancel-handler" => Some("the `cancel()` method of a `task.delay` handle"),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy)]
enum PayloadShape {
    Empty,
    OpenDebugSource,
    DebugBenchmark,
}

fn payload_shape(operation: ShellOperation) -> PayloadShape {
    match operation {
        ShellOperation::ToggleDebugOverlay
        | ShellOperation::ToggleDebugLayoutBounds
        | ShellOperation::ToggleDebugElementPicker
        | ShellOperation::ToggleDebugProfiling => PayloadShape::Empty,
        ShellOperation::OpenDebugSource => PayloadShape::OpenDebugSource,
        ShellOperation::RunDebugBenchmark => PayloadShape::DebugBenchmark,
    }
}

fn validate_payload(
    channel: &str,
    operation: ShellOperation,
    payload: &Value,
) -> Result<(), OperationRejection> {
    let shape = payload_shape(operation);
    let result = validate_payload_shape(shape, payload);
    result.map_err(|reason| OperationRejection::MalformedPayload {
        channel: channel.to_string(),
        reason,
    })
}

fn validate_payload_shape(shape: PayloadShape, payload: &Value) -> Result<(), String> {
    let object = payload
        .as_object()
        .ok_or_else(|| "payload must be an object".to_string())?;
    match shape {
        PayloadShape::Empty => expect_keys(object, &[]),
        PayloadShape::OpenDebugSource => {
            expect_keys(object, &["path", "line"])?;
            require_string(object, "path")?;
            if let Some(line) = object.get("line") {
                let line = line
                    .as_u64()
                    .ok_or_else(|| "line must be a positive integer".to_string())?;
                if line == 0 {
                    return Err("line must be a positive integer".into());
                }
            }
            Ok(())
        }
        PayloadShape::DebugBenchmark => {
            expect_keys(object, &["scenario_id"])?;
            require_string(object, "scenario_id").map(|_| ())
        }
    }
}

fn expect_keys(object: &Map<String, Value>, allowed: &[&str]) -> Result<(), String> {
    if let Some(key) = object
        .keys()
        .find(|key| !allowed.iter().any(|allowed| allowed == key))
    {
        return Err(format!("unexpected field '{key}'"));
    }
    Ok(())
}

fn require_string<'a>(object: &'a Map<String, Value>, key: &str) -> Result<&'a str, String> {
    let value = object
        .get(key)
        .ok_or_else(|| format!("missing field '{key}'"))?
        .as_str()
        .ok_or_else(|| format!("field '{key}' must be a string"))?;
    if value.trim().is_empty() {
        return Err(format!("field '{key}' must not be empty"));
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn capabilities(ids: &[&str]) -> CapabilitySet {
        CapabilitySet::from_ids(ids.iter().copied())
    }

    #[test]
    fn registry_rejects_unknown_shell_channels() {
        let result = OperationRegistry::builtin().authorize_event(
            "shell.not-real",
            &serde_json::json!({}),
            "@mesh/widget",
            &CapabilitySet::default(),
        );
        assert!(matches!(
            result,
            Err(OperationRejection::UnknownChannel { .. })
        ));
    }

    #[test]
    fn removed_channels_name_their_typed_replacement() {
        let registry = OperationRegistry::builtin();
        for (channel, replacement) in [
            ("shell.show-surface", "mesh.shell.show_surface"),
            ("shell.promote-widget", "mesh.shell.promote_widget"),
            ("shell.set-locale", "mesh.locale.set"),
            ("shell.schedule-handler", "task.delay"),
        ] {
            let rejection = registry
                .authorize_event(
                    channel,
                    &serde_json::json!({ "surface_id": "@mesh/settings" }),
                    "@mesh/widget",
                    &capabilities(&["shell.surface", "locale.write"]),
                )
                .unwrap_err();
            assert!(matches!(rejection, OperationRejection::Removed { .. }));
            assert!(rejection.to_string().contains(replacement), "{rejection}");
        }
    }

    #[test]
    fn debug_operations_require_the_debug_inspector_caller() {
        let registry = OperationRegistry::builtin();
        let payload = serde_json::json!({});
        assert!(matches!(
            registry.authorize_event(
                "shell.toggle-debug-overlay",
                &payload,
                "@mesh/widget",
                &capabilities(&["service.debug.control"]),
            ),
            Err(OperationRejection::CallerNotAllowed { .. })
        ));
        assert!(
            registry
                .authorize_event(
                    "shell.toggle-debug-overlay",
                    &payload,
                    "@mesh/debug-inspector",
                    &capabilities(&["service.debug.control"]),
                )
                .is_ok()
        );
    }

    #[test]
    fn debug_operations_do_not_treat_read_access_as_control() {
        let result = OperationRegistry::builtin().authorize_event(
            "shell.toggle-debug-overlay",
            &serde_json::json!({}),
            "@mesh/debug-inspector",
            &capabilities(&["service.debug.read"]),
        );

        assert!(matches!(
            result,
            Err(OperationRejection::Unauthorized { capability, .. })
                if capability == "service.debug.control"
        ));
    }

    #[test]
    fn rejected_side_effect_bytes_do_not_consume_queue_capacity() {
        let mut config = mesh_core_runtime::SandboxConfig::default();
        config.queue_budget = 1;
        config.output_budget = 1;
        let budget = ResourceBudget::new(config);
        let _callback = budget.begin_callback();

        assert!(reserve_side_effect(&budget, 2).is_err());
        assert!(budget.reserve_queue().is_ok());
        budget.release_queue(1);
    }
}
