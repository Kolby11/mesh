use super::{
    ModuleManifestError, ProfilePaths, ShellProfile, load_authoring_snapshot, resolve_composition,
};
use mesh_core_config::SettingsStore;
use std::path::{Path, PathBuf};

/// Layer a profile's sparse preferences and per-instance surface overrides on
/// top of shared user defaults. The shared document remains untouched.
pub fn effective_profile_settings(
    shared: SettingsStore,
    profile: Option<&super::ShellProfile>,
) -> Result<SettingsStore, mesh_core_config::ConfigError> {
    let Some(profile) = profile else {
        return Ok(shared);
    };
    let path = shared.path().to_path_buf();
    let mut document = shared.to_value();
    let root = document
        .as_object_mut()
        .expect("SettingsStore always serializes an object");
    if let Some(theme) = &profile.resources.theme {
        let target = root
            .entry("shell".to_string())
            .or_insert_with(|| serde_json::json!({}));
        mesh_core_config::merge_json(target, &serde_json::json!({ "theme": { "active": theme } }));
    }
    for (namespace, overrides) in &profile.settings {
        let target = root
            .entry(namespace.clone())
            .or_insert_with(|| serde_json::json!({}));
        mesh_core_config::merge_json(target, overrides);
    }
    for (instance_id, instance) in profile.roots.iter().filter(|(_, root)| root.is_active()) {
        let Some(surface) = &instance.surface else {
            continue;
        };
        let target = root
            .entry(instance_id.clone())
            .or_insert_with(|| serde_json::json!({}));
        mesh_core_config::merge_json(target, &serde_json::json!({ "surface": surface }));
    }
    SettingsStore::from_value(path, document)
}

/// A profile-aware settings candidate. Mutations remain sparse in the active
/// profile; shared fallback preferences are never copied into that profile.
pub struct ProfileSettings {
    shared: SettingsStore,
    paths: ProfilePaths,
    profile: Option<(String, ShellProfile)>,
    effective: SettingsStore,
    fallback: SettingsStore,
}

impl ProfileSettings {
    pub fn load(graph_path: &Path, shared: SettingsStore) -> Result<Self, ModuleManifestError> {
        let paths = ProfilePaths::from_root_graph(graph_path)?;
        let profile = paths.load_active()?;
        let graph = if profile
            .as_ref()
            .is_some_and(|(_, profile)| profile.from.is_some())
        {
            Some(load_authoring_snapshot(graph_path)?)
        } else {
            None
        };
        let resolve = |profile: &ShellProfile| -> Result<ShellProfile, ModuleManifestError> {
            match &graph {
                Some(graph) => Ok(resolve_composition(
                    profile,
                    graph.modules().into_iter().map(|node| &node.manifest),
                )?
                .to_profile()),
                None => Ok(profile.clone()),
            }
        };
        let resolved = profile
            .as_ref()
            .map(|(_, profile)| resolve(profile))
            .transpose()?;
        let fallback_profile = profile
            .as_ref()
            .map(|(_, profile)| {
                let mut defaults = profile.clone();
                defaults.settings.clear();
                resolve(&defaults)
            })
            .transpose()?;
        let map_config_error = |error: mesh_core_config::ConfigError| {
            ModuleManifestError::Validation(error.to_string())
        };
        let effective = effective_profile_settings(shared.clone(), resolved.as_ref())
            .map_err(map_config_error)?;
        let fallback = effective_profile_settings(shared.clone(), fallback_profile.as_ref())
            .map_err(map_config_error)?;
        Ok(Self {
            shared,
            paths,
            profile,
            effective,
            fallback,
        })
    }

    pub fn effective(&self) -> &SettingsStore {
        &self.effective
    }

    pub fn path(&self) -> PathBuf {
        self.profile
            .as_ref()
            .map(|(id, _)| self.paths.profile_path(id).expect("loaded profile id"))
            .unwrap_or_else(|| self.shared.path().to_path_buf())
    }

