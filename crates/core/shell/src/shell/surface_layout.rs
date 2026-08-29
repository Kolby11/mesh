#[cfg(test)]
pub(super) use mesh_core_surface_config::resolve_frontend_module_settings;
pub(super) use mesh_core_surface_config::{
    SurfaceLayoutSettings, default_surface_visibility, resolve_frontend_module_settings_with_props,
};

use mesh_core_config::ShellSettings;
use mesh_core_module::package::{InstalledModuleGraph, ModuleKind};
use mesh_core_theme::{
    Theme, ThemeDefaults, ThemeEngine, ThemeError, ThemeModule, ThemeModuleLayer, ThemeProvenance,
    TokenValue, default_theme, load_theme_from_source,
};
use std::collections::HashMap;
use std::path::PathBuf;

use super::types::ThemeWatchState;

pub(super) fn default_theme_state(settings: &ShellSettings) -> (ThemeEngine, ThemeWatchState) {
    let mut theme = default_theme();
    apply_font_family(&mut theme, settings.fonts.ui_family.as_deref());
    let revision = theme.revision();

    (
        ThemeEngine::new(theme),
        ThemeWatchState {
            path: PathBuf::new(),
            modified_at: None,
            fingerprint: None,
            mode: None,
            revision,
        },
    )
}

pub(super) fn prepare_theme_for_graph(
    settings: &ShellSettings,
    graph: &InstalledModuleGraph,
) -> Result<(Theme, ThemeWatchState), ThemeError> {
    let descriptor = graph
        .theme_catalog()
        .get(&settings.theme.active)
        .ok_or_else(|| ThemeError::NotFound(settings.theme.active.clone()))?;
    let mode = selected_theme_mode(settings, descriptor)?;
    let mode_descriptor = descriptor.mode(&mode).ok_or_else(|| {
        ThemeError::Composition(format!("theme '{}' has no mode '{mode}'", descriptor.id))
    })?;
    let pack = load_theme_from_source(
        &mode_descriptor.source,
        &descriptor.id,
        descriptor.label.as_deref().unwrap_or(&descriptor.local_id),
    )?;
    let base = default_theme();
    let module_layers = graph
        .modules_by_kind(ModuleKind::Frontend)
        .into_iter()
        .filter_map(|module| {
            let section = module.manifest.mesh.theme.as_ref()?;
            Some(ThemeModuleLayer {
                module_id: module.id.clone(),
                module: ThemeModule {
                    tokens: section.tokens.clone(),
                    defaults: ThemeDefaults {
                        components: section
                            .defaults
                            .components
                            .iter()
                            .map(|(component, declarations)| {
                                (
                                    component.clone(),
                                    declarations
                                        .iter()
                                        .map(|(property, value)| (property.clone(), value.clone()))
                                        .collect(),
                                )
                            })
                            .collect(),
                    },
                    rules: Vec::new(),
                },
            })
        });
    let user_overrides = settings
        .theme
        .tokens
        .iter()
        .map(|(name, value)| {
            let token = match value {
                serde_json::Value::String(value) => TokenValue::String(value.clone()),
                serde_json::Value::Bool(value) => TokenValue::Bool(*value),
                serde_json::Value::Number(value) => value
                    .as_f64()
                    .filter(|value| value.is_finite())
                    .map(TokenValue::Number)
                    .ok_or_else(|| {
                        ThemeError::Composition(format!(
                            "user theme token '{name}' must contain a finite number"
                        ))
                    })?,
                _ => {
                    return Err(ThemeError::Composition(format!(
                        "user theme token '{name}' must be a string, number, or boolean"
                    )));
                }
            };
            Ok((name.clone(), token))
        })
        .collect::<Result<HashMap<_, _>, _>>()?;
    let mut theme = Theme::compose_layers(
        &base,
        &pack,
        descriptor.id.clone(),
        mode.clone(),
        module_layers,
        &user_overrides,
    )?;
    theme.set_render_metadata(
        mode.clone(),
        mode_descriptor.metadata.color_scheme.clone(),
        mode_descriptor.metadata.contrast.clone(),
    );
    theme.id = descriptor.id.clone();
    if let Some(label) = &descriptor.label {
        theme.name = label.clone();
    }
    apply_font_family(&mut theme, settings.fonts.ui_family.as_deref());
    let path = mode_descriptor.source.candidate_path();
    let modified_at = std::fs::metadata(&path)
        .ok()
        .and_then(|metadata| metadata.modified().ok());
    let fingerprint = mode_descriptor
        .source
        .fingerprint()
        .map_err(|error| ThemeError::Source {
            path: mode_descriptor.source.candidate_path(),
            message: error.to_string(),
        })?;
    let revision = theme.revision();
    Ok((
        theme,
        ThemeWatchState {
            path,
            modified_at,
            fingerprint: Some(fingerprint),
            mode: Some(mode),
            revision,
        },
    ))
}

pub(super) fn selected_theme_mode(
    settings: &ShellSettings,
    descriptor: &mesh_core_theme::ThemePackDescriptor,
) -> Result<String, ThemeError> {
    let system_color_scheme = std::env::var("MESH_SYSTEM_COLOR_SCHEME").ok();
    let local_minute = std::env::var("MESH_LOCAL_MINUTE")
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or_else(mesh_core_theme::local_minutes_since_midnight);
    settings
        .theme
        .mode_policy
        .select_mode(
            &descriptor.modes,
            &descriptor.default_mode,
            settings.theme.mode.as_deref(),
            system_color_scheme.as_deref(),
            local_minute,
        )
        .map_err(ThemeError::Composition)
}

pub(super) fn apply_font_family(theme: &mut Theme, family: Option<&str>) {
    let Some(family) = family.map(str::trim).filter(|family| !family.is_empty()) else {
        return;
    };
    for token in [
        "typography.family",
        "typography.family.brand",
        "typography.family.plain",
    ] {
        theme.set_token(
            token,
            TokenValue::String(family.into()),
            ThemeProvenance::UserOverride,
        );
    }
}
