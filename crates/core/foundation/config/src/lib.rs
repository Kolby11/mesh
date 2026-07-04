/// Configuration loading, validation, and schema support for MESH.
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Top-level MESH shell configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellConfig {
    #[serde(default)]
    pub shell: ShellSection,
    #[serde(default)]
    pub modules: HashMap<String, ModuleConfig>,
}

/// Global shell settings sourced from JSON files.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ShellSettings {
    #[serde(default)]
    pub theme: ThemeSettings,
    #[serde(default)]
    pub i18n: I18nSettings,
    #[serde(default)]
    pub sounds: ShellSounds,
    #[serde(default)]
    pub keyboard: KeyboardSettings,
    #[serde(default)]
    pub icons: IconSettings,
    #[serde(default)]
    pub tooltip: TooltipSettings,
    /// Per-module user-side overrides (icons, etc.). Keyed by module id.
    #[serde(default)]
    pub modules: HashMap<String, ModuleSettingsOverrides>,
}

/// Shell-wide icon configuration. `default_pack` is the user's preferred
/// icon-pack module id, implicitly prepended to every frontend's effective
/// icon-pack chain (unless the frontend opts out via
/// `icons.ignore_shell_default`).
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct IconSettings {
    #[serde(default)]
    pub default_pack: Option<String>,
}

/// Per-module user-side overrides applied on top of values declared in the
/// frontend's manifest.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ModuleSettingsOverrides {
    #[serde(default)]
    pub icons: Option<ModuleIconOverrides>,
}

/// User-side per-module icon overrides.
///
/// `use_packs`, when set, **replaces** the frontend's declared
/// `dependencies.icon_packs` for this module (the shell default is still
/// prepended unless the frontend opted out).
///
/// `overrides` is a flat map of logical-name → pack-qualified target
/// (`<pack-id>/<asset-name>`), prepended in front of every other
/// resolution path for matching names.
///
/// `ignore_shell_default`, when `true`, also suppresses the shell-default
/// pack from this module's effective chain.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ModuleIconOverrides {
    #[serde(default)]
    pub use_packs: Option<Vec<String>>,
    #[serde(default)]
    pub overrides: HashMap<String, String>,
    #[serde(default)]
    pub ignore_shell_default: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct KeyboardSettings {
    #[serde(default = "default_button_activation_keys")]
    pub button_activation_keys: Vec<String>,
    #[serde(default = "default_toggle_activation_keys")]
    pub toggle_activation_keys: Vec<String>,
    #[serde(default = "default_slider_decrement_keys")]
    pub slider_decrement_keys: Vec<String>,
    #[serde(default = "default_slider_increment_keys")]
    pub slider_increment_keys: Vec<String>,
    #[serde(default)]
    pub surface_shortcuts: HashMap<String, HashMap<String, SurfaceShortcutOverride>>,
}

