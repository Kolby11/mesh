use super::types::CoreRequest;
use mesh_core_debug::{DEBUG_INTERFACE, DEBUG_SOURCE_MODULE_ID};
use mesh_core_service::InterfaceProvider;
use serde_json::Value;

/// A command implemented by a core-owned provider.
///
/// Core-owned providers still use the same interface and provider identity as
/// module-backed services. Only the final host action is kept in Rust; the
/// routing, capability, contract, and state paths remain generic.
#[derive(Debug, Clone, Copy)]
pub(super) struct CoreServiceCommand {
    pub(super) name: &'static str,
    translate: fn(&Value) -> Option<CoreRequest>,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct CoreServiceProvider {
    pub(super) interface: &'static str,
    pub(super) base_module: &'static str,
    pub(super) provider_module: &'static str,
    pub(super) backend_name: &'static str,
    pub(super) priority: u32,
    commands: &'static [CoreServiceCommand],
}

impl CoreServiceProvider {
    pub(super) fn interface_provider(self) -> InterfaceProvider {
        InterfaceProvider {
            interface: self.interface.to_string(),
            version: Some("1.0".to_string()),
            base_module: Some(self.base_module.to_string()),
            provider_module: self.provider_module.to_string(),
            backend_name: self.backend_name.to_string(),
            priority: self.priority,
        }
    }

    fn command(self, name: &str) -> Option<CoreServiceCommand> {
        self.commands
            .iter()
            .find(|command| command.name == name)
            .copied()
    }
}

/// Registry for host-backed providers that have no separate process. Keeping
/// their identities and command adapters in one registry prevents service
/// names from leaking into the generic dispatch path.
#[derive(Debug, Clone)]
pub(super) struct CoreServiceRegistry {
    providers: Vec<CoreServiceProvider>,
}

impl CoreServiceRegistry {
    pub(super) fn builtin() -> Self {
        Self {
            providers: vec![
                CoreServiceProvider {
                    interface: DEBUG_INTERFACE,
                    base_module: "@mesh/debug",
                    provider_module: DEBUG_SOURCE_MODULE_ID,
                    backend_name: "Shell",
                    priority: 100,
                    commands: &DEBUG_COMMANDS,
                },
                CoreServiceProvider {
                    interface: "mesh.theme",
                    base_module: "@mesh/theme-interface",
                    provider_module: "@mesh/shell",
                    backend_name: "Shell Theme",
                    priority: 200,
                    commands: &THEME_COMMANDS,
                },
                CoreServiceProvider {
                    interface: "mesh.locale",
                    base_module: "@mesh/locale-interface",
                    provider_module: "@mesh/shell",
                    backend_name: "Shell Locale",
                    priority: 200,
                    commands: &[],
                },
                CoreServiceProvider {
                    interface: "mesh.settings",
                    base_module: "@mesh/settings-interface",
                    provider_module: "@mesh/shell",
                    backend_name: "Shell Settings Store",
                    priority: 200,
                    commands: &[],
                },
                CoreServiceProvider {
                    interface: "mesh.packages",
                    base_module: "@mesh/packages-interface",
                    provider_module: "@mesh/shell",
                    backend_name: "Shell Package Graph",
                    priority: 200,
                    commands: &[],
                },
                CoreServiceProvider {
                    interface: "mesh.composition",
                    base_module: "@mesh/composition-interface",
                    provider_module: "@mesh/shell",
                    backend_name: "Shell Composition",
                    priority: 200,
                    commands: &[],
                },
            ],
        }
    }

    pub(super) fn providers(&self) -> impl Iterator<Item = &CoreServiceProvider> {
        self.providers.iter()
    }

    pub(super) fn provider_for(&self, interface: &str) -> Option<CoreServiceProvider> {
        self.providers
            .iter()
            .find(|provider| provider.interface == interface)
            .copied()
    }

    pub(super) fn provider_id(&self, interface: &str) -> Option<&'static str> {
        self.provider_for(interface)
            .map(|provider| provider.provider_module)
    }

    pub(super) fn is_provider(&self, interface: &str, provider_module: &str) -> bool {
        self.provider_for(interface)
            .is_some_and(|provider| provider.provider_module == provider_module)
    }

