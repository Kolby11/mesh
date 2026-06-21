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
        self.tokens
            .get(name)
            .or_else(|| match split_explicit_module_token(name) {
                Some((module_id, token_name)) => self
                    .modules
                    .get(module_id)
                    .and_then(|module| module.tokens.get(token_name)),
                None => None,
            })
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
        let mut theme = Self {
            id: raw.id,
            name: raw.name,
            tokens: raw.tokens,
            defaults: raw.defaults,
            modules: raw.modules,
        };
        normalize_legacy_default_shell_animations(
            &mut theme,
            raw.default_shell_animations.into_iter().collect(),
        );
        flatten_module_tokens_into(&mut theme.tokens, &theme.modules);
        theme
    }
}

fn normalize_legacy_default_shell_animations(
    theme: &mut Theme,
    mut default_shell_animations: Vec<(String, String)>,
) {
    let mut base_defaults = theme.defaults.components.remove("base").unwrap_or_default();
    let mut legacy_transition_fragments = Vec::new();

    let mut legacy_animation_keys: Vec<String> = theme
        .tokens
        .keys()
        .filter_map(|key| {
            key.strip_prefix(LEGACY_DEFAULT_SHELL_ANIMATION_PREFIX)
                .map(str::to_owned)
        })
        .collect();
    legacy_animation_keys.sort();

    for animation_name in legacy_animation_keys {
        let legacy_key = format!("{LEGACY_DEFAULT_SHELL_ANIMATION_PREFIX}{animation_name}");
        let Some(TokenValue::String(value)) = theme.tokens.remove(&legacy_key) else {
            continue;
        };
        legacy_transition_fragments.push(value);
    }

    default_shell_animations.sort_by(|left, right| left.0.cmp(&right.0));
    for (_name, value) in default_shell_animations {
        legacy_transition_fragments.push(value);
    }

    if !legacy_transition_fragments.is_empty() && !base_defaults.contains_key("transition") {
        base_defaults.insert("transition".into(), legacy_transition_fragments.join(", "));
    }

    if !base_defaults.is_empty() {
        theme
            .defaults
            .components
            .insert("base".into(), base_defaults);
    }
}

fn flatten_module_tokens_into(
    tokens: &mut HashMap<String, TokenValue>,
    modules: &HashMap<String, ThemeModule>,
) {
    for (module_id, module) in modules {
        for (token_name, value) in &module.tokens {
            tokens.insert(format!("{module_id}.{token_name}"), value.clone());
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

    #[error("failed to parse theme css {path}: {message}")]
    CssParse { path: PathBuf, message: String },
}

pub fn default_theme() -> Theme {
    match load_theme_from_path(&default_theme_path()) {
        Ok(theme) => theme,
        Err(err) => {
            tracing::warn!("failed to load default theme, using embedded fallback: {err}");
            embedded_default_theme()
        }
    }
}

pub fn default_theme_path() -> PathBuf {
    theme_path_for_id("mesh-default-dark")
}

pub fn theme_dir_path() -> PathBuf {
    if let Ok(path) = std::env::var("MESH_THEME_DIR")
        && !path.trim().is_empty()
    {
        return PathBuf::from(path);
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
    let package_css = theme_dir_path().join(theme_id).join("theme.css");
    if package_css.exists() {
        return package_css;
    }

    theme_dir_path().join(format!("{theme_id}.json"))
}

/// Load all theme packages and legacy `*.json` theme files found in a directory.
/// Files that fail to parse are silently skipped so one bad theme does not block
/// startup.
pub fn load_themes_from_dir(dir: &Path) -> Vec<Theme> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut themes: Vec<Theme> = entries
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let path = e.path();
            if path.is_dir() {
                let css_path = path.join("theme.css");
                return load_theme_from_path(&css_path).ok();
            }
            if path.extension().map(|x| x == "json").unwrap_or(false) {
                return load_theme_from_path(&path).ok();
            }
            None
        })
        .collect();
    themes.sort_by(|a, b| a.id.cmp(&b.id));
    themes
}

