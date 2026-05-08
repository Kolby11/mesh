/// Token-based theme engine for MESH.
///
/// Themes define design tokens across standard groups: colors, typography,
/// spacing, radius, elevation, borders, motion, and shadows. Components
/// inherit tokens from the active theme.
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

const LEGACY_DEFAULT_SHELL_ANIMATION_PREFIX: &str = "animation.default.";

pub type ComponentDefaults = HashMap<String, String>;

/// A resolved theme token value.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TokenValue {
    String(String),
    Number(f64),
    Bool(bool),
}

impl std::fmt::Display for TokenValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::String(s) => write!(f, "{s}"),
            Self::Number(n) => write!(f, "{n}"),
            Self::Bool(b) => write!(f, "{b}"),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ThemeDefaults {
    #[serde(default)]
    pub components: HashMap<String, ComponentDefaults>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ThemeModule {
    #[serde(default)]
    pub tokens: HashMap<String, TokenValue>,
    #[serde(default)]
    pub defaults: ThemeDefaults,
}

/// A complete theme definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Theme {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub tokens: HashMap<String, TokenValue>,
    #[serde(default)]
    pub defaults: ThemeDefaults,
    #[serde(default)]
    pub modules: HashMap<String, ThemeModule>,
}

impl Theme {
    /// Look up a single token by dotted name (e.g. "color.primary").
    pub fn token(&self, name: &str) -> Option<&TokenValue> {
        match split_explicit_module_token(name) {
            Some((module_id, token_name)) => self
                .modules
                .get(module_id)
                .and_then(|module| module.tokens.get(token_name)),
            None => self.tokens.get(name),
        }
    }

    /// Return all tokens in a group (e.g. "color" returns "color.primary", "color.surface", etc.).
    pub fn tokens_in_group(&self, group: &str) -> HashMap<&str, &TokenValue> {
        let prefix = format!("{group}.");
        self.tokens
            .iter()
            .filter(|(k, _)| k.starts_with(&prefix))
            .map(|(k, v)| (k.as_str(), v))
            .collect()
    }

    pub fn component_defaults(&self, component: &str) -> Option<&ComponentDefaults> {
        self.defaults.components.get(component)
    }

    pub fn module_component_defaults(
        &self,
        module_id: &str,
        component: &str,
    ) -> Option<&ComponentDefaults> {
        self.modules
            .get(module_id)
            .and_then(|module| module.defaults.components.get(component))
    }
}

#[derive(Debug, Deserialize)]
struct RawTheme {
    id: String,
    name: String,
    #[serde(default)]
    tokens: HashMap<String, TokenValue>,
    #[serde(default)]
    defaults: ThemeDefaults,
    #[serde(default)]
    modules: HashMap<String, ThemeModule>,
    #[serde(default)]
    default_shell_animations: HashMap<String, String>,
}

impl From<RawTheme> for Theme {
    fn from(raw: RawTheme) -> Self {
        let mut tokens = raw.tokens;
        let mut defaults = raw.defaults;
        let mut base_defaults = defaults.components.remove("base").unwrap_or_default();

        let legacy_animation_keys: Vec<String> = tokens
            .keys()
            .filter_map(|key| {
                key.strip_prefix(LEGACY_DEFAULT_SHELL_ANIMATION_PREFIX)
                    .map(str::to_owned)
            })
            .collect();

        for animation_name in legacy_animation_keys {
            let legacy_key = format!("{LEGACY_DEFAULT_SHELL_ANIMATION_PREFIX}{animation_name}");
            let Some(TokenValue::String(value)) = tokens.remove(&legacy_key) else {
                continue;
            };
            base_defaults.entry(animation_name).or_insert(value);
        }

        for (name, value) in raw.default_shell_animations {
            base_defaults.entry(name).or_insert(value);
        }

        if !base_defaults.is_empty() {
            defaults.components.insert("base".into(), base_defaults);
        }

        Self {
            id: raw.id,
            name: raw.name,
            tokens,
            defaults,
            modules: raw.modules,
        }
    }
}

/// The theme engine manages the active theme and notifies listeners on change.
#[derive(Debug)]
pub struct ThemeEngine {
    active: Theme,
    available: Vec<Theme>,
}

impl ThemeEngine {
    pub fn new(default_theme: Theme) -> Self {
        Self {
            active: default_theme,
            available: Vec::new(),
        }
    }

    pub fn active(&self) -> &Theme {
        &self.active
    }

    pub fn register_theme(&mut self, theme: Theme) {
        self.available.push(theme);
    }

    pub fn set_active(&mut self, theme_id: &str) -> Result<(), ThemeError> {
        let theme = self
            .available
            .iter()
            .find(|t| t.id == theme_id)
            .ok_or_else(|| ThemeError::NotFound(theme_id.to_string()))?;
        self.active = theme.clone();
        Ok(())
    }

