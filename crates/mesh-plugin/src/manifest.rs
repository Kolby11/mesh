/// Plugin manifest loading and normalized representation.
use mesh_capability::Capability;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// The normalized contents of a plugin manifest, regardless of source format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub package: PackageSection,
    #[serde(default)]
    pub compatibility: CompatibilitySection,
    #[serde(default)]
    pub dependencies: DependenciesSection,
    #[serde(default)]
    pub capabilities: CapabilitiesSection,
    #[serde(default)]
    pub entrypoints: EntrypointsSection,
    #[serde(default)]
    pub accessibility: Option<AccessibilitySection>,
    #[serde(default)]
    pub settings: Option<SettingsSection>,
    #[serde(default)]
    pub i18n: Option<I18nSection>,
    #[serde(default)]
    pub theme: Option<ThemeSection>,
    #[serde(default)]
    pub service: Option<ServiceSection>,
    #[serde(default)]
    pub provides: Vec<ProvidedInterface>,
    #[serde(default)]
    pub interface: Option<InterfaceSection>,
    #[serde(default)]
    pub extensions: Vec<ExtensionSection>,
    #[serde(default)]
    pub exports: ExportsSection,
    #[serde(default)]
    pub provides_slots: HashMap<String, SlotDefinition>,
    #[serde(default)]
    pub slot_contributions: HashMap<String, Vec<SlotContribution>>,
    #[serde(default)]
    pub assets: Option<AssetsSection>,
    #[serde(default)]
    pub translations: HashMap<String, HashMap<String, String>>,
    #[serde(default)]
    pub surface_layout: Option<SurfaceLayoutSection>,
}

impl Manifest {
    /// Return normalized backend/interface declarations.
    pub fn declared_provides(&self) -> Vec<ProvidedInterface> {
        if !self.provides.is_empty() {
            return self.provides.clone();
        }

        self.service
            .as_ref()
            .map(|service| {
                vec![ProvidedInterface {
                    interface: service.provides.clone(),
                    version: None,
                    backend_name: Some(service.backend_name.clone()),
                    priority: service.priority,
                    optional_capabilities: Vec::new(),
                }]
            })
            .unwrap_or_default()
    }

    /// Return the primary service declaration for compatibility with the older runtime.
    pub fn primary_service(&self) -> Option<ServiceSection> {
        if let Some(service) = &self.service {
            return Some(service.clone());
        }

        self.provides.first().map(|provided| ServiceSection {
            provides: provided.interface.clone(),
            backend_name: provided
                .backend_name
                .clone()
                .unwrap_or_else(|| self.package.id.clone()),
            priority: provided.priority,
        })
    }

    pub fn required_plugin_dependencies(&self) -> Vec<String> {
        self.dependencies
            .plugins
            .iter()
            .filter(|(_, spec)| !spec.is_optional())
            .map(|(plugin_id, _)| plugin_id.clone())
            .collect()
    }

    pub fn slot_host_dependencies(&self) -> Vec<String> {
        self.slot_contributions
            .keys()
            .filter_map(|slot_id| slot_id.split_once(':').map(|(plugin_id, _)| plugin_id))
            .map(ToString::to_string)
            .collect()
    }

