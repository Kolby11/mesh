use std::collections::{BTreeMap, BTreeSet};

use crate::{ResourceAssetHandle, ResourcePreparationToken, SystemResourceCatalog};

const STANDARD_ROLES: &[&str] = &[
    "display", "headline", "title", "body", "label", "caption", "mono",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FontPackBindings {
    pub module_id: String,
    pub pack_id: String,
    /// Soft host-font requirements from `font_pack.requires.fonts`.
    pub required_families: Vec<String>,
    /// Pack-declared script or vocabulary coverage. Face-level coverage is
    /// retained separately because a pack may map several families.
    pub covers: BTreeMap<String, String>,
    pub mappings: BTreeMap<String, String>,
    pub faces: Vec<FontFaceBinding>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FontFaceBinding {
    pub family: String,
    pub asset: ResourceAssetHandle,
    pub weight: u16,
    pub style: String,
    pub stretch: u16,
    pub coverage: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FontResolutionSource {
    Pack { pack_id: String },
    InstalledFamily,
    SystemFallback,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FontResolution {
    pub requested: String,
    pub role: Option<String>,
    pub family: String,
    pub available: bool,
    pub source: FontResolutionSource,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum FontRegistryError {
    #[error("font pack module id must not be empty")]
    EmptyModuleId,
    #[error("font pack id '{0}' is not canonical")]
    NonCanonicalPackId(String),
    #[error("font pack id '{0}' is declared more than once")]
    DuplicatePackId(String),
    #[error("font pack module id '{0}' is declared more than once")]
    DuplicateModuleId(String),
    #[error("font chain references pack '{0}' more than once")]
    DuplicatePackReference(String),
    #[error("font pack role must not be empty")]
    EmptyRole,
    #[error("font pack role '{0}' must not contain '/'")]
    QualifiedRole(String),
    #[error("font pack role '{0}' maps to an empty family")]
    EmptyFamily(String),
    #[error("font pack coverage '{0}' has an empty name or description")]
    InvalidCoverage(String),
    #[error("font pack face '{0}' has invalid style, weight, stretch, or coverage")]
    InvalidFace(String),
    #[error("font chain references unknown pack '{0}'")]
    UnknownPack(String),
    #[error("resource preparation cancelled")]
    Cancelled,
    #[error("font face asset could not be prepared: {0}")]
    Asset(String),
}

#[derive(Debug, Clone)]
pub struct FontRegistry {
    packs: BTreeMap<String, FontPackBindings>,
    frontends: BTreeMap<String, FontFrontendBindings>,
    base_chain: Vec<String>,
    shell_chain: Vec<String>,
    chain: Vec<String>,
    host_families: BTreeSet<String>,
    bundled_families: BTreeSet<String>,
    host_database: fontdb::Database,
    database: fontdb::Database,
    revision: u64,
}

impl Default for FontRegistry {
    fn default() -> Self {
        let database = fontdb::Database::new();
        Self {
            packs: BTreeMap::new(),
            frontends: BTreeMap::new(),
            base_chain: Vec::new(),
            shell_chain: Vec::new(),
            chain: Vec::new(),
            host_families: BTreeSet::new(),
            bundled_families: BTreeSet::new(),
            host_database: database.clone(),
            database,
            revision: 0,
        }
    }
}

/// Per-frontend font policy. The author declares a pack chain and optional
/// role overrides; the user may replace that chain and add higher-priority
/// overrides in the module's settings namespace.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FontFrontendBindings {
    pub declared_pack_chain: Vec<String>,
    pub author_overrides: BTreeMap<String, String>,
    pub user_pack_chain: Option<Vec<String>>,
    pub user_overrides: BTreeMap<String, String>,
}

impl FontRegistry {
    pub fn new(host_families: impl IntoIterator<Item = String>) -> Self {
        let host_families = host_families.into_iter().collect();
        Self {
            host_families,
            ..Self::default()
        }
    }

    pub fn from_catalog(catalog: &SystemResourceCatalog) -> Self {
        let database = catalog.font_database();
        Self {
            host_families: catalog
                .font_families
                .iter()
                .map(|family| family.name.clone())
                .collect(),
            host_database: database.clone(),
            database,
            revision: catalog.revision,
            ..Self::default()
        }
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Return the prepared database used by the text renderer. It contains
    /// the host database captured during resource preparation plus every
    /// bundled face in the active candidate. Cloning the database is cheap
    /// compared with rediscovering and parsing the font files on the render
    /// path, and keeps the renderer independent from module file handles.
    pub fn font_database(&self) -> fontdb::Database {
        self.database.clone()
    }

    /// Whether a family name is present in the host database or in one of the
    /// prepared bundled faces. Font-pack requirements are intentionally soft;
    /// callers use this only to explain a fallback, never to block activation.
    pub fn has_family(&self, family: &str) -> bool {
        self.is_host_family(family)
    }

    /// Return soft requirements that are not present in the effective font
    /// database. Missing requirements never reject a pack; the resolver will
    /// use its normal system fallback and callers can surface this explanation.
    pub fn missing_requirements(&self) -> Vec<(String, String)> {
        self.packs
            .values()
            .flat_map(|pack| {
                pack.required_families.iter().filter_map(|family| {
                    (!self.is_host_family(family)).then(|| (pack.pack_id.clone(), family.clone()))
                })
            })
            .collect()
    }

    pub fn replace(
        &mut self,
        packs: Vec<FontPackBindings>,
        chain: Vec<String>,
    ) -> Result<(), FontRegistryError> {
        self.replace_with_cancellation(packs, chain, &ResourcePreparationToken::new())
    }

    /// Build the complete font candidate without mutating the live registry.
    /// The token is checked between packs and bounded face reads so a
    /// superseded profile cannot finish expensive work and then publish.
    pub fn replace_with_cancellation(
        &mut self,
        packs: Vec<FontPackBindings>,
        chain: Vec<String>,
        cancellation: &ResourcePreparationToken,
    ) -> Result<(), FontRegistryError> {
        let mut next = BTreeMap::new();
        let mut next_pack_ids_by_module = BTreeMap::new();
        for pack in packs {
            if cancellation.is_cancelled() {
                return Err(FontRegistryError::Cancelled);
            }
            validate_pack(&pack)?;
            let pack_id = pack.pack_id.clone();
            let module_id = pack.module_id.clone();
            if next.insert(pack_id.clone(), pack).is_some() {
                return Err(FontRegistryError::DuplicatePackId(pack_id));
            }
            if next_pack_ids_by_module
                .insert(module_id.clone(), pack_id.clone())
                .is_some()
            {
                return Err(FontRegistryError::DuplicateModuleId(module_id));
            }
        }
        let effective_chain = normalize_chain(&chain, &next)?;
        let mut next_database = self.host_database.clone();
        for pack in next.values() {
            for face in &pack.faces {
                if cancellation.is_cancelled() {
                    return Err(FontRegistryError::Cancelled);
                }
                let bytes = face
                    .asset
                    .read_bounded_with_cancellation(crate::DEFAULT_MAX_RESOURCE_BYTES, cancellation)
                    .map_err(|error| match error {
                        crate::ResourceAssetError::Cancelled { .. } => FontRegistryError::Cancelled,
                        error => FontRegistryError::Asset(error.to_string()),
                    })?;
                let mut face_database = fontdb::Database::new();
                face_database.load_font_data(bytes.clone());
                if !face_database.faces().any(|candidate| {
                    candidate
                        .families
                        .iter()
                        .any(|(family, _)| family.eq_ignore_ascii_case(&face.family))
                }) {
                    return Err(FontRegistryError::Asset(format!(
                        "font resource {} has no face named '{}'",
                        face.asset.candidate_path().display(),
                        face.family
                    )));
                }
                next_database.load_font_data(bytes);
            }
        }
        if cancellation.is_cancelled() {
            return Err(FontRegistryError::Cancelled);
        }
        self.packs = next;
        self.frontends.clear();
        self.base_chain = effective_chain.clone();
        self.shell_chain.clear();
        self.chain = effective_chain;
        self.bundled_families = self
            .packs
            .values()
            .flat_map(|pack| pack.faces.iter().map(|face| face.family.clone()))
            .collect();
        self.database = next_database;
        self.revision = self.revision.saturating_add(1);
        Ok(())
    }

    /// Prepend the shell-wide font-pack chain to every consumer. The
    /// normalized pack IDs are retained so module IDs and short pack aliases
    /// follow one deterministic lookup path.
    pub fn set_shell_pack_chain(&mut self, chain: &[String]) -> Result<(), FontRegistryError> {
        let shell_chain = normalize_chain(chain, &self.packs)?;
        self.shell_chain = shell_chain;
        self.chain = combined_chain(&self.shell_chain, &self.base_chain);
        self.revision = self.revision.saturating_add(1);
        Ok(())
    }

    /// Replace all frontend font contexts as one prepared snapshot. Unknown
    /// pack IDs are rejected before the live map is changed.
    pub fn set_frontend_bindings(
        &mut self,
        frontends: Vec<(String, FontFrontendBindings)>,
    ) -> Result<(), FontRegistryError> {
        let mut next = BTreeMap::new();
        for (module_id, mut binding) in frontends {
            if module_id.trim().is_empty() {
                return Err(FontRegistryError::EmptyModuleId);
            }
            binding.declared_pack_chain =
                normalize_chain(&binding.declared_pack_chain, &self.packs)?;
            if let Some(chain) = &binding.user_pack_chain {
                binding.user_pack_chain = Some(normalize_chain(chain, &self.packs)?);
            }
            if next.insert(module_id.clone(), binding).is_some() {
                return Err(FontRegistryError::DuplicateModuleId(module_id));
            }
        }
        self.frontends = next;
        self.revision = self.revision.saturating_add(1);
        Ok(())
    }

    pub fn resolve(&self, reference: &str) -> FontResolution {
        self.resolve_in_chain(reference, &self.chain)
    }

    /// Resolve a font reference using one frontend's user/author/declared
    /// policy before falling back to the shell-wide chain.
    pub fn resolve_for_module(&self, module_id: &str, reference: &str) -> FontResolution {
        let Some(frontend) = self.frontends.get(module_id) else {
            return self.resolve(reference);
        };
        let chain = frontend.effective_chain(&self.shell_chain, &self.base_chain);
        if let Some(override_reference) = frontend.user_overrides.get(reference) {
            return self.resolve_override(reference, override_reference, &chain);
        }
        if let Some(override_reference) = frontend.author_overrides.get(reference) {
            return self.resolve_override(reference, override_reference, &chain);
        }
        self.resolve_in_chain(reference, &chain)
    }

    fn resolve_override(
        &self,
        requested: &str,
        override_reference: &str,
        chain: &[String],
    ) -> FontResolution {
        let mut resolution = self.resolve_in_chain(override_reference, chain);
        resolution.requested = requested.into();
        resolution
    }

    fn resolve_in_chain(&self, reference: &str, chain: &[String]) -> FontResolution {
        if let Some((pack_id, role)) = reference.split_once('/') {
            if let Some(family) = self
                .packs
                .get(pack_id)
                .and_then(|pack| pack.mappings.get(role))
            {
                return self.pack_resolution(reference, role, pack_id, family);
            }
        }

        if self.is_host_family(reference) {
            return FontResolution {
                requested: reference.into(),
                role: None,
                family: reference.into(),
                available: true,
                source: FontResolutionSource::InstalledFamily,
            };
        }

        for pack_id in chain {
            if let Some(family) = self
                .packs
                .get(pack_id)
                .and_then(|pack| pack.mappings.get(reference))
            {
                return self.pack_resolution(reference, reference, pack_id, family);
            }
        }

        FontResolution {
            requested: reference.into(),
            role: Some(reference.into()),
            family: "sans-serif".into(),
            available: true,
            source: FontResolutionSource::SystemFallback,
        }
    }

    /// Return aliases accepted by the renderer for direct `font-family`
    /// values. CSS custom-property tokens are resolved by the style engine;
    /// this map covers the pack-qualified escape hatch and bare role values.
    pub fn reference_aliases_for_module(&self, module_id: &str) -> BTreeMap<String, String> {
        let mut aliases = self
            .packs
            .iter()
            .flat_map(|(pack_id, pack)| {
                pack.mappings
                    .iter()
                    .map(move |(role, family)| (format!("{pack_id}/{role}"), family.clone()))
            })
            .collect::<BTreeMap<_, _>>();
        for role in self.roles_for_module(module_id) {
            aliases.insert(
                role.clone(),
                self.resolve_for_module(module_id, &role).family,
            );
        }
        aliases
    }

    pub fn role_tokens(&self) -> BTreeMap<String, String> {
        let mut roles = STANDARD_ROLES
            .iter()
            .map(|role| (*role).to_owned())
            .collect::<BTreeSet<_>>();
        roles.extend(
            self.chain
                .iter()
                .filter_map(|pack_id| self.packs.get(pack_id))
                .flat_map(|pack| pack.mappings.keys().cloned()),
        );
        roles
            .into_iter()
            .map(|role| {
                let resolution = self.resolve(&role);
                (format!("--font-{role}"), resolution.family)
            })
            .collect()
    }

    fn roles_for_module(&self, module_id: &str) -> BTreeSet<String> {
        let mut roles = STANDARD_ROLES
            .iter()
            .map(|role| (*role).to_owned())
            .collect::<BTreeSet<_>>();
        let Some(frontend) = self.frontends.get(module_id) else {
            return roles;
        };
        let chain = frontend.effective_chain(&self.shell_chain, &self.base_chain);
        roles.extend(
            chain
                .iter()
                .filter_map(|pack_id| self.packs.get(pack_id))
                .flat_map(|pack| pack.mappings.keys().cloned()),
        );
        roles
    }

    /// Return internal theme-token bindings for pack-qualified references.
    /// These are kept separate from the public `--font-*` role tokens so a
    /// removed pack cannot leave a stale qualified escape hatch in the theme.
    pub fn qualified_role_tokens(&self) -> BTreeMap<String, String> {
        self.packs
            .iter()
            .flat_map(|(pack_id, pack)| {
                pack.mappings.iter().map(move |(role, family)| {
                    (format!("mesh.font.{pack_id}.{role}"), family.clone())
                })
            })
            .collect()
    }

    /// Return the declared script coverage for the face selected by a role.
    /// An empty set means the role resolves to a host family without a
    /// bundled face declaration; glyph-level fallback remains renderer-owned.
    pub fn coverage_for(&self, reference: &str) -> BTreeSet<String> {
        let Some(pack_id) = self.pack_id_for_reference(reference) else {
            return BTreeSet::new();
        };
        let resolution = self.resolve(reference);
        if !matches!(resolution.source, FontResolutionSource::Pack { .. }) {
            return BTreeSet::new();
        }
        let family = resolution.family;
        let Some(pack) = self.packs.get(&pack_id) else {
            return BTreeSet::new();
        };
        pack.covers
            .keys()
            .cloned()
            .chain(
                pack.faces
                    .iter()
                    .filter(|face| face.family.eq_ignore_ascii_case(&family))
                    .flat_map(|face| face.coverage.iter().cloned()),
            )
            .collect()
    }

    fn pack_id_for_reference(&self, reference: &str) -> Option<String> {
        if let Some((pack_id, _)) = reference.split_once('/') {
            return self.packs.contains_key(pack_id).then(|| pack_id.to_owned());
        }
        self.chain.iter().find_map(|pack_id| {
            self.packs.get(pack_id).and_then(|pack| {
                pack.mappings
                    .contains_key(reference)
                    .then_some(pack_id.clone())
            })
        })
    }

    fn is_host_family(&self, family: &str) -> bool {
        self.host_families
            .iter()
            .chain(self.bundled_families.iter())
            .any(|candidate| candidate.eq_ignore_ascii_case(family))
    }

    fn pack_resolution(
        &self,
        requested: &str,
        role: &str,
        pack_id: &str,
        family: &str,
    ) -> FontResolution {
        if !self.is_host_family(family) {
            return FontResolution {
                requested: requested.into(),
                role: Some(role.into()),
                family: "sans-serif".into(),
                available: true,
                source: FontResolutionSource::SystemFallback,
            };
        }
        FontResolution {
            requested: requested.into(),
            role: Some(role.into()),
            family: family.into(),
            available: true,
            source: FontResolutionSource::Pack {
                pack_id: pack_id.into(),
            },
        }
    }
}

impl FontFrontendBindings {
    fn effective_chain(&self, shell_chain: &[String], base_chain: &[String]) -> Vec<String> {
        match self.user_pack_chain.as_deref() {
            // An explicit user chain, including an explicit empty chain,
            // replaces the author's chain and the profile baseline. The
            // shell-wide chain remains a prepend because it is the global
            // user preference.
            Some(user_chain) => combined_chain(shell_chain, user_chain),
            None if !self.declared_pack_chain.is_empty() => {
                combined_chain(shell_chain, &self.declared_pack_chain)
            }
            None => combined_chain(shell_chain, base_chain),
        }
    }
}

fn combined_chain(prefix: &[String], suffix: &[String]) -> Vec<String> {
    let mut result = Vec::with_capacity(prefix.len() + suffix.len());
    for pack_id in prefix.iter().chain(suffix) {
        if !result.iter().any(|existing| existing == pack_id) {
            result.push(pack_id.clone());
        }
    }
    result
}

fn normalize_chain(
    chain: &[String],
    packs: &BTreeMap<String, FontPackBindings>,
) -> Result<Vec<String>, FontRegistryError> {
    let mut normalized = Vec::with_capacity(chain.len());
    let mut seen = BTreeSet::new();
    for chain_id in chain {
        let pack_id = packs
            .values()
            .find(|pack| &pack.module_id == chain_id)
            .map(|pack| pack.pack_id.clone())
            .or_else(|| packs.contains_key(chain_id).then_some(chain_id.clone()))
            .ok_or_else(|| FontRegistryError::UnknownPack(chain_id.clone()))?;
        if !seen.insert(pack_id.clone()) {
            return Err(FontRegistryError::DuplicatePackReference(pack_id));
        }
        normalized.push(pack_id);
    }
    Ok(normalized)
}

fn validate_pack(pack: &FontPackBindings) -> Result<(), FontRegistryError> {
    if pack.module_id.trim().is_empty() {
        return Err(FontRegistryError::EmptyModuleId);
    }
    if !is_canonical_pack_id(&pack.pack_id) {
        return Err(FontRegistryError::NonCanonicalPackId(pack.pack_id.clone()));
    }
    for (role, family) in &pack.mappings {
        if role.trim().is_empty() {
            return Err(FontRegistryError::EmptyRole);
        }
        if role.contains('/') {
            return Err(FontRegistryError::QualifiedRole(role.clone()));
        }
        if family.trim().is_empty() {
            return Err(FontRegistryError::EmptyFamily(role.clone()));
        }
    }
    for (coverage, description) in &pack.covers {
        if coverage.trim().is_empty() || description.trim().is_empty() {
            return Err(FontRegistryError::InvalidCoverage(coverage.clone()));
        }
    }
    for face in &pack.faces {
        if face.family.trim().is_empty() {
            return Err(FontRegistryError::EmptyFamily("<face>".into()));
        }
        if !(1..=1000).contains(&face.weight) || !(50..=200).contains(&face.stretch) {
            return Err(FontRegistryError::InvalidFace(face.family.clone()));
        }
        if !matches!(face.style.as_str(), "normal" | "italic" | "oblique") {
            return Err(FontRegistryError::InvalidFace(face.family.clone()));
        }
        if face.coverage.iter().any(|entry| entry.trim().is_empty()) {
            return Err(FontRegistryError::InvalidFace(face.family.clone()));
        }
    }
    Ok(())
}

fn is_canonical_pack_id(value: &str) -> bool {
    !value.is_empty()
        && value.trim() == value
        && value == value.to_ascii_lowercase()
        && !value.contains('/')
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registry() -> FontRegistry {
        FontRegistry::new(["Inter".into(), "JetBrains Mono".into()])
    }

    fn default_pack() -> FontPackBindings {
        FontPackBindings {
            module_id: "@mesh/fonts-default".into(),
            pack_id: "default".into(),
            required_families: Vec::new(),
            covers: BTreeMap::new(),
            mappings: BTreeMap::from([
                ("body".into(), "Inter".into()),
                ("mono".into(), "JetBrains Mono".into()),
            ]),
            faces: Vec::new(),
        }
    }

    #[test]
    fn resolves_qualified_and_chained_roles_with_availability() {
        let mut registry = registry();
        registry
            .replace(vec![default_pack()], vec!["default".into()])
            .unwrap();

        assert_eq!(
            registry.resolve("body").source,
            FontResolutionSource::Pack {
                pack_id: "default".into()
            }
        );
        assert_eq!(registry.resolve("body").family, "Inter");
        assert_eq!(registry.resolve("default/mono").family, "JetBrains Mono");
        assert!(registry.resolve("default/mono").available);
    }

    #[test]
    fn exact_installed_family_precedes_role_lookup_and_missing_roles_fallback() {
        let mut role_registry = registry();
        role_registry
            .replace(vec![default_pack()], vec!["default".into()])
            .unwrap();

        assert!(matches!(
            role_registry.resolve("Inter").source,
            FontResolutionSource::InstalledFamily
        ));
        assert_eq!(role_registry.resolve("headline").family, "sans-serif");
        assert_eq!(role_registry.role_tokens()["--font-body"], "Inter");
        assert_eq!(role_registry.role_tokens()["--font-headline"], "sans-serif");
        assert_eq!(
            role_registry.qualified_role_tokens()["mesh.font.default.body"],
            "Inter"
        );

        let mut module_chain_registry = role_registry;
        module_chain_registry
            .replace(vec![default_pack()], vec!["@mesh/fonts-default".into()])
            .unwrap();
        assert_eq!(module_chain_registry.resolve("body").family, "Inter");
    }

    #[test]
    fn unavailable_pack_family_uses_system_fallback() {
        let mut registry = FontRegistry::new(["Inter".into()]);
        registry
            .replace(
                vec![FontPackBindings {
                    module_id: "@mesh/fonts-missing".into(),
                    pack_id: "missing".into(),
                    required_families: Vec::new(),
                    covers: BTreeMap::new(),
                    mappings: BTreeMap::from([("body".into(), "Not Installed".into())]),
                    faces: Vec::new(),
                }],
                vec!["missing".into()],
            )
            .unwrap();

        let resolution = registry.resolve("body");
        assert_eq!(resolution.family, "sans-serif");
        assert_eq!(resolution.source, FontResolutionSource::SystemFallback);
        assert!(resolution.available);
    }

    #[test]
    fn custom_chain_roles_project_to_tokens_and_soft_requirements() {
        let mut registry = FontRegistry::new(["Inter".into()]);
        registry
            .replace(
                vec![FontPackBindings {
                    module_id: "@mesh/fonts-custom".into(),
                    pack_id: "custom".into(),
                    required_families: vec!["Inter".into(), "Missing Family".into()],
                    covers: BTreeMap::from([(String::from("cyrillic"), String::from("Cyrillic"))]),
                    mappings: BTreeMap::from([("brand".into(), "Inter".into())]),
                    faces: Vec::new(),
                }],
                vec!["custom".into()],
            )
            .unwrap();

        assert_eq!(registry.role_tokens()["--font-brand"], "Inter");
        assert_eq!(
            registry.coverage_for("brand"),
            BTreeSet::from(["cyrillic".into()])
        );
        assert_eq!(
            registry.missing_requirements(),
            vec![("custom".into(), "Missing Family".into())]
        );
    }

    #[test]
    fn bundled_face_coverage_is_available_to_role_diagnostics() {
        let temp = tempfile::tempdir().unwrap();
        let mut system_fonts = fontdb::Database::new();
        system_fonts.load_system_fonts();
        let (system_path, family) = system_fonts
            .faces()
            .find_map(|face| match &face.source {
                fontdb::Source::File(path) => face
                    .families
                    .first()
                    .map(|(family, _)| (path.clone(), family.clone())),
                _ => None,
            })
            .expect("test environment should provide a file-backed font");
        std::fs::copy(&system_path, temp.path().join("bundled.ttf")).unwrap();
        let asset = ResourceAssetHandle::new(temp.path(), "bundled.ttf").unwrap();
        let mut pack = FontPackBindings {
            module_id: "@mesh/fonts-test".into(),
            pack_id: "test".into(),
            required_families: Vec::new(),
            covers: BTreeMap::from([(String::from("latin"), String::from("Latin"))]),
            mappings: BTreeMap::from([("body".into(), family.clone())]),
            faces: Vec::new(),
        };
        pack.faces.push(FontFaceBinding {
            family: family.clone(),
            asset,
            weight: 400,
            style: "normal".into(),
            stretch: 100,
            coverage: BTreeSet::from(["latin".into(), "latin-ext".into()]),
        });
        let mut registry = FontRegistry::new([family]);
        registry.replace(vec![pack], vec!["test".into()]).unwrap();
        assert_eq!(
            registry.coverage_for("body"),
            BTreeSet::from(["latin".into(), "latin-ext".into()])
        );
        assert!(registry.resolve("body").available);
    }

    #[test]
    fn replacement_rejects_ambiguous_or_unknown_pack_state() {
        let mut registry_for_rejections = registry();
        let mut invalid = default_pack();
        invalid.mappings.insert("bad/role".into(), "Inter".into());
        assert_eq!(
            registry_for_rejections.replace(vec![invalid], vec!["default".into()]),
            Err(FontRegistryError::QualifiedRole("bad/role".into()))
        );
        assert_eq!(
            registry_for_rejections.replace(vec![default_pack()], vec!["missing".into()]),
            Err(FontRegistryError::UnknownPack("missing".into()))
        );
        assert_eq!(
            registry_for_rejections.replace(
                vec![default_pack()],
                vec!["default".into(), "@mesh/fonts-default".into()]
            ),
            Err(FontRegistryError::DuplicatePackReference("default".into()))
        );

        let cancellation = ResourcePreparationToken::new();
        cancellation.cancel();
        let mut unchanged = registry();
        assert!(matches!(
            unchanged.replace_with_cancellation(
                vec![default_pack()],
                vec!["default".into()],
                &cancellation,
            ),
            Err(FontRegistryError::Cancelled)
        ));
        assert_eq!(unchanged.resolve("body").family, "sans-serif");
    }

    #[test]
    fn explicit_empty_user_chain_replaces_author_and_profile_chains() {
        let mut registry = registry();
        registry
            .replace(vec![default_pack()], vec!["default".into()])
            .unwrap();
        registry
            .set_frontend_bindings(vec![(
                "@mesh/frontend".into(),
                FontFrontendBindings {
                    declared_pack_chain: vec!["default".into()],
                    user_pack_chain: Some(Vec::new()),
                    ..Default::default()
                },
            )])
            .unwrap();

        assert_eq!(
            registry.resolve_for_module("@mesh/frontend", "body").source,
            FontResolutionSource::SystemFallback
        );
    }
}
