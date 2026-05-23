use super::*;

impl FrontendSurfaceComponent {
    pub(super) fn record_declared_missing_icon_diagnostics(&self) {
        let required = &self.compiled.manifest.icon_requirements.required;
        if required.is_empty() {
            return;
        }
        let module_id = self.compiled.manifest.package.id.as_str();
        for semantic_name in required {
            match mesh_core_icon::resolve_icon_for_module(module_id, semantic_name, 24) {
                mesh_core_icon::IconResolution::Found { .. } => {}
                mesh_core_icon::IconResolution::Missing { tried, .. } => {
                    self.record_missing_icon_diagnostic(semantic_name, tried);
                }
            }
        }
    }

    pub(super) fn record_missing_icon_diagnostic(
        &self,
        semantic_name: &str,
        tried: Vec<String>,
    ) -> bool {
        let Some(diagnostics) = &self.diagnostics else {
            return false;
        };
        diagnostics.record_missing_icon(semantic_name.to_string(), tried)
    }

    pub(super) fn record_focused_proof_diagnostic(
        &self,
        diagnostic: &mesh_core_render::FocusedProofDiagnostic,
    ) -> bool {
        let Some(diagnostics) = &self.diagnostics else {
            return false;
        };
        diagnostics.degraded(format!("focused renderer proof: {}", diagnostic.message));
        true
    }

    pub(super) fn record_keybind_diagnostic(&self, action_id: &str, reason: &str) -> bool {
        let Some(diagnostics) = &self.diagnostics else {
            return false;
        };
        diagnostics.degraded(format!(
            "keybind diagnostic: module_id='{}' surface_id='{}' action_id='{action_id}' reason='{reason}'",
            self.compiled.manifest.package.id,
            self.surface_id()
        ));
        true
    }
}
