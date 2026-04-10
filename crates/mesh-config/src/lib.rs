/// Configuration loading, validation, and schema support for MESH.
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Top-level MESH shell configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellConfig {
    #[serde(default)]
    pub shell: ShellSection,
    #[serde(default)]
    pub plugins: HashMap<String, PluginConfig>,
}

/// Global shell settings sourced from JSON files.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ShellSettings {
    #[serde(default)]
    pub theme: ThemeSettings,
    #[serde(default)]
    pub i18n: I18nSettings,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellSection {
    #[serde(default = "default_surface")]
    pub default_surface: String,
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

impl Default for ShellSection {
    fn default() -> Self {
        Self {
            default_surface: default_surface(),
        }
    }
}

/// Per-plugin configuration values.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(flatten)]
    pub values: HashMap<String, toml::Value>,
}

fn default_true() -> bool {
    true
}

/// Schema definition for a plugin's settings.
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
        .join("../..")
        .join("config/shell-settings.json");
    if repo_path.exists() {
        return repo_path;
    }

    dirs_path("config").join("mesh/shell-settings.json")
}

/// Bundled default settings file path.
pub fn default_settings_defaults_path() -> PathBuf {
    if let Ok(path) = std::env::var("MESH_SETTINGS_DEFAULTS_PATH") {
        if !path.trim().is_empty() {
            return PathBuf::from(path);
        }
    }

    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("config/settings-default.json")
}

/// Load shell configuration from a file.
pub fn load_config(path: &Path) -> Result<ShellConfig, ConfigError> {
    if !path.exists() {
        return Ok(ShellConfig {
            shell: ShellSection::default(),
            plugins: HashMap::new(),
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
