use mesh_core_capability::Capability;
use mesh_core_elements::style::is_supported_css_property;
use mesh_core_theme::TokenValue;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// The normalized contents of a module manifest, regardless of source format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub package: ModuleSection,
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
    pub keybinds: KeybindsSection,
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
    pub icons: Option<IconsSection>,
    #[serde(default)]
    pub icon_pack: Option<IconPackSection>,
    #[serde(default)]
    pub icon_requirements: IconRequirementsSection,
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
                    base_module: None,
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

    pub fn required_module_dependencies(&self) -> Vec<String> {
        self.dependencies
            .modules
            .iter()
            .filter(|(_, spec)| !spec.is_optional())
            .map(|(module_id, _)| module_id.clone())
            .collect()
    }

    pub fn slot_host_dependencies(&self) -> Vec<String> {
        self.slot_contributions
            .keys()
            .filter_map(|slot_id| slot_id.split_once(':').map(|(module_id, _)| module_id))
            .map(ToString::to_string)
            .collect()
    }

    pub fn exported_component_tag(&self) -> Option<&str> {
        self.exports
            .component
            .as_ref()
            .map(|component| component.tag.as_str())
    }

    pub fn validate_keybinds(&self) -> Result<(), String> {
        self.keybinds.validate()
    }
}

fn validate_theme_token_key(token_name: &str) -> Result<(), String> {
    if token_name.trim().is_empty() {
        return Err("mesh.theme.tokens cannot contain empty names".into());
    }
    if !token_name.contains('.') {
        return Err(format!(
            "mesh.theme.tokens entry '{token_name}' must use a dotted namespace"
        ));
    }
    Ok(())
}

