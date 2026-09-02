use super::{
    ModuleId, ModuleKind, ModuleManifest, ModuleManifestError, RootLayoutSelection,
    RootModuleGraphManifest, RootThemeSelection,
};
use crate::manifest::SurfaceLayoutSection;
use mesh_core_service::{canonical_interface_name, parse_contract_version, parse_version_req};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

pub const PROFILE_SCHEMA_VERSION: u32 = 3;
pub const DEFAULT_PROFILE_ID: &str = "default";

static PROFILE_TEMP_FILE_SEQUENCE: AtomicU64 = AtomicU64::new(0);
const CONTROL_PLANE_LOCK_FILE: &str = ".mesh-control-plane.lock";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ShellProfile {
    #[serde(default = "default_profile_schema_version")]
    pub schema_version: u32,
    /// Monotonic durable revision used by profile-scoped transactions.
    /// Legacy profiles omit it and begin at zero.
    #[serde(default, skip_serializing_if = "is_zero_revision")]
    pub revision: u64,
    /// The composition module this profile instantiates. Absent means a
    /// hand-built composition: every field below is the whole decision rather
    /// than a delta over an installed composition.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<super::CompositionRef>,
    #[serde(default)]
    pub roots: BTreeMap<String, ProfileRootInstance>,
    #[serde(default)]
    pub background_services: BTreeSet<String>,
    #[serde(default)]
    pub providers: BTreeMap<String, String>,
    #[serde(default)]
    pub resources: ProfileResources,
    /// Sparse ordered placements for author-declared customizable slots.
    #[serde(default)]
    pub node_slots: BTreeMap<String, BTreeMap<String, super::NodeSlotOverride>>,
    /// Sparse preference overrides layered over the shared settings store.
    /// Keys use the same namespaces as `settings.json`; durable module data is
    /// deliberately not stored here.
    #[serde(default)]
    pub settings: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProfileRootInstance {
    /// Omitted in a sparse override; the profile key supplies the module id
    /// during load.  Keeping the field optional on input lets `{ "active":
    /// false }` deactivate an inherited root without repeating its identity.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub module: String,
    #[serde(default = "default_entrypoint")]
    pub entrypoint: String,
    #[serde(default = "default_true")]
    pub active: bool,
    /// Sparse per-instance placement override. Omission inherits `mesh.surface`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub surface: Option<SurfaceLayoutSection>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProfileResources {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme: Option<String>,
    #[serde(default)]
    pub icons: Vec<String>,
    #[serde(default)]
    pub fonts: Vec<String>,
    #[serde(default)]
    pub languages: Vec<String>,
}

fn default_profile_schema_version() -> u32 {
    PROFILE_SCHEMA_VERSION
}

fn is_zero_revision(revision: &u64) -> bool {
    *revision == 0
}

fn default_entrypoint() -> String {
    "main".into()
}

fn default_true() -> bool {
    true
}

fn validate_unique_ordered_ids(field: &str, values: &[String]) -> Result<(), ModuleManifestError> {
    let mut seen = HashSet::new();
    for value in values {
        if !seen.insert(value) {
            return Err(ModuleManifestError::Validation(format!(
                "{field} contains duplicate module '{value}', which would make precedence ambiguous"
            )));
        }
    }
    Ok(())
}

fn version_satisfies_requirement(requirement: &str, version: &str) -> bool {
    parse_version_req(requirement)
        .zip(parse_contract_version(version))
        .is_some_and(|(requirement, version)| requirement.matches(&version))
}

fn provider_version_satisfies(
    offered_version: Option<&str>,
    interface: &str,
    requirement: &str,
    declared_versions: &HashMap<String, Vec<String>>,
) -> bool {
    let declaration_matches = declared_versions.get(interface).is_none_or(|versions| {
        versions
            .iter()
            .any(|version| version_satisfies_requirement(requirement, version))
    });
    if !declaration_matches {
        return false;
    }

    offered_version.is_some_and(|version| version_satisfies_requirement(requirement, version))
        || offered_version.is_none()
            && declared_versions.get(interface).is_some_and(|versions| {
                versions
                    .iter()
                    .any(|version| version_satisfies_requirement(requirement, version))
            })
        || offered_version.is_none() && !declared_versions.contains_key(interface)
}

impl Default for ProfileRootInstance {
    fn default() -> Self {
        Self {
            module: String::new(),
            entrypoint: default_entrypoint(),
            active: true,
            surface: None,
        }
    }
}

impl ShellProfile {
    pub fn new() -> Self {
        Self {
            schema_version: PROFILE_SCHEMA_VERSION,
            revision: 0,
            ..Self::default()
        }
    }

    pub fn from_json_str(input: &str) -> Result<Self, ModuleManifestError> {
        let mut profile: Self =
            serde_json::from_str(input).map_err(|source| ModuleManifestError::Json {
                path: PathBuf::from("<profile>"),
                source,
            })?;
        profile.normalize_sparse_roots();
        profile.validate()?;
        Ok(profile)
    }

    pub fn from_path(path: &Path) -> Result<Self, ModuleManifestError> {
        super::validate_regular_file(path, "shell profile")?;
        let content = fs::read_to_string(path).map_err(|source| ModuleManifestError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        let mut profile: Self =
            serde_json::from_str(&content).map_err(|source| ModuleManifestError::Json {
                path: path.to_path_buf(),
                source,
            })?;
        profile.normalize_sparse_roots();
        profile.validate()?;
        Ok(profile)
    }

    fn normalize_sparse_roots(&mut self) {
        for (instance_id, instance) in &mut self.roots {
            if instance.module.is_empty()
                && let Some((module, _)) = instance_id.rsplit_once('#')
            {
                instance.module = module.to_string();
            }
        }
    }

    /// This profile's own decisions, as a composition layer.
    pub fn as_composition_spec(&self) -> super::CompositionSpec {
        super::CompositionSpec {
            roots: self.roots.clone(),
            background_services: self.background_services.clone(),
            providers: self.providers.clone(),
            resources: self.resources.clone(),
            slots: BTreeMap::new(),
            node_slots: self.node_slots.clone(),
            settings: self.settings.clone(),
        }
    }

    pub fn validate(&self) -> Result<(), ModuleManifestError> {
        if self.schema_version != PROFILE_SCHEMA_VERSION {
            return Err(ModuleManifestError::Validation(format!(
                "unsupported profile schemaVersion {}; supported version is {PROFILE_SCHEMA_VERSION}. \
                 Schema 3 adds sparse nodeSlots; set \"schemaVersion\": {PROFILE_SCHEMA_VERSION} to migrate an otherwise valid profile",
                self.schema_version
            )));
        }
        for (instance_id, instance) in &self.roots {
            validate_instance_id(instance_id, &instance.module)?;
            if instance.entrypoint.trim().is_empty() {
                return Err(ModuleManifestError::Validation(format!(
                    "profile root {instance_id} has an empty entrypoint"
                )));
            }
            if instance.entrypoint != "main" {
                return Err(ModuleManifestError::Validation(format!(
                    "profile root {instance_id} requests entrypoint '{}', but a module exposes only \
                     its primary component and multiple entrypoints per module are not implemented; \
                     the shell would silently mount 'main' instead, so this value is rejected rather \
                     than honored",
                    instance.entrypoint
                )));
            }
        }
        if let Some(from) = &self.from {
            validate_module_ids([&from.module].into_iter(), "from")?;
            if let Some(version) = &from.version
                && parse_contract_version(version).is_none()
            {
                return Err(ModuleManifestError::Validation(format!(
                    "profile composition version pin '{}' is not a valid semantic version",
                    version
                )));
            }
        }
        validate_module_ids(self.background_services.iter(), "backgroundServices")?;
        validate_module_ids(self.providers.values(), "providers")?;
        validate_module_ids(
            self.resources
                .theme
                .iter()
                .chain(self.resources.icons.iter())
                .chain(self.resources.fonts.iter())
                .chain(self.resources.languages.iter()),
            "resources",
        )?;
        validate_unique_ordered_ids("resources.languages", &self.resources.languages)?;
        for (instance_id, slots) in &self.node_slots {
            if instance_id.trim().is_empty() {
                return Err(ModuleManifestError::Validation(
                    "nodeSlots cannot contain an empty root instance id".into(),
                ));
            }
            for (slot_name, slot) in slots {
                if slot_name.trim().is_empty() {
                    return Err(ModuleManifestError::Validation(format!(
                        "nodeSlots.{instance_id} cannot contain an empty slot name"
                    )));
                }
                let mut ids = HashSet::new();
                for node in &slot.nodes {
                    if node.id.trim().is_empty() || !ids.insert(node.id.as_str()) {
                        return Err(ModuleManifestError::Validation(format!(
                            "nodeSlots.{instance_id}.{slot_name} has an empty or duplicate placement id '{}'",
                            node.id
                        )));
                    }
                    let Some((module, contribution)) = node.contribution.rsplit_once(':') else {
                        return Err(ModuleManifestError::Validation(format!(
                            "node placement '{}' must use module-id:contribution-id",
                            node.contribution
                        )));
                    };
                    let module = module.to_string();
                    validate_module_ids([&module].into_iter(), "nodeSlots")?;
                    if contribution.trim().is_empty() {
                        return Err(ModuleManifestError::Validation(format!(
                            "node placement '{}' has an empty contribution id",
                            node.contribution
                        )));
                    }
                }
            }
        }
        for (namespace, value) in &self.settings {
            if namespace != "shell" && !namespace.starts_with('@') && !namespace.contains('.') {
                return Err(ModuleManifestError::Validation(format!(
                    "profile settings key '{namespace}' is not a shell, module, instance, or interface namespace"
                )));
            }
            if !value.is_object() {
                return Err(ModuleManifestError::Validation(format!(
                    "profile settings namespace '{namespace}' must contain an object"
                )));
            }
        }
        Ok(())
    }

    pub fn add_frontend(
        &mut self,
        manifest: &ModuleManifest,
    ) -> Result<String, ModuleManifestError> {
        if manifest.mesh.kind != ModuleKind::Frontend {
            return Err(ModuleManifestError::Validation(format!(
                "{} is {:?}; only frontend modules create profile root instances",
                manifest.name, manifest.mesh.kind
            )));
        }
        let instance_id = format!("{}#default", manifest.name);
        self.roots
            .entry(instance_id.clone())
            .and_modify(|instance| instance.active = true)
            .or_insert_with(|| ProfileRootInstance {
                module: manifest.name.clone(),
                entrypoint: "main".into(),
                active: true,
                surface: None,
            });
        Ok(instance_id)
    }

    pub fn set_instance_active(
        &mut self,
        instance_id: &str,
        active: bool,
    ) -> Result<(), ModuleManifestError> {
        let instance = self.roots.get_mut(instance_id).ok_or_else(|| {
            ModuleManifestError::Validation(format!("profile has no root instance {instance_id}"))
        })?;
        instance.active = active;
        Ok(())
    }

    pub fn remove_instance(&mut self, instance_id: &str) -> bool {
        self.roots.remove(instance_id).is_some()
    }

    /// Whether any profile-owned root, provider, service, resource, or
    /// composition reference points at a module.
    pub fn references_module(&self, module_id: &str) -> bool {
        self.from
            .as_ref()
            .is_some_and(|from| from.module == module_id)
            || self.roots.values().any(|root| root.module == module_id)
            || self.background_services.contains(module_id)
            || self
                .providers
                .values()
                .any(|provider| provider == module_id)
            || self.resources.theme.as_deref() == Some(module_id)
            || self.resources.icons.iter().any(|id| id == module_id)
            || self.resources.fonts.iter().any(|id| id == module_id)
            || self.resources.languages.iter().any(|id| id == module_id)
    }

    /// Remove all profile-owned references to a forcibly uninstalled module.
    pub fn remove_module_references(&mut self, module_id: &str) {
        if self
            .from
            .as_ref()
            .is_some_and(|from| from.module == module_id)
        {
            self.from = None;
        }
        self.roots.retain(|_, root| root.module != module_id);
        self.background_services.remove(module_id);
        self.providers.retain(|_, provider| provider != module_id);
        if self.resources.theme.as_deref() == Some(module_id) {
            self.resources.theme = None;
        }
        self.resources.icons.retain(|id| id != module_id);
        self.resources.fonts.retain(|id| id != module_id);
        self.resources.languages.retain(|id| id != module_id);
    }

    /// Resolve the modules needed by this profile. Roots are explicit; declared
    /// module/resource dependencies and sole interface providers are inferred.
    pub fn active_module_ids<'a>(
        &self,
        manifests: impl IntoIterator<Item = &'a ModuleManifest>,
    ) -> Result<HashSet<String>, ModuleManifestError> {
        let manifest_list = manifests.into_iter().collect::<Vec<_>>();
        let mut manifests = HashMap::new();
        for manifest in manifest_list.iter().copied() {
            if manifests.insert(manifest.name.as_str(), manifest).is_some() {
                return Err(ModuleManifestError::Validation(format!(
                    "duplicate installed module manifest '{}'",
                    manifest.name
                )));
            }
        }
        let mut active = HashSet::new();
        let mut queue = VecDeque::new();

        // A composition module contributes a dependency node as well as the
        // roots and resources that its profile declares. Resolve the same
        // effective spec used by graph loading so the activation set contains
        // the complete composition closure, including `extends` ancestors.
        let profile_spec = self.as_composition_spec();
        let effective = if self.from.is_some() {
            Some(super::resolve_composition(
                self,
                manifest_list.iter().copied(),
            )?)
        } else {
            None
        };
        let activation_spec = effective
            .as_ref()
            .map(|resolved| &resolved.spec)
            .unwrap_or(&profile_spec);
        let mut declared_interface_versions = HashMap::<String, Vec<String>>::new();
        for manifest in &manifest_list {
            if let Some(declaration) = &manifest.mesh.interface
                && let Some(version) = &declaration.version
            {
                declared_interface_versions
                    .entry(canonical_interface_name(&declaration.name))
                    .or_default()
                    .push(version.clone());
            }
            for declaration in &manifest.mesh.interfaces {
                if let Some(version) = &declaration.version {
                    declared_interface_versions
                        .entry(canonical_interface_name(&declaration.name))
                        .or_default()
                        .push(version.clone());
                }
            }
        }
        let mut requested_providers = HashMap::new();
        for (interface, module_id) in &activation_spec.providers {
            let interface = canonical_interface_name(interface);
            if let Some(previous) = requested_providers.insert(interface.clone(), module_id)
                && previous != module_id
            {
                return Err(ModuleManifestError::Validation(format!(
                    "profile declares conflicting providers for interface {interface}: {previous} and {module_id}"
                )));
            }
        }
        for language_pack in &activation_spec.resources.languages {
            let manifest = manifests.get(language_pack.as_str()).ok_or_else(|| {
                ModuleManifestError::Validation(format!(
                    "profile references language pack {language_pack}, but it is not installed"
                ))
            })?;
            if manifest.mesh.kind != ModuleKind::LanguagePack {
                return Err(ModuleManifestError::Validation(format!(
                    "profile resource {language_pack} is {:?}; resources.languages requires a language-pack module",
                    manifest.mesh.kind
                )));
            }
        }
        let orphaned = effective
            .as_ref()
            .map(|resolved| resolved.orphaned_overrides.as_slice())
            .unwrap_or(&[]);
        let is_active_host = |instance_id: &str| {
            activation_spec
                .roots
                .get(instance_id)
                .is_some_and(|instance| instance.active)
                && !orphaned.iter().any(|orphan| {
                    orphan == instance_id || orphan == &format!("nodeSlots.{instance_id}")
                })
        };

        for (instance_id, instance) in &activation_spec.roots {
            if !instance.active || !is_active_host(instance_id) {
                continue;
            }
            queue.push_back(instance.module.clone());
        }
        queue.extend(activation_spec.background_services.iter().cloned());
        queue.extend(activation_spec.providers.values().cloned());
        if let Some(theme) = &activation_spec.resources.theme {
            queue.push_back(theme.clone());
        }
        queue.extend(activation_spec.resources.icons.iter().cloned());
        queue.extend(activation_spec.resources.fonts.iter().cloned());
        queue.extend(activation_spec.resources.languages.iter().cloned());
        for (instance_id, slots) in &activation_spec.node_slots {
            if !is_active_host(instance_id) {
                continue;
            }
            for slot in slots.values() {
                for node in &slot.nodes {
                    if let Some((module_id, _)) = node.contribution.rsplit_once(':') {
                        queue.push_back(module_id.to_string());
                    }
                }
            }
        }

        if let Some(from) = &self.from {
            let compositions = manifest_list
                .iter()
                .copied()
                .filter(|manifest| manifest.mesh.kind == ModuleKind::Composition)
                .map(|manifest| (manifest.name.as_str(), manifest))
                .collect::<HashMap<_, _>>();
            for composition_id in super::composition_chain(&from.module, &compositions)? {
                queue.push_back(composition_id);
            }
        }

        while let Some(module_id) = queue.pop_front() {
            if !active.insert(module_id.clone()) {
                continue;
            }
            let manifest = manifests.get(module_id.as_str()).ok_or_else(|| {
                ModuleManifestError::Validation(format!(
                    "active profile references module {module_id}, but it is not installed"
                ))
            })?;

            queue.extend(
                manifest
                    .mesh
                    .dependencies
                    .modules
                    .iter()
                    .filter(|(_, dependency)| !dependency.is_optional())
                    .map(|(module_id, _)| module_id.clone()),
            );
            if manifest.mesh.kind == ModuleKind::Composition
                && let Some(parent) = &manifest.mesh.extends
            {
                queue.push_back(parent.clone());
            }
            queue.extend(manifest.mesh.uses.resources.icons.iter().cloned());
            queue.extend(manifest.mesh.uses.resources.fonts.iter().cloned());
            queue.extend(manifest.mesh.uses.resources.i18n.iter().cloned());
            queue.extend(manifest.mesh.uses.resources.themes.iter().cloned());

            let interface_requirements = manifest
                .mesh
                .dependencies
                .backend
                .iter()
                .chain(manifest.mesh.uses.interfaces.iter())
                .map(|(interface, requirement)| {
                    (canonical_interface_name(interface), requirement.clone())
                })
                .collect::<BTreeMap<_, _>>();
            for (interface, requirement) in interface_requirements {
                for candidate in manifests.values().filter(|candidate| {
                    candidate.mesh.kind == ModuleKind::Interface
                        && candidate
                            .mesh
                            .interface
                            .as_ref()
                            .is_some_and(|declaration| {
                                canonical_interface_name(&declaration.name) == interface
                                    && declaration.version.as_deref().is_some_and(|version| {
                                        version_satisfies_requirement(&requirement, version)
                                    })
                            })
                }) {
                    queue.push_back(candidate.name.clone());
                }

                let compatible_provider = |candidate: &ModuleManifest| {
                    candidate.mesh.kind == ModuleKind::Backend
                        && candidate.mesh.implementations().any(|implementation| {
                            canonical_interface_name(&implementation.interface) == interface
                                && provider_version_satisfies(
                                    implementation.version.as_deref(),
                                    &interface,
                                    &requirement,
                                    &declared_interface_versions,
                                )
                        })
                };
                if let Some(provider) = requested_providers.get(&interface) {
                    let candidate = manifests.get(provider.as_str()).ok_or_else(|| {
                        ModuleManifestError::Validation(format!(
                            "profile selects provider {provider} for {interface}, but it is not installed"
                        ))
                    })?;
                    if !compatible_provider(candidate) {
                        return Err(ModuleManifestError::Validation(format!(
                            "profile selects provider {provider} for {interface} {requirement}, but it does not provide a compatible version"
                        )));
                    }
                    queue.push_back((*provider).clone());
                    continue;
                }
                let providers = manifests
                    .values()
                    .filter(|candidate| compatible_provider(candidate))
                    .collect::<Vec<_>>();
                let mut providers = providers;
                providers.sort_by(|left, right| left.name.cmp(&right.name));
                if providers.len() == 1 {
                    queue.push_back(providers[0].name.clone());
                } else if providers.len() > 1 {
                    return Err(ModuleManifestError::Validation(format!(
                        "profile requires {interface} {requirement}, but compatible providers are ambiguous: {}",
                        providers
                            .iter()
                            .map(|provider| provider.name.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    )));
                } else if manifests.values().any(|candidate| {
                    candidate.mesh.kind == ModuleKind::Backend
                        && candidate.mesh.implementations().any(|implementation| {
                            canonical_interface_name(&implementation.interface) == interface
                        })
                }) || declared_interface_versions.contains_key(&interface)
                {
                    return Err(ModuleManifestError::Validation(format!(
                        "profile requires {interface} {requirement}, but no installed provider satisfies that version"
                    )));
                }
            }

            if manifest.mesh.kind == ModuleKind::Backend {
                for implementation in manifest.mesh.implementations() {
                    if let Some(base_module) = &implementation.base_module {
                        queue.push_back(base_module.clone());
                    }
                }
            }
        }
        let resolution = super::resolve_closure(
            active.iter().map(String::as_str),
            manifest_list.iter().copied(),
        );
        if !resolution.is_satisfiable() {
            if let Some(conflict) = resolution.conflicts.first() {
                return Err(ModuleManifestError::Validation(format!(
                    "profile activation rejected: {}",
                    conflict.message()
                )));
            }
            if let Some((dependency, requirers)) = resolution.missing.iter().next() {
                return Err(ModuleManifestError::Validation(format!(
                    "profile activation rejected: required module {dependency} is missing (requested by {})",
                    requirers.iter().cloned().collect::<Vec<_>>().join(", ")
                )));
            }
            if let Some((module_id, reasons)) = resolution
                .blocked
                .iter()
                .find(|(module_id, _)| resolution.required_closure.contains(*module_id))
            {
                return Err(ModuleManifestError::Validation(format!(
                    "profile activation rejected: module {module_id} is blocked because {}",
                    reasons.iter().cloned().collect::<Vec<_>>().join("; ")
                )));
            }
        }
        Ok(active)
    }

    pub fn apply_to_root(
        &self,
        root: &mut RootModuleGraphManifest,
        manifests: &[ModuleManifest],
    ) -> Result<(), ModuleManifestError> {
        self.validate()?;
        let active = self.active_module_ids(manifests)?;
        for (module_id, entry) in &mut root.modules {
            entry.enabled = active.contains(module_id);
        }
        root.disabled.clear();
        root.providers = self.providers.clone().into_iter().collect();
        root.theme = self
            .resources
            .theme
            .as_ref()
            .map(|active| RootThemeSelection {
                active: active.clone(),
                mode: None,
            });
        root.layout = self
            .roots
            .values()
            .find(|instance| instance.active)
            .map(|instance| RootLayoutSelection {
                entrypoint: format!("{}:{}", instance.module, instance.entrypoint),
            });
        root.validate()
    }
}

impl Default for ShellProfile {
    fn default() -> Self {
        Self {
            schema_version: PROFILE_SCHEMA_VERSION,
            revision: 0,
            from: None,
            roots: BTreeMap::new(),
            background_services: BTreeSet::new(),
            providers: BTreeMap::new(),
            resources: ProfileResources::default(),
            node_slots: BTreeMap::new(),
            settings: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ProfilePaths {
    config_dir: PathBuf,
}

impl ProfilePaths {
    pub fn from_root_graph(root_graph_path: &Path) -> Result<Self, ModuleManifestError> {
        let config_dir = root_graph_path.parent().ok_or_else(|| {
            ModuleManifestError::Validation(format!(
                "root module graph has no parent directory: {}",
                root_graph_path.display()
            ))
        })?;
        Ok(Self {
            config_dir: config_dir.to_path_buf(),
        })
    }

    pub fn profiles_dir(&self) -> PathBuf {
        self.config_dir.join("profiles")
    }

    pub fn active_profile_path(&self) -> PathBuf {
        self.config_dir.join("active-profile")
    }

    pub fn profile_path(&self, profile_id: &str) -> Result<PathBuf, ModuleManifestError> {
        validate_profile_id(profile_id)?;
        Ok(self.profiles_dir().join(format!("{profile_id}.json")))
    }

    pub fn active_profile_id(&self) -> Result<Option<String>, ModuleManifestError> {
        let path = self.active_profile_path();
        if !path.exists() {
            return Ok(None);
        }
        super::validate_regular_file(&path, "active profile pointer")?;
        let profile_id =
            fs::read_to_string(&path).map_err(|source| ModuleManifestError::Io { path, source })?;
        let profile_id = profile_id.trim();
        validate_profile_id(profile_id)?;
        Ok(Some(profile_id.to_string()))
    }

    pub fn load(&self, profile_id: &str) -> Result<ShellProfile, ModuleManifestError> {
        ShellProfile::from_path(&self.profile_path(profile_id)?)
    }

    pub fn load_or_default(&self, profile_id: &str) -> Result<ShellProfile, ModuleManifestError> {
        let path = self.profile_path(profile_id)?;
        if fs::symlink_metadata(&path)
            .map(|metadata| metadata.file_type().is_symlink())
            .unwrap_or(false)
        {
            return Err(ModuleManifestError::Validation(format!(
                "shell profile {} must not be a symlink",
                path.display()
            )));
        }
        if path.exists() {
            ShellProfile::from_path(&path)
        } else {
            Ok(ShellProfile::new())
        }
    }

    pub fn load_active(&self) -> Result<Option<(String, ShellProfile)>, ModuleManifestError> {
        let Some(profile_id) = self.active_profile_id()? else {
            return Ok(None);
        };
        let profile = self.load(&profile_id)?;
        Ok(Some((profile_id, profile)))
    }

    pub fn save(
        &self,
        profile_id: &str,
        profile: &ShellProfile,
    ) -> Result<(), ModuleManifestError> {
        profile.validate()?;
        let path = self.profile_path(profile_id)?;
        let _lock = self.acquire_control_plane_lock()?;
        self.save_unlocked(&path, profile)
    }

    fn save_unlocked(
        &self,
        path: &Path,
        profile: &ShellProfile,
    ) -> Result<(), ModuleManifestError> {
        let mut content =
            serde_json::to_string_pretty(profile).map_err(|source| ModuleManifestError::Json {
                path: path.to_path_buf(),
                source,
            })?;
        content.push('\n');
        atomic_write_unlocked(path, content.as_bytes())
    }

    /// Persist a profile candidate only if the on-disk profile still has the
    /// expected revision. The returned profile carries the newly committed
    /// revision for callers that keep the candidate in memory.
    pub fn save_if_revision(
        &self,
        profile_id: &str,
        profile: &ShellProfile,
        expected_revision: u64,
    ) -> Result<ShellProfile, ModuleManifestError> {
        if profile.revision != expected_revision {
            return Err(ModuleManifestError::Validation(format!(
                "profile revision conflict: expected {expected_revision}, found {}",
                profile.revision
            )));
        }

        let _lock = self.acquire_control_plane_lock()?;
        let current_revision = match self.load(profile_id) {
            Ok(current) => current.revision,
            Err(ModuleManifestError::Io { source, .. })
                if source.kind() == std::io::ErrorKind::NotFound =>
            {
                0
            }
            Err(error) => return Err(error),
        };
        if current_revision != expected_revision {
            return Err(ModuleManifestError::Validation(format!(
                "profile revision conflict: expected {expected_revision}, found {current_revision}"
            )));
        }

        let mut committed = profile.clone();
        committed.revision =
            expected_revision
                .checked_add(1)
                .ok_or(ModuleManifestError::RevisionExhausted {
                    current: expected_revision,
                })?;
        let path = self.profile_path(profile_id)?;
        self.save_unlocked(&path, &committed)?;
        Ok(committed)
    }

    fn acquire_control_plane_lock(&self) -> Result<ControlPlaneLock, ModuleManifestError> {
        let path = self.config_dir.join(CONTROL_PLANE_LOCK_FILE);
        let mut options = OpenOptions::new();
        options.create(true).read(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;

            options.mode(0o600);
        }
        let file = options
            .open(&path)
            .map_err(|source| ModuleManifestError::Io {
                path: path.clone(),
                source,
            })?;
        #[cfg(unix)]
        {
            let result =
                unsafe { libc::flock(std::os::fd::AsRawFd::as_raw_fd(&file), libc::LOCK_EX) };
            if result != 0 {
                return Err(ModuleManifestError::Io {
                    path,
                    source: std::io::Error::last_os_error(),
                });
            }
        }
        Ok(ControlPlaneLock(file))
    }

    /// Restore the active-profile pointer to a previously observed value.
    /// Used to roll back a partially-committed profile switch when a later
    /// transaction step fails after the pointer was already advanced to the
    /// candidate: `None` restores the pre-switch legacy/no-profile state by
    /// removing the pointer file rather than leaving it pointed at a
    /// candidate the running shell never actually adopted.
    pub fn restore_active(&self, profile_id: Option<&str>) -> Result<(), ModuleManifestError> {
        match profile_id {
            Some(profile_id) => self.set_active(profile_id),
            None => {
                let path = self.active_profile_path();
                match fs::remove_file(&path) {
                    Ok(()) => Ok(()),
                    Err(source) if source.kind() == std::io::ErrorKind::NotFound => Ok(()),
                    Err(source) => Err(ModuleManifestError::Io { path, source }),
                }
            }
        }
    }

    pub fn set_active(&self, profile_id: &str) -> Result<(), ModuleManifestError> {
        let profile_path = self.profile_path(profile_id)?;
        if !profile_path.exists() {
            return Err(ModuleManifestError::Validation(format!(
                "profile {profile_id} does not exist at {}",
                profile_path.display()
            )));
        }
        atomic_write(
            &self.active_profile_path(),
            format!("{profile_id}\n").as_bytes(),
        )
    }

    pub fn list(&self) -> Result<Vec<String>, ModuleManifestError> {
        let directory = self.profiles_dir();
        if !directory.exists() {
            return Ok(Vec::new());
        }
        if fs::symlink_metadata(&directory)
            .map(|metadata| metadata.file_type().is_symlink())
            .unwrap_or(false)
        {
            return Err(ModuleManifestError::Validation(format!(
                "profiles directory {} must not be a symlink",
                directory.display()
            )));
        }
        let mut ids = fs::read_dir(&directory)
            .map_err(|source| ModuleManifestError::Io {
                path: directory.clone(),
                source,
            })?
            .filter_map(Result::ok)
            .filter_map(|entry| {
                let path = entry.path();
                (path.extension().and_then(|value| value.to_str()) == Some("json"))
                    .then(|| path.file_stem()?.to_str().map(str::to_string))
                    .flatten()
            })
            .collect::<Vec<_>>();
        ids.sort();
        Ok(ids)
    }
}

fn validate_instance_id(instance_id: &str, module_id: &str) -> Result<(), ModuleManifestError> {
    ModuleId::parse(module_id)?;
    let expected_prefix = format!("{module_id}#");
    let suffix = instance_id
        .strip_prefix(&expected_prefix)
        .unwrap_or_default();
    if !instance_id.starts_with(&expected_prefix)
        || suffix.is_empty()
        || !suffix
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
    {
        return Err(ModuleManifestError::Validation(format!(
            "profile root key {instance_id} must use {module_id}#<instance-id>"
        )));
    }
    Ok(())
}

fn validate_module_ids<'a>(
    ids: impl IntoIterator<Item = &'a String>,
    field: &str,
) -> Result<(), ModuleManifestError> {
    for id in ids {
        ModuleId::parse(id).map_err(|_| {
            ModuleManifestError::Validation(format!(
                "profile {field} entry '{id}' must be a module id such as @scope/name"
            ))
        })?;
    }
    Ok(())
}

fn validate_profile_id(profile_id: &str) -> Result<(), ModuleManifestError> {
    if profile_id.is_empty()
        || !profile_id
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
    {
        return Err(ModuleManifestError::Validation(format!(
            "profile id '{profile_id}' must contain only ASCII letters, digits, '-' or '_'"
        )));
    }
    Ok(())
}

struct ControlPlaneLock(File);

impl Drop for ControlPlaneLock {
    fn drop(&mut self) {
        #[cfg(unix)]
        {
            let _ = unsafe { libc::flock(std::os::fd::AsRawFd::as_raw_fd(&self.0), libc::LOCK_UN) };
        }
    }
}

pub(super) fn atomic_write(path: &Path, content: &[u8]) -> Result<(), ModuleManifestError> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|source| ModuleManifestError::Io {
        path: parent.to_path_buf(),
        source,
    })?;
    let lock_path = parent.join(CONTROL_PLANE_LOCK_FILE);
    let lock_file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .open(&lock_path)
        .map_err(|source| ModuleManifestError::Io {
            path: lock_path.clone(),
            source,
        })?;
    #[cfg(unix)]
    {
        let result =
            unsafe { libc::flock(std::os::fd::AsRawFd::as_raw_fd(&lock_file), libc::LOCK_EX) };
        if result != 0 {
            return Err(ModuleManifestError::Io {
                path: lock_path,
                source: std::io::Error::last_os_error(),
            });
        }
    }
    atomic_write_unlocked(path, content)
}

fn atomic_write_unlocked(path: &Path, content: &[u8]) -> Result<(), ModuleManifestError> {
    super::validate_no_symlink_path(path, "package write target")?;
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|source| ModuleManifestError::Io {
        path: parent.to_path_buf(),
        source,
    })?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("profile.json");
    super::transaction::maybe_inject_failure("package.write.before")?;

    for _ in 0..128 {
        let sequence = PROFILE_TEMP_FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let temporary = parent.join(format!(
            ".{file_name}.tmp-{}-{sequence}",
            std::process::id()
        ));
        let file = match OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)
        {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(source) => {
                return Err(ModuleManifestError::Io {
                    path: temporary,
                    source,
                });
            }
        };
        let result = (|| {
            let mut file = file;
            file.write_all(content)?;
            file.sync_all()?;
            drop(file);
            fs::rename(&temporary, path)?;
            let directory = OpenOptions::new().read(true).open(parent)?;
            directory.sync_all()
        })();
        if let Err(source) = result {
            let _ = fs::remove_file(&temporary);
            return Err(ModuleManifestError::Io {
                path: path.to_path_buf(),
                source,
            });
        }
        super::transaction::maybe_inject_failure("package.write.after")?;
        return Ok(());
    }

    Err(ModuleManifestError::Validation(format!(
        "could not allocate a unique temporary profile file beside {}",
        path.display()
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(json: &str) -> ModuleManifest {
        ModuleManifest::from_json_str(json).unwrap()
    }

    #[test]
    fn frontend_install_adds_active_instance_without_copying_surface_defaults() {
        let module = manifest(
            r#"{"name":"@me/weather","version":"1","mesh":{"apiVersion":"0.1","kind":"frontend","entry":"src/main.mesh","surface":{"anchor":"top"}}}"#,
        );
        let mut profile = ShellProfile::new();
        let instance_id = profile.add_frontend(&module).unwrap();
        let instance = &profile.roots[&instance_id];
        assert!(instance.active);
        assert!(instance.surface.is_none());
    }

    #[test]
    fn activation_infers_declared_dependencies_and_sole_provider() {
        let frontend = manifest(
            r#"{"name":"@me/panel","version":"1","mesh":{"apiVersion":"0.1","kind":"frontend","entry":"main.mesh","uses":{"modules":{"@me/component":"1"},"interfaces":{"mesh.audio":">=1"}}}}"#,
        );
        let component = manifest(
            r#"{"name":"@me/component","version":"1","mesh":{"apiVersion":"0.1","kind":"component","entry":"main.mesh"}}"#,
        );
        let backend = manifest(
            r#"{"name":"@me/audio","version":"1","mesh":{"apiVersion":"0.1","kind":"backend","entry":"main.luau","implements":[{"interface":"mesh.audio","version":"1"}]}}"#,
        );
        let mut profile = ShellProfile::new();
        profile.add_frontend(&frontend).unwrap();
        let active = profile
            .active_module_ids([&frontend, &component, &backend])
            .unwrap();
        assert_eq!(
            active,
            HashSet::from([
                "@me/panel".to_string(),
                "@me/component".to_string(),
                "@me/audio".to_string(),
            ])
        );

        let mut root = RootModuleGraphManifest {
            schema_version: 1,
            modules_dir: "../modules".into(),
            capability_approvals: Default::default(),
            trust_policy: Default::default(),
            modules: [
                ("@me/panel", ModuleKind::Frontend),
                ("@me/component", ModuleKind::Component),
                ("@me/audio", ModuleKind::Backend),
                ("@me/unrelated", ModuleKind::Library),
            ]
            .into_iter()
            .map(|(id, kind)| {
                (
                    id.to_string(),
                    super::super::InstalledModuleEntry {
                        kind,
                        path: id.trim_start_matches("@me/").into(),
                        enabled: true,
                    },
                )
            })
            .collect(),
            disabled: Vec::new(),
            providers: HashMap::new(),
            layout: None,
            theme: None,
        };
        let unrelated = manifest(
            r#"{"name":"@me/unrelated","version":"1","mesh":{"apiVersion":"0.1","kind":"library"}}"#,
        );
        profile
            .apply_to_root(&mut root, &[frontend, component, backend, unrelated])
            .unwrap();
        assert!(root.modules["@me/panel"].enabled);
        assert!(root.modules["@me/component"].enabled);
        assert!(root.modules["@me/audio"].enabled);
        assert!(!root.modules["@me/unrelated"].enabled);
        assert_eq!(
            root.layout.unwrap().entrypoint,
            "@me/panel:main".to_string()
        );
    }

    #[test]
    fn activation_rejects_a_required_module_version_mismatch() {
        let frontend = manifest(
            r#"{"name":"@me/panel","version":"1.0.0","mesh":{"apiVersion":"0.1","kind":"frontend","entry":"main.mesh","uses":{"modules":{"@me/helpers":"^2.0.0"}}}}"#,
        );
        let helpers = manifest(
            r#"{"name":"@me/helpers","version":"1.5.0","mesh":{"apiVersion":"0.1","kind":"library"}}"#,
        );
        let mut profile = ShellProfile::new();
        profile.add_frontend(&frontend).unwrap();

        let error = profile
            .active_module_ids([&frontend, &helpers])
            .unwrap_err()
            .to_string();
        assert!(error.contains("@me/helpers"), "{error}");
        assert!(error.contains("^2.0.0"), "{error}");
    }

    #[test]
    fn activation_keeps_an_optional_module_version_mismatch_degraded() {
        let frontend = manifest(
            r#"{"name":"@me/panel","version":"1.0.0","mesh":{"apiVersion":"0.1","kind":"frontend","entry":"main.mesh","uses":{"modules":{"@me/helpers":{"version":"^2.0.0","optional":true}}}}}"#,
        );
        let helpers = manifest(
            r#"{"name":"@me/helpers","version":"1.5.0","mesh":{"apiVersion":"0.1","kind":"library"}}"#,
        );
        let mut profile = ShellProfile::new();
        profile.add_frontend(&frontend).unwrap();

        let active = profile.active_module_ids([&frontend, &helpers]).unwrap();
        assert_eq!(active, HashSet::from(["@me/panel".to_string()]));
    }

    #[test]
    fn activation_rejects_an_incompatible_required_interface_provider() {
        let frontend = manifest(
            r#"{"name":"@me/panel","version":"1.0.0","mesh":{"apiVersion":"0.1","kind":"frontend","entry":"main.mesh","uses":{"interfaces":{"mesh.audio":">=2.0.0"}}}}"#,
        );
        let interface = manifest(
            r#"{"name":"@me/audio-interface","version":"1.0.0","mesh":{"apiVersion":"0.1","kind":"interface","interface":{"name":"mesh.audio","version":"1.0.0","contract":{}}}}"#,
        );
        let backend = manifest(
            r#"{"name":"@me/audio","version":"1.0.0","mesh":{"apiVersion":"0.1","kind":"backend","implements":[{"interface":"mesh.audio","version":"1.0.0"}]}}"#,
        );
        let mut profile = ShellProfile::new();
        profile.add_frontend(&frontend).unwrap();

        let error = profile
            .active_module_ids([&frontend, &interface, &backend])
            .unwrap_err()
            .to_string();
        assert!(error.contains("mesh.audio"), "{error}");
        assert!(error.contains("no installed provider"), "{error}");
    }

    #[test]
    fn activation_includes_required_composition_extends_ancestors() {
        let base = manifest(
            r#"{"name":"@me/base","version":"1.0.0","mesh":{"apiVersion":"0.1","kind":"composition","compose":{}}}"#,
        );
        let derived = manifest(
            r#"{"name":"@me/derived","version":"1.0.0","mesh":{"apiVersion":"0.1","kind":"composition","extends":"@me/base","compose":{"roots":{"@me/panel#top":{"module":"@me/panel"}}}}}"#,
        );
        let panel = manifest(
            r#"{"name":"@me/panel","version":"1.0.0","mesh":{"apiVersion":"0.1","kind":"frontend","entry":"main.mesh"}}"#,
        );
        let profile = ShellProfile::from_json_str(
            r#"{"schemaVersion":3,"from":{"module":"@me/derived","version":"1.0.0"}}"#,
        )
        .unwrap();

        let active = profile
            .active_module_ids([&base, &derived, &panel])
            .unwrap();
        assert!(active.contains("@me/base"));
        assert!(active.contains("@me/derived"));
        assert!(active.contains("@me/panel"));
    }

    #[test]
    fn composition_version_pins_must_be_semantic_versions() {
        let error = ShellProfile::from_json_str(
            r#"{"schemaVersion":3,"from":{"module":"@me/desk","version":"not-a-version"}}"#,
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("not a valid semantic version"), "{error}");
    }

    #[test]
    fn sparse_root_overrides_inherit_the_module_from_the_instance_key() {
        let profile = ShellProfile::from_json_str(
            r#"{"schemaVersion":3,"roots":{"@me/panel#top":{"active":false}}}"#,
        )
        .unwrap();
        let root = &profile.roots["@me/panel#top"];
        assert_eq!(root.module, "@me/panel");
        assert!(!root.active);
    }

    #[test]
    fn profile_root_with_a_non_main_entrypoint_is_rejected_rather_than_silently_ignored() {
        let error = ShellProfile::from_json_str(
            r#"{"schemaVersion":3,"roots":{"@me/panel#top":{"module":"@me/panel","entrypoint":"sidebar"}}}"#,
        )
        .unwrap_err();
        let message = error.to_string();
        assert!(message.contains("entrypoint 'sidebar'"), "{message}");
        assert!(message.contains("not implemented"), "{message}");

        // The default entrypoint still loads.
        ShellProfile::from_json_str(
            r#"{"schemaVersion":3,"roots":{"@me/panel#top":{"module":"@me/panel","entrypoint":"main"}}}"#,
        )
        .unwrap();
    }

    #[test]
    fn composition_activation_includes_the_composition_and_its_dependency_closure() {
        let composition = manifest(
            r#"{"name":"@me/desk","version":"1.0.0","mesh":{"apiVersion":"0.1","kind":"composition","uses":{"modules":{"@me/helpers":"^1.0.0"}},"compose":{"roots":{"@me/panel#top":{"module":"@me/panel"}}}}}"#,
        );
        let panel = manifest(
            r#"{"name":"@me/panel","version":"1.0.0","mesh":{"apiVersion":"0.1","kind":"frontend","entry":"main.mesh"}}"#,
        );
        let helpers = manifest(
            r#"{"name":"@me/helpers","version":"1.2.0","mesh":{"apiVersion":"0.1","kind":"library"}}"#,
        );
        let profile = ShellProfile::from_json_str(
            r#"{"schemaVersion":3,"from":{"module":"@me/desk","version":"1.0.0"}}"#,
        )
        .unwrap();
        let active = profile
            .active_module_ids([&composition, &panel, &helpers])
            .unwrap();
        assert!(active.contains("@me/desk"));
        assert!(active.contains("@me/panel"));
        assert!(active.contains("@me/helpers"));
    }

    #[test]
    fn inactive_root_node_slots_do_not_activate_their_contributions() {
        let composition = manifest(
            r#"{"name":"@me/desk","version":"1.0.0","mesh":{"apiVersion":"0.1","kind":"composition","compose":{"roots":{"@me/panel#top":{"module":"@me/panel"}},"nodeSlots":{"@me/panel#top":{"start":{"nodes":[{"id":"clock","use":"@me/items:clock"}]}}}}}}"#,
        );
        let panel = manifest(
            r#"{"name":"@me/panel","version":"1.0.0","mesh":{"apiVersion":"0.1","kind":"frontend","entry":"main.mesh"}}"#,
        );
        let items = manifest(
            r#"{"name":"@me/items","version":"1.0.0","mesh":{"apiVersion":"0.1","kind":"component","entry":"main.mesh"}}"#,
        );
        let profile = ShellProfile::from_json_str(
            r#"{"schemaVersion":3,"from":{"module":"@me/desk","version":"1.0.0"},"roots":{"@me/panel#top":{"active":false},"@me/gone#default":{"active":true}}}"#,
        )
        .unwrap();
        let active = profile
            .active_module_ids([&composition, &panel, &items])
            .unwrap();
        assert!(active.contains("@me/desk"));
        assert!(!active.contains("@me/panel"));
        assert!(!active.contains("@me/items"));
        assert!(!active.contains("@me/gone"));
    }

    #[test]
    fn profile_paths_reject_traversal() {
        let paths = ProfilePaths::from_root_graph(Path::new("/tmp/mesh/module.json")).unwrap();
        assert!(paths.profile_path("../outside").is_err());
    }

    #[test]
    fn profile_settings_are_sparse_namespaced_objects() {
        let profile = ShellProfile::from_json_str(
            r#"{
                "schemaVersion": 3,
                "settings": {
                    "shell": { "i18n": { "locale": "sk-SK" } },
                    "@me/panel#work": { "props": { "global": { "dense": true } } }
                }
            }"#,
        )
        .unwrap();
        assert_eq!(profile.settings["shell"]["i18n"]["locale"], "sk-SK");
        assert!(
            ShellProfile::from_json_str(
                r#"{"schemaVersion":3,"settings":{"shell":"not-an-object"}}"#
            )
            .is_err()
        );
    }

    #[test]
    fn schema_two_profiles_receive_a_focused_node_slots_migration_diagnostic() {
        let error = ShellProfile::from_json_str(r#"{"schemaVersion":2,"roots":{}}"#)
            .expect_err("schema 2 must not be accepted as a compatibility input")
            .to_string();
        assert!(error.contains("Schema 3 adds sparse nodeSlots"), "{error}");
        assert!(error.contains("\"schemaVersion\": 3"), "{error}");
    }

    #[test]
    fn node_slot_placements_reject_duplicate_ids_and_invalid_references() {
        let duplicate = ShellProfile::from_json_str(
            r#"{
                "schemaVersion": 3,
                "nodeSlots": {"@me/panel#top":{"start":{"nodes":[
                    {"id":"clock","use":"@me/clock:small"},
                    {"id":"clock","use":"@me/clock:large"}
                ]}}}
            }"#,
        )
        .expect_err("placement ids are slot-local stable identities")
        .to_string();
        assert!(duplicate.contains("duplicate placement id"), "{duplicate}");

        let invalid = ShellProfile::from_json_str(
            r#"{
                "schemaVersion": 3,
                "nodeSlots": {"@me/panel#top":{"start":{"nodes":[
                    {"id":"clock","use":"not-a-contribution"}
                ]}}}
            }"#,
        )
        .expect_err("placements must reference public contributions")
        .to_string();
        assert!(invalid.contains("module-id:contribution-id"), "{invalid}");
    }

    #[test]
    fn language_pack_chains_reject_duplicate_order_and_non_pack_resources() {
        let duplicate = ShellProfile::from_json_str(
            r#"{
                "schemaVersion": 3,
                "resources": { "languages": ["@me/cs", "@me/cs"] }
            }"#,
        )
        .expect_err("duplicate pack entries have ambiguous precedence")
        .to_string();
        assert!(duplicate.contains("resources.languages"), "{duplicate}");
        assert!(duplicate.contains("ambiguous"), "{duplicate}");

        let profile = ShellProfile::from_json_str(
            r#"{
                "schemaVersion": 3,
                "resources": { "languages": ["@me/not-a-pack"] }
            }"#,
        )
        .unwrap();
        let frontend = manifest(
            r#"{"name":"@me/not-a-pack","version":"1.0.0","mesh":{"apiVersion":"0.1","kind":"frontend"}}"#,
        );
        let error = profile
            .active_module_ids([&frontend])
            .expect_err("resources.languages must name language-pack modules")
            .to_string();
        assert!(error.contains("requires a language-pack"), "{error}");
    }

    #[test]
    fn revision_checked_profile_save_rejects_a_stale_locale_candidate() {
        let root = std::env::temp_dir().join(format!(
            "mesh-profile-revision-{}-{}",
            std::process::id(),
            line!()
        ));
        let graph_path = root.join("module.json");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(&graph_path, "{}").unwrap();
        let paths = ProfilePaths::from_root_graph(&graph_path).unwrap();

        let initial = ShellProfile::new();
        let committed = paths
            .save_if_revision("work", &initial, 0)
            .expect("initial profile revision commits");
        assert_eq!(committed.revision, 1);

        let stale = paths.load("work").unwrap();
        let mut current = stale.clone();
        current.settings.insert(
            "shell".into(),
            serde_json::json!({ "i18n": { "locale": "cs" } }),
        );
        let committed = paths
            .save_if_revision("work", &current, 1)
            .expect("current profile revision commits");
        assert_eq!(committed.revision, 2);

        let mut stale = stale;
        stale.settings.insert(
            "shell".into(),
            serde_json::json!({ "i18n": { "locale": "de" } }),
        );
        let error = paths
            .save_if_revision("work", &stale, 1)
            .expect_err("stale profile locale writes must not overwrite a newer choice");
        assert!(error.to_string().contains("profile revision conflict"));
        assert_eq!(paths.load("work").unwrap().revision, 2);
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn concurrent_revision_checked_profile_writers_have_one_winner() {
        let root = std::env::temp_dir().join(format!(
            "mesh-profile-cas-{}-{}",
            std::process::id(),
            line!()
        ));
        fs::create_dir_all(&root).unwrap();
        let graph_path = root.join("module.json");
        fs::write(&graph_path, "{}").unwrap();
        let paths = ProfilePaths::from_root_graph(&graph_path).unwrap();
        paths
            .save_if_revision("work", &ShellProfile::new(), 0)
            .unwrap();

        let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
        let workers = ["cs", "de"].into_iter().map(|locale| {
            let paths = paths.clone();
            let barrier = barrier.clone();
            let mut candidate = paths.load("work").unwrap();
            candidate.settings.insert(
                "shell".into(),
                serde_json::json!({ "i18n": { "locale": locale } }),
            );
            std::thread::spawn(move || {
                barrier.wait();
                paths.save_if_revision("work", &candidate, 1)
            })
        });
        let workers = workers.collect::<Vec<_>>();
        let results = workers
            .into_iter()
            .map(|worker| worker.join().expect("profile CAS writer must not panic"))
            .collect::<Vec<_>>();
        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
        assert_eq!(
            results
                .iter()
                .filter(|result| {
                    result
                        .as_ref()
                        .is_err_and(|error| error.to_string().contains("profile revision conflict"))
                })
                .count(),
            1
        );
        let committed = paths.load("work").unwrap();
        assert_eq!(committed.revision, 2);
        assert!(matches!(
            committed.settings["shell"]["i18n"]["locale"].as_str(),
            Some("cs" | "de")
        ));
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn profile_revision_cannot_wrap() {
        let root = std::env::temp_dir().join(format!(
            "mesh-profile-revision-exhausted-{}-{}",
            std::process::id(),
            line!()
        ));
        fs::create_dir_all(&root).unwrap();
        let graph_path = root.join("module.json");
        fs::write(&graph_path, "{}").unwrap();
        let paths = ProfilePaths::from_root_graph(&graph_path).unwrap();
        let profile =
            ShellProfile::from_json_str(&format!("{{\"revision\":{}}}", u64::MAX)).unwrap();
        paths.save("work", &profile).unwrap();
        let error = paths
            .save_if_revision("work", &profile, u64::MAX)
            .unwrap_err();
        assert!(matches!(
            error,
            ModuleManifestError::RevisionExhausted { .. }
        ));
        assert_eq!(paths.load("work").unwrap().revision, u64::MAX);
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn restore_active_reverts_to_a_previous_profile() {
        let root = std::env::temp_dir().join(format!(
            "mesh-profile-restore-{}-{}",
            std::process::id(),
            line!()
        ));
        let graph_path = root.join("module.json");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(&graph_path, "{}").unwrap();
        let paths = ProfilePaths::from_root_graph(&graph_path).unwrap();

        paths
            .save_if_revision("old", &ShellProfile::new(), 0)
            .unwrap();
        paths
            .save_if_revision("new", &ShellProfile::new(), 0)
            .unwrap();
        paths.set_active("old").unwrap();
        assert_eq!(paths.active_profile_id().unwrap().as_deref(), Some("old"));

        paths.set_active("new").unwrap();
        assert_eq!(paths.active_profile_id().unwrap().as_deref(), Some("new"));

        paths.restore_active(Some("old")).unwrap();
        assert_eq!(paths.active_profile_id().unwrap().as_deref(), Some("old"));

        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn restore_active_clears_the_pointer_when_there_was_no_prior_profile() {
        let root = std::env::temp_dir().join(format!(
            "mesh-profile-restore-none-{}-{}",
            std::process::id(),
            line!()
        ));
        let graph_path = root.join("module.json");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(&graph_path, "{}").unwrap();
        let paths = ProfilePaths::from_root_graph(&graph_path).unwrap();

        // Restoring `None` before any pointer exists must not error.
        paths.restore_active(None).unwrap();
        assert_eq!(paths.active_profile_id().unwrap(), None);

        paths
            .save_if_revision("new", &ShellProfile::new(), 0)
            .unwrap();
        paths.set_active("new").unwrap();
        assert_eq!(paths.active_profile_id().unwrap().as_deref(), Some("new"));

        paths.restore_active(None).unwrap();
        assert_eq!(paths.active_profile_id().unwrap(), None);

        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn module_reference_cleanup_covers_all_profile_owned_state() {
        let mut profile = ShellProfile::new();
        profile.from = Some(crate::package::CompositionRef {
            module: "@mesh/desktop".into(),
            version: None,
        });
        profile.roots.insert(
            "@mesh/panel#default".into(),
            ProfileRootInstance {
                module: "@mesh/panel".into(),
                ..Default::default()
            },
        );
        profile.background_services.insert("@mesh/audio".into());
        profile
            .providers
            .insert("mesh.audio".into(), "@mesh/audio".into());
        profile.resources.theme = Some("@mesh/theme".into());
        profile.resources.icons.push("@mesh/icons".into());

        assert!(profile.references_module("@mesh/audio"));
        profile.remove_module_references("@mesh/audio");
        assert!(!profile.references_module("@mesh/audio"));
        assert!(profile.references_module("@mesh/desktop"));
    }
}