pub fn load_theme_from_path(path: &Path) -> Result<Theme, ThemeError> {
    if path.is_dir() {
        return load_theme_from_path(&path.join("theme.css"));
    }

    let content = std::fs::read_to_string(path).map_err(|source| ThemeError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    match path.extension().and_then(|extension| extension.to_str()) {
        Some("css") => parse_theme_css_file(path, &content),
        _ => parse_theme(&content).map_err(|source| ThemeError::Parse {
            path: path.to_path_buf(),
            source,
        }),
    }
}

fn embedded_default_theme() -> Theme {
    parse_theme_css(
        "mesh-default-dark",
        "MESH Default Dark",
        include_str!("../../../../../config/themes/mesh-default-dark/theme.css"),
    )
    .expect("embedded default theme css must be valid")
}

fn mesh_home_path() -> PathBuf {
    if let Ok(path) = std::env::var("MESH_HOME")
        && !path.trim().is_empty()
    {
        return PathBuf::from(path);
    }

    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(home).join(".mesh")
}

fn parse_theme(content: &str) -> Result<Theme, serde_json::Error> {
    serde_json::from_str::<RawTheme>(content).map(Theme::from)
}

#[derive(Debug, Deserialize)]
struct ThemePackageManifest {
    #[serde(default)]
    name: String,
    mesh: ThemePackageMesh,
}

#[derive(Debug, Deserialize)]
struct ThemePackageMesh {
    theme: ThemePackageTheme,
}

#[derive(Debug, Deserialize)]
struct ThemePackageTheme {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    label: Option<String>,
}

fn parse_theme_css_file(path: &Path, content: &str) -> Result<Theme, ThemeError> {
    let (id, name) = load_theme_package_metadata(path)?;
    parse_theme_css(&id, &name, content).map_err(|message| ThemeError::CssParse {
        path: path.to_path_buf(),
        message,
    })
}

fn load_theme_package_metadata(path: &Path) -> Result<(String, String), ThemeError> {
    let package_dir = path.parent().unwrap_or_else(|| Path::new("."));
    let manifest_path = package_dir.join("module.json");
    let manifest_content =
        std::fs::read_to_string(&manifest_path).map_err(|source| ThemeError::Io {
            path: manifest_path.clone(),
            source,
        })?;
    let manifest: ThemePackageManifest =
        serde_json::from_str(&manifest_content).map_err(|source| ThemeError::Parse {
            path: manifest_path,
            source,
        })?;

    let id = manifest
        .mesh
        .theme
        .id
        .unwrap_or_else(|| manifest.name.trim_start_matches("@mesh/").to_string());
    let name = manifest.mesh.theme.label.unwrap_or_else(|| id.clone());
    Ok((id, name))
}

fn parse_theme_css(id: &str, name: &str, content: &str) -> Result<Theme, String> {
    let content = strip_css_comments(content);
    let mut theme = Theme {
        id: id.to_string(),
        name: name.to_string(),
        tokens: HashMap::new(),
        defaults: ThemeDefaults::default(),
        modules: HashMap::new(),
    };

    parse_theme_css_blocks(content.as_str(), &mut theme)?;
    normalize_legacy_default_shell_animations(&mut theme, Vec::new());
    flatten_module_tokens_into(&mut theme.tokens, &theme.modules);
    Ok(theme)
}

fn strip_css_comments(content: &str) -> String {
    let mut output = String::with_capacity(content.len());
    let mut rest = content;
    while let Some(start) = rest.find("/*") {
        output.push_str(&rest[..start]);
        let after_start = &rest[start + 2..];
        if let Some(end) = after_start.find("*/") {
            rest = &after_start[end + 2..];
        } else {
            return output;
        }
    }
    output.push_str(rest);
    output
}

fn parse_theme_css_blocks(mut rest: &str, theme: &mut Theme) -> Result<(), String> {
    while let Some(open) = rest.find('{') {
        let selector = rest[..open].trim();
        let body_start = open + 1;
        let close = find_matching_brace(rest, open)
            .ok_or_else(|| format!("missing closing brace for selector '{selector}'"))?;
        let body = &rest[body_start..close];
        parse_theme_css_block(selector, body, theme)?;
        rest = &rest[close + 1..];
    }
    Ok(())
}

fn find_matching_brace(content: &str, open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (offset, byte) in content.as_bytes()[open..].iter().enumerate() {
        match byte {
            b'{' => depth += 1,
            b'}' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(open + offset);
                }
            }
            _ => {}
        }
    }
    None
}

