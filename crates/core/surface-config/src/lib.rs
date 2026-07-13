use mesh_core_module::Manifest;
use mesh_core_wayland::{Edge, KeyboardMode, Layer};
use std::collections::BTreeMap;
use std::path::Path;

/// Surface **placement**, resolved from the manifest and user settings.
///
/// Sizing is no longer part of this struct: every surface is sized by CSS
/// content measurement of its component root (`width`/`height`/`min-*`/`max-*`
/// on the surface root, resolved by the layout engine) and the show/hide
/// transition is a CSS `transition` on that root. See
/// `docs/spec/03-components.md` §2.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SurfaceLayoutSettings {
    pub edge: Edge,
    pub layer: Layer,
    pub exclusive_zone: i32,
    pub keyboard_mode: KeyboardMode,
    pub visible_on_start: bool,
    pub margin_top: i32,
    pub margin_right: i32,
    pub margin_bottom: i32,
    pub margin_left: i32,
}

#[derive(Debug, Clone)]
pub struct FrontendModuleSettingsState {
    pub raw: serde_json::Value,
    pub layout: SurfaceLayoutSettings,
    pub props: FrontendModulePropSettings,
}

/// User prop overrides loaded from `config/settings.json`.
///
/// Shape:
/// `{ "props": { "global": { ... }, "instances": { "<instance_key>": { ... } } } }`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FrontendModulePropSettings {
    pub global: BTreeMap<String, serde_json::Value>,
    pub instances: BTreeMap<String, BTreeMap<String, serde_json::Value>>,
}

pub fn default_surface_visibility() -> bool {
    false
}

pub fn generic_surface_layout_fallback() -> SurfaceLayoutSettings {
    SurfaceLayoutSettings {
        edge: Edge::Top,
        layer: Layer::Top,
        exclusive_zone: 0,
        keyboard_mode: KeyboardMode::None,
        visible_on_start: false,
        margin_top: 0,
        margin_right: 0,
        margin_bottom: 0,
        margin_left: 0,
    }
}

/// Resolve a surface's baseline layout from its manifest.
///
/// Core owns the canonical defaults (`generic_surface_layout_fallback`). The
/// module's compact `mesh.surface` block (normalized into `surface_layout`)
/// overrides only the fields it declares. User overrides from
/// `config/settings.json` are applied on top of this in
/// `load_frontend_module_settings`.
pub fn surface_layout_from_manifest(manifest: &Manifest) -> SurfaceLayoutSettings {
    let mut layout = generic_surface_layout_fallback();

    let Some(surface) = &manifest.surface_layout else {
        return layout;
    };

    if let Some(edge) = surface.anchor.as_deref().and_then(parse_surface_edge) {
        layout.edge = edge;
    }
    if let Some(layer) = surface.layer.as_deref().and_then(parse_surface_layer) {
        layout.layer = layer;
    }
    if let Some(zone) = surface.exclusive_zone {
        layout.exclusive_zone = zone;
    }
    if let Some(mode) = surface
        .keyboard_mode
        .as_deref()
        .and_then(parse_keyboard_mode)
    {
        layout.keyboard_mode = mode;
    }
    if let Some(visible) = surface.visible_on_start {
        layout.visible_on_start = visible;
    }
    if let Some(margins) = &surface.margins {
        layout.margin_top = margins.top;
        layout.margin_right = margins.right;
        layout.margin_bottom = margins.bottom;
        layout.margin_left = margins.left;
    }

    layout
}

pub fn load_frontend_module_settings(
    settings_path: &Path,
    manifest: &Manifest,
) -> FrontendModuleSettingsState {
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

    let props = load_prop_settings(&raw);

    FrontendModuleSettingsState { raw, layout, props }
}

fn load_prop_settings(raw: &serde_json::Value) -> FrontendModulePropSettings {
    let mut settings = FrontendModulePropSettings::default();
    let Some(props) = raw.get("props").and_then(serde_json::Value::as_object) else {
        return settings;
    };
    if let Some(global) = props.get("global").and_then(serde_json::Value::as_object) {
        settings.global = global
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();
    }
    if let Some(instances) = props
        .get("instances")
        .and_then(serde_json::Value::as_object)
    {
        for (instance_key, values) in instances {
            let Some(values) = values.as_object() else {
                continue;
            };
            settings.instances.insert(
                instance_key.clone(),
                values
                    .iter()
                    .map(|(key, value)| (key.clone(), value.clone()))
                    .collect(),
            );
        }
    }
    settings
}

pub fn parse_surface_edge(value: &str) -> Option<Edge> {
    match value.trim().to_ascii_lowercase().as_str() {
        "top" => Some(Edge::Top),
        "bottom" => Some(Edge::Bottom),
        "left" => Some(Edge::Left),
        "right" => Some(Edge::Right),
        _ => None,
    }
}