fn validate_theme_value_references(value: &str) -> Result<(), String> {
    let mut rest = value;
    while let Some(start) = rest.find("token(") {
        let token_start = start + "token(".len();
        let token_end = rest[token_start..]
            .find(')')
            .map(|offset| token_start + offset)
            .ok_or_else(|| format!("invalid token() reference in '{value}'"))?;
        let token_name = rest[token_start..token_end].trim();
        if token_name.is_empty() {
            return Err(format!("empty token() reference in '{value}'"));
        }
        if let Some(module_ref) = token_name.strip_prefix('@') {
            let Some((module_id, owned_token)) = module_ref.split_once('.') else {
                return Err(format!(
                    "explicit module token reference '{token_name}' must use <module-id>.<token-name>"
                ));
            };
            if module_id.trim().is_empty() || owned_token.trim().is_empty() {
                return Err(format!(
                    "explicit module token reference '{token_name}' must use <module-id>.<token-name>"
                ));
            }
        }
        rest = &rest[token_end + 1..];
    }

    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleSection {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    pub version: String,
    #[serde(rename = "type")]
    pub module_type: ModuleType,
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
pub enum ModuleType {
    Surface,
    Widget,
    Backend,
    Theme,
    LanguagePack,
    IconPack,
    Interface,
}

impl std::fmt::Display for ModuleType {
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

/// New-style backend/interface declaration from `package.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvidedInterface {
    pub interface: String,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub base_module: Option<String>,
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
    pub modules: HashMap<String, DependencySpec>,
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
        self.modules.is_empty()
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
    pub optional: bool,
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

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct KeybindsSection {
    #[serde(default, flatten)]
    pub actions: HashMap<String, KeybindAction>,
}

impl KeybindsSection {
    pub fn is_empty(&self) -> bool {
        self.actions.is_empty()
    }

    pub fn validate(&self) -> Result<(), String> {
        for (action_id, action) in &self.actions {
            if action_id.trim().is_empty() {
                return Err("mesh.keybinds cannot contain empty action ids".into());
            }
            action.validate(action_id)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeybindAction {
    #[serde(default)]
    pub scope: KeybindScope,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub trigger: KeybindTrigger,
    #[serde(default, alias = "localizedTriggers")]
    pub localized_triggers: HashMap<String, KeybindTrigger>,
}

impl KeybindAction {
    fn validate(&self, action_id: &str) -> Result<(), String> {
        validate_optional_keybind_text_key(action_id, "label", self.label.as_deref())?;
        validate_optional_keybind_text_key(action_id, "description", self.description.as_deref())?;
        validate_optional_keybind_text_key(action_id, "category", self.category.as_deref())?;
        self.trigger.validate(action_id)?;
        for locale in self.localized_triggers.keys() {
            if locale.trim().is_empty() {
                return Err(format!(
                    "mesh.keybinds.{action_id}.localized_triggers cannot contain empty locale ids"
                ));
            }
        }
        Ok(())
    }
}

impl Default for KeybindAction {
    fn default() -> Self {
        Self {
            scope: KeybindScope::default(),
            label: None,
            description: None,
            category: None,
            trigger: KeybindTrigger::default(),
            localized_triggers: HashMap::new(),
        }
    }
}

fn validate_optional_keybind_text_key(
    action_id: &str,
    field: &str,
    value: Option<&str>,
) -> Result<(), String> {
    if value.is_some_and(|value| value.trim().is_empty()) {
        return Err(format!("mesh.keybinds.{action_id}.{field} cannot be empty"));
    }
    Ok(())
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeybindTrigger {
    #[serde(default)]
    pub kind: KeybindTriggerKind,
    #[serde(default)]
    pub key: Option<String>,
    #[serde(default)]
    pub modifiers: Vec<String>,
}

impl KeybindTrigger {
    fn validate(&self, action_id: &str) -> Result<(), String> {
        match self.kind {
            KeybindTriggerKind::Shortcut | KeybindTriggerKind::AccessKey => {
                if self.key.as_ref().is_some_and(|key| key.trim().is_empty()) {
                    return Err(format!(
                        "mesh.keybinds.{action_id}.trigger.key cannot be empty"
                    ));
                }
            }
        }

        for modifier in &self.modifiers {
            if modifier.trim().is_empty() {
                return Err(format!(
                    "mesh.keybinds.{action_id}.trigger.modifiers cannot contain empty values"
                ));
            }
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeybindScope {
    #[default]
    Surface,
    Access,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeybindTriggerKind {
    #[default]
    Shortcut,
    AccessKey,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct I18nSection {
    pub default_locale: String,
    pub bundled: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeSection {
    #[serde(default)]
    pub tokens: HashMap<String, TokenValue>,
    #[serde(default)]
    pub defaults: ThemeDefaultsSection,
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

impl ThemeSection {
    pub fn validate(&self) -> Result<(), String> {
        for token_name in self.tokens.keys() {
            validate_theme_token_key(token_name)?;
        }

        for (component_name, defaults) in &self.defaults.components {
            if component_name.trim().is_empty() {
                return Err("mesh.theme.defaults.components keys cannot be empty".into());
            }
            for (property, value) in defaults {
                if !is_supported_css_property(property) {
                    return Err(format!(
                        "mesh.theme.defaults.components.{component_name} uses unsupported CSS property '{property}'"
                    ));
                }
                validate_theme_value_references(value)?;
            }
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ThemeDefaultsSection {
    #[serde(default)]
    pub components: HashMap<String, HashMap<String, String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterfaceSection {
    pub name: String,
    pub version: String,
    pub file: String,
    #[serde(default)]
    pub extends: Option<String>,
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
    pub icons: Option<IconAssets>,
}

/// Module-shipped icons. Authoring shortcut: `"icons": "assets/icons"` is
/// equivalent to `"icons": { "path": "assets/icons", "kind": "xdg" }` -
/// the directory is treated as an XDG icon pack rooted there.
///
/// For font-glyph icon packs (Nerd Fonts and similar), use the object form
/// with `kind = "font"` and the in-pack paths to the font file and glyph
/// map JSON. The shell registers the pack at `<module_id>` so authors can
/// reference its assets via candidates like `<module_id>:audio-volume-muted`
/// in `icons.toml`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum IconAssets {
    Path(String),
    Detailed(DetailedIconAssets),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetailedIconAssets {
    pub path: String,
    #[serde(default)]
    pub kind: IconAssetsKind,
    /// Required when `kind = "font"`. Path to the font file relative to
    /// `path`. Ignored for `kind = "xdg"`.
    #[serde(default)]
    pub font_file: Option<String>,
    /// Required when `kind = "font"`. Path to the JSON glyph map relative
    /// to `path`. Ignored for `kind = "xdg"`.
    #[serde(default)]
    pub glyph_map: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IconAssetsKind {
    #[default]
    Xdg,
    Font,
}

impl IconAssets {
    pub fn path(&self) -> &str {
        match self {
            Self::Path(path) => path,
            Self::Detailed(details) => &details.path,
        }
    }
}

/// Frontend-side icon configuration declared in `package.json`. Mappings
/// belong in icon-pack modules — frontends only declare per-icon
/// **overrides** (an author-side escape hatch for pinning a specific glyph
/// regardless of the active icon-pack chain) and an opt-out flag for the
/// shell's implicit default pack.
///
/// Override values use the `<pack-id>/<asset-name>` syntax shared with
/// pack-qualified template names and shell user overrides.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct IconsSection {
    #[serde(default)]
    pub overrides: HashMap<String, String>,
    /// When `true`, the shell's `icons.default_pack` is **not** prepended
    /// to this frontend's effective icon-pack chain. Frontends typically
    /// leave this `false` so the user's chosen default applies.
    #[serde(default)]
    pub ignore_shell_default: bool,
}

impl IconsSection {
    pub fn is_empty(&self) -> bool {
        self.overrides.is_empty() && !self.ignore_shell_default
    }
}

/// Icon-pack module section (`mesh.kind = "icon-pack"`). Contains only
/// the mapping table and metadata — no icon assets are shipped.
///
/// `id` is the short alias used in pack-qualified syntax
/// (`<pack-id>/<asset-name>`). Distinct from the full module id so the
/// alias can be short and stable.
///
/// `requires` declares system assets the pack expects to resolve against.
/// All version constraints are **soft** — a missing or older asset logs
/// a warning at discovery time but never blocks loading.
///
/// `axes` declares which variable-font axes the underlying assets
/// expose; the painter uses this to gate CSS `--icon-*` custom
/// properties.
///
/// `mappings` is a flat 1:1 table from logical name to a target string
/// of the form `<asset-pack>/<asset-name>` where `asset-pack` is either
/// an XDG icon theme name installed on the system, an alias declared in
/// `requires.fonts`, or an absolute file path.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct IconPackSection {
    pub id: String,
    #[serde(default)]
    pub requires: IconPackRequires,
    #[serde(default)]
    pub axes: IconPackAxes,
    #[serde(default)]
    pub mappings: HashMap<String, String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct IconPackRequires {
    #[serde(default)]
    pub fonts: Vec<IconPackFontRequirement>,
    #[serde(default)]
    pub themes: Vec<IconPackThemeRequirement>,
}

/// One entry in an icon-pack's `requires.fonts` list. `alias` is the
/// short name used in mapping targets (`<alias>/<glyph-name>`); `family`
/// is the actual fontconfig family name to match against; `glyph_map`
/// is a path inside the pack module pointing at a JSON codepoints file
/// (or Google's `name codepoint` text format), used to translate glyph
/// names to codepoints at render time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IconPackFontRequirement {
    pub alias: String,
    pub family: String,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub glyph_map: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IconPackThemeRequirement {
    pub name: String,
    #[serde(default)]
    pub version: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct IconPackAxes {
    #[serde(default)]
    pub fill: bool,
    #[serde(default)]
    pub weight: bool,
    #[serde(default)]
    pub grade: bool,
    #[serde(default)]
    pub optical_size: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct IconRequirementsSection {
    #[serde(default)]
    pub required: Vec<String>,
    #[serde(default)]
    pub optional: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManifestSource {
    CanonicalModuleJson,
    LegacyPackageJson,
    LegacyMeshToml,
    LegacyModuleJson,
}

impl std::fmt::Display for ManifestSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CanonicalModuleJson => write!(f, "module.json"),
            Self::LegacyPackageJson => write!(f, "package.json (legacy migration)"),
            Self::LegacyMeshToml => write!(f, "mesh.toml (legacy migration)"),
            Self::LegacyModuleJson => write!(f, "module.json (legacy migration)"),
        }
    }
}
