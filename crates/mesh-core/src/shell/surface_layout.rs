use std::path::Path;
use mesh_plugin::Manifest;
use mesh_theme::{ThemeEngine, default_theme, load_theme_from_path, theme_path_for_id};
use mesh_wayland::{Edge, KeyboardMode, Layer};

use super::types::ThemeWatchState;
use mesh_config::ShellSettings;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SurfaceSizePolicy {
    Fixed,
    ContentMeasured,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SurfaceLayoutSettings {
    pub(super) edge: Edge,
    pub(super) layer: Layer,
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) exclusive_zone: i32,
    pub(super) keyboard_mode: KeyboardMode,
    pub(super) visible_on_start: bool,
    pub(super) size_policy: SurfaceSizePolicy,
    pub(super) margin_top: i32,
    pub(super) margin_right: i32,
    pub(super) margin_bottom: i32,
    pub(super) margin_left: i32,
}

#[derive(Debug, Clone)]
pub(super) struct FrontendPluginSettingsState {
    pub(super) raw: serde_json::Value,
    pub(super) layout: SurfaceLayoutSettings,
}

pub(super) fn default_surface_visibility() -> bool {
    false
}

pub(super) fn generic_surface_layout_fallback() -> SurfaceLayoutSettings {
    SurfaceLayoutSettings {
        edge: Edge::Top,
        layer: Layer::Top,
        width: 480,
        height: 240,
        exclusive_zone: 0,
        keyboard_mode: KeyboardMode::None,
        visible_on_start: false,
        size_policy: SurfaceSizePolicy::Fixed,
        margin_top: 0,
        margin_right: 0,
        margin_bottom: 0,
        margin_left: 0,
    }
}

pub(super) fn surface_layout_from_manifest(manifest: &Manifest) -> SurfaceLayoutSettings {
    let mut layout = generic_surface_layout_fallback();

    let props = manifest
        .settings
        .as_ref()
        .and_then(|s| s.inline_schema.as_ref())
        .and_then(|schema| schema.pointer("/surface/properties"))
        .and_then(|v| v.as_object());

    if let Some(props) = props {
        if let Some(edge) = props
            .get("anchor")
            .and_then(|p| p.get("default"))
            .and_then(serde_json::Value::as_str)
            .and_then(parse_surface_edge)
        {
            layout.edge = edge;
        }
        if let Some(layer) = props
            .get("layer")
            .and_then(|p| p.get("default"))
            .and_then(serde_json::Value::as_str)
            .and_then(parse_surface_layer)
        {
            layout.layer = layer;
        }
        if let Some(width) = props
            .get("width")
            .and_then(|p| p.get("default"))
            .and_then(serde_json::Value::as_u64)
            .and_then(|v| u32::try_from(v).ok())
        {
            layout.width = width.max(1);
        }
        if let Some(height) = props
            .get("height")
            .and_then(|p| p.get("default"))
            .and_then(serde_json::Value::as_u64)
            .and_then(|v| u32::try_from(v).ok())
        {
            layout.height = height.max(1);
        }
        if let Some(zone) = props
            .get("exclusive_zone")
            .and_then(|p| p.get("default"))
            .and_then(serde_json::Value::as_i64)
            .and_then(|v| i32::try_from(v).ok())
        {
            layout.exclusive_zone = zone;
        }
        if let Some(mode) = props
            .get("keyboard_mode")
            .and_then(|p| p.get("default"))
            .and_then(serde_json::Value::as_str)
            .and_then(parse_keyboard_mode)
        {
            layout.keyboard_mode = mode;
        }
        if let Some(visible) = props
            .get("visible_on_start")
            .and_then(|p| p.get("default"))
            .and_then(serde_json::Value::as_bool)
        {
            layout.visible_on_start = visible;
        }
        if let Some(v) = props
            .get("margin_top")
            .and_then(|p| p.get("default"))
            .and_then(serde_json::Value::as_i64)
            .and_then(|v| i32::try_from(v).ok())
        {
            layout.margin_top = v;
        }
        if let Some(v) = props
            .get("margin_right")
            .and_then(|p| p.get("default"))
            .and_then(serde_json::Value::as_i64)
            .and_then(|v| i32::try_from(v).ok())
        {
            layout.margin_right = v;
        }
        if let Some(v) = props
            .get("margin_bottom")
            .and_then(|p| p.get("default"))
            .and_then(serde_json::Value::as_i64)
            .and_then(|v| i32::try_from(v).ok())
        {
            layout.margin_bottom = v;
        }
        if let Some(v) = props
            .get("margin_left")
            .and_then(|p| p.get("default"))
            .and_then(serde_json::Value::as_i64)
            .and_then(|v| i32::try_from(v).ok())
        {
            layout.margin_left = v;
        }
    }

    if let Some(sl) = &manifest.surface_layout {
        layout.size_policy = match sl.size_policy.as_deref() {
            Some("content_measured") => SurfaceSizePolicy::ContentMeasured,
            _ => SurfaceSizePolicy::Fixed,
        };
    }

    layout
}

