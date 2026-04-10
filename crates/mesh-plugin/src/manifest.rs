/// Plugin manifest (mesh.toml) parsing and representation.
use mesh_capability::Capability;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// The complete contents of a plugin's `mesh.toml`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub package: PackageSection,
    #[serde(default)]
    pub compatibility: CompatibilitySection,
    #[serde(default)]
    pub dependencies: HashMap<String, DependencySpec>,
    #[serde(default)]
    pub capabilities: CapabilitiesSection,
    #[serde(default)]
    pub entrypoints: EntrypointsSection,
    #[serde(default)]
    pub settings: Option<SettingsSection>,
    #[serde(default)]
    pub i18n: Option<I18nSection>,
    #[serde(default)]
    pub theme: Option<ThemeUsageSection>,
    #[serde(default)]
    pub service: Option<ServiceSection>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageSection {
    pub id: String,
    pub version: String,
    #[serde(rename = "type")]
    pub plugin_type: PluginType,
    pub api_version: String,
    #[serde(default)]
    pub license: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub authors: Vec<String>,
    #[serde(default)]
    pub repository: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PluginType {
    /// A top-level shell UI (panel, launcher, notification center).
    Surface,
    /// An embeddable UI component inside a surface.
    Widget,
    /// A backend that implements a service trait (e.g. audio via PipeWire).
    Backend,
    /// A visual token set.
    Theme,
    /// Translations.
    LanguagePack,
    /// Icon set.
    IconPack,
}

impl std::fmt::Display for PluginType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Surface => write!(f, "surface"),
            Self::Widget => write!(f, "widget"),
            Self::Backend => write!(f, "backend"),
            Self::Theme => write!(f, "theme"),
            Self::LanguagePack => write!(f, "language-pack"),
            Self::IconPack => write!(f, "icon-pack"),
        }
    }
}

/// Declares which service trait a backend plugin implements.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceSection {
    /// The service trait this backend provides (e.g. "audio", "network", "power").
    pub provides: String,
    /// Human-readable name for this backend (e.g. "PipeWire", "PulseAudio").
    pub backend_name: String,
    /// Priority when auto-selecting (higher wins). User config overrides this.
    #[serde(default)]
    pub priority: u32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CompatibilitySection {
    #[serde(default)]
    pub mesh: Option<String>,
    #[serde(default)]
    pub compositors: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum DependencySpec {
    Simple(String),
    Detailed { version: String, optional: Option<bool> },
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CapabilitiesSection {
    #[serde(default)]
    pub required: Vec<String>,
    #[serde(default)]
    pub optional: Vec<String>,
}

impl CapabilitiesSection {
    pub fn required_capabilities(&self) -> Vec<Capability> {
        self.required.iter().map(|s| Capability::new(s)).collect()
    }

    pub fn optional_capabilities(&self) -> Vec<Capability> {
        self.optional.iter().map(|s| Capability::new(s)).collect()
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EntrypointsSection {
    #[serde(default)]
    pub main: Option<String>,
    #[serde(default)]
    pub settings_ui: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettingsSection {
    pub schema: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct I18nSection {
    pub default_locale: String,
    pub translations: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeUsageSection {
    #[serde(default)]
    pub tokens_used: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum ManifestError {
    #[error("failed to read manifest: {0}")]
    Io(#[from] std::io::Error),

    #[error("failed to parse manifest: {0}")]
    Parse(#[from] toml::de::Error),

    #[error("missing required field: {0}")]
    MissingField(String),
}

/// Load a manifest from a plugin directory.
pub fn load_manifest(plugin_dir: &Path) -> Result<Manifest, ManifestError> {
    let path = plugin_dir.join("mesh.toml");
    let content = std::fs::read_to_string(&path)?;
    let manifest: Manifest = toml::from_str(&content)?;
    Ok(manifest)
}
