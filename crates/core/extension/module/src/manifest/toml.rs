use super::{
    AccessibilitySection, AssetsSection, CapabilitiesSection, CompatibilitySection,
    DependenciesSection, DependencySpec, EntrypointsSection, ExportsSection, ExtensionSection,
    I18nSection, IconPackSection, IconRequirementsSection, IconsSection, InterfaceSection,
    KeybindsSection, Manifest, ModuleSection, ProvidedInterface, ServiceSection,
    SlotContribution, SlotDefinition, SurfaceLayoutSection, ThemeDefaultsSection, ThemeSection,
};
use mesh_core_theme::TokenValue;
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Clone, Deserialize)]
pub(super) struct TomlManifest {
    package: ModuleSection,
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
    keybinds: KeybindsSection,
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
    icons: Option<IconsSection>,
    #[serde(default)]
    icon_pack: Option<IconPackSection>,
    #[serde(default)]
    icon_requirements: IconRequirementsSection,
    #[serde(default)]
    translations: HashMap<String, HashMap<String, String>>,
    #[serde(default, rename = "surface-layout")]
    surface_layout: Option<SurfaceLayoutSection>,
}

impl TomlManifest {
    pub(super) fn into_manifest(self) -> Manifest {
        Manifest {
            package: self.package,
            compatibility: self.compatibility,
            dependencies: DependenciesSection {
                modules: self.dependencies,
                ..DependenciesSection::default()
            },
            capabilities: self.capabilities,
            entrypoints: self.entrypoints,
            accessibility: self.accessibility,
            keybinds: self.keybinds,
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
            icons: self.icons,
            icon_pack: self.icon_pack,
            icon_requirements: self.icon_requirements,
            translations: self.translations,
            surface_layout: self.surface_layout,
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
    tokens: HashMap<String, TokenValue>,
    #[serde(default)]
    defaults: ThemeDefaultsSection,
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
            tokens: self.tokens,
            defaults: self.defaults,
            tokens_used: self.tokens_used,
            base: self.base,
            modes: self.modes,
            default_mode: self.default_mode,
            extends: self.extends,
        }
    }
}