    pub fn exported_component_tag(&self) -> Option<&str> {
        self.exports
            .component
            .as_ref()
            .map(|component| component.tag.as_str())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageSection {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
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

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AccessibilitySection {
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PluginType {
    Surface,
    Widget,
    Backend,
    Theme,
    LanguagePack,
    IconPack,
    Interface,
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
            Self::Interface => write!(f, "interface"),
        }
    }
}

/// Legacy single-service declaration used by current `mesh.toml` manifests.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceSection {
    pub provides: String,
    pub backend_name: String,
    #[serde(default)]
    pub priority: u32,
}

/// New-style backend/interface declaration from `plugin.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvidedInterface {
    pub interface: String,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub backend_name: Option<String>,
    #[serde(default)]
    pub priority: u32,
    #[serde(default)]
    pub optional_capabilities: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CompatibilitySection {
    #[serde(default)]
    pub mesh: Option<String>,
    #[serde(default)]
    pub compositors: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DependenciesSection {
    #[serde(default)]
    pub plugins: HashMap<String, DependencySpec>,
    #[serde(default)]
    pub interfaces: Vec<InterfaceDependency>,
    #[serde(default)]
    pub icon_packs: OptionalDependencyGroup,
    #[serde(default)]
    pub language_packs: OptionalDependencyGroup,
    #[serde(default)]
    pub themes: OptionalDependencyGroup,
    #[serde(default)]
    pub native_libs: Vec<NativeDependency>,
    #[serde(default)]
    pub binaries: Vec<BinaryDependency>,
    #[serde(default)]
    pub fonts: Vec<FontDependency>,
}

impl DependenciesSection {
    pub fn is_empty(&self) -> bool {
        self.plugins.is_empty()
            && self.interfaces.is_empty()
            && self.icon_packs.is_empty()
            && self.language_packs.is_empty()
            && self.themes.is_empty()
            && self.native_libs.is_empty()
            && self.binaries.is_empty()
            && self.fonts.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum DependencySpec {
    Simple(String),
    Detailed {
        version: String,
        #[serde(default)]
        optional: Option<bool>,
    },
}

impl DependencySpec {
    pub fn is_optional(&self) -> bool {
        matches!(
            self,
            Self::Detailed {
                optional: Some(true),
                ..
            }
        )
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OptionalDependencyGroup {
    #[serde(default)]
    pub required: Vec<String>,
    #[serde(default)]
    pub optional: Vec<String>,
}

impl OptionalDependencyGroup {
    pub fn is_empty(&self) -> bool {
        self.required.is_empty() && self.optional.is_empty()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterfaceDependency {
    pub name: String,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NativeDependency {
    pub name: String,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BinaryDependency {
    pub name: String,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub packages: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FontDependency {
    pub family: String,
    #[serde(default)]
    pub reason: Option<String>,
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
        self.required.iter().map(Capability::new).collect()
    }

    pub fn optional_capabilities(&self) -> Vec<Capability> {
        self.optional.iter().map(Capability::new).collect()
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
    #[serde(default)]
    pub namespace: Option<String>,
    #[serde(default)]
    pub schema_path: Option<String>,
    #[serde(default)]
    pub inline_schema: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct I18nSection {
    pub default_locale: String,
    pub bundled: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeSection {
    #[serde(default)]
    pub tokens_used: Vec<String>,
    #[serde(default)]
    pub base: Option<String>,
    #[serde(default)]
    pub modes: HashMap<String, String>,
    #[serde(default)]
    pub default_mode: Option<String>,
    #[serde(default)]
    pub extends: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterfaceSection {
    pub name: String,
    pub version: String,
    pub file: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtensionSection {
    pub interface: String,
    pub version: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExportsSection {
    #[serde(default)]
    pub component: Option<ComponentExport>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentExport {
    pub tag: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SlotDefinition {
    #[serde(default)]
    pub accepts: Option<String>,
    #[serde(default)]
    pub layout: Option<String>,
    #[serde(default)]
    pub max: Option<u32>,
    #[serde(default)]
    pub min: Option<u32>,
    #[serde(default)]
    pub default: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SlotContribution {
    #[serde(default)]
    pub widget: Option<String>,
    #[serde(default)]
    pub props: serde_json::Map<String, serde_json::Value>,
    #[serde(default)]
    pub order: Option<i64>,
    #[serde(default)]
    pub when: Option<String>,
    #[serde(default)]
    pub id: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SurfaceLayoutSection {
    /// "fixed" | "content_measured"
    #[serde(default)]
    pub size_policy: Option<String>,
    /// Use content-children bounds (vs root bounds) when measuring size
    #[serde(default)]
    pub prefers_content_children_sizing: Option<bool>,
    #[serde(default)]
    pub min_width: Option<u32>,
    #[serde(default)]
    pub max_width: Option<u32>,
    #[serde(default)]
    pub min_height: Option<u32>,
    #[serde(default)]
    pub max_height: Option<u32>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AssetsSection {
    #[serde(default)]
    pub icons: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManifestSource {
    MeshToml,
    PluginJson,
}

impl std::fmt::Display for ManifestSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MeshToml => write!(f, "mesh.toml"),
            Self::PluginJson => write!(f, "plugin.json"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct LoadedManifest {
    pub manifest: Manifest,
    pub path: PathBuf,
    pub source: ManifestSource,
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum DependencyGraphError {
    #[error("plugin dependency cycle detected: {cycle:?}")]
    Cycle { cycle: Vec<String> },
}

pub fn validate_plugin_dependency_graph<'a, I>(manifests: I) -> Result<(), DependencyGraphError>
where
    I: IntoIterator<Item = &'a Manifest>,
{
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum VisitState {
        Visiting,
        Visited,
    }

    let manifest_map: HashMap<String, &Manifest> = manifests
        .into_iter()
        .map(|manifest| (manifest.package.id.clone(), manifest))
        .collect();
    let mut state = HashMap::<String, VisitState>::new();
    let mut stack = Vec::<String>::new();
    let mut plugin_ids: Vec<String> = manifest_map.keys().cloned().collect();
    plugin_ids.sort();

    fn adjacency(manifest: &Manifest, known_plugins: &HashMap<String, &Manifest>) -> Vec<String> {
        let mut neighbors = manifest.required_plugin_dependencies();
        neighbors.extend(
            manifest
                .slot_host_dependencies()
                .into_iter()
                .filter(|plugin_id| known_plugins.contains_key(plugin_id)),
        );
        neighbors.sort();
        neighbors.dedup();
        neighbors
    }

    fn visit(
        plugin_id: &str,
        manifest_map: &HashMap<String, &Manifest>,
        state: &mut HashMap<String, VisitState>,
        stack: &mut Vec<String>,
    ) -> Result<(), DependencyGraphError> {
        state.insert(plugin_id.to_string(), VisitState::Visiting);
        stack.push(plugin_id.to_string());

        for neighbor in adjacency(manifest_map[plugin_id], manifest_map) {
            match state.get(&neighbor).copied() {
                Some(VisitState::Visited) => continue,
                Some(VisitState::Visiting) => {
                    let cycle_start = stack
                        .iter()
                        .position(|entry| entry == &neighbor)
                        .unwrap_or_default();
                    let mut cycle = stack[cycle_start..].to_vec();
                    cycle.push(neighbor);
                    return Err(DependencyGraphError::Cycle { cycle });
                }
                None => visit(&neighbor, manifest_map, state, stack)?,
            }
        }

        stack.pop();
        state.insert(plugin_id.to_string(), VisitState::Visited);
        Ok(())
    }

    for plugin_id in plugin_ids {
        if state.contains_key(&plugin_id) {
            continue;
        }
        visit(&plugin_id, &manifest_map, &mut state, &mut stack)?;
    }

    Ok(())
}

#[derive(Debug, thiserror::Error)]
pub enum ManifestError {
    #[error("failed to read manifest: {0}")]
    Io(#[from] std::io::Error),

    #[error("failed to parse mesh.toml manifest: {0}")]
    Toml(#[from] toml::de::Error),

    #[error("failed to parse plugin.json manifest: {0}")]
    Json(#[from] serde_json::Error),

    #[error("no manifest found in plugin directory {0}")]
    NotFound(PathBuf),
}

pub fn load_manifest(plugin_dir: &Path) -> Result<LoadedManifest, ManifestError> {
    let plugin_json_path = plugin_dir.join("plugin.json");
    if plugin_json_path.exists() {
        return load_plugin_json(&plugin_json_path);
    }

    let mesh_toml_path = plugin_dir.join("mesh.toml");
    if mesh_toml_path.exists() {
        return load_mesh_toml(&mesh_toml_path);
    }

    Err(ManifestError::NotFound(plugin_dir.to_path_buf()))
}

fn load_plugin_json(path: &Path) -> Result<LoadedManifest, ManifestError> {
    let content = std::fs::read_to_string(path)?;
    let parsed: JsonManifest = serde_json::from_str(&content)?;

    Ok(LoadedManifest {
        manifest: parsed.into_manifest(),
        path: path.to_path_buf(),
        source: ManifestSource::PluginJson,
    })
}

fn load_mesh_toml(path: &Path) -> Result<LoadedManifest, ManifestError> {
    let content = std::fs::read_to_string(path)?;
    let parsed: TomlManifest = toml::from_str(&content)?;

    Ok(LoadedManifest {
        manifest: parsed.into_manifest(),
        path: path.to_path_buf(),
        source: ManifestSource::MeshToml,
    })
}

#[derive(Debug, Clone, Deserialize)]
struct TomlManifest {
    package: PackageSection,
    #[serde(default)]
    compatibility: CompatibilitySection,
    #[serde(default)]
    dependencies: HashMap<String, DependencySpec>,
    #[serde(default)]
    capabilities: CapabilitiesSection,
    #[serde(default)]
    entrypoints: EntrypointsSection,
    #[serde(default)]
    accessibility: Option<AccessibilitySection>,
    #[serde(default)]
    settings: Option<TomlSettingsSection>,
    #[serde(default)]
    i18n: Option<TomlI18nSection>,
    #[serde(default)]
    theme: Option<TomlThemeSection>,
    #[serde(default)]
    service: Option<ServiceSection>,
    #[serde(default)]
    provides: Vec<ProvidedInterface>,
    #[serde(default)]
    interface: Option<InterfaceSection>,
    #[serde(default)]
    extensions: Vec<ExtensionSection>,
    #[serde(default)]
    exports: ExportsSection,
    #[serde(default)]
    provides_slots: HashMap<String, SlotDefinition>,
    #[serde(default, rename = "slot-contributions")]
    slot_contributions: HashMap<String, Vec<SlotContribution>>,
    #[serde(default)]
    assets: Option<AssetsSection>,
    #[serde(default)]
    translations: HashMap<String, HashMap<String, String>>,
    #[serde(default, rename = "surface-layout")]
    surface_layout: Option<SurfaceLayoutSection>,
}

impl TomlManifest {
    fn into_manifest(self) -> Manifest {
        Manifest {
            package: self.package,
            compatibility: self.compatibility,
            dependencies: DependenciesSection {
                plugins: self.dependencies,
                ..DependenciesSection::default()
            },
            capabilities: self.capabilities,
            entrypoints: self.entrypoints,
            accessibility: self.accessibility,
            settings: self.settings.map(TomlSettingsSection::into_settings),
            i18n: self.i18n.map(TomlI18nSection::into_i18n),
            theme: self.theme.map(TomlThemeSection::into_theme),
            service: self.service,
            provides: self.provides,
            interface: self.interface,
            extensions: self.extensions,
            exports: self.exports,
            provides_slots: self.provides_slots,
            slot_contributions: self.slot_contributions,
            assets: self.assets,
            translations: self.translations,
            surface_layout: self.surface_layout,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
struct TomlSettingsSection {
    #[serde(default)]
    namespace: Option<String>,
    #[serde(default)]
    schema: Option<toml::Value>,
}

impl TomlSettingsSection {
    fn into_settings(self) -> SettingsSection {
        let (schema_path, inline_schema) = match self.schema {
            Some(toml::Value::String(path)) => (Some(path), None),
            Some(other) => (None, serde_json::to_value(other).ok()),
            None => (None, None),
        };

        SettingsSection {
            namespace: self.namespace,
            schema_path,
            inline_schema,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
struct TomlI18nSection {
    default_locale: String,
    #[serde(default, alias = "translations")]
    bundled: String,
}

impl TomlI18nSection {
    fn into_i18n(self) -> I18nSection {
        I18nSection {
            default_locale: self.default_locale,
            bundled: self.bundled,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
struct TomlThemeSection {
    #[serde(default)]
    tokens_used: Vec<String>,
    #[serde(default)]
    base: Option<String>,
    #[serde(default)]
    modes: HashMap<String, String>,
    #[serde(default)]
    default_mode: Option<String>,
    #[serde(default)]
    extends: Option<String>,
}

impl TomlThemeSection {
    fn into_theme(self) -> ThemeSection {
        ThemeSection {
            tokens_used: self.tokens_used,
            base: self.base,
            modes: self.modes,
            default_mode: self.default_mode,
            extends: self.extends,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
struct JsonManifest {
    id: String,
    #[serde(default)]
    name: Option<String>,
    version: String,
    #[serde(rename = "type")]
    plugin_type: PluginType,
    api_version: String,
    #[serde(default)]
    license: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    authors: Vec<String>,
    #[serde(default)]
    repository: Option<String>,
    #[serde(default)]
    compatibility: CompatibilitySection,
    #[serde(default)]
    dependencies: JsonDependenciesSection,
    #[serde(default)]
    capabilities: CapabilitiesSection,
    #[serde(default)]
    entrypoints: EntrypointsSection,
    #[serde(default)]
    accessibility: Option<AccessibilitySection>,
    #[serde(default)]
    settings: Option<JsonSettingsSection>,
    #[serde(default)]
    i18n: Option<JsonI18nSection>,
    #[serde(default)]
    theme: Option<JsonThemeSection>,
    #[serde(default)]
    provides: Vec<ProvidedInterface>,
    #[serde(default)]
    interface: Option<InterfaceSection>,
    #[serde(default)]
    extensions: Vec<ExtensionSection>,
    #[serde(default)]
    exports: ExportsSection,
    #[serde(default)]
    provides_slots: HashMap<String, SlotDefinition>,
    #[serde(default)]
    slot_contributions: HashMap<String, Vec<SlotContribution>>,
    #[serde(default)]
    assets: Option<AssetsSection>,
    #[serde(default)]
    translations: HashMap<String, HashMap<String, String>>,
    #[serde(default, rename = "surface_layout")]
    surface_layout: Option<SurfaceLayoutSection>,
}

impl JsonManifest {
    fn into_manifest(self) -> Manifest {
        Manifest {
            package: PackageSection {
                id: self.id,
                name: self.name,
                version: self.version,
                plugin_type: self.plugin_type,
                api_version: self.api_version,
                license: self.license,
                description: self.description,
                authors: self.authors,
                repository: self.repository,
            },
            compatibility: self.compatibility,
            dependencies: self.dependencies.into_dependencies(),
            capabilities: self.capabilities,
            entrypoints: self.entrypoints,
            accessibility: self.accessibility,
            settings: self.settings.map(JsonSettingsSection::into_settings),
            i18n: self.i18n.map(JsonI18nSection::into_i18n),
            theme: self.theme.map(JsonThemeSection::into_theme),
            service: None,
            provides: self.provides,
            interface: self.interface,
            extensions: self.extensions,
            exports: self.exports,
            provides_slots: self.provides_slots,
            slot_contributions: self.slot_contributions,
            assets: self.assets,
            translations: self.translations,
            surface_layout: self.surface_layout,
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
struct JsonDependenciesSection {
    #[serde(default)]
    plugins: HashMap<String, DependencySpec>,
    #[serde(default)]
    interfaces: Vec<InterfaceDependency>,
    #[serde(default)]
    icon_packs: OptionalDependencyGroup,
    #[serde(default)]
    language_packs: OptionalDependencyGroup,
    #[serde(default)]
    themes: OptionalDependencyGroup,
    #[serde(default)]
    native_libs: Vec<NativeDependency>,
    #[serde(default)]
    binaries: Vec<BinaryDependency>,
    #[serde(default)]
    fonts: Vec<FontDependency>,
}

impl JsonDependenciesSection {
    fn into_dependencies(self) -> DependenciesSection {
        DependenciesSection {
            plugins: self.plugins,
            interfaces: self.interfaces,
            icon_packs: self.icon_packs,
            language_packs: self.language_packs,
            themes: self.themes,
            native_libs: self.native_libs,
            binaries: self.binaries,
            fonts: self.fonts,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
struct JsonSettingsSection {
    #[serde(default)]
    namespace: Option<String>,
    #[serde(default)]
    schema: Option<serde_json::Value>,
}

impl JsonSettingsSection {
    fn into_settings(self) -> SettingsSection {
        SettingsSection {
            namespace: self.namespace,
            schema_path: None,
            inline_schema: self.schema,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
struct JsonI18nSection {
    default_locale: String,
    bundled: String,
}

impl JsonI18nSection {
    fn into_i18n(self) -> I18nSection {
        I18nSection {
            default_locale: self.default_locale,
            bundled: self.bundled,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
struct JsonThemeSection {
    #[serde(default)]
    tokens_used: Vec<String>,
    #[serde(default)]
    base: Option<String>,
    #[serde(default)]
    modes: HashMap<String, String>,
    #[serde(default)]
    default_mode: Option<String>,
    #[serde(default)]
    extends: Option<String>,
}

impl JsonThemeSection {
    fn into_theme(self) -> ThemeSection {
        ThemeSection {
            tokens_used: self.tokens_used,
            base: self.base,
            modes: self.modes,
            default_mode: self.default_mode,
            extends: self.extends,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_legacy_mesh_toml_manifest() {
        let content = r#"
[package]
id = "@mesh/panel"
version = "0.1.0"
type = "surface"
api_version = "0.1"

[service]
provides = "audio"
backend_name = "PipeWire"
priority = 100

[entrypoints]
main = "src/main.mesh"
"#;

        let parsed: TomlManifest = toml::from_str(content).unwrap();
        let manifest = parsed.into_manifest();

        assert_eq!(manifest.package.id, "@mesh/panel");
        assert_eq!(manifest.primary_service().unwrap().provides, "audio");
    }

    #[test]
    fn parses_plugin_json_manifest() {
        let content = r#"
{
  "id": "@mesh/panel",
  "version": "0.1.0",
  "type": "surface",
  "api_version": "0.1",
  "dependencies": {
    "plugins": {
      "@mesh/audio-contract": ">=1.0.0"
    },
    "interfaces": [
      { "name": "mesh.audio", "version": ">=1.0", "required": false }
    ]
  },
  "entrypoints": {
    "main": "src/main.mesh"
  },
  "exports": {
    "component": {
      "tag": "PanelRoot"
    }
  },
  "provides": [
    {
      "interface": "mesh.audio",
      "version": "1.0",
      "backend_name": "PipeWire",
      "priority": 100
    }
  ]
}
"#;

        let parsed: JsonManifest = serde_json::from_str(content).unwrap();
        let manifest = parsed.into_manifest();

        assert_eq!(manifest.package.id, "@mesh/panel");
        assert_eq!(manifest.exported_component_tag(), Some("PanelRoot"));
        assert_eq!(
            manifest.dependencies.plugins["@mesh/audio-contract"],
            DependencySpec::Simple(">=1.0.0".to_string())
        );
        assert_eq!(manifest.declared_provides()[0].interface, "mesh.audio");
    }

    fn manifest_with_dependencies(
        id: &str,
        dependencies: &[(&str, bool)],
        slot_contributions: &[&str],
    ) -> Manifest {
        Manifest {
            package: PackageSection {
                id: id.to_string(),
                name: None,
                version: "0.1.0".into(),
                plugin_type: PluginType::Widget,
                api_version: "0.1".into(),
                license: None,
                description: None,
                authors: Vec::new(),
                repository: None,
            },
            compatibility: CompatibilitySection::default(),
            dependencies: DependenciesSection {
                plugins: dependencies
                    .iter()
                    .map(|(dependency_id, optional)| {
                        let spec = if *optional {
                            DependencySpec::Detailed {
                                version: ">=0.1.0".into(),
                                optional: Some(true),
                            }
                        } else {
                            DependencySpec::Simple(">=0.1.0".into())
                        };
                        ((*dependency_id).to_string(), spec)
                    })
                    .collect(),
                ..DependenciesSection::default()
            },
            capabilities: CapabilitiesSection::default(),
            entrypoints: EntrypointsSection {
                main: Some("src/main.mesh".into()),
                settings_ui: None,
            },
            accessibility: None,
            settings: None,
            i18n: None,
            theme: None,
            service: None,
            provides: Vec::new(),
            interface: None,
            extensions: Vec::new(),
            exports: ExportsSection::default(),
            provides_slots: HashMap::new(),
            slot_contributions: slot_contributions
                .iter()
                .map(|slot_id| ((*slot_id).to_string(), vec![SlotContribution::default()]))
                .collect(),
            assets: None,
            translations: HashMap::new(),
            surface_layout: None,
        }
    }

    #[test]
    fn detects_required_plugin_dependency_cycles() {
        let a = manifest_with_dependencies("@mesh/a", &[("@mesh/b", false)], &[]);
        let b = manifest_with_dependencies("@mesh/b", &[("@mesh/a", false)], &[]);

        let err = validate_plugin_dependency_graph([&a, &b]).unwrap_err();
        match err {
            DependencyGraphError::Cycle { cycle } => {
                assert_eq!(cycle, vec!["@mesh/a", "@mesh/b", "@mesh/a"]);
            }
        }
    }

    #[test]
    fn ignores_optional_dependencies_for_cycle_detection() {
        let a = manifest_with_dependencies("@mesh/a", &[("@mesh/b", true)], &[]);
        let b = manifest_with_dependencies("@mesh/b", &[("@mesh/a", false)], &[]);

        validate_plugin_dependency_graph([&a, &b]).unwrap();
    }

    #[test]
    fn detects_cycles_through_slot_hosts() {
        let a = manifest_with_dependencies("@mesh/a", &[("@mesh/b", false)], &[]);
        let b = manifest_with_dependencies("@mesh/b", &[], &["@mesh/a:main"]);

        let err = validate_plugin_dependency_graph([&a, &b]).unwrap_err();
        match err {
            DependencyGraphError::Cycle { cycle } => {
                assert_eq!(cycle, vec!["@mesh/a", "@mesh/b", "@mesh/a"]);
            }
        }
    }
}
