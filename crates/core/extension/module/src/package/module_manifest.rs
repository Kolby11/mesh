use super::{ModuleId, ModuleManifestDiagnostic, ModuleManifestError, validate_relative_path};
use crate::manifest::{self, CapabilitiesSection, DependencySpec, Manifest, ModuleType};
use mesh_core_service::{canonical_interface_name, parse_contract_version, parse_version_req};
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
        let path = PathBuf::from("<inline>");
        reject_legacy_top_level_fields(input, &path)?;
        reject_legacy_surface_layout(input, &path)?;
        let mut parsed: Self = serde_json::from_str(input)
            .map_err(|source| ModuleManifestError::Json { path, source })?;
        parsed.normalize();
        parsed.validate()?;
        Ok(parsed)
    }

    pub fn from_path(path: &Path) -> Result<Self, ModuleManifestError> {
        super::validate_regular_file(path, "module manifest")?;
        let content = std::fs::read_to_string(path).map_err(|source| ModuleManifestError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        reject_legacy_top_level_fields(&content, path)?;
        reject_legacy_surface_layout(&content, path)?;
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
        ModuleId::parse(&self.name)?;
        if self.version.trim().is_empty() {
            return Err(ModuleManifestError::Validation(format!(
                "module {} version cannot be empty",
                self.name
            )));
        }
        if parse_contract_version(&self.version).is_none() {
            return Err(ModuleManifestError::Validation(format!(
                "module {} version '{}' is not a valid semantic version",
                self.name, self.version
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
        let map_interface_section =
            |interface: MeshInterfaceDeclaration| -> Option<manifest::InterfaceSection> {
                let version = interface.version?;
                Some(manifest::InterfaceSection {
                    name: interface.name,
                    version,
                    contract: interface.contract,
                    extends: interface.extends,
                })
            };
        let interface = mesh.interface.clone().and_then(map_interface_section);
        let interfaces = mesh
            .interfaces
            .clone()
            .into_iter()
            .filter_map(map_interface_section)
            .collect();

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
            },
            accessibility: mesh.accessibility,
            keybinds: mesh.keybinds,
            i18n,
            theme: manifest_theme,
            service: None,
            provides,
            interface,
            interfaces,
            extensions: Vec::new(),
            exports: manifest::ExportsSection::default(),
            hosted_extension_points: mesh.hosts,
            extension_point_contributions: mesh.contributes.extension_points,
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
    /// Inline interface contract declarations on a backend module. This is the
    /// low-friction path for single-provider domains: the backend that
    /// implements the interface also declares its contract. Multi-provider
    /// domains should keep a standalone `interface` module, which always wins
    /// over inline declarations of the same name.
    #[serde(default)]
    pub interfaces: Vec<MeshInterfaceDeclaration>,
    #[serde(default)]
    pub theme: Option<manifest::ThemeSection>,
    #[serde(default)]
    pub contributes: MeshContributes,
    /// Extension points **declared** by this module. Only `interface` modules
    /// may declare them: an extension point is a contract, like an interface,
    /// and lives with the other data-only contracts.
    #[serde(default, rename = "extensionPoints", alias = "extension_points")]
    pub extension_points: HashMap<String, MeshExtensionPointDeclaration>,
    /// Composition modules only: what this composition selects.
    #[serde(default)]
    pub compose: Option<super::CompositionSpec>,
    /// Composition modules only: the composition this one refines. Forking a
    /// shell family is `extends` plus the deltas you disagree with.
    #[serde(default)]
    pub extends: Option<String>,
    /// Extension points this module **hosts** — renders foreign contributions
    /// into. Keyed by contract name, never by module id, so a host can be
    /// replaced without breaking contributors.
    #[serde(default)]
    pub hosts: HashMap<String, manifest::HostedExtensionPoint>,
    #[serde(default)]
    pub icons: Option<manifest::IconsSection>,
    #[serde(default)]
    pub fonts: Option<manifest::FontsSection>,
    #[serde(default)]
    pub icon_pack: Option<manifest::IconPackSection>,
    #[serde(default)]
    pub font_pack: Option<manifest::FontPackSection>,
    #[serde(default, rename = "iconRequirements", alias = "icon_requirements")]
    pub icon_requirements: manifest::IconRequirementsSection,
    #[serde(default)]
    pub accessibility: Option<manifest::AccessibilitySection>,
    /// Compact author-facing surface block. Core ships the canonical surface
    /// schema; authors declare only deltas here. Normalized into
    /// `surface_layout`, the single typed runtime home.
    #[serde(default)]
    pub surface: Option<manifest::SurfaceLayoutSection>,
    /// Internal normalized representation of `mesh.surface`. This must never
    /// deserialize directly: `mesh.surfaceLayout` was a legacy compatibility
    /// input and receives a migration diagnostic before manifest parsing.
    #[serde(skip_deserializing)]
    pub surface_layout: Option<manifest::SurfaceLayoutSection>,
    #[serde(default)]
    pub experimental: serde_json::Value,
}

fn reject_legacy_top_level_fields(input: &str, path: &Path) -> Result<(), ModuleManifestError> {
    let document: serde_json::Value =
        serde_json::from_str(input).map_err(|source| ModuleManifestError::Json {
            path: path.to_path_buf(),
            source,
        })?;
    let Some(object) = document.as_object() else {
        return Ok(());
    };
    let Some(field) = ["id", "type", "api_version"]
        .into_iter()
        .find(|field| object.contains_key(*field))
    else {
        return Ok(());
    };
    let module_id = document
        .get("name")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned);
    Err(ModuleManifestError::Diagnostic {
        diagnostic: ModuleManifestDiagnostic::error(
            path,
            module_id,
            Some(field.into()),
            format!("top-level {field} is a legacy module manifest field and is not supported"),
            "replace legacy module.json fields with canonical name/version/mesh",
        ),
    })
}

fn reject_legacy_surface_layout(input: &str, path: &Path) -> Result<(), ModuleManifestError> {
    let document: serde_json::Value =
        serde_json::from_str(input).map_err(|source| ModuleManifestError::Json {
            path: path.to_path_buf(),
            source,
        })?;
    let Some(mesh) = document.get("mesh").and_then(serde_json::Value::as_object) else {
        return Ok(());
    };
    let Some(field) = ["surfaceLayout", "surface_layout"]
        .into_iter()
        .find(|field| mesh.contains_key(*field))
    else {
        return Ok(());
    };
    let module_id = document
        .get("name")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned);
    Err(ModuleManifestError::Diagnostic {
        diagnostic: ModuleManifestDiagnostic::error(
            path,
            module_id,
            Some(format!("mesh.{field}")),
            format!("mesh.{field} is a legacy surface declaration and is not supported"),
            "replace mesh.surfaceLayout with mesh.surface",
        ),
    })
}

impl MeshModuleSection {
    fn normalize(&mut self) {
        if let Some(entry) = &self.entry
            && self.entrypoints.main.is_none()
        {
            self.entrypoints.main = Some(entry.clone());
        }
        // Frontends that set `entry` and omit `provides.layout` get the default
        // layout contribution, so a simple module declares the path only once.
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
        // `mesh.surface` is the author-facing block; normalizing it into
        // `surface_layout` gives `surface_layout_from_manifest` one typed home.
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
        if let Some(entry) = &self.entry {
            validate_relative_path("mesh.entry", entry)?;
        }
        if let Some(entry) = &self.entrypoints.main {
            validate_relative_path("mesh.entrypoints.main", entry)?;
        }
        if self.kind == ModuleKind::Interface
            && self.interface.is_none()
            && self.extension_points.is_empty()
        {
            return Err(ModuleManifestError::Validation(
                "interface modules must declare mesh.interface or mesh.extensionPoints".into(),
            ));
        }
        self.uses.validate()?;
        self.dependencies.validate()?;
        if let Some(interface) = &self.interface {
            interface.validate()?;
            if self.kind == ModuleKind::Interface && interface.version.is_none() {
                return Err(ModuleManifestError::Validation(
                    "interface modules must declare mesh.interface.version".into(),
                ));
            }
            // `mesh.interface.contract` is optional: a module may ship only
            // name/version/domain and let the contract be inferred from the
            // provider's emitted state. The graph then reports
            // `missing_interface_contract` and contract-based validation
            // (capabilities, events) does not apply.
        }
        if !self.interfaces.is_empty() {
            if self.kind != ModuleKind::Backend {
                return Err(ModuleManifestError::Validation(
                    "mesh.interfaces (inline contract declarations) is only supported for backend modules; interface modules declare mesh.interface".into(),
                ));
            }
            for declaration in &self.interfaces {
                declaration.validate()?;
                if declaration.version.is_none() {
                    return Err(ModuleManifestError::Validation(format!(
                        "mesh.interfaces entry '{}' must declare a version",
                        declaration.name
                    )));
                }
                if declaration.contract.is_none() {
                    return Err(ModuleManifestError::Validation(format!(
                        "mesh.interfaces entry '{}' must declare a contract; an inline interface declaration exists to carry its contract",
                        declaration.name
                    )));
                }
            }
            let mut inline_interface_names = self
                .interfaces
                .iter()
                .map(|declaration| canonical_interface_name(&declaration.name))
                .collect::<Vec<_>>();
            inline_interface_names.sort();
            for pair in inline_interface_names.windows(2) {
                if pair[0] == pair[1] {
                    return Err(ModuleManifestError::Validation(format!(
                        "mesh.interfaces contains duplicate interface declaration '{}'",
                        pair[0]
                    )));
                }
            }
        }
        if self.kind == ModuleKind::Composition {
            // A composition selects among what its members already declare. If
            // it could request capabilities of its own it would become the
            // privileged layer that replaceable modules exist to avoid.
            if !self.uses.capabilities.is_empty()
                || !self.uses.optional_capabilities.is_empty()
                || !self.capabilities.required.is_empty()
                || !self.capabilities.optional.is_empty()
            {
                return Err(ModuleManifestError::Validation(
                    "composition modules must not request capabilities; a composition selects among what its members declare".into(),
                ));
            }
            if self.entry.is_some() || self.entrypoints.main.is_some() {
                return Err(ModuleManifestError::Validation(
                    "composition modules have no entry; they compose other modules' roots".into(),
                ));
            }
            if self.surface.is_some() || self.surface_layout.is_some() {
                return Err(ModuleManifestError::Validation(
                    "composition modules declare no mesh.surface; placement belongs to the roots they compose".into(),
                ));
            }
            if !self.implements.is_empty() {
                return Err(ModuleManifestError::Validation(
                    "composition modules implement no interfaces; they bind providers instead"
                        .into(),
                ));
            }
            if let Some(parent) = &self.extends {
                validate_module_dependency_id("mesh.extends", parent)?;
            }
        } else {
            if self.compose.is_some() {
                return Err(ModuleManifestError::Validation(
                    "mesh.compose is only supported for composition modules".into(),
                ));
            }
            if self.extends.is_some() {
                return Err(ModuleManifestError::Validation(
                    "mesh.extends is only supported for composition modules".into(),
                ));
            }
        }
        if !self.extension_points.is_empty() && self.kind != ModuleKind::Interface {
            return Err(ModuleManifestError::Validation(
                "mesh.extensionPoints is only supported for interface modules; an extension point is a contract, like an interface".into(),
            ));
        }
        for (point_name, declaration) in &self.extension_points {
            validate_extension_point_name("mesh.extensionPoints", point_name)?;
            declaration.validate(point_name)?;
        }
        for (point_name, host) in &self.hosts {
            validate_extension_point_name("mesh.hosts", point_name)?;
            for (slot_name, slot) in &host.slots {
                if slot_name.trim().is_empty() {
                    return Err(ModuleManifestError::Validation(format!(
                        "mesh.hosts.{point_name}.slots cannot contain an empty name"
                    )));
                }
                for reference in &slot.defaults {
                    let Some((module_id, contribution_id)) = reference.rsplit_once(':') else {
                        return Err(ModuleManifestError::Validation(format!(
                            "mesh.hosts.{point_name}.slots.{slot_name} default '{reference}' must use module-id:contribution-id"
                        )));
                    };
                    validate_module_dependency_id(
                        "mesh.hosts customizable slot defaults",
                        module_id,
                    )?;
                    if contribution_id.trim().is_empty() {
                        return Err(ModuleManifestError::Validation(format!(
                            "mesh.hosts.{point_name}.slots.{slot_name} has an empty contribution id"
                        )));
                    }
                }
            }
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
        if self.fonts.is_some()
            && !matches!(self.kind, ModuleKind::Frontend | ModuleKind::Component)
        {
            return Err(ModuleManifestError::Validation(
                "mesh.fonts is only supported for frontend or component modules".into(),
            ));
        }
        if self.font_pack.is_some() && self.kind != ModuleKind::FontPack {
            return Err(ModuleManifestError::Validation(
                "mesh.font_pack is only supported for font-pack modules".into(),
            ));
        }
        if let Some(font_pack) = &self.font_pack {
            if font_pack.id.trim().is_empty() {
                return Err(ModuleManifestError::Validation(
                    "mesh.font_pack.id must not be empty".into(),
                ));
            }
            for (role, family) in &font_pack.mappings {
                if role.trim().is_empty() || role.contains('/') {
                    return Err(ModuleManifestError::Validation(format!(
                        "mesh.font_pack.mappings role '{role}' is invalid"
                    )));
                }
                if family.trim().is_empty() {
                    return Err(ModuleManifestError::Validation(format!(
                        "mesh.font_pack.mappings role '{role}' has an empty family"
                    )));
                }
            }
            for (coverage, description) in &font_pack.covers {
                if coverage.trim().is_empty() || description.trim().is_empty() {
                    return Err(ModuleManifestError::Validation(
                        "mesh.font_pack.covers entries must have non-empty names and descriptions"
                            .into(),
                    ));
                }
            }
            for face in &font_pack.faces {
                if face.family.trim().is_empty() || face.file.trim().is_empty() {
                    return Err(ModuleManifestError::Validation(
                        "mesh.font_pack.faces family and file must not be empty".into(),
                    ));
                }
                if !(1..=1000).contains(&face.weight) {
                    return Err(ModuleManifestError::Validation(format!(
                        "mesh.font_pack.faces '{}' weight must be between 1 and 1000",
                        face.family
                    )));
                }
                if !(50..=200).contains(&face.stretch) {
                    return Err(ModuleManifestError::Validation(format!(
                        "mesh.font_pack.faces '{}' stretch must be between 50 and 200",
                        face.family
                    )));
                }
                if face.coverage.iter().any(|entry| entry.trim().is_empty()) {
                    return Err(ModuleManifestError::Validation(format!(
                        "mesh.font_pack.faces '{}' has an empty coverage entry",
                        face.family
                    )));
                }
            }
            for requirement in &font_pack.requires.fonts {
                if requirement.family.trim().is_empty() {
                    return Err(ModuleManifestError::Validation(
                        "mesh.font_pack.requires.fonts family must not be empty".into(),
                    ));
                }
            }
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
        let mut implementation_ids = self
            .implements
            .iter()
            .map(|provided| {
                let version = provided
                    .version
                    .as_deref()
                    .and_then(parse_contract_version)
                    .map(|version| version.to_string())
                    .unwrap_or_else(|| "*".into());
                format!(
                    "{}@{}",
                    canonical_interface_name(&provided.interface),
                    version
                )
            })
            .collect::<Vec<_>>();
        implementation_ids.sort();
        for pair in implementation_ids.windows(2) {
            if pair[0] == pair[1] {
                return Err(ModuleManifestError::Validation(format!(
                    "mesh.implements contains duplicate contract identity '{}'",
                    pair[0]
                )));
            }
        }
        self.provides.validate()?;
        self.contributes.validate()?;
        validate_i18n_contributions(self.kind, &self.contributes.i18n)
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
    /// An installable shell composition: which root components run, which
    /// providers are bound, which resources apply, and how extension points are
    /// arranged. It *binds*, it never *owns* — see `package::composition`.
    Composition,
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
            ModuleType::FontPack => Self::FontPack,
            ModuleType::LanguagePack => Self::LanguagePack,
            ModuleType::Interface => Self::Interface,
            ModuleType::Library => Self::Library,
            ModuleType::Component => Self::Component,
            ModuleType::Composition => Self::Composition,
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
            ModuleKind::FontPack => Self::FontPack,
            ModuleKind::LanguagePack => Self::LanguagePack,
            ModuleKind::Interface => Self::Interface,
            ModuleKind::Library => Self::Library,
            ModuleKind::Component => Self::Component,
            ModuleKind::Composition => Self::Composition,
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
    /// Where to fetch a dependency that is not installed yet. Without a
    /// registry a version range needs somewhere to resolve against; a registry
    /// later fills this same map from an index.
    #[serde(default)]
    pub sources: HashMap<String, super::SourceSpec>,
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
        for (module_id, spec) in &self.modules {
            validate_module_dependency_id("mesh.uses.modules", module_id)?;
            validate_dependency_version("mesh.uses.modules", module_id, spec)?;
        }
        for module_id in self.sources.keys() {
            validate_module_dependency_id("mesh.uses.sources", module_id)?;
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
        for (interface, version) in &self.interfaces {
            validate_interface_dependency_id(interface)?;
            validate_version_requirement("mesh.uses.interfaces", interface, version)?;
        }
        for (interface, version) in &self.optional_interfaces {
            validate_interface_dependency_id(interface)?;
            validate_version_requirement("mesh.uses.optionalInterfaces", interface, version)?;
        }
        for interface in self.interfaces.keys() {
            if self.optional_interfaces.contains_key(interface) {
                return Err(ModuleManifestError::Validation(format!(
                    "mesh.uses.interfaces and mesh.uses.optionalInterfaces cannot both declare '{interface}'"
                )));
            }
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

fn validate_dependency_version(
    field: &str,
    dependency_id: &str,
    spec: &DependencySpec,
) -> Result<(), ModuleManifestError> {
    let version = match spec {
        DependencySpec::Simple(version) => version,
        DependencySpec::Detailed { version, .. } => version,
    };
    validate_version_requirement(field, dependency_id, version)
}

fn validate_version_requirement(
    field: &str,
    name: &str,
    version: &str,
) -> Result<(), ModuleManifestError> {
    if parse_version_req(version).is_none() {
        return Err(ModuleManifestError::Validation(format!(
            "{field} dependency '{name}' has invalid version range '{version}'"
        )));
    }
    Ok(())
}

fn validate_declared_version(
    field: &str,
    name: &str,
    version: &str,
) -> Result<(), ModuleManifestError> {
    if parse_contract_version(version).is_none() {
        return Err(ModuleManifestError::Validation(format!(
            "{field} '{name}' has invalid semantic version '{version}'"
        )));
    }
    Ok(())
}

fn validate_module_dependency_id(field: &str, value: &str) -> Result<(), ModuleManifestError> {
    ModuleId::parse(value).map(|_| ()).map_err(|error| match error {
        ModuleManifestError::Validation(_) => ModuleManifestError::Validation(format!(
            "{field} entry '{value}' must be a module id such as @scope/name; interfaces belong in mesh.uses.interfaces and host powers belong in mesh.uses.capabilities"
        )),
        other => other,
    })
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
    fn validate(&self) -> Result<(), ModuleManifestError> {
        for (module_id, spec) in &self.modules {
            validate_module_dependency_id("mesh.dependencies.modules", module_id)?;
            validate_dependency_version("mesh.dependencies.modules", module_id, spec)?;
        }
        for (interface, version) in self.backend.iter().chain(self.optional_backend.iter()) {
            validate_interface_dependency_id(interface)?;
            validate_version_requirement("mesh.dependencies.interfaces", interface, version)?;
        }
        for interface in self.backend.keys() {
            if self.optional_backend.contains_key(interface) {
                return Err(ModuleManifestError::Validation(format!(
                    "mesh.dependencies.backend and mesh.dependencies.optionalBackend cannot both declare '{interface}'"
                )));
            }
        }
        Ok(())
    }

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
        validate_interface_dependency_id(&self.interface)?;
        if let Some(version) = &self.version {
            validate_declared_version("mesh.provides version", &self.interface, version)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct MeshInterfaceDeclaration {
    pub name: String,
    #[serde(default)]
    pub version: Option<String>,
    /// Inline contract JSON or a module-relative `contract.json` path.
    #[serde(default)]
    pub contract: Option<serde_json::Value>,
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
        if let Some(version) = &self.version {
            validate_declared_version("mesh.interface version", &self.name, version)?;
        }
        if let Some(contract) = &self.contract {
            match contract {
                serde_json::Value::Object(_) => {}
                serde_json::Value::String(path) => {
                    validate_relative_path("mesh interface declaration contract path", path)?
                }
                _ => {
                    return Err(ModuleManifestError::Validation(format!(
                        "mesh interface declaration '{}' contract must be a JSON object or relative path",
                        self.name
                    )));
                }
            }
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

/// A UI extension point declaration: the contract between a host that renders
/// a region and the modules that fill it.
///
/// Declared by `interface` modules for the same reason service contracts are —
/// it is data, it is versioned, and both sides must be able to depend on it
/// without depending on each other.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct MeshExtensionPointDeclaration {
    pub version: String,
    #[serde(default)]
    pub description: Option<String>,
    /// Whether a host renders every contribution (`true`, the default) or only
    /// the highest-precedence one.
    #[serde(default = "default_true")]
    pub multiple: bool,
    /// Props the host passes to each contribution, typed with the same grammar
    /// as interface contracts.
    #[serde(default)]
    pub props: Vec<MeshExtensionPointProp>,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MeshExtensionPointProp {
    pub name: String,
    #[serde(rename = "type")]
    pub prop_type: String,
    #[serde(default)]
    pub description: Option<String>,
}

impl MeshExtensionPointDeclaration {
    fn validate(&self, point_name: &str) -> Result<(), ModuleManifestError> {
        if self.version.trim().is_empty() {
            return Err(ModuleManifestError::Validation(format!(
                "mesh.extensionPoints '{point_name}' must declare a version"
            )));
        }
        validate_declared_version("mesh.extensionPoints version", point_name, &self.version)?;
        for prop in &self.props {
            if prop.name.trim().is_empty() {
                return Err(ModuleManifestError::Validation(format!(
                    "mesh.extensionPoints '{point_name}' has a prop with an empty name"
                )));
            }
            if prop.prop_type.trim().is_empty() {
                return Err(ModuleManifestError::Validation(format!(
                    "mesh.extensionPoints '{point_name}' prop '{}' must declare a type",
                    prop.name
                )));
            }
        }
        Ok(())
    }
}

/// Extension point names share the interface-name shape: dotted contract names,
/// never module ids. Keeping them in one namespace is deliberate — a host
/// depending on `@mesh/settings:custom-settings` is the coupling this replaces.
fn validate_extension_point_name(field: &str, value: &str) -> Result<(), ModuleManifestError> {
    if value.trim().is_empty() {
        return Err(ModuleManifestError::Validation(format!(
            "{field} entries cannot be empty"
        )));
    }
    if value.starts_with('@') {
        return Err(ModuleManifestError::Validation(format!(
            "{field} entry '{value}' must be an extension point contract name; module ids are not extension points"
        )));
    }
    if !value.contains('.') {
        return Err(ModuleManifestError::Validation(format!(
            "{field} entry '{value}' must use a dotted contract name such as mesh.settings.page"
        )));
    }
    Ok(())
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
    /// Contributions to extension points hosted by other modules, keyed by the
    /// point's contract name.
    #[serde(default, rename = "extensionPoints", alias = "extension_points")]
    pub extension_points: HashMap<String, Vec<manifest::ExtensionPointContribution>>,
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
            extension_points: self.extension_points.clone(),
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
    #[serde(default, rename = "extensionPoints", alias = "extension_points")]
    pub extension_points: HashMap<String, Vec<manifest::ExtensionPointContribution>>,
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
        validate_unique_contribution_ids(
            "mesh.provides.layout",
            self.layout
                .iter()
                .map(|contribution| contribution.id.as_str()),
        )?;
        validate_unique_contribution_ids(
            "mesh.provides.themes",
            self.themes
                .iter()
                .map(|contribution| contribution.id.as_str()),
        )?;
        validate_unique_contribution_ids(
            "mesh.provides.icons",
            self.icons
                .iter()
                .map(|contribution| contribution.id.as_str()),
        )?;
        validate_unique_contribution_ids(
            "mesh.provides.fonts",
            self.fonts
                .iter()
                .map(|contribution| contribution.id.as_str()),
        )?;
        validate_unique_contribution_ids(
            "mesh.provides.i18n",
            self.i18n
                .iter()
                .map(|contribution| contribution.id.as_str()),
        )?;
        validate_unique_contribution_ids(
            "mesh.provides.libraries",
            self.libraries
                .iter()
                .map(|contribution| contribution.namespace.as_str()),
        )?;

        let extension_point_ids = self.extension_points.values().flat_map(|contributions| {
            contributions
                .iter()
                .map(|contribution| contribution.id.as_str())
        });
        validate_unique_contribution_ids(
            "mesh.provides.extensionPoints contributions",
            extension_point_ids,
        )?;
        for (point, contributions) in &self.extension_points {
            validate_extension_point_name("mesh.provides.extensionPoints", point)?;
            for contribution in contributions {
                if contribution.id.trim().is_empty() {
                    return Err(ModuleManifestError::Validation(format!(
                        "mesh.provides.extensionPoints '{point}' has a contribution with an empty id"
                    )));
                }
                validate_relative_path("extension point contribution entry", &contribution.entry)?;
            }
        }
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
        for (point, contributions) in &provides.extension_points {
            self.extension_points
                .entry(point.clone())
                .or_default()
                .extend(contributions.iter().cloned());
        }
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

fn validate_unique_contribution_ids<'a>(
    field: &str,
    ids: impl IntoIterator<Item = &'a str>,
) -> Result<(), ModuleManifestError> {
    let mut ids = ids.into_iter().map(str::to_string).collect::<Vec<_>>();
    ids.sort();
    for pair in ids.windows(2) {
        if pair[0] == pair[1] {
            return Err(ModuleManifestError::Validation(format!(
                "{field} contains duplicate contribution identity '{}'",
                pair[0]
            )));
        }
    }
    Ok(())
}

fn validate_i18n_contributions(
    kind: ModuleKind,
    contributions: &[I18nContribution],
) -> Result<(), ModuleManifestError> {
    let mut target_locales = Vec::new();
    for contribution in contributions {
        if contribution.id.trim().is_empty() {
            return Err(ModuleManifestError::Validation(
                "i18n contribution id cannot be empty".into(),
            ));
        }
        if contribution.locale.trim().is_empty() {
            return Err(ModuleManifestError::Validation(format!(
                "i18n contribution '{}' locale cannot be empty",
                contribution.id
            )));
        }
        validate_relative_path("i18n contribution", &contribution.path)?;

        match (kind, contribution.module.as_deref()) {
            (ModuleKind::LanguagePack, Some(target)) => {
                ModuleId::parse(target)?;
                target_locales.push((target.to_string(), contribution.locale.to_ascii_lowercase()));
            }
            (ModuleKind::LanguagePack, None) => {
                return Err(ModuleManifestError::Validation(format!(
                    "language-pack i18n contribution '{}' must declare its target module",
                    contribution.id
                )));
            }
            (_, Some(_)) => {
                return Err(ModuleManifestError::Validation(format!(
                    "i18n contribution '{}' targets another module but its owner is not a language-pack",
                    contribution.id
                )));
            }
            (_, None) => {
                target_locales.push((String::new(), contribution.locale.to_ascii_lowercase()));
            }
        }
    }

    target_locales.sort();
    for pair in target_locales.windows(2) {
        if pair[0] == pair[1] {
            let target = if pair[0].0.is_empty() {
                "the owning module".to_string()
            } else {
                pair[0].0.clone()
            };
            return Err(ModuleManifestError::Validation(format!(
                "i18n contributions contain duplicate target/locale pair for {target} and {}",
                pair[0].1
            )));
        }
    }
    Ok(())
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
    /// Explicit rendering semantics for each mode. These are kept separate
    /// from the source path so the shell never infers them from an ID.
    #[serde(default)]
    pub mode_metadata: HashMap<String, ThemeModeMetadata>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
pub struct ThemeModeMetadata {
    #[serde(default)]
    pub color_scheme: String,
    #[serde(default)]
    pub contrast: String,
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
    /// Language-pack contributions target another module's translation
    /// namespace. Module-owned catalogs omit this field.
    #[serde(default)]
    pub module: Option<String>,
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