fn parse_theme_css_block(selector: &str, body: &str, theme: &mut Theme) -> Result<(), String> {
    if selector.is_empty() {
        return Ok(());
    }

    if let Some(module_id) = parse_module_selector(selector) {
        parse_theme_module_css(module_id, body, theme)?;
        return Ok(());
    }

    let declarations = parse_css_declarations(body)?;
    if selector == ":root" {
        for (property, value) in declarations {
            let Some(token_name) = css_variable_to_token_name(&property) else {
                continue;
            };
            theme.tokens.insert(token_name, parse_token_value(&value));
        }
        return Ok(());
    }

    let component = if selector == "node" { "base" } else { selector };
    theme
        .defaults
        .components
        .entry(component.to_string())
        .or_default()
        .extend(declarations);
    Ok(())
}

fn parse_module_selector(selector: &str) -> Option<&str> {
    let selector = selector.strip_prefix("@module")?.trim();
    selector.strip_prefix('"')?.strip_suffix('"')
}

fn parse_theme_module_css(module_id: &str, content: &str, theme: &mut Theme) -> Result<(), String> {
    let mut module = theme.modules.remove(module_id).unwrap_or_default();
    let mut rest = content;
    while let Some(open) = rest.find('{') {
        let selector = rest[..open].trim();
        let close = find_matching_brace(rest, open)
            .ok_or_else(|| format!("missing closing brace for module selector '{selector}'"))?;
        let body = &rest[open + 1..close];
        parse_theme_module_css_block(selector, body, &mut module)?;
        rest = &rest[close + 1..];
    }
    theme.modules.insert(module_id.to_string(), module);
    Ok(())
}

fn parse_theme_module_css_block(
    selector: &str,
    body: &str,
    module: &mut ThemeModule,
) -> Result<(), String> {
    let declarations = parse_css_declarations(body)?;
    if selector == ":root" {
        for (property, value) in declarations {
            let Some(token_name) = css_variable_to_token_name(&property) else {
                continue;
            };
            module.tokens.insert(token_name, parse_token_value(&value));
        }
        return Ok(());
    }

    let component = if selector == "node" { "base" } else { selector };
    module
        .defaults
        .components
        .entry(component.to_string())
        .or_default()
        .extend(declarations);
    Ok(())
}

fn parse_css_declarations(body: &str) -> Result<ComponentDefaults, String> {
    let mut declarations = ComponentDefaults::new();
    for raw in body.split(';') {
        let declaration = raw.trim();
        if declaration.is_empty() {
            continue;
        }
        let Some((property, value)) = declaration.split_once(':') else {
            return Err(format!("invalid declaration '{declaration}'"));
        };
        let property = property.trim();
        let value = value.trim();
        if property.is_empty() || value.is_empty() {
            return Err(format!("invalid declaration '{declaration}'"));
        }
        declarations.insert(property.to_string(), value.to_string());
    }
    Ok(declarations)
}

fn css_variable_to_token_name(property: &str) -> Option<String> {
    let token = property.strip_prefix("--")?;
    if token.is_empty() {
        return None;
    }
    Some(css_custom_property_to_token_name(token))
}

