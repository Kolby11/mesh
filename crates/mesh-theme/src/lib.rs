/// Token-based theme engine for MESH.
///
/// Themes define design tokens across standard groups: colors, typography,
/// spacing, radius, elevation, borders, motion, and shadows. Components
/// inherit tokens from the active theme.
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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
}

#[derive(Debug, thiserror::Error)]
pub enum ThemeError {
    #[error("theme not found: {0}")]
    NotFound(String),
}

/// Build a minimal default theme with core tokens.
pub fn default_theme() -> Theme {
    let mut tokens = HashMap::new();

    // Colors
    tokens.insert("color.primary".into(), TokenValue::String("#6750A4".into()));
    tokens.insert("color.on-primary".into(), TokenValue::String("#FFFFFF".into()));
    tokens.insert("color.surface".into(), TokenValue::String("#1C1B1F".into()));
    tokens.insert("color.on-surface".into(), TokenValue::String("#E6E1E5".into()));
    tokens.insert("color.background".into(), TokenValue::String("#1C1B1F".into()));
    tokens.insert("color.error".into(), TokenValue::String("#F2B8B5".into()));

    // Typography
    tokens.insert("typography.family".into(), TokenValue::String("Inter".into()));
    tokens.insert("typography.size.sm".into(), TokenValue::Number(12.0));
    tokens.insert("typography.size.md".into(), TokenValue::Number(14.0));
    tokens.insert("typography.size.lg".into(), TokenValue::Number(16.0));

    // Spacing
    tokens.insert("spacing.xs".into(), TokenValue::Number(4.0));
    tokens.insert("spacing.sm".into(), TokenValue::Number(8.0));
    tokens.insert("spacing.md".into(), TokenValue::Number(16.0));
    tokens.insert("spacing.lg".into(), TokenValue::Number(24.0));
    tokens.insert("spacing.xl".into(), TokenValue::Number(32.0));

    // Radius
    tokens.insert("radius.sm".into(), TokenValue::Number(4.0));
    tokens.insert("radius.md".into(), TokenValue::Number(8.0));
    tokens.insert("radius.lg".into(), TokenValue::Number(16.0));
    tokens.insert("radius.full".into(), TokenValue::Number(9999.0));

    // Elevation
    tokens.insert("elevation.none".into(), TokenValue::Number(0.0));
    tokens.insert("elevation.sm".into(), TokenValue::Number(1.0));
    tokens.insert("elevation.md".into(), TokenValue::Number(3.0));
    tokens.insert("elevation.lg".into(), TokenValue::Number(6.0));

    Theme {
        id: "mesh-default-dark".into(),
        name: "MESH Default Dark".into(),
        tokens,
    }
}
