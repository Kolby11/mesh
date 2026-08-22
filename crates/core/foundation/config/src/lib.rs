//! Shell configuration loading and validation.

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub use mesh_core_theme::ThemeModePolicy;

pub mod settings;
pub mod validate;

pub use settings::{
    SETTINGS_SCHEMA_VERSION, SHELL_NAMESPACE, SettingsNamespaceSchema, SettingsSchemaError,
    SettingsSchemaRegistry, SettingsStore, default_settings_path, load_shell_settings, merge_json,
};
pub use validate::{
    FieldKind, FieldSpec, SettingsDiagnostic, SettingsDiagnosticSeverity, log_settings_diagnostics,
    new_settings_diagnostics, validate_json_schema, validate_object,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellConfig {
    #[serde(default)]
    pub shell: ShellSection,
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
    pub fonts: FontSettings,
    #[serde(default)]
    pub tooltip: TooltipSettings,
    #[serde(default)]
    pub render: RenderSettings,
}

/// The `"shell"` namespace's schema. One entry per [`ShellSettings`] field:
/// serde defines what a valid value falls back to, this defines what is valid.
/// A field added without a line here is reported as an unknown key.
pub const SHELL_SETTINGS_FIELDS: &[FieldSpec] = &[
    FieldSpec::new(
        "theme",
        FieldKind::Section(&[
            FieldSpec::new("active", FieldKind::Str),
            FieldSpec::new("mode", FieldKind::Str),
            FieldSpec::new("mode_policy", FieldKind::Opaque),
            FieldSpec::new("tokens", FieldKind::Map(&THEME_TOKEN_VALUE)),
        ]),
    ),
    FieldSpec::new(
        "i18n",
        FieldKind::Section(&[
            FieldSpec::new(
                "policy",
                FieldKind::Enum {
                    canonicalize: canonical_locale_policy,
                    values: LOCALE_POLICIES,
                },
            ),
            FieldSpec::new("locale", FieldKind::Str),
            FieldSpec::new("fallback_locale", FieldKind::Str),
        ]),
    ),
    FieldSpec::new(
        "sounds",
        FieldKind::Section(&[
            FieldSpec::new("startup", FieldKind::Str),
            FieldSpec::new("shutdown", FieldKind::Str),
            FieldSpec::new("device_connected", FieldKind::Str),
            FieldSpec::new("device_disconnected", FieldKind::Str),
            FieldSpec::new("error", FieldKind::Str),
            FieldSpec::new("notification", FieldKind::Str),
        ]),
    ),
    FieldSpec::new(
        "keyboard",
        FieldKind::Section(&[
            FieldSpec::new("button_activation_keys", FieldKind::StrArray),
            FieldSpec::new("toggle_activation_keys", FieldKind::StrArray),
            FieldSpec::new("slider_decrement_keys", FieldKind::StrArray),
            FieldSpec::new("slider_increment_keys", FieldKind::StrArray),
            // module id -> action name -> { key }; both levels are the user's
            // own vocabulary, so only the leaf shape is checked.
            FieldSpec::new(
                "surface_shortcuts",
                FieldKind::Map(&FieldKind::Map(&FieldKind::Section(&[FieldSpec::new(
                    "key",
                    FieldKind::Str,
                )]))),
            ),
        ]),
    ),
    FieldSpec::new(
        "icons",
        FieldKind::Section(&[FieldSpec::new("default_pack", FieldKind::Str)]),
    ),
    FieldSpec::new(
        "fonts",
        FieldKind::Section(&[
            FieldSpec::new("ui_family", FieldKind::Str),
            FieldSpec::new("packs", FieldKind::StrArray),
        ]),
    ),
    FieldSpec::new(
        "render",
        FieldKind::Section(&[FieldSpec::new(
            "blur",
            FieldKind::Section(&[
                FieldSpec::new("passes", FieldKind::UIntRange { min: 1, max: 3 }),
                FieldSpec::new(
                    "max_radius",
                    FieldKind::FloatRange {
                        min: 0.0,
                        max: None,
                    },
                ),
            ]),
        )]),
    ),
    FieldSpec::new(
        "tooltip",
        FieldKind::Section(&[
            FieldSpec::new(
                "position",
                FieldKind::Enum {
                    canonicalize: canonical_tooltip_position,
                    values: TOOLTIP_POSITIONS,
                },
            ),
            FieldSpec::new("delay_ms", FieldKind::UInt),
            FieldSpec::new("gap", FieldKind::Float),
            FieldSpec::new("cursor_offset_x", FieldKind::Float),
            FieldSpec::new("cursor_offset_y", FieldKind::Float),
        ]),
    ),
];

const THEME_TOKEN_VALUE: FieldKind = FieldKind::Token;

/// Accepted values for [`TooltipSettings::position`]. A `mesh-core-shell` test
/// walks this list to keep anchor resolution from drifting apart from it.
pub const TOOLTIP_POSITIONS: &[&str] = &["auto", "bottom", "top", "left", "right", "cursor"];

fn canonical_tooltip_position(value: &str) -> Option<&'static str> {
    let value = value.trim();
    TOOLTIP_POSITIONS
        .iter()
        .copied()
        .find(|candidate| *candidate == value)
}

