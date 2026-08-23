use super::*;

pub(super) fn record_localized_miss(
    diagnostics: &Option<Diagnostics>,
    resolution: &mesh_core_locale::LocalizedTextResolution,
    field_path: Option<&str>,
) -> bool {
    let Some(key) = resolution.key.as_deref() else {
        return false;
    };
    let Some(diagnostics) = diagnostics else {
        return false;
    };
    let field = resolution
        .field_path
        .as_deref()
        .or(field_path)
        .unwrap_or("runtime");
    let subject = if resolution.field_path.is_some() || field_path.is_some() {
        "missing localized manifest text"
    } else {
        "missing localized text"
    };
    diagnostics.record_issue(
        format!("i18n-missing:{}:{key}", resolution.owner_module_id),
        mesh_core_diagnostics::IssueSeverity::Warning,
        format!(
            "{subject}: owner='{}' field_path='{field}' key='{key}' fallback='{}' source='{}' snapshot_revision={}",
            resolution.owner_module_id,
            resolution.fallback.as_deref().unwrap_or(""),
            resolution
                .source
                .as_ref()
                .map(|source| source.path.display().to_string())
                .unwrap_or_else(|| "missing".to_string()),
            resolution.snapshot_revision,
        ),
    )
}

impl FrontendSurfaceComponent {
    pub(super) fn record_child_surface_diagnostic(
        &self,
        diagnostic: &ChildSurfaceDiagnostic,
    ) -> bool {
        let Some(diagnostics) = &self.diagnostics else {
            return false;
        };
        let (issue_code, message) = match diagnostic {
            ChildSurfaceDiagnostic::Placement {
                node_key,
                diagnostic,
            } => (
                format!("popover-placement:{node_key}:{}", diagnostic.code()),
                format!("node '{node_key}': {diagnostic}"),
            ),
            ChildSurfaceDiagnostic::MissingTrigger {
                node_key,
                reference,
            } => (
                format!("popover-trigger:{node_key}"),
                format!(
                    "node '{node_key}' references missing popover trigger '{}'",
                    reference.reference
                ),
            ),
        };
        diagnostics.record_issue(
            issue_code,
            mesh_core_diagnostics::IssueSeverity::Error,
            message,
        )
    }

    pub(super) fn record_localized_miss(
        &self,
        resolution: &mesh_core_locale::LocalizedTextResolution,
        field_path: Option<&str>,
    ) -> bool {
        record_localized_miss(&self.diagnostics, resolution, field_path)
    }

    pub(super) fn record_declared_missing_icon_diagnostics(&self) {
        let required = &self.compiled.manifest.icon_requirements.required;
        let module_id = self.compiled.manifest.package.id.as_str();
        for semantic_name in required {
            match mesh_core_icon::resolve_icon_for_module(module_id, semantic_name, 24) {
                mesh_core_icon::IconResolution::Found { .. } => {}
                mesh_core_icon::IconResolution::Missing { tried, .. } => {
                    self.record_missing_icon_diagnostic(semantic_name, tried);
                }
            }
        }
        for semantic_name in &self.compiled.manifest.icon_requirements.optional {
            if let mesh_core_icon::IconResolution::Missing { tried, .. } =
                mesh_core_icon::resolve_icon_for_module(module_id, semantic_name, 24)
                && let Some(diagnostics) = &self.diagnostics
            {
                diagnostics.record_optional_missing_icon(semantic_name.clone(), tried);
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

    #[cfg(test)]
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
