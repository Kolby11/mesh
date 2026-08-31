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

/// The complete theme state prepared from one graph/settings snapshot. The
/// engine contains both the selected active theme and the catalog presented to
/// consumers; the watch metadata is committed alongside it so a source change
/// cannot leave the renderer and watcher looking at different revisions.
#[derive(Debug, Clone)]
pub(super) struct PreparedThemeState {
    pub(super) engine: ThemeEngine,
    pub(super) watch: ThemeWatchState,
}

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

/// Prepare the graph-authorized theme catalog and selected snapshot together.
///
/// A descriptor is enough to advertise a theme in the catalog even when an
/// inactive mode is currently malformed: its source is opened and parsed only
/// when selected. The selected descriptor, however, must parse successfully
/// before this candidate can be committed. This keeps an editor's partial CSS
/// write from terminating the shell or replacing its last-known-good snapshot.
pub(super) fn prepare_theme_state_for_graph(
    settings: &ShellSettings,
    graph: &InstalledModuleGraph,
) -> Result<Option<PreparedThemeState>, ThemeError> {
    if graph.theme_catalog().is_empty() {
        return Ok(None);
    }

    let (active, watch) = prepare_theme_for_graph(settings, graph)?;
    let active_id = active.id.clone();
    let mut engine = ThemeEngine::new(active);

    for descriptor in graph.theme_catalog().iter() {
        if descriptor.id == active_id {
            continue;
        }
        let mode = descriptor
            .mode(&descriptor.default_mode)
            .expect("theme descriptor default mode was validated by the graph");
        let mut theme = load_theme_from_source(
            &mode.source,
            &descriptor.id,
            descriptor.label.as_deref().unwrap_or(&descriptor.local_id),
        )
        .unwrap_or_else(|_| {
            // Keep the graph identity discoverable while deferring an
            // inactive source's parse failure until the user selects it.
            Theme::new(
                descriptor.id.clone(),
                descriptor.label.as_deref().unwrap_or(&descriptor.local_id),
            )
        });
        theme.id = descriptor.id.clone();
        if let Some(label) = &descriptor.label {
            theme.name = label.clone();
        }
        theme.set_render_metadata(
            mode.name.clone(),
            mode.metadata.color_scheme.clone(),
            mode.metadata.contrast.clone(),
        );
        engine
            .register_theme(theme)
            .map_err(|error| ThemeError::Composition(error.to_string()))?;
    }

    Ok(Some(PreparedThemeState { engine, watch }))
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

    // Shipped theme CSS repeats these custom properties in every component's
    // defaults. Updating only the root token leaves those per-component
    // values (usually `Inter`) shadowing the selected global family.
    for defaults in theme.defaults_mut().components.values_mut() {
        replace_ui_font_defaults(defaults, family);
    }
    for module in theme.modules_mut().values_mut() {
        for defaults in module.defaults.components.values_mut() {
            replace_ui_font_defaults(defaults, family);
        }
    }
}

fn replace_ui_font_defaults(defaults: &mut mesh_core_theme::ComponentDefaults, family: &str) {
    for property in [
        "--typography-family",
        "--typography-family-brand",
        "--typography-family-plain",
    ] {
        if defaults.contains_key(property) {
            defaults.insert(property.into(), family.into());
        }
    }
}
