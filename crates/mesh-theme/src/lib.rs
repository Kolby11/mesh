/// Token-based theme engine for MESH.
///
/// Themes define design tokens across standard groups: colors, typography,
/// spacing, radius, elevation, borders, motion, and shadows. Components
/// inherit tokens from the active theme.
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

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

/// A complete theme definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Theme {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub tokens: HashMap<String, TokenValue>,
}

impl Theme {
    /// Look up a single token by dotted name (e.g. "color.primary").
    pub fn token(&self, name: &str) -> Option<&TokenValue> {
        self.tokens.get(name)
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

    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("config/themes")
}

pub fn theme_path_for_id(theme_id: &str) -> PathBuf {
    theme_dir_path().join(format!("{theme_id}.json"))
}

pub fn load_theme_from_path(path: &Path) -> Result<Theme, ThemeError> {
    let content = std::fs::read_to_string(path).map_err(|source| ThemeError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    serde_json::from_str(&content).map_err(|source| ThemeError::Parse {
        path: path.to_path_buf(),
        source,
    })
}

fn embedded_default_theme() -> Theme {
    serde_json::from_str(include_str!(
        "../../../config/themes/mesh-default-dark.json"
    ))
    .expect("embedded default theme json must be valid")
}