pub fn parse_surface_layer(value: &str) -> Option<Layer> {
    match value.trim().to_ascii_lowercase().as_str() {
        "background" => Some(Layer::Background),
        "bottom" => Some(Layer::Bottom),
        "top" => Some(Layer::Top),
        "overlay" => Some(Layer::Overlay),
        _ => None,
    }
}

pub fn parse_keyboard_mode(value: &str) -> Option<KeyboardMode> {
    match value.trim().to_ascii_lowercase().as_str() {
        "none" => Some(KeyboardMode::None),
        "exclusive" => Some(KeyboardMode::Exclusive),
        "on_demand" | "ondemand" | "on-demand" => Some(KeyboardMode::OnDemand),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mesh_core_module::manifest::{Manifest, ModuleSection, ModuleType, SurfaceLayoutSection};
    use std::collections::HashMap;
    use std::fs;

    fn manifest_with_surface_layout(surface_layout: SurfaceLayoutSection) -> Manifest {
        Manifest {
            package: ModuleSection {
                id: "@mesh/test".into(),
                name: None,
                version: "0.1.0".into(),
                module_type: ModuleType::Surface,
                api_version: "0.1".into(),
                license: None,
                description: None,
                authors: Vec::new(),
                repository: None,
            },
            compatibility: Default::default(),
            dependencies: Default::default(),
            capabilities: Default::default(),
            entrypoints: Default::default(),
            accessibility: None,
            keybinds: Default::default(),
            i18n: None,
            theme: None,
            service: None,
            provides: Vec::new(),
            interface: None,
            interfaces: Vec::new(),
            extensions: Vec::new(),
            exports: Default::default(),
            provides_slots: HashMap::new(),
            slot_contributions: HashMap::new(),
            assets: None,
            icons: None,
            icon_pack: None,
            icon_requirements: Default::default(),
            translations: HashMap::new(),
            surface_layout: Some(surface_layout),
        }
    }

    #[test]
    fn manifest_surface_layout_sets_keyboard_mode_default() {
        let manifest = manifest_with_surface_layout(SurfaceLayoutSection {
            keyboard_mode: Some("on_demand".into()),
            ..Default::default()
        });

        let layout = surface_layout_from_manifest(&manifest);

        assert_eq!(layout.keyboard_mode, KeyboardMode::OnDemand);
    }

    #[test]
    fn user_settings_override_manifest_keyboard_mode_default() {
        let manifest = manifest_with_surface_layout(SurfaceLayoutSection {
            keyboard_mode: Some("on_demand".into()),
            ..Default::default()
        });
        let raw = serde_json::json!({
            "surface": {
                "keyboard_mode": "exclusive"
            }
        });
        let path = std::env::temp_dir().join(format!(
            "mesh-surface-config-test-{}-settings.json",
            std::process::id()
        ));
        fs::write(&path, raw.to_string()).expect("write test settings");

        let settings = load_frontend_module_settings(&path, &manifest);
        fs::remove_file(&path).ok();

        assert_eq!(settings.layout.keyboard_mode, KeyboardMode::Exclusive);
    }

    #[test]
    fn load_frontend_module_settings_reads_prop_scopes() {
        let manifest = manifest_with_surface_layout(SurfaceLayoutSection::default());
        let raw = serde_json::json!({
            "props": {
                "global": { "track_width": "24px", "anim_ms": 90 },
                "instances": {
                    "@mesh/navigation-bar/import:audio": { "track_width": "28px" }
                }
            }
        });
        let path = std::env::temp_dir().join(format!(
            "mesh-surface-config-test-{}-props-settings.json",
            std::process::id()
        ));
        fs::write(&path, raw.to_string()).expect("write test settings");

        let settings = load_frontend_module_settings(&path, &manifest);
        fs::remove_file(&path).ok();

        assert_eq!(
            settings.props.global.get("track_width"),
            Some(&serde_json::json!("24px"))
        );
        assert_eq!(
            settings
                .props
                .instances
                .get("@mesh/navigation-bar/import:audio")
                .and_then(|props| props.get("track_width")),
            Some(&serde_json::json!("28px"))
        );
    }

    #[test]
    fn compact_surface_block_resolves_editable_defaults() {
        let manifest = manifest_with_surface_layout(SurfaceLayoutSection {
            anchor: Some("bottom".into()),
            layer: Some("overlay".into()),
            exclusive_zone: Some(48),
            visible_on_start: Some(true),
            keyboard_mode: Some("none".into()),
            ..Default::default()
        });

        let layout = surface_layout_from_manifest(&manifest);

        assert_eq!(layout.edge, Edge::Bottom);
        assert_eq!(layout.layer, Layer::Overlay);
        assert_eq!(layout.exclusive_zone, 48);
        assert!(layout.visible_on_start);
        assert_eq!(layout.keyboard_mode, KeyboardMode::None);
    }

    #[test]
    fn unset_surface_layout_uses_core_defaults() {
        let manifest = manifest_with_surface_layout(SurfaceLayoutSection::default());
        let layout = surface_layout_from_manifest(&manifest);
        assert_eq!(layout, generic_surface_layout_fallback());
    }
}