fn css_custom_property_to_token_name(token: &str) -> String {
    let Some((group, rest)) = token.split_once('-') else {
        return token.to_string();
    };

    let rest = match group {
        "animation" => canonicalize_prefixed(
            rest,
            &["curves-bezier", "default", "duration", "opacity", "scale"],
        ),
        "border" => canonicalize_prefixed(rest, &["style", "width"]),
        "shadow" => canonicalize_prefixed(rest, &["colored", "umbra"]),
        "shape" => canonicalize_prefixed(rest, &["corner"]),
        "spacing" => canonicalize_prefixed(rest, &["inset"]),
        "state" => canonicalize_suffixed(rest, &["opacity"]),
        "icon" => canonicalize_prefixed(rest, &["size"]),
        "typography" => canonicalize_prefixed(
            rest,
            &[
                "family",
                "line-height",
                "scale-body-large",
                "scale-body-medium",
                "scale-body-small",
                "scale-display-large",
                "scale-display-medium",
                "scale-display-small",
                "scale-headline-large",
                "scale-headline-medium",
                "scale-headline-small",
                "scale-label-large",
                "scale-label-medium",
                "scale-label-small",
                "scale-title-large",
                "scale-title-medium",
                "scale-title-small",
                "size",
                "tracking",
                "weight",
            ],
        ),
        "color" | "elevation" | "radius" => rest.to_string(),
        _ => rest.replace('-', "."),
    };

    format!("{group}.{rest}")
}

fn canonicalize_prefixed(value: &str, prefixes: &[&str]) -> String {
    let mut prefixes = prefixes.to_vec();
    prefixes.sort_by_key(|prefix| std::cmp::Reverse(prefix.len()));
    for prefix in prefixes {
        if value == prefix {
            return prefix.to_string();
        }
        if let Some(rest) = value.strip_prefix(&format!("{prefix}-")) {
            return format!("{}.{}", prefix.replace('-', "."), rest);
        }
    }
    value.to_string()
}

fn canonicalize_suffixed(value: &str, suffixes: &[&str]) -> String {
    for suffix in suffixes {
        if let Some(rest) = value.strip_suffix(&format!("-{suffix}")) {
            return format!("{rest}.{suffix}");
        }
    }
    value.to_string()
}

