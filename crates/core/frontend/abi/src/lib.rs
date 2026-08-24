//! Renderer-neutral contracts emitted by frontend components.
//!
//! This crate deliberately does not know about Wayland, paint buffers, debug
//! backends, or package storage. A frontend instance produces typed effects
//! with the identity and capability proof of its caller; a host adapter owns
//! the policy-specific lowering of those effects into shell operations.

use mesh_core_capability::{Capability, CapabilitySet};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectSource {
    pub module_id: String,
    pub instance_id: Option<String>,
}

impl EffectSource {
    pub fn new(module_id: impl Into<String>, instance_id: Option<String>) -> Self {
        Self {
            module_id: module_id.into(),
            instance_id,
        }
    }
}

/// The immutable authority attached to all effects from one frontend caller.
/// Capabilities are retained here rather than copied into individual effect
/// variants, so adapters cannot accidentally authorize an unscoped payload.
#[derive(Debug, Clone)]
pub struct EffectScope {
    source: EffectSource,
    capabilities: CapabilitySet,
}

impl EffectScope {
    pub fn new(source: EffectSource, capabilities: CapabilitySet) -> Self {
        Self {
            source,
            capabilities,
        }
    }

    pub fn source(&self) -> &EffectSource {
        &self.source
    }

    pub fn capabilities(&self) -> &CapabilitySet {
        &self.capabilities
    }

    pub fn authorize(&self, effect: &FrontendEffect) -> Result<(), EffectRejection> {
        let required = effect.required_capabilities();
        if required
            .iter()
            .any(|capability| self.capabilities.is_granted(capability))
        {
            return Ok(());
        }

        Err(EffectRejection::MissingCapability {
            module_id: self.source.module_id.clone(),
            effect: effect.kind().to_owned(),
            required: required
                .iter()
                .map(|capability| capability.id().to_owned())
                .collect(),
        })
    }
}

#[derive(Debug, Clone)]
pub struct ScopedFrontendEffect {
    pub scope: EffectScope,
    pub effect: FrontendEffect,
}

impl ScopedFrontendEffect {
    pub fn new(scope: EffectScope, effect: FrontendEffect) -> Self {
        Self { scope, effect }
    }

    pub fn authorize(&self) -> Result<(), EffectRejection> {
        self.scope.authorize(&self.effect)
    }
}

#[derive(Debug, Clone)]
pub struct FrontendEffectBatch {
    pub scope: EffectScope,
    pub effects: Vec<FrontendEffect>,
}

impl FrontendEffectBatch {
    pub fn new(scope: EffectScope, effects: Vec<FrontendEffect>) -> Self {
        Self { scope, effects }
    }

    pub fn into_scoped(self) -> impl Iterator<Item = ScopedFrontendEffect> {
        let scope = self.scope;
        self.effects
            .into_iter()
            .map(move |effect| ScopedFrontendEffect::new(scope.clone(), effect))
    }
}

#[derive(Debug, Clone)]
pub enum FrontendEffect {
    Surface(SurfaceEffect),
    Service(ServiceEffect),
    SetLocale { locale: String },
    WriteClipboard { text: String },
    Debug(DebugEffect),
}

impl FrontendEffect {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Surface(effect) => effect.kind(),
            Self::Service(effect) => effect.kind(),
            Self::SetLocale { .. } => "set-locale",
            Self::WriteClipboard { .. } => "write-clipboard",
            Self::Debug(effect) => effect.kind(),
        }
    }

    fn required_capabilities(&self) -> Vec<Capability> {
        match self {
            Self::Surface(_) => vec![Capability::new("shell.surface")],
            Self::Service(effect) => vec![Capability::new(format!(
                "service.{}.control",
                service_name(&effect.interface())
            ))],
            Self::SetLocale { .. } => vec![Capability::new("locale.write")],
            Self::WriteClipboard { .. } => vec![Capability::new("shell.clipboard.write")],
            Self::Debug(_) => vec![Capability::new("service.debug.read")],
        }
    }
}