pub(super) fn load_frontend_plugin_settings(
    settings_path: &Path,
    manifest: &Manifest,
) -> FrontendPluginSettingsState {
    let raw = match std::fs::read_to_string(settings_path) {
        Ok(content) => match serde_json::from_str::<serde_json::Value>(&content) {
            Ok(value) => value,
            Err(err) => {
                tracing::warn!(
                    "failed to parse frontend settings at {}: {}",
                    settings_path.display(),
                    err
                );
                serde_json::Value::Object(serde_json::Map::new())
            }
        },
        Err(_) => serde_json::Value::Object(serde_json::Map::new()),
    };

    let mut layout = surface_layout_from_manifest(manifest);
    let surface = raw.get("surface");

    if let Some(anchor) = surface
        .and_then(|value| value.get("anchor"))
        .and_then(serde_json::Value::as_str)
        .and_then(parse_surface_edge)
    {
        layout.edge = anchor;
    }

    if let Some(layer) = surface
        .and_then(|value| value.get("layer"))
        .and_then(serde_json::Value::as_str)
        .and_then(parse_surface_layer)
    {
        layout.layer = layer;
    }

    if let Some(width) = surface
        .and_then(|value| value.get("width"))
        .and_then(serde_json::Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
    {
        layout.width = width.max(1);
        layout.size_policy = SurfaceSizePolicy::Fixed;
    }

    if let Some(height) = surface
        .and_then(|value| value.get("height"))
        .and_then(serde_json::Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
    {
        layout.height = height.max(1);
        layout.size_policy = SurfaceSizePolicy::Fixed;
    }

    if let Some(zone) = surface
        .and_then(|value| value.get("exclusive_zone"))
        .and_then(serde_json::Value::as_i64)
        .and_then(|value| i32::try_from(value).ok())
    {
        layout.exclusive_zone = zone;
    }

    if let Some(mode) = surface
        .and_then(|value| value.get("keyboard_mode"))
        .and_then(serde_json::Value::as_str)
        .and_then(parse_keyboard_mode)
    {
        layout.keyboard_mode = mode;
    }

    if let Some(visible_on_start) = surface
        .and_then(|value| value.get("visible_on_start"))
        .and_then(serde_json::Value::as_bool)
    {
        layout.visible_on_start = visible_on_start;
    }

    if let Some(v) = surface
        .and_then(|value| value.get("margin_top"))
        .and_then(serde_json::Value::as_i64)
        .and_then(|v| i32::try_from(v).ok())
    {
        layout.margin_top = v;
    }
    if let Some(v) = surface
        .and_then(|value| value.get("margin_right"))
        .and_then(serde_json::Value::as_i64)
        .and_then(|v| i32::try_from(v).ok())
    {
        layout.margin_right = v;
    }
    if let Some(v) = surface
        .and_then(|value| value.get("margin_bottom"))
        .and_then(serde_json::Value::as_i64)
        .and_then(|v| i32::try_from(v).ok())
    {
        layout.margin_bottom = v;
    }
    if let Some(v) = surface
        .and_then(|value| value.get("margin_left"))
        .and_then(serde_json::Value::as_i64)
        .and_then(|v| i32::try_from(v).ok())
    {
        layout.margin_left = v;
    }

    FrontendPluginSettingsState { raw, layout }
}

pub(super) fn parse_surface_edge(value: &str) -> Option<Edge> {
    match value.trim().to_ascii_lowercase().as_str() {
        "top" => Some(Edge::Top),
        "bottom" => Some(Edge::Bottom),
        "left" => Some(Edge::Left),
        "right" => Some(Edge::Right),
        _ => None,
    }
}

pub(super) fn parse_surface_layer(value: &str) -> Option<Layer> {
    match value.trim().to_ascii_lowercase().as_str() {
        "background" => Some(Layer::Background),
        "bottom" => Some(Layer::Bottom),
        "top" => Some(Layer::Top),
        "overlay" => Some(Layer::Overlay),
        _ => None,
    }
}

pub(super) fn parse_keyboard_mode(value: &str) -> Option<KeyboardMode> {
    match value.trim().to_ascii_lowercase().as_str() {
        "none" => Some(KeyboardMode::None),
        "exclusive" => Some(KeyboardMode::Exclusive),
        "on_demand" | "ondemand" | "on-demand" => Some(KeyboardMode::OnDemand),
        _ => None,
    }
}

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