    pub fn available_themes(&self) -> &[Theme] {
        &self.available
    }

    pub fn replace_active(&mut self, theme: Theme) {
        self.active = theme;
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ThemeError {
    #[error("theme not found: {0}")]
    NotFound(String),

    #[error("failed to read theme file {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("failed to parse theme file {path}: {source}")]
    Parse {
        path: PathBuf,
        source: serde_json::Error,
    },
}

pub fn default_theme() -> Theme {
    match load_theme_from_path(&default_theme_path()) {
        Ok(theme) => theme,
        Err(err) => {
            tracing::warn!("failed to load default theme json, using embedded fallback: {err}");
            embedded_default_theme()
        }
    }
}

pub fn default_theme_path() -> PathBuf {
    theme_path_for_id("mesh-default-dark")
}

pub fn theme_dir_path() -> PathBuf {
    if let Ok(path) = std::env::var("MESH_THEME_DIR") {
        if !path.trim().is_empty() {
            return PathBuf::from(path);
        }
    }

    let repo_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../..")
        .join("config/themes");
    if repo_path.exists() {
        return repo_path;
    }

    mesh_home_path().join("themes")
}

pub fn theme_path_for_id(theme_id: &str) -> PathBuf {
    theme_dir_path().join(format!("{theme_id}.json"))
}

/// Load all `*.json` theme files found in a directory. Files that fail to
/// parse are silently skipped so a single bad file does not block startup.
pub fn load_themes_from_dir(dir: &Path) -> Vec<Theme> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut themes: Vec<Theme> = entries
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map(|x| x == "json").unwrap_or(false))
        .filter_map(|e| load_theme_from_path(&e.path()).ok())
        .collect();
    themes.sort_by(|a, b| a.id.cmp(&b.id));
    themes
}

pub fn load_theme_from_path(path: &Path) -> Result<Theme, ThemeError> {
    let content = std::fs::read_to_string(path).map_err(|source| ThemeError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    parse_theme(&content).map_err(|source| ThemeError::Parse {
        path: path.to_path_buf(),
        source,
    })
}

fn embedded_default_theme() -> Theme {
    parse_theme(include_str!(
        "../../../../../config/themes/mesh-default-dark.json"
    ))
    .expect("embedded default theme json must be valid")
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

fn parse_theme(content: &str) -> Result<Theme, serde_json::Error> {
    serde_json::from_str::<RawTheme>(content).map(Theme::from)
}

fn split_explicit_module_token(name: &str) -> Option<(&str, &str)> {
    if !name.starts_with('@') {
        return None;
    }

    let (module_id, token_name) = name.split_once('.')?;
    if module_id.is_empty() || token_name.is_empty() {
        return None;
    }
    Some((module_id, token_name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_module_token_lookup_reads_module_subtree() {
        let theme = parse_theme(
            r##"{
              "id": "scoped",
              "name": "Scoped",
              "tokens": {
                "color.primary": "#000000"
              },
              "modules": {
                "@mesh/weather": {
                  "tokens": {
                    "weather.color.sunny": "#f6b73c"
                  }
                }
              }
            }"##,
        )
        .expect("theme parses");

        assert_eq!(
            theme
                .token("@mesh/weather.weather.color.sunny")
                .map(ToString::to_string),
            Some("#f6b73c".into())
        );
        assert!(theme.token("weather.color.sunny").is_none());
    }

    #[test]
    fn legacy_default_shell_animation_tokens_are_extracted_into_base_component_defaults() {
        let theme = parse_theme(
            r##"{
              "id": "legacy",
              "name": "Legacy",
              "tokens": {
                "color.primary": "#000000",
                "animation.duration.fast": 90.0,
                "animation.default.hover": "all 90ms ease-out"
              }
            }"##,
        )
        .expect("legacy theme parses");

        assert!(theme.token("animation.default.hover").is_none());
        assert_eq!(
            theme
                .component_defaults("base")
                .and_then(|defaults| defaults.get("hover"))
                .map(String::as_str),
            Some("all 90ms ease-out")
        );
        assert!(theme.token("animation.duration.fast").is_some());
    }

    #[test]
    fn explicit_base_component_defaults_are_preserved() {
        let theme = parse_theme(
            r##"{
              "id": "separated",
              "name": "Separated",
              "tokens": {
                "animation.duration.fast": 90.0
              },
              "defaults": {
                "components": {
                  "base": {
                    "transition": "all token(animation.duration.fast) ease-out"
                  }
                }
              }
            }"##,
        )
        .expect("separated theme parses");

        assert_eq!(
            theme
                .component_defaults("base")
                .and_then(|defaults| defaults.get("transition"))
                .map(String::as_str),
            Some("all token(animation.duration.fast) ease-out")
        );
        assert!(theme.token("animation.default.hover").is_none());
    }
}
