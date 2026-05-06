use super::*;

impl FrontendSurfaceComponent {
    pub(super) fn record_declared_missing_icon_diagnostics(&self) {
        let required = &self.compiled.manifest.icon_requirements.required;
        if required.is_empty() {
            return;
        }

        let Some(config) = self.load_icon_config_for_diagnostics() else {
            return;
        };
        let Ok(mut registry) = mesh_core_icon::IconRegistry::from_config(config) else {
            return;
        };

        for semantic_name in required {
            match registry.resolve(semantic_name, 24) {
                mesh_core_icon::IconResolution::Found { .. } => {}
                mesh_core_icon::IconResolution::Missing { tried, .. } => {
                    self.record_missing_icon_diagnostic(semantic_name, tried);
                }
            }
        }
    }

    pub(super) fn load_icon_config_for_diagnostics(&self) -> Option<mesh_core_icon::IconConfig> {
        let shell_manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let workspace_root = shell_manifest_dir.join("../../..");
        let config_path = workspace_root.join("config/icons.toml");

        if let Ok(input) = std::fs::read_to_string(&config_path) {
            if let Ok(mut config) = mesh_core_icon::IconConfig::from_toml_str(&input) {
                for pack in &mut config.packs {
                    if let Some(root) = &pack.root {
                        if root.is_relative() {
                            pack.root = Some(workspace_root.join(root));
                        }
                    }
                }
                if config.validate().is_ok() {
                    return Some(config);
                }
            }
        }

        mesh_core_icon::IconConfig::builtin_material(
            shell_manifest_dir.join("../ui/icon/assets/material"),
        )
        .ok()
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
}