#[derive(Debug, Clone)]
pub enum SurfaceEffect {
    Toggle {
        surface_id: String,
    },
    Show {
        surface_id: String,
    },
    Hide {
        surface_id: String,
    },
    HidePopover {
        surface_id: String,
        defer_for_hover_bridge: bool,
    },
    SetRole {
        surface_id: String,
        role: SurfaceRole,
    },
    ToggleRole {
        surface_id: String,
    },
    SetChildRole {
        surface_id: String,
        node_key: String,
        role: SurfaceRole,
    },
    Position {
        surface_id: String,
        margin_top: i32,
        margin_left: i32,
    },
    ActivatePopover {
        surface_id: String,
        trigger_surface: String,
        trigger_key: String,
        focus: bool,
    },
}

impl SurfaceEffect {
    fn kind(&self) -> &'static str {
        match self {
            Self::Toggle { .. } => "toggle-surface",
            Self::Show { .. } => "show-surface",
            Self::Hide { .. } => "hide-surface",
            Self::HidePopover { .. } => "hide-popover",
            Self::SetRole { .. } => "set-surface-role",
            Self::ToggleRole { .. } => "toggle-surface-role",
            Self::SetChildRole { .. } => "set-child-surface-role",
            Self::Position { .. } => "position-surface",
            Self::ActivatePopover { .. } => "activate-popover",
        }
    }
}

#[derive(Debug, Clone)]
pub enum ServiceEffect {
    Command {
        interface: String,
        command: String,
        payload: Value,
    },
    Call {
        interface: String,
        command: String,
        payload: Value,
        call_id: u64,
        instance_id: String,
    },
    Cancel {
        interface: String,
        call_id: u64,
        instance_id: String,
    },
}

impl ServiceEffect {
    fn interface(&self) -> &str {
        match self {
            Self::Command { interface, .. }
            | Self::Call { interface, .. }
            | Self::Cancel { interface, .. } => interface,
        }
    }

    fn kind(&self) -> &'static str {
        match self {
            Self::Command { .. } => "service-command",
            Self::Call { .. } => "service-call",
            Self::Cancel { .. } => "service-cancel",
        }
    }
}

#[derive(Debug, Clone)]
pub enum DebugEffect {
    ToggleOverlay,
    ToggleLayoutBounds,
    ToggleElementPicker,
    OpenSource { path: String, line: u32 },
    ToggleProfiling,
    RunBenchmark { scenario_id: String },
}

impl DebugEffect {
    fn kind(&self) -> &'static str {
        match self {
            Self::ToggleOverlay => "debug-overlay",
            Self::ToggleLayoutBounds => "debug-layout-bounds",
            Self::ToggleElementPicker => "debug-element-picker",
            Self::OpenSource { .. } => "debug-source",
            Self::ToggleProfiling => "debug-profiling",
            Self::RunBenchmark { .. } => "debug-benchmark",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceRole {
    Layer,
    Window,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EffectRejection {
    #[error("frontend effect '{effect}' from '{module_id}' requires one of {required:?}")]
    MissingCapability {
        module_id: String,
        effect: String,
        required: Vec<String>,
    },
}

fn service_name(interface: &str) -> String {
    interface
        .strip_prefix("mesh.")
        .unwrap_or(interface)
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn surface_effects_are_authorized_by_the_scope_not_the_variant() {
        let mut capabilities = CapabilitySet::new();
        capabilities.grant(Capability::new("shell.surface"));
        let effect = ScopedFrontendEffect::new(
            EffectScope::new(EffectSource::new("@mesh/panel", None), capabilities),
            FrontendEffect::Surface(SurfaceEffect::Show {
                surface_id: "@mesh/panel".into(),
            }),
        );

        assert!(effect.authorize().is_ok());
    }

    #[test]
    fn service_effects_reject_missing_control_capability() {
        let effect = ScopedFrontendEffect::new(
            EffectScope::new(EffectSource::new("@mesh/panel", None), CapabilitySet::new()),
            FrontendEffect::Service(ServiceEffect::Command {
                interface: "mesh.audio".into(),
                command: "set_volume".into(),
                payload: serde_json::json!({ "percent": 50 }),
            }),
        );

        assert_eq!(
            effect.authorize(),
            Err(EffectRejection::MissingCapability {
                module_id: "@mesh/panel".into(),
                effect: "service-command".into(),
                required: vec!["service.audio.control".into()],
            })
        );
    }
}
