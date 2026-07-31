use super::{
    ModuleKind, ModuleManifest, ModuleManifestError, RootLayoutSelection, RootModuleGraphManifest,
};
use crate::manifest::SurfaceLayoutSection;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

pub const PROFILE_SCHEMA_VERSION: u32 = 1;
pub const DEFAULT_PROFILE_ID: &str = "default";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ShellProfile {
    #[serde(default = "default_profile_schema_version")]
    pub schema_version: u32,
    #[serde(default)]
    pub roots: BTreeMap<String, ProfileRootInstance>,
    #[serde(default)]
    pub background_services: BTreeSet<String>,
    #[serde(default)]
    pub providers: BTreeMap<String, String>,
    #[serde(default)]
    pub resources: ProfileResources,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProfileRootInstance {
    pub module: String,
    #[serde(default = "default_entrypoint")]
    pub entrypoint: String,
    #[serde(default = "default_true")]
    pub active: bool,
    /// Sparse per-instance placement override. Omission inherits `mesh.surface`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub surface: Option<SurfaceLayoutSection>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
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

fn default_entrypoint() -> String {
    "main".into()
}

fn default_true() -> bool {
    true
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
            ..Self::default()
        }
    }

    pub fn from_json_str(input: &str) -> Result<Self, ModuleManifestError> {
        let profile: Self =
            serde_json::from_str(input).map_err(|source| ModuleManifestError::Json {
                path: PathBuf::from("<profile>"),
                source,
            })?;
        profile.validate()?;
        Ok(profile)
    }

    pub fn from_path(path: &Path) -> Result<Self, ModuleManifestError> {
        let content = fs::read_to_string(path).map_err(|source| ModuleManifestError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        let profile: Self =
            serde_json::from_str(&content).map_err(|source| ModuleManifestError::Json {
                path: path.to_path_buf(),
                source,
            })?;
        profile.validate()?;
        Ok(profile)
    }

    pub fn validate(&self) -> Result<(), ModuleManifestError> {
        if self.schema_version != PROFILE_SCHEMA_VERSION {
            return Err(ModuleManifestError::Validation(format!(
                "unsupported profile schemaVersion {}; supported version is {PROFILE_SCHEMA_VERSION}",
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
        )
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

    /// Resolve the modules needed by this profile. Roots are explicit; declared
    /// module/resource dependencies and sole interface providers are inferred.
    pub fn active_module_ids<'a>(
        &self,
        manifests: impl IntoIterator<Item = &'a ModuleManifest>,
    ) -> Result<HashSet<String>, ModuleManifestError> {
        let manifests = manifests
            .into_iter()
            .map(|manifest| (manifest.name.as_str(), manifest))
            .collect::<HashMap<_, _>>();
        let mut active = HashSet::new();
        let mut queue = VecDeque::new();

        for instance in self.roots.values().filter(|instance| instance.active) {
            queue.push_back(instance.module.clone());
        }
        queue.extend(self.background_services.iter().cloned());
        queue.extend(self.providers.values().cloned());
        if let Some(theme) = &self.resources.theme {
            queue.push_back(theme.clone());
        }
        queue.extend(self.resources.icons.iter().cloned());
        queue.extend(self.resources.fonts.iter().cloned());
        queue.extend(self.resources.languages.iter().cloned());

        while let Some(module_id) = queue.pop_front() {
            if !active.insert(module_id.clone()) {
                continue;
            }
            let manifest = manifests.get(module_id.as_str()).ok_or_else(|| {
                ModuleManifestError::Validation(format!(
                    "active profile references module {module_id}, but it is not installed"
                ))
            })?;

            queue.extend(manifest.mesh.uses.modules.keys().cloned());
            queue.extend(manifest.mesh.uses.resources.icons.iter().cloned());
            queue.extend(manifest.mesh.uses.resources.fonts.iter().cloned());
            queue.extend(manifest.mesh.uses.resources.i18n.iter().cloned());
            queue.extend(manifest.mesh.uses.resources.themes.iter().cloned());

            for interface in manifest.mesh.uses.interfaces.keys() {
                for candidate in manifests.values().filter(|candidate| {
                    candidate.mesh.kind == ModuleKind::Interface
                        && candidate
                            .mesh
                            .interface
                            .as_ref()
                            .is_some_and(|declaration| declaration.name == *interface)
                }) {
                    queue.push_back(candidate.name.clone());
                }

                if let Some(provider) = self.providers.get(interface) {
                    queue.push_back(provider.clone());
                    continue;
                }
                let providers = manifests
                    .values()
                    .filter(|candidate| {
                        candidate.mesh.kind == ModuleKind::Backend
                            && candidate
                                .mesh
                                .implementations()
                                .any(|implementation| implementation.interface == *interface)
                    })
                    .collect::<Vec<_>>();
                if providers.len() == 1 {
                    queue.push_back(providers[0].name.clone());
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
            roots: BTreeMap::new(),
            background_services: BTreeSet::new(),
            providers: BTreeMap::new(),
            resources: ProfileResources::default(),
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
        let mut content =
            serde_json::to_string_pretty(profile).map_err(|source| ModuleManifestError::Json {
                path: path.clone(),
                source,
            })?;
        content.push('\n');
        atomic_write(&path, content.as_bytes())
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
        if !id.starts_with('@') || id.trim() != id {
            return Err(ModuleManifestError::Validation(format!(
                "profile {field} entry '{id}' must be a module id such as @scope/name"
            )));
        }
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

fn atomic_write(path: &Path, content: &[u8]) -> Result<(), ModuleManifestError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| ModuleManifestError::Io {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    let temporary = path.with_extension(format!("tmp-{}", std::process::id()));
    let result = (|| {
        let mut file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&temporary)?;
        file.write_all(content)?;
        file.sync_all()?;
        fs::rename(&temporary, path)
    })();
    if let Err(source) = result {
        let _ = fs::remove_file(&temporary);
        return Err(ModuleManifestError::Io {
            path: path.to_path_buf(),
            source,
        });
    }
    Ok(())
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
    fn profile_paths_reject_traversal() {
        let paths = ProfilePaths::from_root_graph(Path::new("/tmp/mesh/module.json")).unwrap();
        assert!(paths.profile_path("../outside").is_err());
    }
}
