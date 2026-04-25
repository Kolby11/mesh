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
    pub plugins: HashMap<String, PluginConfig>,
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
}

/// System sound file mappings for shell events.
///
/// Paths are absolute or relative to the shell's data directory.
/// The audio backend plugin plays these via its `play-sound` command.
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

/// Write a per-plugin overrides file under XDG config (~/..../mesh/plugins/<scope>/<name>.json).
pub fn plugin_override_path(plugin_id: &str) -> PathBuf {
    let mut parts = plugin_id.splitn(2, '/');
    let scope = parts.next().unwrap_or(plugin_id);
    let name = parts.next().unwrap_or("");

    let mut path = dirs_path("config").join("mesh").join("plugins");
    if !name.is_empty() {
        path = path.join(scope).join(format!("{}.json", name));
    } else {
        // fallback: write as a single file named after the scope
        path = path.join(format!("{}.json", scope));
    }
    path
}

/// Persist per-plugin overrides atomically.
pub fn save_plugin_overrides(plugin_id: &str, overrides: &JsonValue) -> Result<(), ConfigError> {
    let path = plugin_override_path(plugin_id);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let tmp = path.with_extension("json.tmp");
    let content = serde_json::to_string_pretty(overrides)?;
    std::fs::write(&tmp, content)?;
    std::fs::rename(&tmp, &path)?;
    Ok(())
}

/// Remove a single key from a plugin overrides file. If the file becomes empty it is removed.
pub fn remove_plugin_override(plugin_id: &str, key: &str) -> Result<(), ConfigError> {
    let path = plugin_override_path(plugin_id);
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

/// Reset (remove) the per-plugin overrides file entirely.
pub fn reset_plugin_overrides(plugin_id: &str) -> Result<(), ConfigError> {
    let path = plugin_override_path(plugin_id);
    if path.exists() {
        std::fs::remove_file(path)?;
    }
    Ok(())
}

/// Validate a JSON value against a simple SettingsSchema. This performs basic
/// checks for type, enum values, and numeric min/max. It is intentionally
/// conservative: unknown keys are allowed (validation is per-field).
pub fn validate_plugin_settings(
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_validate_plugin_settings_valid() {
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

        assert!(validate_plugin_settings(&schema, &valid_json).is_ok());
    }

    #[test]
    fn test_validate_plugin_settings_invalid_enum() {
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

        assert!(validate_plugin_settings(&schema, &invalid_json).is_err());
    }

    #[test]
    fn test_validate_plugin_settings_invalid_type() {
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

        assert!(validate_plugin_settings(&schema, &invalid_json).is_err());
    }

    #[test]
    fn test_validate_plugin_settings_out_of_bounds() {
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

        assert!(validate_plugin_settings(&schema, &invalid_json_low).is_err());
        assert!(validate_plugin_settings(&schema, &invalid_json_high).is_err());
    }

    #[test]
    fn test_plugin_override_path_formats() {
        let path1 = plugin_override_path("@mesh/system-panel");
        assert!(path1.ends_with("mesh/plugins/@mesh/system-panel.json"));

        let path2 = plugin_override_path("generic-plugin");
        assert!(path2.ends_with("mesh/plugins/generic-plugin.json"));
    }
}
