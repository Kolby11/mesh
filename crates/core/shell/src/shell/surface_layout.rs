#[cfg(test)]
pub(super) use mesh_core_surface_config::resolve_frontend_module_settings;
pub(super) use mesh_core_surface_config::{
    SurfaceLayoutSettings, default_surface_visibility, resolve_frontend_module_settings_with_props,
};

use mesh_core_config::ShellSettings;
use mesh_core_theme::{ThemeEngine, default_theme, load_theme_from_path, theme_path_for_id};

use super::types::ThemeWatchState;

pub(super) fn load_active_theme(settings: &ShellSettings) -> (ThemeEngine, ThemeWatchState) {
    let theme_path = theme_path_for_id(&settings.theme.active);
    let theme = match load_theme_from_path(&theme_path) {
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