    pub(super) fn request(
        &self,
        interface: &str,
        command: &str,
        payload: &Value,
    ) -> Option<CoreRequest> {
        let provider = self.provider_for(interface)?;
        let command = provider.command(command)?;
        (command.translate)(payload)
    }
}

const DEBUG_COMMANDS: [CoreServiceCommand; 7] = [
    CoreServiceCommand {
        name: "toggle_overlay",
        translate: |_| Some(CoreRequest::ToggleDebugOverlay),
    },
    CoreServiceCommand {
        name: "toggle_layout_bounds",
        translate: |_| Some(CoreRequest::ToggleDebugLayoutBounds),
    },
    CoreServiceCommand {
        name: "toggle_element_picker",
        translate: |_| Some(CoreRequest::ToggleDebugElementPicker),
    },
    CoreServiceCommand {
        name: "open_source",
        translate: translate_open_debug_source,
    },
    CoreServiceCommand {
        name: "toggle_profiling",
        translate: |_| Some(CoreRequest::ToggleDebugProfiling),
    },
    CoreServiceCommand {
        name: "run_benchmark",
        translate: translate_run_debug_benchmark,
    },
    CoreServiceCommand {
        name: "cycle_tab",
        translate: |_| Some(CoreRequest::CycleDebugTab),
    },
];

const THEME_COMMANDS: [CoreServiceCommand; 3] = [
    CoreServiceCommand {
        name: "set_theme",
        translate: |payload| {
            Some(CoreRequest::SetTheme {
                theme_id: required_text(payload, "theme_id")?,
            })
        },
    },
    CoreServiceCommand {
        name: "set_icon_theme",
        translate: |payload| {
            Some(CoreRequest::SetIconTheme {
                theme_id: required_text(payload, "theme_id")?,
            })
        },
    },
    CoreServiceCommand {
        name: "set_font_family",
        translate: |payload| {
            Some(CoreRequest::SetFontFamily {
                family: required_text(payload, "family")?,
            })
        },
    },
];

fn required_text(payload: &Value, key: &str) -> Option<String> {
    payload
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
}

fn translate_open_debug_source(payload: &Value) -> Option<CoreRequest> {
    let path = required_text(payload, "path")?;
    let line = payload
        .get("line")
        .and_then(Value::as_u64)
        .unwrap_or(1)
        .clamp(1, u64::from(u32::MAX)) as u32;
    Some(CoreRequest::OpenDebugSource { path, line })
}

fn translate_run_debug_benchmark(payload: &Value) -> Option<CoreRequest> {
    Some(CoreRequest::RunDebugBenchmark {
        scenario_id: required_text(payload, "scenario_id")?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_registry_exposes_core_provider_metadata() {
        let registry = CoreServiceRegistry::builtin();

        assert!(registry.is_provider(DEBUG_INTERFACE, DEBUG_SOURCE_MODULE_ID));
        assert_eq!(registry.provider_id("mesh.theme"), Some("@mesh/shell"));
        assert_eq!(registry.provider_id("mesh.locale"), Some("@mesh/shell"));
        assert!(registry.is_provider("mesh.settings", "@mesh/shell"));
        assert_eq!(registry.providers().count(), 6);
    }

    #[test]
    fn builtin_registry_translates_debug_and_theme_commands() {
        let registry = CoreServiceRegistry::builtin();

        assert!(matches!(
            registry.request(DEBUG_INTERFACE, "toggle_profiling", &serde_json::json!({})),
            Some(CoreRequest::ToggleDebugProfiling)
        ));
        assert!(matches!(
            registry.request(
                DEBUG_INTERFACE,
                "open_source",
                &serde_json::json!({ "path": "/tmp/main.mesh", "line": 42 })
            ),
            Some(CoreRequest::OpenDebugSource { path, line })
                if path == "/tmp/main.mesh" && line == 42
        ));
        assert!(matches!(
            registry.request(
                "mesh.theme",
                "set_theme",
                &serde_json::json!({ "theme_id": "nord" })
            ),
            Some(CoreRequest::SetTheme { theme_id }) if theme_id == "nord"
        ));
        assert!(
            registry
                .request("mesh.locale", "set_locale", &serde_json::json!({}))
                .is_none()
        );
    }
}
