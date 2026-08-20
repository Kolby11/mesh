//! The host-owned registry for operations that cross from Luau into the shell.
//!
//! Raw event channels are still useful for service commands, but shell
//! operations have a finite ABI. Keeping their operation identity, caller
//! policy, capability requirement, and payload shape together prevents a
//! dedicated helper and `mesh.events.publish` from acquiring different
//! authorization rules.

use mesh_core_capability::{Capability, CapabilitySet};
use serde_json::{Map, Value};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellOperation {
    ShowSurface,
    HideSurface,
    HidePopover,
    SetSurfaceRole,
    ToggleSurfaceRole,
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
        if let Some(capability) = capability
            && !capabilities.is_granted(&Capability::new(capability))
        {
            return Err(OperationRejection::Unauthorized {
                channel: channel.to_string(),
                module_id: module_id.to_string(),
                capability: capability.to_string(),
            });
        }
        validate_payload(channel, operation, payload)?;
        Ok(Some(operation))
    }
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
        "shell.toggle-surface" => (ShellOperation::ToggleSurface, Some("shell.surface"), None),
        "shell.position-surface" => (ShellOperation::PositionSurface, Some("shell.surface"), None),
        "shell.activate-popover" => (ShellOperation::ActivatePopover, Some("shell.surface"), None),
        "shell.set-locale" => (ShellOperation::SetLocale, Some("locale.write"), None),
        "shell.toggle-debug-overlay" => (
            ShellOperation::ToggleDebugOverlay,
            Some("service.debug.read"),
            Some("@mesh/debug-inspector"),
        ),
        "shell.toggle-debug-layout-bounds" => (
            ShellOperation::ToggleDebugLayoutBounds,
            Some("service.debug.read"),
            Some("@mesh/debug-inspector"),
        ),
        "shell.toggle-debug-element-picker" => (
            ShellOperation::ToggleDebugElementPicker,
            Some("service.debug.read"),
            Some("@mesh/debug-inspector"),
        ),
        "shell.open-debug-source" => (
            ShellOperation::OpenDebugSource,
            Some("service.debug.read"),
            Some("@mesh/debug-inspector"),
        ),
        "shell.toggle-debug-profiling" => (
            ShellOperation::ToggleDebugProfiling,
            Some("service.debug.read"),
            Some("@mesh/debug-inspector"),
        ),
        "shell.run-debug-benchmark" => (
            ShellOperation::RunDebugBenchmark,
            Some("service.debug.read"),
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
        let mut capabilities = CapabilitySet::new();
        for id in ids {
            capabilities.grant(Capability::new(*id));
        }
        capabilities
    }

    #[test]
    fn registry_rejects_unknown_shell_channels() {
        let result = OperationRegistry::builtin().authorize_event(
            "shell.not-real",
            &serde_json::json!({}),
            "@mesh/widget",
            &CapabilitySet::new(),
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
                &CapabilitySet::new(),
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
    fn debug_operations_require_the_debug_inspector_caller() {
        let registry = OperationRegistry::builtin();
        let payload = serde_json::json!({});
        assert!(matches!(
            registry.authorize_event(
                "shell.toggle-debug-overlay",
                &payload,
                "@mesh/widget",
                &capabilities(&["service.debug.read"]),
            ),
            Err(OperationRejection::CallerNotAllowed { .. })
        ));
        assert!(
            registry
                .authorize_event(
                    "shell.toggle-debug-overlay",
                    &payload,
                    "@mesh/debug-inspector",
                    &capabilities(&["service.debug.read"]),
                )
                .is_ok()
        );
    }
}
