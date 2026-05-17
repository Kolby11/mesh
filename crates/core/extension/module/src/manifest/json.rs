use super::{
    AccessibilitySection, AssetsSection, BinaryDependency, CapabilitiesSection,
    CompatibilitySection, DependenciesSection, DependencySpec, EntrypointsSection, ExportsSection,
    ExtensionSection, FontDependency, I18nSection, IconPackSection, IconRequirementsSection,
    IconsSection, InterfaceDependency, InterfaceSection, KeybindsSection, Manifest, ModuleSection,
    ModuleType, NativeDependency, OptionalDependencyGroup, ProvidedInterface, SettingsSection,
    SlotContribution, SlotDefinition, SurfaceLayoutSection, ThemeDefaultsSection, ThemeSection,
};
use mesh_core_theme::TokenValue;
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Clone, Deserialize)]
pub(super) struct JsonManifest {
    id: String,
    #[serde(default)]
    name: Option<String>,
    version: String,
    #[serde(rename = "type")]
    module_type: ModuleType,
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
    keybinds: KeybindsSection,
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
    icons: Option<IconsSection>,
    #[serde(default)]
    icon_pack: Option<IconPackSection>,
    #[serde(default)]
    icon_requirements: IconRequirementsSection,
    #[serde(default)]
    translations: HashMap<String, HashMap<String, String>>,
    #[serde(default, rename = "surface_layout")]
    surface_layout: Option<SurfaceLayoutSection>,
}

impl JsonManifest {
    pub(super) fn into_manifest(self) -> Manifest {
        Manifest {
            package: ModuleSection {
                id: self.id,
                name: self.name,
                version: self.version,
                module_type: self.module_type,
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
            keybinds: self.keybinds,
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
            icons: self.icons,
            icon_pack: self.icon_pack,
            icon_requirements: self.icon_requirements,
            translations: self.translations,
            surface_layout: self.surface_layout,
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
struct JsonDependenciesSection {
    #[serde(default)]
    modules: HashMap<String, DependencySpec>,
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
            modules: self.modules,
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

impl JsonThemeSection {
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