fn parse_token_value(value: &str) -> TokenValue {
    match value {
        "true" => TokenValue::Bool(true),
        "false" => TokenValue::Bool(false),
        _ => value
            .parse::<f64>()
            .map(TokenValue::Number)
            .unwrap_or_else(|_| TokenValue::String(value.to_string())),
    }
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
    fn legacy_default_shell_animation_tokens_are_extracted_into_base_transition() {
        let theme = parse_theme(
            r##"{
              "id": "legacy",
              "name": "Legacy",
              "tokens": {
                "color.primary": "#000000",
                "animation.duration.fast": 90.0,
                "animation.default.border-radius": "border-radius 90ms ease-out",
                "animation.default.opacity": "opacity 90ms ease-out"
              }
            }"##,
        )
        .expect("legacy theme parses");

        assert!(theme.token("animation.default.opacity").is_none());
        assert_eq!(
            theme
                .component_defaults("base")
                .and_then(|defaults| defaults.get("transition"))
                .map(String::as_str),
            Some("border-radius 90ms ease-out, opacity 90ms ease-out")
        );
        assert!(
            theme
                .component_defaults("base")
                .is_none_or(|defaults| !defaults.contains_key("opacity"))
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
                    "transition": "all var(--animation-duration-fast) ease-out"
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
            Some("all var(--animation-duration-fast) ease-out")
        );
        assert!(theme.token("animation.default.hover").is_none());
    }

    #[test]
    fn css_theme_parses_tokens_and_component_defaults() {
        let theme = parse_theme_css(
            "css-theme",
            "CSS Theme",
            r#"
            :root {
              --color-on-primary: #ffffff;
              --typography-size-md: 14;
              --feature-enabled: true;
            }

            node {
              color: var(--color-on-primary);
            }

            button {
              border-radius: var(--radius-md);
            }
            "#,
        )
        .expect("css theme parses");

        assert_eq!(
            theme.token("color.on-primary").map(ToString::to_string),
            Some("#ffffff".into())
        );
        assert_eq!(
            theme.token("typography.size.md").map(ToString::to_string),
            Some("14".into())
        );
        assert_eq!(
            theme.token("feature.enabled").map(ToString::to_string),
            Some("true".into())
        );
        assert_eq!(
            theme
                .component_defaults("base")
                .and_then(|defaults| defaults.get("color"))
                .map(String::as_str),
            Some("var(--color-on-primary)")
        );
        assert_eq!(
            theme
                .component_defaults("button")
                .and_then(|defaults| defaults.get("border-radius"))
                .map(String::as_str),
            Some("var(--radius-md)")
        );
    }

    #[test]
    fn css_theme_does_not_interpret_double_dash_as_token_separator() {
        let theme = parse_theme_css(
            "css-theme",
            "CSS Theme",
            r#"
            :root {
              --color--on-primary: #ffffff;
            }
            "#,
        )
        .expect("css theme parses");

        assert!(theme.token("color.on-primary").is_none());
        assert_eq!(
            theme.token("color.-on-primary").map(ToString::to_string),
            Some("#ffffff".into())
        );
    }

    #[test]
    fn css_theme_parses_module_scoped_contributions() {
        let theme = parse_theme_css(
            "css-theme",
            "CSS Theme",
            r#"
            :root {
              --color-primary: #000000;
            }

            @module "@mesh/weather" {
              :root {
                --weather-color-sunny: #f6b73c;
              }

              node {
                color: var(--weather-color-sunny);
              }

              weather-chip {
                background: var(--weather-color-sunny);
              }
            }
            "#,
        )
        .expect("css theme parses");

        assert_eq!(
            theme
                .token("@mesh/weather.weather.color.sunny")
                .map(ToString::to_string),
            Some("#f6b73c".into())
        );
        assert_eq!(
            theme
                .module_component_defaults("@mesh/weather", "base")
                .and_then(|defaults| defaults.get("color"))
                .map(String::as_str),
            Some("var(--weather-color-sunny)")
        );
        assert_eq!(
            theme
                .module_component_defaults("@mesh/weather", "weather-chip")
                .and_then(|defaults| defaults.get("background"))
                .map(String::as_str),
            Some("var(--weather-color-sunny)")
        );
    }

    #[test]
    fn shipped_default_css_theme_exposes_expected_tokens() {
        let theme = default_theme();

        assert_eq!(theme.id, "mesh-default-dark");
        assert_eq!(
            theme
                .token("color.surface-container")
                .map(ToString::to_string),
            Some("#211F26".into())
        );
        assert_eq!(
            theme.token("color.on-surface").map(ToString::to_string),
            Some("#E6E1E5".into())
        );
        assert_eq!(
            theme
                .component_defaults("base")
                .and_then(|defaults| defaults.get("transition"))
                .map(String::as_str),
            Some(
                "background-color var(--animation-duration-short) var(--animation-curves-bezier-standard), color var(--animation-duration-short) var(--animation-curves-bezier-standard), border-color var(--animation-duration-short) var(--animation-curves-bezier-standard), opacity var(--animation-duration-medium) var(--animation-curves-bezier-standard), border-radius var(--animation-duration-short) var(--animation-curves-bezier-emphasized-decelerate), border-width var(--animation-duration-medium) var(--animation-curves-bezier-standard), width var(--animation-duration-medium) var(--animation-curves-bezier-standard), height var(--animation-duration-medium) var(--animation-curves-bezier-standard), padding var(--animation-duration-medium) var(--animation-curves-bezier-standard), margin var(--animation-duration-medium) var(--animation-curves-bezier-standard), transform var(--animation-duration-short) var(--animation-curves-bezier-emphasized-decelerate)"
            )
        );
    }
}
