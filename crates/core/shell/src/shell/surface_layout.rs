#[cfg(test)]
pub(super) use mesh_core_surface_config::resolve_frontend_module_settings;
pub(super) use mesh_core_surface_config::{
    SurfaceLayoutSettings, default_surface_visibility, resolve_frontend_module_settings_with_props,
};

use mesh_core_config::ShellSettings;
use mesh_core_module::package::InstalledModuleGraph;
use mesh_core_theme::{
    Theme, ThemeEngine, ThemeError, TokenValue, default_theme, load_theme_from_path,
    load_theme_from_source, theme_path_for_id,
};

use super::types::ThemeWatchState;

pub(super) fn load_active_theme(settings: &ShellSettings) -> (ThemeEngine, ThemeWatchState) {
    let theme_path = theme_path_for_id(&settings.theme.active);
    let mut theme = match load_theme_from_path(&theme_path) {
        Ok(theme) => theme,
        Err(err) => {
            tracing::warn!(
                "failed to load requested theme '{}' from {}: {err}; using default theme",
                settings.theme.active,
                theme_path.display()
            );
            default_theme()
        }
    };
    apply_font_family(&mut theme, settings.fonts.ui_family.as_deref());
    let modified_at = std::fs::metadata(&theme_path)
        .ok()
        .and_then(|metadata| metadata.modified().ok());

    (
        ThemeEngine::new(theme),
        ThemeWatchState {
            path: theme_path,
            modified_at,
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
    let mut theme = load_theme_from_source(descriptor.default_source())?;
    theme.id = descriptor.id.clone();
    if let Some(label) = &descriptor.label {
        theme.name = label.clone();
    }
    apply_font_family(&mut theme, settings.fonts.ui_family.as_deref());
    let path = descriptor.default_source().candidate_path();
    let modified_at = std::fs::metadata(&path)
        .ok()
        .and_then(|metadata| metadata.modified().ok());
    Ok((theme, ThemeWatchState { path, modified_at }))
}

pub(super) fn apply_font_family(theme: &mut Theme, family: Option<&str>) {
    let Some(family) = family.map(str::trim).filter(|family| !family.is_empty()) else {
        return;
    };
    let tokens = theme.tokens_mut();
    for token in [
        "typography.family",
        "typography.family.brand",
        "typography.family.plain",
    ] {
        tokens.insert(token.into(), TokenValue::String(family.into()));
    }
}
