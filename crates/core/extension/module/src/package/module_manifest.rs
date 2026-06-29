use super::{ModuleManifestDiagnostic, ModuleManifestError, validate_relative_path};
use crate::manifest::{self, CapabilitiesSection, DependencySpec, Manifest, ModuleType};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ModuleManifest {
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub license: Option<String>,
    #[serde(default)]
    pub repository: Option<ModuleRepository>,
    pub mesh: MeshModuleSection,
}

impl ModuleManifest {
    pub fn from_json_str(input: &str) -> Result<Self, ModuleManifestError> {
        let mut parsed: Self =
            serde_json::from_str(input).map_err(|source| ModuleManifestError::Json {
                path: PathBuf::from("<inline>"),
                source,
            })?;
        parsed.normalize();
        parsed.validate()?;
        Ok(parsed)
    }

    pub fn from_path(path: &Path) -> Result<Self, ModuleManifestError> {
        let content = std::fs::read_to_string(path).map_err(|source| ModuleManifestError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        let mut parsed: Self =
            serde_json::from_str(&content).map_err(|source| ModuleManifestError::Json {
                path: path.to_path_buf(),
                source,
            })?;
        parsed.normalize();
        parsed.validate()?;
        Ok(parsed)
    }

    pub fn normalize(&mut self) {
        self.mesh.normalize();
    }

    pub fn validate(&self) -> Result<(), ModuleManifestError> {
        if self.name.trim().is_empty() {
            return Err(ModuleManifestError::Validation(
                "module name cannot be empty".into(),
            ));
        }
        if self.version.trim().is_empty() {
            return Err(ModuleManifestError::Validation(format!(
                "module {} version cannot be empty",
                self.name
            )));
        }
        if let Some(repository) = &self.repository {
            repository.validate()?;
        }
        self.mesh.validate()
    }

    pub(crate) fn localized_text_diagnostics(&self, path: &Path) -> Vec<ModuleManifestDiagnostic> {
        self.mesh
            .localized_text_diagnostics(path, self.name.as_str())
    }

    pub fn into_runtime_manifest(self) -> Manifest {
        let mut mesh = self.mesh;
        mesh.normalize();
        let settings =
            mesh.contributes
                .settings
                .clone()
                .map(|settings| manifest::SettingsSection {
                    namespace: Some(settings.namespace),
                    schema_path: None,
                    inline_schema: Some(settings.schema),
                });
        let i18n = mesh
            .contributes
            .i18n
            .first()
            .map(|i18n| manifest::I18nSection {
                default_locale: i18n.locale.clone(),
                bundled: i18n.path.clone(),
            });
        let contributed_theme =
            mesh.contributes
                .themes
                .first()
                .map(|theme| manifest::ThemeSection {
                    tokens: HashMap::new(),
                    defaults: manifest::ThemeDefaultsSection::default(),
                    tokens_used: Vec::new(),
                    base: None,
                    modes: theme.modes.clone(),
                    default_mode: theme.default_mode.clone(),
                    extends: None,
                });
        let assets = mesh
            .contributes
            .icons
            .first()
            .map(|icons| manifest::AssetsSection {
                icons: Some(manifest::IconAssets::Path(icons.path.clone())),
            });
        let provides = mesh
            .implementations()
            .cloned()
            .into_iter()
            .map(|provided| manifest::ProvidedInterface {
                interface: provided.interface,
                version: provided.version,
                base_module: provided.base_module,
                backend_name: provided
                    .label
                    .as_ref()
                    .map(manifest::LocalizedText::fallback_text)
                    .map(str::to_string)
                    .or(provided.provider),
                priority: provided.priority,
                optional_capabilities: Vec::new(),
            })
            .collect();
        let interface = mesh.interface.clone().and_then(|interface| {
            let version = interface.version?;
            let file = interface.file?;
            Some(manifest::InterfaceSection {
                name: interface.name,
                version,
                file,
                extends: interface.extends,
            })
        });

        let manifest_theme = mesh.theme.clone().or(contributed_theme);

        Manifest {
            package: manifest::ModuleSection {
                id: self.name,
                name: None,
                version: self.version,
                module_type: ModuleType::from(mesh.kind),
                api_version: mesh.api_version,
                license: self.license,
                description: self.description,
                authors: Vec::new(),
                repository: self.repository.map(|repository| repository.url),
            },
            compatibility: manifest::CompatibilitySection::default(),
            dependencies: mesh.dependencies.into_manifest_dependencies(),
            capabilities: mesh.capabilities,
            entrypoints: manifest::EntrypointsSection {
                main: mesh.entrypoints.main,
                settings_ui: mesh.entrypoints.settings_ui,
            },
            accessibility: mesh.accessibility,
            settings,
            keybinds: mesh.keybinds,
            i18n,
            theme: manifest_theme,
            service: None,
            provides,
            interface,
            extensions: Vec::new(),
            exports: manifest::ExportsSection::default(),
            provides_slots: HashMap::new(),
            slot_contributions: HashMap::new(),
            assets,
            icons: mesh.icons,
            icon_pack: mesh.icon_pack,
            icon_requirements: mesh.icon_requirements,
            translations: HashMap::new(),
            surface_layout: mesh.surface_layout,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MeshModuleSection {
    #[serde(rename = "apiVersion")]
    pub api_version: String,
    pub kind: ModuleKind,
    #[serde(default)]
    pub entry: Option<String>,
    #[serde(default)]
    pub uses: MeshUses,
    #[serde(default)]
    pub capabilities: CapabilitiesSection,
    #[serde(default)]
    pub i18n: MeshI18nSupport,
    #[serde(default)]
    pub entrypoints: MeshEntrypoints,
    #[serde(default)]
    pub keybinds: manifest::KeybindsSection,
    #[serde(default)]
    pub dependencies: MeshDependencies,
    #[serde(default)]
    pub provides: MeshProvides,
    #[serde(default)]
    pub implements: Vec<MeshProvidesDeclaration>,
    #[serde(default)]
    pub interface: Option<MeshInterfaceDeclaration>,
    #[serde(default)]
    pub theme: Option<manifest::ThemeSection>,
    #[serde(default)]
    pub contributes: MeshContributes,
    #[serde(default)]
    pub icons: Option<manifest::IconsSection>,
    #[serde(default)]
    pub icon_pack: Option<manifest::IconPackSection>,
    #[serde(default, rename = "iconRequirements", alias = "icon_requirements")]
    pub icon_requirements: manifest::IconRequirementsSection,
    #[serde(default)]
    pub accessibility: Option<manifest::AccessibilitySection>,
    /// Compact author-facing surface block. Core ships the canonical surface
    /// schema; authors declare only deltas here. Normalized into
    /// `surface_layout`, the single typed runtime home.
    #[serde(default)]
    pub surface: Option<manifest::SurfaceLayoutSection>,
    /// Legacy non-user renderer hints. Superseded by `surface`; kept so older
    /// manifests still parse. When `surface` is present it wins.
    #[serde(default, rename = "surfaceLayout", alias = "surface_layout")]
    pub surface_layout: Option<manifest::SurfaceLayoutSection>,
    #[serde(default)]
    pub experimental: serde_json::Value,
}

impl MeshModuleSection {
    fn normalize(&mut self) {
        if let Some(entry) = &self.entry
            && self.entrypoints.main.is_none()
        {
            self.entrypoints.main = Some(entry.clone());
        }
        // Auto-generate the default layout contribution for frontend modules that
        // set `entry` but omit an explicit `provides.layout` / `contributes.layout`.
        // This lets simple frontends declare only `entry` rather than also repeating
        // the same path under `provides.layout`.
        if self.kind == ModuleKind::Frontend
            && self.contributes.layout.is_empty()
            && self.provides.layout.is_empty()
            && let Some(entry) = &self.entry
        {
            self.provides.layout.push(LayoutContribution {
                id: "main".into(),
                entrypoint: entry.clone(),
                label: None,
            });
        }
        // The compact `mesh.surface` block is the canonical author surface for
        // surface placement/sizing/policy. It supersedes the legacy
        // `mesh.surfaceLayout` key: when present it becomes `surface_layout`,
        // the single typed runtime home read by `surface_layout_from_manifest`.
        if let Some(surface) = self.surface.take() {
            self.surface_layout = Some(surface);
        }
        self.dependencies.merge_uses(&self.uses);
        merge_unique(&mut self.capabilities.required, &self.uses.capabilities);
        merge_unique(
            &mut self.capabilities.optional,
            &self.uses.optional_capabilities,
        );
        merge_unique(
            &mut self.icon_requirements.required,
            &self.uses.icon_requirements.required,
        );
        merge_unique(
            &mut self.icon_requirements.optional,
            &self.uses.icon_requirements.optional,
        );
        self.contributes.merge_provides(&self.provides);
    }

    fn validate(&self) -> Result<(), ModuleManifestError> {
        if self.api_version.trim().is_empty() {
            return Err(ModuleManifestError::Validation(
                "mesh.apiVersion cannot be empty".into(),
            ));
        }
        self.i18n.validate()?;
        if self.kind == ModuleKind::Interface && self.interface.is_none() {
            return Err(ModuleManifestError::Validation(
                "interface modules must declare mesh.interface".into(),
            ));
        }
        self.uses.validate()?;
        if let Some(interface) = &self.interface {
            interface.validate()?;
            if self.kind == ModuleKind::Interface && interface.version.is_none() {
                return Err(ModuleManifestError::Validation(
                    "interface modules must declare mesh.interface.version".into(),
                ));
            }
            // `mesh.interface.file` (the contract TOML) is optional for v0: an
            // interface module may ship only name/version/domain and let the
            // contract be inferred from the provider's emitted state. When a
            // file is declared but absent on disk, the graph still reports
            // `missing_interface_contract_file`; contract-based validation
            // (capabilities, events) simply does not apply until a contract
            // file exists.
        }
        if self.kind == ModuleKind::Library && !self.capabilities.required.is_empty() {
            return Err(ModuleManifestError::Validation(
                "library modules must not declare mesh.capabilities.required; consuming modules request capabilities instead".into(),
            ));
        }
        if let Some(theme) = &self.theme {
            if self.kind != ModuleKind::Frontend {
                return Err(ModuleManifestError::Validation(
                    "mesh.theme is only supported for frontend modules".into(),
                ));
            }
            theme.validate().map_err(ModuleManifestError::Validation)?;
        }
        if self.icon_pack.is_some() && self.kind != ModuleKind::IconPack {
            return Err(ModuleManifestError::Validation(
                "mesh.icon_pack is only supported for icon-pack modules".into(),
            ));
        }
        if !self.contributes.icons.is_empty() && self.kind != ModuleKind::IconPack {
            return Err(ModuleManifestError::Validation(
                "mesh.provides.icons is only supported for icon-pack modules".into(),
            ));
        }
        if !self.contributes.fonts.is_empty() && self.kind != ModuleKind::FontPack {
            return Err(ModuleManifestError::Validation(
                "mesh.provides.fonts is only supported for font-pack modules".into(),
            ));
        }
        if !self.contributes.themes.is_empty() && self.kind != ModuleKind::Theme {
            return Err(ModuleManifestError::Validation(
                "mesh.provides.themes is only supported for theme modules".into(),
            ));
        }
        self.keybinds
            .validate()
            .map_err(ModuleManifestError::Validation)?;
        for provided in self.implementations() {
            provided.validate()?;
        }
        self.provides.validate()?;
        self.contributes.validate()
    }

    pub fn implementations(&self) -> impl Iterator<Item = &MeshProvidesDeclaration> {
        self.implements.iter()
    }

    fn localized_text_diagnostics(
        &self,
        path: &Path,
        module_id: &str,
    ) -> Vec<ModuleManifestDiagnostic> {
        let mut diagnostics = Vec::new();

        for (action_id, action) in &self.keybinds.actions {
            for (field, value) in [
                ("label", action.label.as_ref()),
                ("description", action.description.as_ref()),
                ("category", action.category.as_ref()),
            ] {
                let Some(value) = value else {
                    continue;
                };
                if !value.is_suspicious_raw_i18n_key() {
                    continue;
                }

                let field_path = format!("mesh.keybinds.{action_id}.{field}");
                let key = value.fallback_text();
                diagnostics.push(ModuleManifestDiagnostic::warning(
                    path,
                    Some(module_id.to_string()),
                    Some(field_path.clone()),
                    format!("{field_path} looks like an i18n key but is a raw literal string"),
                    format!(
                        "use {{ \"t\": \"{key}\", \"fallback\": \"...\" }} to localize this field"
                    ),
                ));
            }
        }
        for (index, contribution) in self.contributes.layout.iter().enumerate() {
            let Some(value) = contribution.label.as_ref() else {
                continue;
            };
            if !value.is_suspicious_raw_i18n_key() {
                continue;
            }

            let field_path = format!("mesh.provides.layout[{index}].label");
            let key = value.fallback_text();
            diagnostics.push(ModuleManifestDiagnostic::warning(
                path,
                Some(module_id.to_string()),
                Some(field_path.clone()),
                format!("{field_path} looks like an i18n key but is a raw literal string"),
                format!("use {{ \"t\": \"{key}\", \"fallback\": \"...\" }} to localize this field"),
            ));
        }
        for (index, contribution) in self.contributes.themes.iter().enumerate() {
            let Some(value) = contribution.label.as_ref() else {
                continue;
            };
            if !value.is_suspicious_raw_i18n_key() {
                continue;
            }
            let field_path = format!("mesh.provides.themes[{index}].label");
            let key = value.fallback_text();
            diagnostics.push(ModuleManifestDiagnostic::warning(
                path,
                Some(module_id.to_string()),
                Some(field_path.clone()),
                format!("{field_path} looks like an i18n key but is a raw literal string"),
                format!("use {{ \"t\": \"{key}\", \"fallback\": \"...\" }} to localize this field"),
            ));
        }
        for (index, contribution) in self
            .contributes
            .icons
            .iter()
            .chain(self.contributes.fonts.iter())
            .enumerate()
        {
            let Some(value) = contribution.label.as_ref() else {
                continue;
            };
            if !value.is_suspicious_raw_i18n_key() {
                continue;
            }
            let field_path = format!("mesh.provides.resources[{index}].label");
            let key = value.fallback_text();
            diagnostics.push(ModuleManifestDiagnostic::warning(
                path,
                Some(module_id.to_string()),
                Some(field_path.clone()),
                format!("{field_path} looks like an i18n key but is a raw literal string"),
                format!("use {{ \"t\": \"{key}\", \"fallback\": \"...\" }} to localize this field"),
            ));
        }
        for provided in self.implements.iter() {
            let Some(value) = provided.label.as_ref() else {
                continue;
            };
            if !value.is_suspicious_raw_i18n_key() {
                continue;
            }
            let field_path = format!("mesh.implements[{}].label", provided.interface);
            let key = value.fallback_text();
            diagnostics.push(ModuleManifestDiagnostic::warning(
                path,
                Some(module_id.to_string()),
                Some(field_path.clone()),
                format!("{field_path} looks like an i18n key but is a raw literal string"),
                format!("use {{ \"t\": \"{key}\", \"fallback\": \"...\" }} to localize this field"),
            ));
        }

        diagnostics
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct MeshI18nSupport {
    #[serde(default, rename = "defaultLocale", alias = "default_locale")]
    pub default_locale: Option<String>,
    #[serde(default, rename = "supportedLocales", alias = "supported_locales")]
    pub supported_locales: Vec<String>,
}

impl MeshI18nSupport {
    fn validate(&self) -> Result<(), ModuleManifestError> {
        if let Some(default_locale) = &self.default_locale {
            if default_locale.trim().is_empty() {
                return Err(ModuleManifestError::Validation(
                    "mesh.i18n.defaultLocale cannot be empty".into(),
                ));
            }
            if !self.supported_locales.is_empty()
                && !self
                    .supported_locales
                    .iter()
                    .any(|locale| locale == default_locale)
            {
                return Err(ModuleManifestError::Validation(format!(
                    "mesh.i18n.defaultLocale {default_locale} must be listed in supportedLocales"
                )));
            }
        }

        for locale in &self.supported_locales {
            if locale.trim().is_empty() {
                return Err(ModuleManifestError::Validation(
                    "mesh.i18n.supportedLocales cannot contain empty locales".into(),
                ));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ModuleKind {
    Frontend,
    Backend,
    Theme,
    IconPack,
    FontPack,
    LanguagePack,
    Interface,
    Library,
    /// Embeddable component module — has an entry `.mesh` file consumed by other
    /// modules via `require("@scope/name")` but owns no shell surface of its own.
    /// No `mesh.surface` block is required or allowed.
    Component,
}

impl From<ModuleType> for ModuleKind {
    fn from(module_type: ModuleType) -> Self {
        match module_type {
            ModuleType::Surface | ModuleType::Widget => Self::Frontend,
            ModuleType::Backend => Self::Backend,
            ModuleType::Theme => Self::Theme,
            ModuleType::IconPack => Self::IconPack,
            ModuleType::LanguagePack => Self::LanguagePack,
            ModuleType::Interface => Self::Interface,
        }
    }
}

impl From<ModuleKind> for ModuleType {
    fn from(kind: ModuleKind) -> Self {
        match kind {
            ModuleKind::Frontend => Self::Surface,
            ModuleKind::Backend => Self::Backend,
            ModuleKind::Theme => Self::Theme,
            ModuleKind::IconPack => Self::IconPack,
            ModuleKind::LanguagePack => Self::LanguagePack,
            ModuleKind::Interface => Self::Interface,
            ModuleKind::FontPack | ModuleKind::Library | ModuleKind::Component => Self::Widget,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ModuleRepository {
    #[serde(rename = "type")]
    pub repository_type: String,
    pub url: String,
}

impl ModuleRepository {
    fn validate(&self) -> Result<(), ModuleManifestError> {
        if self.repository_type == "git" && self.url.trim().is_empty() {
            return Err(ModuleManifestError::Validation(
                "repository.url cannot be empty when repository.type is git".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct MeshEntrypoints {
    #[serde(default)]
    pub main: Option<String>,
    #[serde(default)]
    pub settings_ui: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct MeshUses {
    #[serde(default)]
    pub modules: HashMap<String, DependencySpec>,
    #[serde(default)]
    pub interfaces: HashMap<String, String>,
    #[serde(default, rename = "optionalInterfaces", alias = "optional_interfaces")]
    pub optional_interfaces: HashMap<String, String>,
    #[serde(default)]
    pub resources: MeshResourceUses,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(
        default,
        rename = "optionalCapabilities",
        alias = "optional_capabilities"
    )]
    pub optional_capabilities: Vec<String>,
    #[serde(default)]
    pub binaries: Vec<manifest::BinaryDependency>,
    #[serde(default, rename = "iconRequirements", alias = "icon_requirements")]
    pub icon_requirements: manifest::IconRequirementsSection,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct MeshResourceUses {
    #[serde(default)]
    pub icons: Vec<String>,
    #[serde(default)]
    pub fonts: Vec<String>,
    #[serde(default)]
    pub i18n: Vec<String>,
    #[serde(default)]
    pub themes: Vec<String>,
}

impl MeshUses {
    fn validate(&self) -> Result<(), ModuleManifestError> {
        for module_id in self.modules.keys() {
            validate_module_dependency_id("mesh.uses.modules", module_id)?;
        }
        for module_id in self
            .resources
            .icons
            .iter()
            .chain(self.resources.fonts.iter())
            .chain(self.resources.i18n.iter())
            .chain(self.resources.themes.iter())
        {
            validate_module_dependency_id("mesh.uses.resources", module_id)?;
        }
        for interface in self
            .interfaces
            .keys()
            .chain(self.optional_interfaces.keys())
        {
            validate_interface_dependency_id(interface)?;
        }
        for capability in self
            .capabilities
            .iter()
            .chain(self.optional_capabilities.iter())
        {
            validate_capability_id(capability)?;
        }
        Ok(())
    }
}

fn validate_module_dependency_id(field: &str, value: &str) -> Result<(), ModuleManifestError> {
    if value.trim().is_empty() {
        return Err(ModuleManifestError::Validation(format!(
            "{field} entries cannot be empty"
        )));
    }
    if !value.starts_with('@') {
        return Err(ModuleManifestError::Validation(format!(
            "{field} entry '{value}' must be a module id such as @scope/name; interfaces belong in mesh.uses.interfaces and host powers belong in mesh.uses.capabilities"
        )));
    }
    Ok(())
}

fn validate_interface_dependency_id(value: &str) -> Result<(), ModuleManifestError> {
    if value.trim().is_empty() {
        return Err(ModuleManifestError::Validation(
            "mesh.uses.interfaces entries cannot be empty".into(),
        ));
    }
    if value.starts_with('@') {
        return Err(ModuleManifestError::Validation(format!(
            "mesh.uses.interfaces entry '{value}' must be an interface contract name; module ids belong in mesh.uses.modules"
        )));
    }
    if !value.contains('.') {
        return Err(ModuleManifestError::Validation(format!(
            "mesh.uses.interfaces entry '{value}' must use a dotted interface name such as mesh.audio"
        )));
    }
    Ok(())
}

fn validate_capability_id(value: &str) -> Result<(), ModuleManifestError> {
    if value.trim().is_empty() {
        return Err(ModuleManifestError::Validation(
            "mesh.uses.capabilities entries cannot be empty".into(),
        ));
    }
    if value.starts_with('@') {
        return Err(ModuleManifestError::Validation(format!(
            "mesh.uses.capabilities entry '{value}' looks like a module id; dependencies belong in mesh.uses.modules"
        )));
    }
    if value.starts_with("mesh.") {
        return Err(ModuleManifestError::Validation(format!(
            "mesh.uses.capabilities entry '{value}' looks like an interface contract; interfaces belong in mesh.uses.interfaces"
        )));
    }
    if !value.contains('.') {
        return Err(ModuleManifestError::Validation(format!(
            "mesh.uses.capabilities entry '{value}' must use a dotted capability name such as service.audio.read"
        )));
    }
    Ok(())
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct MeshDependencies {
    #[serde(default)]
    pub modules: HashMap<String, DependencySpec>,
    #[serde(default)]
    pub backend: HashMap<String, String>,
    #[serde(default)]
    pub optional_backend: HashMap<String, String>,
    #[serde(default)]
    pub icons: HashMap<String, String>,
    #[serde(default)]
    pub fonts: HashMap<String, String>,
    #[serde(default)]
    pub i18n: HashMap<String, String>,
    #[serde(default)]
    pub themes: HashMap<String, String>,
    #[serde(default)]
    pub binaries: Vec<manifest::BinaryDependency>,
}

impl MeshDependencies {
    fn into_manifest_dependencies(self) -> manifest::DependenciesSection {
        let interfaces = self
            .backend
            .into_iter()
            .map(|(name, version)| manifest::InterfaceDependency {
                name,
                version: Some(version),
                required: true,
            })
            .chain(self.optional_backend.into_iter().map(|(name, version)| {
                manifest::InterfaceDependency {
                    name,
                    version: Some(version),
                    required: false,
                }
            }))
            .collect();
        manifest::DependenciesSection {
            modules: self.modules,
            interfaces,
            icon_packs: manifest::OptionalDependencyGroup {
                required: self.icons.keys().cloned().collect(),
                optional: Vec::new(),
            },
            language_packs: manifest::OptionalDependencyGroup {
                required: self.i18n.keys().cloned().collect(),
                optional: Vec::new(),
            },
            themes: manifest::OptionalDependencyGroup {
                required: self.themes.keys().cloned().collect(),
                optional: Vec::new(),
            },
            native_libs: Vec::new(),
            binaries: self.binaries,
            fonts: self
                .fonts
                .keys()
                .cloned()
                .map(|family| manifest::FontDependency {
                    family,
                    reason: None,
                })
                .collect(),
        }
    }

    fn merge_uses(&mut self, uses: &MeshUses) {
        for (id, spec) in &uses.modules {
            self.modules
                .entry(id.clone())
                .or_insert_with(|| spec.clone());
        }
        for (interface, spec) in &uses.interfaces {
            self.backend
                .entry(interface.clone())
                .or_insert_with(|| spec.clone());
        }
        for (interface, spec) in &uses.optional_interfaces {
            self.optional_backend
                .entry(interface.clone())
                .or_insert_with(|| spec.clone());
        }
        for icon_pack in &uses.resources.icons {
            self.icons
                .entry(icon_pack.clone())
                .or_insert_with(|| "*".into());
        }
        for font_pack in &uses.resources.fonts {
            self.fonts
                .entry(font_pack.clone())
                .or_insert_with(|| "*".into());
        }
        for language_pack in &uses.resources.i18n {
            self.i18n
                .entry(language_pack.clone())
                .or_insert_with(|| "*".into());
        }
        for theme in &uses.resources.themes {
            self.themes
                .entry(theme.clone())
                .or_insert_with(|| "*".into());
        }
        self.binaries.extend(uses.binaries.iter().cloned());
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MeshProvidesDeclaration {
    pub interface: String,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default, rename = "baseModule", alias = "base_module")]
    pub base_module: Option<String>,
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub label: Option<manifest::LocalizedText>,
    #[serde(default)]
    pub priority: u32,
}

impl MeshProvidesDeclaration {
    fn validate(&self) -> Result<(), ModuleManifestError> {
        if self.interface.trim().is_empty() {
            return Err(ModuleManifestError::Validation(
                "mesh.provides interface cannot be empty".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct MeshInterfaceDeclaration {
    pub name: String,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub file: Option<String>,
    #[serde(default)]
    pub domain: Option<String>,
    #[serde(default)]
    pub extends: Option<String>,
    #[serde(default)]
    pub relationship: Option<InterfaceRelationship>,
    #[serde(default)]
    pub reason: Option<String>,
}

impl MeshInterfaceDeclaration {
    fn validate(&self) -> Result<(), ModuleManifestError> {
        if self.name.trim().is_empty() {
            return Err(ModuleManifestError::Validation(
                "mesh.interface.name cannot be empty".into(),
            ));
        }
        if let Some(version) = &self.version
            && version.trim().is_empty()
        {
            return Err(ModuleManifestError::Validation(
                "mesh.interface.version cannot be empty".into(),
            ));
        }
        if let Some(file) = &self.file
            && file.trim().is_empty()
        {
            return Err(ModuleManifestError::Validation(
                "mesh.interface.file cannot be empty".into(),
            ));
        }
        if let Some(domain) = &self.domain
            && domain.trim().is_empty()
        {
            return Err(ModuleManifestError::Validation(
                "mesh.interface.domain cannot be empty".into(),
            ));
        }
        if let Some(extends) = &self.extends
            && extends.trim().is_empty()
        {
            return Err(ModuleManifestError::Validation(
                "mesh.interface.extends cannot be empty".into(),
            ));
        }
        match (self.relationship, self.extends.as_ref()) {
            (Some(InterfaceRelationship::Extension), None) => {
                return Err(ModuleManifestError::Validation(
                    "mesh.interface.relationship extension requires mesh.interface.extends".into(),
                ));
            }
            (Some(InterfaceRelationship::Base), Some(_)) => {
                return Err(ModuleManifestError::Validation(
                    "mesh.interface.relationship base cannot set mesh.interface.extends".into(),
                ));
            }
            (Some(InterfaceRelationship::Independent), Some(_)) => {
                return Err(ModuleManifestError::Validation(
                    "mesh.interface.relationship independent cannot set mesh.interface.extends"
                        .into(),
                ));
            }
            _ => {}
        }
        Ok(())
    }

    pub(crate) fn effective_relationship(&self) -> InterfaceRelationship {
        self.relationship.unwrap_or_else(|| {
            if self.extends.is_some() {
                InterfaceRelationship::Extension
            } else if self.name.starts_with("mesh.") {
                InterfaceRelationship::Base
            } else {
                InterfaceRelationship::Independent
            }
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum InterfaceRelationship {
    Base,
    Extension,
    Independent,
}

impl From<crate::manifest::ProvidedInterface> for MeshProvidesDeclaration {
    fn from(provided: crate::manifest::ProvidedInterface) -> Self {
        Self {
            interface: provided.interface,
            version: provided.version,
            base_module: provided.base_module,
            provider: provided.backend_name.clone(),
            label: provided
                .backend_name
                .map(crate::manifest::LocalizedText::Literal),
            priority: provided.priority,
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct MeshProvides {
    #[serde(default)]
    pub layout: Vec<LayoutContribution>,
    #[serde(default)]
    pub settings: Option<SettingsContribution>,
    #[serde(default)]
    pub themes: Vec<ThemeContribution>,
    #[serde(default)]
    pub icons: Vec<PathContribution>,
    #[serde(default)]
    pub fonts: Vec<PathContribution>,
    #[serde(default)]
    pub i18n: Vec<I18nContribution>,
    #[serde(default)]
    pub libraries: Vec<LibraryContribution>,
}

impl MeshProvides {
    fn validate(&self) -> Result<(), ModuleManifestError> {
        MeshContributes {
            layout: self.layout.clone(),
            settings: self.settings.clone(),
            themes: self.themes.clone(),
            icons: self.icons.clone(),
            fonts: self.fonts.clone(),
            i18n: self.i18n.clone(),
            libraries: self.libraries.clone(),
        }
        .validate()
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct MeshContributes {
    #[serde(default)]
    pub layout: Vec<LayoutContribution>,
    #[serde(default)]
    pub settings: Option<SettingsContribution>,
    #[serde(default)]
    pub themes: Vec<ThemeContribution>,
    #[serde(default)]
    pub icons: Vec<PathContribution>,
    #[serde(default)]
    pub fonts: Vec<PathContribution>,
    #[serde(default)]
    pub i18n: Vec<I18nContribution>,
    #[serde(default)]
    pub libraries: Vec<LibraryContribution>,
}

impl MeshContributes {
    fn validate(&self) -> Result<(), ModuleManifestError> {
        for contribution in &self.layout {
            validate_relative_path("layout entrypoint", &contribution.entrypoint)?;
            if let Some(label) = &contribution.label {
                label
                    .validate("mesh.provides.layout[].label")
                    .map_err(ModuleManifestError::Validation)?;
            }
        }
        for contribution in &self.themes {
            for path in contribution.modes.values() {
                validate_relative_path("theme mode", path)?;
            }
        }
        for contribution in &self.icons {
            validate_relative_path("icon contribution", &contribution.path)?;
        }
        for contribution in &self.fonts {
            validate_relative_path("font contribution", &contribution.path)?;
        }
        for contribution in &self.i18n {
            validate_relative_path("i18n contribution", &contribution.path)?;
        }
        for contribution in &self.libraries {
            contribution.validate()?;
        }
        Ok(())
    }

    fn merge_provides(&mut self, provides: &MeshProvides) {
        self.layout.extend(provides.layout.iter().cloned());
        self.themes.extend(provides.themes.iter().cloned());
        self.icons.extend(provides.icons.iter().cloned());
        self.fonts.extend(provides.fonts.iter().cloned());
        self.i18n.extend(provides.i18n.iter().cloned());
        self.libraries.extend(provides.libraries.iter().cloned());
        if self.settings.is_none() {
            self.settings = provides.settings.clone();
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LayoutContribution {
    pub id: String,
    pub entrypoint: String,
    #[serde(default)]
    pub label: Option<manifest::LocalizedText>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SettingsContribution {
    pub namespace: String,
    #[serde(default)]
    pub schema: serde_json::Value,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ThemeContribution {
    pub id: String,
    #[serde(default)]
    pub label: Option<manifest::LocalizedText>,
    #[serde(default)]
    pub modes: HashMap<String, String>,
    #[serde(default)]
    pub default_mode: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PathContribution {
    pub id: String,
    pub path: String,
    #[serde(default)]
    pub label: Option<manifest::LocalizedText>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct I18nContribution {
    pub id: String,
    pub locale: String,
    pub path: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LibraryContribution {
    pub namespace: String,
    pub path: String,
}

impl LibraryContribution {
    pub(crate) fn validate(&self) -> Result<(), ModuleManifestError> {
        if self.namespace.trim().is_empty() {
            return Err(ModuleManifestError::Validation(
                "library namespace cannot be empty".into(),
            ));
        }
        validate_relative_path("library contribution", &self.path)
    }
}

fn merge_unique(target: &mut Vec<String>, additions: &[String]) {
    for item in additions {
        if !target.iter().any(|existing| existing == item) {
            target.push(item.clone());
        }
    }
}