impl Default for KeyboardSettings {
    fn default() -> Self {
        Self {
            button_activation_keys: default_button_activation_keys(),
            toggle_activation_keys: default_toggle_activation_keys(),
            slider_decrement_keys: default_slider_decrement_keys(),
            slider_increment_keys: default_slider_increment_keys(),
            surface_shortcuts: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct SurfaceShortcutOverride {
    #[serde(default)]
    pub key: Option<String>,
}

/// System sound file mappings for shell events.
///
/// Paths are absolute or relative to the shell's data directory.
/// The audio backend module plays these via its `play-sound` command.
/// Leave a field as `None` to silence that event.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ShellSounds {
    #[serde(default)]
    pub startup: Option<String>,
    #[serde(default)]
    pub shutdown: Option<String>,
    #[serde(default)]
    pub device_connected: Option<String>,
    #[serde(default)]
    pub device_disconnected: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub notification: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeSettings {
    #[serde(default = "default_theme_id")]
    pub active: String,
}

impl Default for ThemeSettings {
    fn default() -> Self {
        Self {
            active: default_theme_id(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct I18nSettings {
    #[serde(default = "default_locale")]
    pub locale: String,
    #[serde(default = "default_fallback_locale")]
    pub fallback_locale: String,
}

impl Default for I18nSettings {
    fn default() -> Self {
        Self {
            locale: default_locale(),
            fallback_locale: default_fallback_locale(),
        }
    }
}

/// Global tooltip behavior settings.
///
/// These control the default tooltip positioning and timing for all shell
/// components. Individual elements can override the positioning strategy via
/// the `tooltip-anchor` CSS property. The enter animation is not configured
/// here — it is authored in theme CSS (`tooltip { animation: ... }` plus a
/// theme-level `@keyframes` rule).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TooltipSettings {
    /// Default positioning strategy when the element does not specify
    /// `tooltip-anchor`. Accepted values: `"auto"`, `"bottom"`, `"top"`,
    /// `"left"`, `"right"`, `"cursor"`.
    ///
    /// Placement resolves in three steps: this shell-wide default, then the
    /// element's `tooltip-anchor` CSS preference on top, then automatic
    /// container-aware adjustment — if the chosen side would overflow the
    /// element's nearest clipping container (or the paint surface), the
    /// tooltip flips to the opposite side when it fits there.
    ///
    /// - `"auto"` picks whichever side has room, preferring below.
    /// - `"bottom"` / `"top"` / `"left"` / `"right"` place the tooltip at
    ///   the corresponding edge of the hovered element, flipping when needed.
    /// - `"cursor"` places the tooltip near the cursor position.
    #[serde(default = "default_tooltip_position")]
    pub position: String,

    /// Delay in milliseconds before the tooltip appears after hover starts.
    #[serde(default = "default_tooltip_delay_ms")]
    pub delay_ms: u64,

    /// Gap in pixels between the tooltip and the hovered element.
    #[serde(default = "default_tooltip_gap")]
    pub gap: f32,

    /// Horizontal offset from the cursor when using cursor positioning.
    #[serde(default = "default_tooltip_cursor_offset_x")]
    pub cursor_offset_x: f32,

    /// Vertical offset from the cursor when using cursor positioning.
    #[serde(default = "default_tooltip_cursor_offset_y")]
    pub cursor_offset_y: f32,
}

impl Default for TooltipSettings {
    fn default() -> Self {
        Self {
            position: default_tooltip_position(),
            delay_ms: default_tooltip_delay_ms(),
            gap: default_tooltip_gap(),
            cursor_offset_x: default_tooltip_cursor_offset_x(),
            cursor_offset_y: default_tooltip_cursor_offset_y(),
        }
    }
}

fn default_tooltip_position() -> String {
    "bottom".into()
}
fn default_tooltip_delay_ms() -> u64 {
    300
}
fn default_tooltip_gap() -> f32 {
    6.0
}
fn default_tooltip_cursor_offset_x() -> f32 {
    14.0
}
fn default_tooltip_cursor_offset_y() -> f32 {
    18.0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellSection {
    #[serde(default = "default_surface")]
    pub default_surface: String,
    #[serde(default)]
    pub discovery_paths: Vec<String>,
}

fn default_surface() -> String {
    "@mesh/launcher".to_string()
}

fn default_theme_id() -> String {
    "mesh-default-dark".to_string()
}

fn default_locale() -> String {
    "en".to_string()
}

fn default_fallback_locale() -> String {
    "en".to_string()
}

fn default_button_activation_keys() -> Vec<String> {
    vec!["Enter".into(), "Space".into()]
}

fn default_toggle_activation_keys() -> Vec<String> {
    vec!["Space".into(), "Enter".into()]
}

fn default_slider_decrement_keys() -> Vec<String> {
    vec!["ArrowLeft".into(), "ArrowDown".into()]
}

fn default_slider_increment_keys() -> Vec<String> {
    vec!["ArrowRight".into(), "ArrowUp".into()]
}

impl Default for ShellSection {
    fn default() -> Self {
        Self {
            default_surface: default_surface(),
            discovery_paths: Vec::new(),
        }
    }
}

pub fn resolve_discovery_paths(workspace_root: &Path, configured_paths: &[String]) -> Vec<PathBuf> {
    let mut resolved = if configured_paths.is_empty() {
        default_discovery_paths(workspace_root)
    } else {
        configured_paths
            .iter()
            .filter_map(|path| {
                let trimmed = path.trim();
                if trimmed.is_empty() {
                    return None;
                }
                let candidate = PathBuf::from(trimmed);
                Some(if candidate.is_absolute() {
                    candidate
                } else {
                    workspace_root.join(candidate)
                })
            })
            .collect::<Vec<_>>()
    };

    resolved.dedup();
    resolved
}

fn default_discovery_paths(workspace_root: &Path) -> Vec<PathBuf> {
    let mut paths = vec![workspace_root.join("modules")];

    let mesh_home_modules = mesh_home_path().join("modules");
    if mesh_home_modules != workspace_root.join("modules") {
        paths.push(mesh_home_modules);
    }

    let system_modules = PathBuf::from("/usr/share/mesh/modules");
    if system_modules != workspace_root.join("modules") {
        paths.push(system_modules);
    }

    paths
}

/// Per-module configuration values.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(flatten)]
    pub values: HashMap<String, toml::Value>,
}

fn default_true() -> bool {
    true
}

/// Schema definition for a module's settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettingsSchema {
    #[serde(flatten)]
    pub fields: HashMap<String, SchemaField>,
}

/// A single field in a settings schema.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaField {
    #[serde(rename = "type")]
    pub field_type: String,
    pub default: Option<toml::Value>,
    pub description: Option<String>,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub values: Option<Vec<String>>,
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("failed to read config file: {0}")]
    Io(#[from] std::io::Error),

    #[error("failed to parse config: {0}")]
    Parse(#[from] toml::de::Error),

    #[error("failed to parse json config: {0}")]
    Json(#[from] serde_json::Error),

    #[error("validation error: {0}")]
    Validation(String),
}

/// Load shell configuration from the standard path.
pub fn default_config_path() -> PathBuf {
    dirs_path("config").join("mesh/config.toml")
}

/// Load shell settings from the standard settings path, merging user settings over defaults.
pub fn load_shell_settings() -> Result<ShellSettings, ConfigError> {
    let defaults_path = default_settings_defaults_path();
    let settings_path = default_settings_path();

    let mut settings = if defaults_path.exists() {
        load_json_settings_file(&defaults_path)?
    } else {
        ShellSettings::default()
    };

    if settings_path.exists() {
        let user_settings = load_json_settings_file(&settings_path)?;
        merge_shell_settings(&mut settings, user_settings);
    }

    Ok(settings)
}

/// Standard user shell settings path.
pub fn default_settings_path() -> PathBuf {
    if let Ok(path) = std::env::var("MESH_SETTINGS_PATH") {
        if !path.trim().is_empty() {
            return PathBuf::from(path);
        }
    }

    let repo_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../..")
        .join("config/shell-settings.json");
    if repo_path.exists() {
        return repo_path;
    }

    mesh_home_path().join("settings.json")
}

/// Bundled default settings file path.
pub fn default_settings_defaults_path() -> PathBuf {
    if let Ok(path) = std::env::var("MESH_SETTINGS_DEFAULTS_PATH") {
        if !path.trim().is_empty() {
            return PathBuf::from(path);
        }
    }

    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../..")
        .join("config/settings-default.json")
}

/// Load shell configuration from a file.
pub fn load_config(path: &Path) -> Result<ShellConfig, ConfigError> {
    if !path.exists() {
        return Ok(ShellConfig {
            shell: ShellSection::default(),
            modules: HashMap::new(),
        });
    }
    let content = std::fs::read_to_string(path)?;
    let config: ShellConfig = toml::from_str(&content)?;
    Ok(config)
}

fn load_json_settings_file(path: &Path) -> Result<ShellSettings, ConfigError> {
    let content = std::fs::read_to_string(path)?;
    let settings: ShellSettings = serde_json::from_str(&content)?;
    Ok(settings)
}

fn merge_shell_settings(base: &mut ShellSettings, overrides: ShellSettings) {
    base.theme = overrides.theme;
    base.i18n = overrides.i18n;
    base.sounds = overrides.sounds;
    base.keyboard = overrides.keyboard;
    base.tooltip = overrides.tooltip;
    if overrides.icons != IconSettings::default() {
        base.icons = overrides.icons;
    }
    if !overrides.modules.is_empty() {
        for (id, module_overrides) in overrides.modules {
            base.modules.insert(id, module_overrides);
        }
    }
}

/// Write a per-module overrides file under XDG config (~/..../mesh/modules/<scope>/<name>.json).
pub fn module_override_path(module_id: &str) -> PathBuf {
    let mut parts = module_id.splitn(2, '/');
    let scope = parts.next().unwrap_or(module_id);
    let name = parts.next().unwrap_or("");

    let mut path = dirs_path("config").join("mesh").join("modules");
    if !name.is_empty() {
        path = path.join(scope).join(format!("{}.json", name));
    } else {
        // fallback: write as a single file named after the scope
        path = path.join(format!("{}.json", scope));
    }
    path
}

/// Persist per-module overrides atomically.
pub fn save_module_overrides(module_id: &str, overrides: &JsonValue) -> Result<(), ConfigError> {
    let path = module_override_path(module_id);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let tmp = path.with_extension("json.tmp");
    let content = serde_json::to_string_pretty(overrides)?;
    std::fs::write(&tmp, content)?;
    std::fs::rename(&tmp, &path)?;
    Ok(())
}

/// Remove a single key from a module overrides file. If the file becomes empty it is removed.
pub fn remove_module_override(module_id: &str, key: &str) -> Result<(), ConfigError> {
    let path = module_override_path(module_id);
    if !path.exists() {
        return Ok(());
    }

    let content = std::fs::read_to_string(&path)?;
    let mut doc: JsonValue = serde_json::from_str(&content)?;
    if let Some(obj) = doc.as_object_mut() {
        obj.remove(key);
        if obj.is_empty() {
            let _ = std::fs::remove_file(&path);
            return Ok(());
        }
    }

    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_string_pretty(&doc)?)?;
    std::fs::rename(&tmp, &path)?;
    Ok(())
}

/// Reset (remove) the per-module overrides file entirely.
pub fn reset_module_overrides(module_id: &str) -> Result<(), ConfigError> {
    let path = module_override_path(module_id);
    if path.exists() {
        std::fs::remove_file(path)?;
    }
    Ok(())
}

/// Validate a JSON value against a simple SettingsSchema. This performs basic
/// checks for type, enum values, and numeric min/max. It is intentionally
/// conservative: unknown keys are allowed (validation is per-field).
pub fn validate_module_settings(
    schema: &SettingsSchema,
    values: &JsonValue,
) -> Result<(), ConfigError> {
    for (key, field) in &schema.fields {
        if let Some(v) = values.get(key) {
            // check type
            match field.field_type.as_str() {
                "string" => {
                    if !v.is_string() {
                        return Err(ConfigError::Validation(format!("{} must be a string", key)));
                    }
                    if let Some(vals) = &field.values {
                        if let Some(s) = v.as_str() {
                            if !vals.contains(&s.to_string()) {
                                return Err(ConfigError::Validation(format!(
                                    "{}: invalid enum value",
                                    key
                                )));
                            }
                        }
                    }
                }
                "integer" => {
                    if !v.is_i64() && !v.is_u64() {
                        // JSON numbers are f64 by default; allow numeric but check integer-ness
                        if let Some(n) = v.as_f64() {
                            if n.fract() != 0.0 {
                                return Err(ConfigError::Validation(format!(
                                    "{} must be an integer",
                                    key
                                )));
                            }
                        } else {
                            return Err(ConfigError::Validation(format!(
                                "{} must be an integer",
                                key
                            )));
                        }
                    }
                    if let Some(min) = field.min {
                        if let Some(n) = v.as_f64() {
                            if n < min {
                                return Err(ConfigError::Validation(format!("{} below min", key)));
                            }
                        }
                    }
                    if let Some(max) = field.max {
                        if let Some(n) = v.as_f64() {
                            if n > max {
                                return Err(ConfigError::Validation(format!("{} above max", key)));
                            }
                        }
                    }
                }
                "float" => {
                    if !v.is_number() {
                        return Err(ConfigError::Validation(format!("{} must be a number", key)));
                    }
                    if let Some(min) = field.min {
                        if let Some(n) = v.as_f64() {
                            if n < min {
                                return Err(ConfigError::Validation(format!("{} below min", key)));
                            }
                        }
                    }
                    if let Some(max) = field.max {
                        if let Some(n) = v.as_f64() {
                            if n > max {
                                return Err(ConfigError::Validation(format!("{} above max", key)));
                            }
                        }
                    }
                }
                "boolean" => {
                    if !v.is_boolean() {
                        return Err(ConfigError::Validation(format!(
                            "{} must be a boolean",
                            key
                        )));
                    }
                }
                "enum" => {
                    if let Some(vals) = &field.values {
                        if !v.is_string() {
                            return Err(ConfigError::Validation(format!(
                                "{} must be an enum/string",
                                key
                            )));
                        }
                        if let Some(s) = v.as_str() {
                            if !vals.contains(&s.to_string()) {
                                return Err(ConfigError::Validation(format!(
                                    "{}: invalid enum value",
                                    key
                                )));
                            }
                        }
                    }
                }
                "array" => {
                    if !v.is_array() {
                        return Err(ConfigError::Validation(format!("{} must be an array", key)));
                    }
                }
                "object" => {
                    if !v.is_object() {
                        return Err(ConfigError::Validation(format!(
                            "{} must be an object",
                            key
                        )));
                    }
                }
                other => {
                    // Unknown types are ignored for now
                    tracing::debug!("unknown schema field type: {}", other);
                }
            }
        }
    }
    Ok(())
}

fn dirs_path(kind: &str) -> PathBuf {
    match kind {
        "config" => std::env::var("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
                PathBuf::from(home).join(".config")
            }),
        "data" => std::env::var("XDG_DATA_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
                PathBuf::from(home).join(".local/share")
            }),
        _ => PathBuf::from("/tmp"),
    }
}

fn mesh_home_path() -> PathBuf {
    if let Ok(path) = std::env::var("MESH_HOME") {
        if !path.trim().is_empty() {
            return PathBuf::from(path);
        }
    }

    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(home).join(".mesh")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_validate_module_settings_valid() {
        let mut fields = HashMap::new();
        fields.insert(
            "active".to_string(),
            SchemaField {
                field_type: "string".to_string(),
                default: None,
                description: None,
                min: None,
                max: None,
                values: Some(vec!["dark".to_string(), "light".to_string()]),
            },
        );
        fields.insert(
            "speed".to_string(),
            SchemaField {
                field_type: "integer".to_string(),
                default: None,
                description: None,
                min: Some(1.0),
                max: Some(10.0),
                values: None,
            },
        );

        let schema = SettingsSchema { fields };

        let valid_json = json!({
            "active": "dark",
            "speed": 5
        });

        assert!(validate_module_settings(&schema, &valid_json).is_ok());
    }

    #[test]
    fn test_validate_module_settings_invalid_enum() {
        let mut fields = HashMap::new();
        fields.insert(
            "active".to_string(),
            SchemaField {
                field_type: "string".to_string(),
                default: None,
                description: None,
                min: None,
                max: None,
                values: Some(vec!["dark".to_string(), "light".to_string()]),
            },
        );

        let schema = SettingsSchema { fields };

        let invalid_json = json!({
            "active": "neon"
        });

        assert!(validate_module_settings(&schema, &invalid_json).is_err());
    }

    #[test]
    fn test_validate_module_settings_invalid_type() {
        let mut fields = HashMap::new();
        fields.insert(
            "speed".to_string(),
            SchemaField {
                field_type: "integer".to_string(),
                default: None,
                description: None,
                min: Some(1.0),
                max: Some(10.0),
                values: None,
            },
        );

        let schema = SettingsSchema { fields };

        let invalid_json = json!({
            "speed": "fast"
        });

        assert!(validate_module_settings(&schema, &invalid_json).is_err());
    }

    #[test]
    fn test_validate_module_settings_out_of_bounds() {
        let mut fields = HashMap::new();
        fields.insert(
            "speed".to_string(),
            SchemaField {
                field_type: "integer".to_string(),
                default: None,
                description: None,
                min: Some(1.0),
                max: Some(10.0),
                values: None,
            },
        );

        let schema = SettingsSchema { fields };

        let invalid_json_low = json!({ "speed": 0 });
        let invalid_json_high = json!({ "speed": 11 });

        assert!(validate_module_settings(&schema, &invalid_json_low).is_err());
        assert!(validate_module_settings(&schema, &invalid_json_high).is_err());
    }

    #[test]
    fn test_module_override_path_formats() {
        let path1 = module_override_path("@mesh/system-panel");
        assert!(path1.ends_with("mesh/modules/@mesh/system-panel.json"));

        let path2 = module_override_path("generic-module");
        assert!(path2.ends_with("mesh/modules/generic-module.json"));
    }

    #[test]
    fn keyboard_settings_defaults_and_overrides_merge_deterministically() {
        let mut base = ShellSettings::default();
        let overrides = ShellSettings {
            keyboard: KeyboardSettings {
                surface_shortcuts: HashMap::from([(
                    "@mesh/navigation-bar".into(),
                    HashMap::from([(
                        "mute".into(),
                        SurfaceShortcutOverride {
                            key: Some("u".into()),
                        },
                    )]),
                )]),
                ..KeyboardSettings::default()
            },
            ..ShellSettings::default()
        };

        merge_shell_settings(&mut base, overrides);

        assert_eq!(
            base.keyboard.button_activation_keys,
            vec!["Enter".to_string(), "Space".to_string()]
        );
        assert_eq!(
            base.keyboard
                .surface_shortcuts
                .get("@mesh/navigation-bar")
                .and_then(|shortcuts| shortcuts.get("mute"))
                .and_then(|shortcut| shortcut.key.as_deref()),
            Some("u")
        );
    }

    #[test]
    fn keyboard_settings_default_shortcuts_remain_available_without_user_overrides() {
        let settings = ShellSettings::default();
        assert_eq!(
            settings.keyboard.toggle_activation_keys,
            vec!["Space".to_string(), "Enter".to_string()]
        );
        assert!(
            settings.keyboard.surface_shortcuts.is_empty(),
            "module-owned defaults should remain the fallback when shell overrides are absent"
        );
    }

    #[test]
    fn tooltip_settings_overrides_merge_into_shell_settings() {
        let mut base = ShellSettings::default();
        let overrides = ShellSettings {
            tooltip: TooltipSettings {
                position: "cursor".into(),
                delay_ms: 25,
                gap: 10.0,
                cursor_offset_x: 3.0,
                cursor_offset_y: 4.0,
            },
            ..ShellSettings::default()
        };

        merge_shell_settings(&mut base, overrides);

        assert_eq!(base.tooltip.position, "cursor");
        assert_eq!(base.tooltip.delay_ms, 25);
        assert_eq!(base.tooltip.gap, 10.0);
        assert_eq!(base.tooltip.cursor_offset_x, 3.0);
        assert_eq!(base.tooltip.cursor_offset_y, 4.0);
    }
}
