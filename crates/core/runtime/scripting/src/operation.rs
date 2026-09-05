//! The host-owned registry for operations that cross from Luau into the shell.
//!
//! Raw event channels are still useful for service commands, but shell
//! operations have a finite ABI. Keeping their operation identity, caller
//! policy, capability requirement, and payload shape together prevents a
//! dedicated helper and `mesh.events.publish` from acquiring different
//! authorization rules.

use crate::policy::ResourceBudget;
use mesh_core_capability::{Capability, CapabilityCatalog, CapabilitySet};
use mesh_core_service::{InterfaceContract, InterfaceMethod, ResolvedServiceCatalog, TypeExpr};
use serde_json::{Map, Value};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellOperation {
    ShowSurface,
    HideSurface,
    HidePopover,
    SetSurfaceRole,
    ToggleSurfaceRole,
    PromoteWidget,
    DemoteWidget,
    SetWidgetRole,
    ToggleSurface,
    PositionSurface,
    ActivatePopover,
    SetLocale,
    ToggleDebugOverlay,
    ToggleDebugLayoutBounds,
    ToggleDebugElementPicker,
    OpenDebugSource,
    ToggleDebugProfiling,
    RunDebugBenchmark,
    ScheduleHandler,
    CancelHandler,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OperationRejection {
    UnknownChannel {
        channel: String,
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
        if !interface.starts_with("mesh.") || interface == "mesh" || method.is_empty() {
            return Err(OperationRejection::UnknownChannel {
                channel: channel.to_string(),
            });
        }
        let resolution = catalog.resolve(interface, None);
        let Some(contract) = resolution.contract.as_deref() else {
            return Err(OperationRejection::UnknownChannel {
                channel: channel.to_string(),
            });
        };
        let Some(method_spec) = contract
            .methods
            .iter()
            .find(|candidate| candidate.name == method)
        else {
            return Err(OperationRejection::UnknownChannel {
                channel: channel.to_string(),
            });
        };
        self.authorize_service_call(
            channel,
            method_spec,
            contract,
            payload,
            module_id,
            capabilities,
        )?;
        Ok(None)
    }

    /// Apply the same contract capability and exact payload decision to a
    /// service proxy call that already resolved its interface method. Keeping
    /// this decision here means direct `mesh.events.publish` and a typed proxy
    /// cannot diverge before either one reserves queue resources.
    pub(crate) fn authorize_service_call(
        &self,
        channel: &str,
        method: &InterfaceMethod,
        contract: &InterfaceContract,
        payload: &Value,
        module_id: &str,
        capabilities: &CapabilitySet,
    ) -> Result<(), OperationRejection> {
        authorize_contract_capabilities(channel, module_id, capabilities, contract, method)?;
        validate_fields(channel, &method.args, contract, payload)
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

fn authorize_contract_capabilities(
    channel: &str,
    module_id: &str,
    capabilities: &CapabilitySet,
    contract: &InterfaceContract,
    method: &InterfaceMethod,
) -> Result<(), OperationRejection> {
    let service_name = contract
        .interface
        .strip_prefix("mesh.")
        .unwrap_or(&contract.interface);
    let declared = contract
        .capabilities
        .methods
        .get(&method.name)
        .filter(|required| !required.is_empty());
    if let Some(required) = declared {
        for capability in required {
            if !capabilities.is_granted(&Capability::new(capability)) {
                return Err(OperationRejection::Unauthorized {
                    channel: channel.to_string(),
                    module_id: module_id.to_string(),
                    capability: capability.clone(),
                });
            }
        }
    } else {
        let capability = format!("service.{service_name}.control");
        if !capabilities.is_granted(&Capability::new(&capability)) {
            return Err(OperationRejection::Unauthorized {
                channel: channel.to_string(),
                module_id: module_id.to_string(),
                capability,
            });
        }
    }
    Ok(())
}

fn validate_fields(
    channel: &str,
    fields: &[mesh_core_service::InterfaceArgument],
    contract: &InterfaceContract,
    payload: &Value,
) -> Result<(), OperationRejection> {
    let object = payload
        .as_object()
        .ok_or_else(|| malformed(channel, "payload must be an object"))?;
    for field in fields {
        let value = match object.get(&field.name) {
            Some(value) => value,
            None if field.arg_type.trim().ends_with('?') => continue,
            None => {
                return Err(malformed(
                    channel,
                    &format!("missing required field '{}'", field.name),
                ));
            }
        };
        let value_type = TypeExpr::parse(&field.arg_type).map_err(|error| {
            malformed(
                channel,
                &format!("invalid type for '{}': {error}", field.name),
            )
        })?;
        if !value_type.matches_with_types(value, &contract.types) {
            return Err(malformed(
                channel,
                &format!("field '{}' expected {}", field.name, field.arg_type),
            ));
        }
    }
    if let Some(unknown) = object
        .keys()
        .find(|key| !fields.iter().any(|field| field.name == **key))
    {
        return Err(malformed(channel, &format!("unexpected field '{unknown}'")));
    }
    Ok(())
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
        "shell.show-surface" => (ShellOperation::ShowSurface, Some("shell.surface"), None),
        "shell.hide-surface" => (ShellOperation::HideSurface, Some("shell.surface"), None),
        "shell.hide-popover" => (ShellOperation::HidePopover, Some("shell.surface"), None),
        "shell.set-surface-role" => (ShellOperation::SetSurfaceRole, Some("shell.surface"), None),
        "shell.toggle-surface-role" => (
            ShellOperation::ToggleSurfaceRole,
            Some("shell.surface"),
            None,
        ),
        "shell.promote-widget" => (ShellOperation::PromoteWidget, Some("shell.surface"), None),
        "shell.demote-widget" => (ShellOperation::DemoteWidget, Some("shell.surface"), None),
        "shell.set-widget-role" => (ShellOperation::SetWidgetRole, Some("shell.surface"), None),
        "shell.toggle-surface" => (ShellOperation::ToggleSurface, Some("shell.surface"), None),
        "shell.position-surface" => (ShellOperation::PositionSurface, Some("shell.surface"), None),
        "shell.activate-popover" => (ShellOperation::ActivatePopover, Some("shell.surface"), None),
        "shell.set-locale" => (ShellOperation::SetLocale, Some("locale.write"), None),
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
        "shell.schedule-handler" => (ShellOperation::ScheduleHandler, None, None),
        "shell.cancel-handler" => (ShellOperation::CancelHandler, None, None),
        _ => return None,
    };
    Some(definition)
}

#[derive(Debug, Clone, Copy)]
enum PayloadShape {
    Empty,
    SurfaceId,
    HidePopover,
    SurfaceRole,
    WidgetTarget,
    WidgetRole,
    PositionSurface,
    ActivatePopover,
    Locale,
    OpenDebugSource,
    DebugBenchmark,
    ScheduleHandler,
    CancelHandler,
}

fn payload_shape(operation: ShellOperation) -> PayloadShape {
    match operation {
        ShellOperation::ShowSurface
        | ShellOperation::HideSurface
        | ShellOperation::ToggleSurfaceRole
        | ShellOperation::ToggleSurface => PayloadShape::SurfaceId,
        ShellOperation::HidePopover => PayloadShape::HidePopover,
        ShellOperation::SetSurfaceRole => PayloadShape::SurfaceRole,
        ShellOperation::PromoteWidget | ShellOperation::DemoteWidget => PayloadShape::WidgetTarget,
        ShellOperation::SetWidgetRole => PayloadShape::WidgetRole,
        ShellOperation::PositionSurface => PayloadShape::PositionSurface,
        ShellOperation::ActivatePopover => PayloadShape::ActivatePopover,
        ShellOperation::SetLocale => PayloadShape::Locale,
        ShellOperation::ToggleDebugOverlay
        | ShellOperation::ToggleDebugLayoutBounds
        | ShellOperation::ToggleDebugElementPicker
        | ShellOperation::ToggleDebugProfiling => PayloadShape::Empty,
        ShellOperation::OpenDebugSource => PayloadShape::OpenDebugSource,
        ShellOperation::RunDebugBenchmark => PayloadShape::DebugBenchmark,
        ShellOperation::ScheduleHandler => PayloadShape::ScheduleHandler,
        ShellOperation::CancelHandler => PayloadShape::CancelHandler,
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
        PayloadShape::SurfaceId => {
            expect_keys(object, &["surface_id"])?;
            require_string(object, "surface_id").map(|_| ())
        }
        PayloadShape::HidePopover => {
            expect_keys(object, &["surface_id", "defer_for_hover_bridge"])?;
            require_string(object, "surface_id")?;
            optional_bool(object, "defer_for_hover_bridge")
        }
        PayloadShape::SurfaceRole => {
            expect_keys(object, &["surface_id", "role"])?;
            require_string(object, "surface_id")?;
            let role = require_string(object, "role")?;
            if matches!(
                role.trim().to_ascii_lowercase().as_str(),
                "layer" | "window" | "toplevel"
            ) {
                Ok(())
            } else {
                Err("role must be layer, window, or toplevel".into())
            }
        }
        PayloadShape::WidgetTarget => {
            expect_keys(object, &["surface_id", "node_key"])?;
            require_string(object, "surface_id")?;
            require_string(object, "node_key").map(|_| ())
        }
        PayloadShape::WidgetRole => {
            expect_keys(object, &["surface_id", "node_key", "role"])?;
            require_string(object, "surface_id")?;
            require_string(object, "node_key")?;
            let role = require_string(object, "role")?;
            if matches!(
                role.trim().to_ascii_lowercase().as_str(),
                "layer" | "window" | "toplevel"
            ) {
                Ok(())
            } else {
                Err("role must be layer, window, or toplevel".into())
            }
        }
        PayloadShape::PositionSurface => {
            expect_keys(object, &["surface_id", "margin_top", "margin_left"])?;
            require_string(object, "surface_id")?;
            require_i32(object, "margin_top")?;
            require_i32(object, "margin_left")
        }
        PayloadShape::ActivatePopover => {
            expect_keys(
                object,
                &["surface_id", "trigger_surface", "trigger_key", "focus"],
            )?;
            require_string(object, "surface_id")?;
            require_string(object, "trigger_surface")?;
            require_string(object, "trigger_key")?;
            optional_bool(object, "focus")
        }
        PayloadShape::Locale => {
            expect_keys(object, &["locale"])?;
            require_string(object, "locale").map(|_| ())
        }
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
        PayloadShape::ScheduleHandler => {
            expect_keys(object, &["key", "handler", "delay_ms"])?;
            require_string(object, "key")?;
            require_string(object, "handler")?;
            if let Some(delay) = object.get("delay_ms") {
                let delay = delay
                    .as_u64()
                    .ok_or_else(|| "delay_ms must be an integer".to_string())?;
                if delay > 5_000 {
                    return Err("delay_ms must be at most 5000".into());
                }
            }
            Ok(())
        }
        PayloadShape::CancelHandler => {
            expect_keys(object, &["key"])?;
            require_string(object, "key").map(|_| ())
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

fn optional_bool(object: &Map<String, Value>, key: &str) -> Result<(), String> {
    if let Some(value) = object.get(key)
        && !value.is_boolean()
    {
        return Err(format!("field '{key}' must be a boolean"));
    }
    Ok(())
}

fn require_i32(object: &Map<String, Value>, key: &str) -> Result<(), String> {
    let value = object
        .get(key)
        .ok_or_else(|| format!("missing field '{key}'"))?
        .as_i64()
        .ok_or_else(|| format!("field '{key}' must be an integer"))?;
    i32::try_from(value)
        .map(|_| ())
        .map_err(|_| format!("field '{key}' is outside the i32 range"))
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
    fn registry_requires_surface_capability_and_valid_payload() {
        let registry = OperationRegistry::builtin();
        let payload = serde_json::json!({ "surface_id": "@mesh/settings" });
        assert!(matches!(
            registry.authorize_event(
                "shell.show-surface",
                &payload,
                "@mesh/widget",
                &CapabilitySet::default(),
            ),
            Err(OperationRejection::Unauthorized { .. })
        ));
        assert!(
            registry
                .authorize_event(
                    "shell.show-surface",
                    &payload,
                    "@mesh/widget",
                    &capabilities(&["shell.surface"]),
                )
                .is_ok()
        );
    }

    #[test]
    fn registry_rejects_malformed_position_instead_of_coercing_it() {
        let result = OperationRegistry::builtin().authorize_event(
            "shell.position-surface",
            &serde_json::json!({
                "surface_id": "@mesh/popover",
                "margin_top": "bad",
                "margin_left": 0,
            }),
            "@mesh/widget",
            &capabilities(&["shell.surface"]),
        );
        assert!(matches!(
            result,
            Err(OperationRejection::MalformedPayload { .. })
        ));
    }

    #[test]
    fn widget_role_operations_require_a_surface_and_node_target() {
        let registry = OperationRegistry::builtin();
        let capabilities = capabilities(&["shell.surface"]);
        let target = serde_json::json!({
            "surface_id": "@mesh/panel",
            "node_key": "root/widgets/media"
        });

        assert_eq!(
            registry
                .authorize_event(
                    "shell.promote-widget",
                    &target,
                    "@mesh/panel",
                    &capabilities,
                )
                .unwrap(),
            Some(ShellOperation::PromoteWidget)
        );
        assert_eq!(
            registry
                .authorize_event("shell.demote-widget", &target, "@mesh/panel", &capabilities,)
                .unwrap(),
            Some(ShellOperation::DemoteWidget)
        );
        assert_eq!(
            registry
                .authorize_event(
                    "shell.set-widget-role",
                    &serde_json::json!({
                        "surface_id": "@mesh/panel",
                        "node_key": "root/widgets/media",
                        "role": "toplevel"
                    }),
                    "@mesh/panel",
                    &capabilities,
                )
                .unwrap(),
            Some(ShellOperation::SetWidgetRole)
        );

        let malformed = registry.authorize_event(
            "shell.promote-widget",
            &serde_json::json!({ "surface_id": "@mesh/panel" }),
            "@mesh/panel",
            &capabilities,
        );
        assert!(matches!(
            malformed,
            Err(OperationRejection::MalformedPayload { .. })
        ));
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