/// `default_pack` is prepended to every frontend's effective icon-pack chain
/// unless the frontend sets `icons.ignore_shell_default`.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct IconSettings {
    #[serde(default)]
    pub default_pack: Option<String>,
}

/// The system font family used for shell typography. Individual components
/// can still select another family or a semantic font-pack role explicitly.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct FontSettings {
    #[serde(default)]
    pub ui_family: Option<String>,
    #[serde(default)]
    pub packs: Vec<String>,
}

/// User overrides layered on the frontend manifest, read from the module's own
/// namespace in the settings store rather than from a shell-wide map.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ModuleSettingsOverrides {
    #[serde(default)]
    pub icons: Option<ModuleIconOverrides>,
    #[serde(default)]
    pub fonts: Option<ModuleFontOverrides>,
}

impl ModuleSettingsOverrides {
    /// Unknown keys are ignored: each consumer reads only what it owns.
    pub fn from_namespace(namespace: &JsonValue) -> Self {
        let icons = namespace
            .get("icons")
            .and_then(|icons| serde_json::from_value(icons.clone()).ok());
        let fonts = namespace
            .get("fonts")
            .and_then(|fonts| serde_json::from_value(fonts.clone()).ok());
        Self { icons, fonts }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ModuleFontOverrides {
    /// Replaces the frontend's declared `mesh.uses.resources.fonts` chain.
    #[serde(default)]
    pub use_packs: Option<Vec<String>>,
    /// Logical role or family reference → another role/family reference.
    #[serde(default)]
    pub overrides: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ModuleIconOverrides {
    /// Replaces the frontend's declared `dependencies.icon_packs`.
    #[serde(default)]
    pub use_packs: Option<Vec<String>>,
    /// Logical name → `<pack-id>/<asset-name>`, tried before every other
    /// resolution path for matching names.
    #[serde(default)]
    pub overrides: HashMap<String, String>,
    /// Also drop the shell-default pack from this module's chain.
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

/// Sound files for shell events, absolute or relative to the data directory,
/// played through `mesh.audio.play_sound`. `None` silences the event.
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThemeSettings {
    #[serde(default = "default_theme_id")]
    pub active: String,
    #[serde(default)]
    pub mode: Option<String>,
    #[serde(default)]
    pub mode_policy: ThemeModePolicy,
    #[serde(default)]
    pub tokens: HashMap<String, JsonValue>,
}

impl Default for ThemeSettings {
    fn default() -> Self {
        Self {
            active: default_theme_id(),
            mode: None,
            mode_policy: ThemeModePolicy::Manual,
            tokens: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocalePolicy {
    #[serde(rename = "manual")]
    Manual,
    #[serde(rename = "follow_system")]
    FollowSystem,
}

impl Default for LocalePolicy {
    fn default() -> Self {
        Self::Manual
    }
}

impl LocalePolicy {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::FollowSystem => "follow_system",
        }
    }
}

pub const LOCALE_POLICIES: &[&str] = &["manual", "follow_system"];

fn canonical_locale_policy(value: &str) -> Option<&'static str> {
    LOCALE_POLICIES
        .iter()
        .copied()
        .find(|candidate| *candidate == value.trim())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct I18nSettings {
    #[serde(default)]
    pub policy: LocalePolicy,
    #[serde(default = "default_locale")]
    pub locale: String,
    #[serde(default = "default_fallback_locale")]
    pub fallback_locale: String,
}

impl Default for I18nSettings {
    fn default() -> Self {
        Self {
            policy: LocalePolicy::Manual,
            locale: default_locale(),
            fallback_locale: default_fallback_locale(),
        }
    }
}

/// Resolve host-dependent locale policy for runtime consumers without
/// changing the sparse durable settings document. A missing or POSIX host
/// locale falls back to the declared locale default, while the policy itself
/// remains `follow_system` for later re-evaluation.
pub fn resolve_shell_locale_settings(settings: &ShellSettings) -> ShellSettings {
    resolve_shell_locale_settings_with_host_locale(settings, mesh_core_locale::system_locale())
}

fn resolve_shell_locale_settings_with_host_locale(
    settings: &ShellSettings,
    host_locale: Option<String>,
) -> ShellSettings {
    let mut resolved = settings.clone();
    if resolved.i18n.policy == LocalePolicy::FollowSystem {
        resolved.i18n.locale = host_locale.unwrap_or_else(default_locale);
    }
    resolved
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct RenderSettings {
    #[serde(default)]
    pub blur: BlurSettings,
}

/// Blur cost is the one part of painting worth a user-facing dial: it scales
/// with covered *area* rather than element count, so a weak machine can trade
/// fidelity for frame time without giving up the frosted look.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BlurSettings {
    /// Passes per filtered layer (1–3). Each runs at a reduced sigma that keeps
    /// total blur constant, so more passes buy a smoother falloff, not a
    /// stronger blur, and cost roughly linearly.
    #[serde(default = "default_blur_passes")]
    pub passes: u8,

    /// Larger radii are dropped with a diagnostic rather than rasterized,
    /// bounding the worst frame a stylesheet can ask for.
    #[serde(default = "default_blur_max_radius")]
    pub max_radius: f32,
}

impl Default for BlurSettings {
    fn default() -> Self {
        Self {
            passes: default_blur_passes(),
            max_radius: default_blur_max_radius(),
        }
    }
}

fn default_blur_passes() -> u8 {
    1
}

fn default_blur_max_radius() -> f32 {
    96.0
}

/// Shell-wide tooltip defaults. The enter animation is not configured here —
/// it is authored in theme CSS (`tooltip { animation: ... }` plus `@keyframes`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TooltipSettings {
    /// One of [`TOOLTIP_POSITIONS`], used when the element sets no
    /// `tooltip-anchor`. Placement resolves in three steps: this default, the
    /// element's CSS preference, then a container-aware flip to the opposite
    /// side if the chosen one would overflow the nearest clipping container.
    #[serde(default = "default_tooltip_position")]
    pub position: String,

    /// Delay after hover starts, in milliseconds.
    #[serde(default = "default_tooltip_delay_ms")]
    pub delay_ms: u64,

    /// Gap in pixels between the tooltip and the hovered element.
    #[serde(default = "default_tooltip_gap")]
    pub gap: f32,

    #[serde(default = "default_tooltip_cursor_offset_x")]
    pub cursor_offset_x: f32,

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
    "tokyo-night".to_string()
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

    #[error("settings revision conflict: expected {expected}, found {actual}")]
    RevisionConflict { expected: u64, actual: u64 },
}

pub fn default_config_path() -> PathBuf {
    dirs_path("config").join("mesh/config.toml")
}

pub fn load_config(path: &Path) -> Result<ShellConfig, ConfigError> {
    if !path.exists() {
        return Ok(ShellConfig {
            shell: ShellSection::default(),
        });
    }
    let content = std::fs::read_to_string(path)?;
    let config: ShellConfig = toml::from_str(&content)?;
    Ok(config)
}

fn dirs_path(kind: &str) -> PathBuf {
    match kind {
        "config" => non_empty_env_path(std::env::var_os("XDG_CONFIG_HOME")).unwrap_or_else(|| {
            let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
            PathBuf::from(home).join(".config")
        }),
        "data" => non_empty_env_path(std::env::var_os("XDG_DATA_HOME")).unwrap_or_else(|| {
            let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
            PathBuf::from(home).join(".local/share")
        }),
        _ => PathBuf::from("/tmp"),
    }
}

fn non_empty_env_path(value: Option<std::ffi::OsString>) -> Option<PathBuf> {
    value.filter(|value| !value.is_empty()).map(PathBuf::from)
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
    fn empty_xdg_directory_values_are_absent() {
        assert_eq!(non_empty_env_path(Some(std::ffi::OsString::new())), None);
        assert_eq!(
            non_empty_env_path(Some(std::ffi::OsString::from("/tmp/mesh-config"))),
            Some(PathBuf::from("/tmp/mesh-config"))
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
    fn module_icon_overrides_read_from_the_modules_own_namespace() {
        let namespace = json!({
            "surface": { "anchor": "bottom" },
            "icons": {
                "use_packs": ["@mesh/user-icons"],
                "overrides": { "settings": "lucide/settings" },
                "ignore_shell_default": true
            }
        });

        let overrides = ModuleSettingsOverrides::from_namespace(&namespace);
        let icons = overrides.icons.expect("icon overrides parsed");

        assert_eq!(
            icons.use_packs.as_deref(),
            Some(&["@mesh/user-icons".to_string()][..])
        );
        assert_eq!(
            icons.overrides.get("settings").map(String::as_str),
            Some("lucide/settings")
        );
        assert!(icons.ignore_shell_default);
    }

    #[test]
    fn a_module_namespace_without_icons_yields_no_overrides() {
        let overrides =
            ModuleSettingsOverrides::from_namespace(&json!({ "surface": { "anchor": "top" } }));
        assert!(overrides.icons.is_none());
    }

    #[test]
    fn locale_policy_is_backward_compatible_and_explicit() {
        let legacy: I18nSettings = serde_json::from_value(json!({
            "locale": "sk",
            "fallback_locale": "en"
        }))
        .unwrap();
        assert_eq!(legacy.policy, LocalePolicy::Manual);

        let follow_system: I18nSettings = serde_json::from_value(json!({
            "policy": "follow_system",
            "locale": "sk",
            "fallback_locale": "en"
        }))
        .unwrap();
        assert_eq!(follow_system.policy, LocalePolicy::FollowSystem);
        assert_eq!(follow_system.policy.as_str(), "follow_system");
    }

    #[test]
    fn locale_policy_resolution_changes_only_follow_system_runtime_values() {
        let mut settings = ShellSettings::default();
        settings.i18n.locale = "sk".into();
        settings.i18n.fallback_locale = "en".into();

        let manual = resolve_shell_locale_settings_with_host_locale(&settings, Some("de".into()));
        assert_eq!(manual.i18n.locale, "sk");
        assert_eq!(manual.i18n.policy, LocalePolicy::Manual);

        settings.i18n.policy = LocalePolicy::FollowSystem;
        let follow_system =
            resolve_shell_locale_settings_with_host_locale(&settings, Some("de-DE".into()));
        assert_eq!(follow_system.i18n.locale, "de-DE");
        assert_eq!(follow_system.i18n.fallback_locale, "en");
        assert_eq!(follow_system.i18n.policy, LocalePolicy::FollowSystem);
    }
}