    pub fn merge_namespace(&mut self, namespace: &str, value: &serde_json::Value) {
        if let Some((_, profile)) = &mut self.profile {
            mesh_core_config::merge_json(
                profile
                    .settings
                    .entry(namespace.to_string())
                    .or_insert_with(|| serde_json::json!({})),
                value,
            );
        } else {
            self.shared.merge_namespace(namespace, value);
        }
        self.effective.merge_namespace(namespace, value);
    }

    pub fn reset_namespace(&mut self, namespace: &str) -> bool {
        if let Some((_, profile)) = &mut self.profile {
            let had = profile.settings.remove(namespace).is_some();
            self.effective.reset_namespace(namespace);
            self.effective
                .merge_namespace(namespace, &self.fallback.namespace(namespace));
            had
        } else {
            let had = self.shared.has_namespace(namespace);
            self.shared.reset_namespace(namespace);
            self.effective = self.shared.clone();
            had
        }
    }

    pub fn save(mut self) -> Result<(), ModuleManifestError> {
        let config_error = |error: mesh_core_config::ConfigError| {
            ModuleManifestError::Validation(error.to_string())
        };
        let graph_directory = self
            .paths
            .active_profile_path()
            .parent()
            .expect("profile pointer parent")
            .to_path_buf();
        let shared_directory = self
            .shared
            .path()
            .parent()
            .unwrap_or_else(|| Path::new("."));
        let mut directories = vec![graph_directory, shared_directory.to_path_buf()];
        for directory in &mut directories {
            std::fs::create_dir_all(&directory).map_err(|source| ModuleManifestError::Io {
                path: directory.clone(),
                source,
            })?;
            *directory =
                std::fs::canonicalize(&directory).map_err(|source| ModuleManifestError::Io {
                    path: directory.clone(),
                    source,
                })?;
        }
        let graph_directory = directories[0].clone();
        let shared_directory = directories[1].clone();
        directories.sort();
        directories.dedup();
        let locks = directories
            .iter()
            .map(|directory| super::profile::acquire_control_plane_lock(directory))
            .collect::<Result<Vec<_>, _>>()?;
        let lock = locks
            .iter()
            .find(|lock| lock.directory() == graph_directory)
            .expect("graph lock acquired");
        let shared_guard = locks
            .iter()
            .find(|lock| lock.directory() == shared_directory)
            .expect("settings lock acquired");
        if let Some((id, profile)) = &self.profile {
            if self.paths.active_profile_id()?.as_deref() != Some(id) {
                return Err(ModuleManifestError::Validation(
                    "active profile changed during settings preparation".into(),
                ));
            }
            self.shared
                .check_revision_locked(self.shared.revision(), shared_guard)
                .map_err(config_error)?;
            self.paths
                .save_if_revision_locked(id, profile, profile.revision, lock)?;
        } else {
            if self.paths.active_profile_id()?.is_some() {
                return Err(ModuleManifestError::Validation(
                    "active profile changed during settings preparation".into(),
                ));
            }
            self.shared
                .save_if_revision_locked(self.shared.revision(), shared_guard)
                .map_err(config_error)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn fixture() -> (PathBuf, ProfilePaths, SettingsStore) {
        let root = std::env::temp_dir().join(format!(
            "mesh-preferences-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(root.join("modules/desk")).unwrap();
        fs::write(
            root.join("module.json"),
            r#"{"name":"@me/config","version":"1","mesh":{"modulesDir":"modules","modules":{}}}"#,
        )
        .unwrap();
        fs::write(root.join("modules/desk/module.json"), r#"{"name":"@me/desk","version":"1","mesh":{"apiVersion":"0.1","kind":"composition","compose":{"settings":{"shell":{"theme":{"active":"composed"}},"@me/widget":{"density":"compact"}}}}}"#).unwrap();
        let shared = SettingsStore::from_value(
            root.join("external/settings.json"),
            serde_json::json!({"shell":{"theme":{"active":"shared"}},"@me/widget":{"size":10}}),
        )
        .unwrap();
        shared.save().unwrap();
        let paths = ProfilePaths::from_root_graph(&root.join("module.json")).unwrap();
        (root, paths, shared)
    }

    #[test]
    fn composition_preferences_are_read_and_profile_writes_remain_sparse() {
        let (root, paths, shared) = fixture();
        let profile = ShellProfile::from_json_str(r#"{"schemaVersion":3,"from":{"module":"@me/desk"},"settings":{"shell":{"i18n":{"policy":"manual","locale":"sk"}}}}"#).unwrap();
        paths.save("work", &profile).unwrap();
        paths.set_active("work").unwrap();
        let before = fs::read(shared.path()).unwrap();
        let mut preferences =
            ProfileSettings::load(&root.join("module.json"), shared.clone()).unwrap();
        assert_eq!(preferences.effective().shell().theme.active, "composed");
        assert_eq!(preferences.effective().shell().i18n.locale, "sk");
        assert_eq!(
            preferences.effective().namespace("@me/widget"),
            serde_json::json!({"size":10,"density":"compact"})
        );
        assert_eq!(preferences.path(), paths.profile_path("work").unwrap());
        preferences.merge_namespace("@me/widget", &serde_json::json!({"size":20}));
        preferences.save().unwrap();
        let saved = paths.load("work").unwrap();
        assert_eq!(saved.settings["@me/widget"], serde_json::json!({"size":20}));
        assert_eq!(fs::read(shared.path()).unwrap(), before);
        let mut preferences = ProfileSettings::load(&root.join("module.json"), shared).unwrap();
        assert!(preferences.reset_namespace("@me/widget"));
        assert_eq!(
            preferences.effective().namespace("@me/widget"),
            serde_json::json!({"size":10,"density":"compact"})
        );
        preferences.save().unwrap();
        assert!(
            !paths
                .load("work")
                .unwrap()
                .settings
                .contains_key("@me/widget")
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn stale_shared_profile_and_active_pointer_candidates_are_rejected() {
        let (root, paths, mut shared) = fixture();
        paths.save("work", &ShellProfile::new()).unwrap();
        paths.set_active("work").unwrap();
        let stale = ProfileSettings::load(&root.join("module.json"), shared.clone()).unwrap();
        shared.merge_namespace("shell", &serde_json::json!({"theme":{"active":"changed"}}));
        shared.save_if_revision(0).unwrap();
        assert!(stale.save().is_err());
        let stale = ProfileSettings::load(&root.join("module.json"), shared.clone()).unwrap();
        paths
            .save_if_revision("work", &ShellProfile::new(), 0)
            .unwrap();
        assert!(stale.save().is_err());
        let stale = ProfileSettings::load(&root.join("module.json"), shared).unwrap();
        paths.restore_active(None).unwrap();
        assert!(stale.save().is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn legacy_writes_stay_in_shared_settings_and_reject_a_new_active_profile() {
        let (root, paths, shared) = fixture();
        let stale = ProfileSettings::load(&root.join("module.json"), shared.clone()).unwrap();
        let mut preferences =
            ProfileSettings::load(&root.join("module.json"), shared.clone()).unwrap();
        assert_eq!(preferences.path(), shared.path());
        preferences.merge_namespace("@me/widget", &serde_json::json!({"size":30}));
        preferences.save().unwrap();
        assert_eq!(
            SettingsStore::load_from(shared.path())
                .unwrap()
                .namespace("@me/widget")["size"],
            30
        );
        paths.save("work", &ShellProfile::new()).unwrap();
        paths.set_active("work").unwrap();
        assert!(stale.save().is_err());
        fs::remove_dir_all(root).unwrap();
    }
}
