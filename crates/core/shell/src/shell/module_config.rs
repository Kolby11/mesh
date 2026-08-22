use mesh_core_module::{
    RootModuleGraphManifest,
    package::{ModuleKind, ProfilePaths, ProfileRootInstance},
};
use serde_json::{Map, Value};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static TEMP_FILE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub(super) struct ModuleConfigRollback {
    path: PathBuf,
    content: Vec<u8>,
}

impl ModuleConfigRollback {
    pub(super) fn restore(self) -> Result<(), ModuleConfigWriteError> {
        atomic_write(&self.path, &self.content)
    }
}

#[derive(Debug, thiserror::Error)]
pub(super) enum ModuleConfigWriteError {
    #[error("failed to read root module graph {path}: {source}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("failed to parse root module graph {path}: {source}")]
    Parse {
        path: PathBuf,
        source: serde_json::Error,
    },
    #[error("root module graph {path} has no object-valued mesh section")]
    MissingMeshObject { path: PathBuf },
    #[error("root module graph {path} has a non-object mesh.providers value")]
    InvalidProvidersObject { path: PathBuf },
    #[error("root module graph {path} has a non-object mesh.modules value")]
    InvalidModulesObject { path: PathBuf },
    #[error("root module graph {path} has a non-array mesh.disabled value")]
    InvalidDisabledArray { path: PathBuf },
    #[error("provider selection is invalid: {0}")]
    InvalidSelection(String),
    #[error("updated root module graph is invalid: {0}")]
    InvalidGraph(mesh_core_module::package::ModuleManifestError),
    #[error("failed to serialize updated root module graph: {0}")]
    Serialize(serde_json::Error),
    #[error("failed to write root module graph {path}: {source}")]
    Write {
        path: PathBuf,
        source: std::io::Error,
    },
}

pub(super) fn write_active_provider_selection(
    path: &Path,
    interface: &str,
    provider_id: &str,
) -> Result<(), ModuleConfigWriteError> {
    if interface.trim().is_empty() || provider_id.trim().is_empty() {
        return Err(ModuleConfigWriteError::InvalidSelection(
            "interface and provider id must be non-empty".into(),
        ));
    }

    let content = fs::read_to_string(path).map_err(|source| ModuleConfigWriteError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    let mut document: Value =
        serde_json::from_str(&content).map_err(|source| ModuleConfigWriteError::Parse {
            path: path.to_path_buf(),
            source,
        })?;
    let mesh = document
        .get_mut("mesh")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| ModuleConfigWriteError::MissingMeshObject {
            path: path.to_path_buf(),
        })?;
    let providers = mesh
        .entry("providers")
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .ok_or_else(|| ModuleConfigWriteError::InvalidProvidersObject {
            path: path.to_path_buf(),
        })?;
    providers.insert(
        interface.to_string(),
        Value::String(provider_id.to_string()),
    );

    let mut updated =
        serde_json::to_string_pretty(&document).map_err(ModuleConfigWriteError::Serialize)?;
    updated.push('\n');
    RootModuleGraphManifest::from_json_str(&updated)
        .map_err(ModuleConfigWriteError::InvalidGraph)?;
    atomic_write(path, updated.as_bytes())
}

/// Persist a provider choice in the active composition boundary. Profiles own
/// provider policy when selected; the legacy root graph remains the fallback.
pub(super) fn write_composed_provider_selection(
    path: &Path,
    interface: &str,
    provider_id: &str,
) -> Result<(), ModuleConfigWriteError> {
    let paths = ProfilePaths::from_root_graph(path)
        .map_err(|error| ModuleConfigWriteError::InvalidSelection(error.to_string()))?;
    let Some(profile_id) = paths
        .active_profile_id()
        .map_err(|error| ModuleConfigWriteError::InvalidSelection(error.to_string()))?
    else {
        return write_active_provider_selection(path, interface, provider_id);
    };
    let mut profile = paths
        .load(&profile_id)
        .map_err(|error| ModuleConfigWriteError::InvalidSelection(error.to_string()))?;
    let expected_revision = profile.revision;
    profile
        .providers
        .insert(interface.to_string(), provider_id.to_string());
    paths
        .save_if_revision(&profile_id, &profile, expected_revision)
        .map(|_| ())
        .map_err(|error| ModuleConfigWriteError::InvalidSelection(error.to_string()))
}

pub(super) fn write_module_enabled(
    path: &Path,
    module_id: &str,
    enabled: bool,
) -> Result<ModuleConfigRollback, ModuleConfigWriteError> {
    if module_id.trim().is_empty() {
        return Err(ModuleConfigWriteError::InvalidSelection(
            "module id must be non-empty".into(),
        ));
    }

    let content = fs::read_to_string(path).map_err(|source| ModuleConfigWriteError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    let mut document: Value =
        serde_json::from_str(&content).map_err(|source| ModuleConfigWriteError::Parse {
            path: path.to_path_buf(),
            source,
        })?;
    let mesh = document
        .get_mut("mesh")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| ModuleConfigWriteError::MissingMeshObject {
            path: path.to_path_buf(),
        })?;

    let uses_explicit_inventory = mesh
        .get("modules")
        .and_then(Value::as_object)
        .is_some_and(|modules| !modules.is_empty());
    if uses_explicit_inventory {
        let modules = mesh
            .get_mut("modules")
            .and_then(Value::as_object_mut)
            .ok_or_else(|| ModuleConfigWriteError::InvalidModulesObject {
                path: path.to_path_buf(),
            })?;
        let entry = modules
            .get_mut(module_id)
            .and_then(Value::as_object_mut)
            .ok_or_else(|| {
                ModuleConfigWriteError::InvalidSelection(format!(
                    "module {module_id} is not present in the explicit root inventory"
                ))
            })?;
        entry.insert("enabled".into(), Value::Bool(enabled));
    } else {
        let disabled = mesh
            .entry("disabled")
            .or_insert_with(|| Value::Array(Vec::new()))
            .as_array_mut()
            .ok_or_else(|| ModuleConfigWriteError::InvalidDisabledArray {
                path: path.to_path_buf(),
            })?;
        disabled.retain(|value| value.as_str() != Some(module_id));
        if !enabled {
            disabled.push(Value::String(module_id.to_string()));
        }
        disabled.sort_by(|left, right| left.as_str().cmp(&right.as_str()));
    }

    let mut updated =
        serde_json::to_string_pretty(&document).map_err(ModuleConfigWriteError::Serialize)?;
    updated.push('\n');
    RootModuleGraphManifest::from_json_str(&updated)
        .map_err(ModuleConfigWriteError::InvalidGraph)?;
    atomic_write(path, updated.as_bytes())?;
    Ok(ModuleConfigRollback {
        path: path.to_path_buf(),
        content: content.into_bytes(),
    })
}

/// Persist activation in the active profile when profiles are in use, falling
/// back to the development root graph during migration.
pub(super) fn write_composed_module_enabled(
    path: &Path,
    module_id: &str,
    kind: ModuleKind,
    enabled: bool,
) -> Result<ModuleConfigRollback, ModuleConfigWriteError> {
    let paths = ProfilePaths::from_root_graph(path)
        .map_err(|error| ModuleConfigWriteError::InvalidSelection(error.to_string()))?;
    let Some(profile_id) = paths
        .active_profile_id()
        .map_err(|error| ModuleConfigWriteError::InvalidSelection(error.to_string()))?
    else {
        return write_module_enabled(path, module_id, enabled);
    };

    let profile_path = paths
        .profile_path(&profile_id)
        .map_err(|error| ModuleConfigWriteError::InvalidSelection(error.to_string()))?;
    let content = fs::read(&profile_path).map_err(|source| ModuleConfigWriteError::Read {
        path: profile_path.clone(),
        source,
    })?;
    let mut profile = paths
        .load(&profile_id)
        .map_err(|error| ModuleConfigWriteError::InvalidSelection(error.to_string()))?;
    let expected_revision = profile.revision;

    match kind {
        ModuleKind::Frontend => {
            let mut found = false;
            for instance in profile
                .roots
                .values_mut()
                .filter(|instance| instance.module == module_id)
            {
                instance.active = enabled;
                found = true;
            }
            if enabled && !found {
                profile.roots.insert(
                    format!("{module_id}#default"),
                    ProfileRootInstance {
                        module: module_id.to_string(),
                        entrypoint: "main".into(),
                        active: true,
                        surface: None,
                    },
                );
            }
        }
        ModuleKind::Backend => {
            if enabled {
                profile.background_services.insert(module_id.to_string());
            } else {
                profile.background_services.remove(module_id);
                profile
                    .providers
                    .retain(|_, provider| provider != module_id);
            }
        }
        _ => {
            return Err(ModuleConfigWriteError::InvalidSelection(format!(
                "{} modules are available through installation/dependency resolution and do not have an independent enabled state",
                format!("{kind:?}").to_lowercase()
            )));
        }
    }

    paths
        .save_if_revision(&profile_id, &profile, expected_revision)
        .map_err(|error| ModuleConfigWriteError::InvalidSelection(error.to_string()))?;
    Ok(ModuleConfigRollback {
        path: profile_path,
        content,
    })
}

fn atomic_write(path: &Path, content: &[u8]) -> Result<(), ModuleConfigWriteError> {
    let sequence = TEMP_FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("module.json");
    let temporary = path.with_file_name(format!(
        ".{file_name}.tmp-{}-{sequence}",
        std::process::id()
    ));
    let result = (|| {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)?;
        file.write_all(content)?;
        file.sync_all()?;
        fs::rename(&temporary, path)
    })();
    if let Err(source) = result {
        let _ = fs::remove_file(&temporary);
        return Err(ModuleConfigWriteError::Write {
            path: path.to_path_buf(),
            source,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_selection_preserves_other_root_graph_decisions() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("module.json");
        fs::write(
            &path,
            r#"{
  "name": "@mesh/local-config",
  "version": "0.1.0",
  "private": true,
  "mesh": {
    "schemaVersion": 1,
    "modulesDir": "../modules",
    "disabled": ["@mesh/debug-inspector"],
    "providers": {"mesh.audio": "@mesh/pipewire-audio"},
    "layout": {"entrypoint": "@mesh/navigation-bar:main"}
  }
}"#,
        )
        .unwrap();

        write_active_provider_selection(&path, "mesh.audio", "@mesh/pulseaudio-audio").unwrap();

        let updated: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(updated["name"], "@mesh/local-config");
        assert_eq!(updated["private"], true);
        assert_eq!(updated["mesh"]["disabled"][0], "@mesh/debug-inspector");
        assert_eq!(
            updated["mesh"]["providers"]["mesh.audio"],
            "@mesh/pulseaudio-audio"
        );
        assert_eq!(
            updated["mesh"]["layout"]["entrypoint"],
            "@mesh/navigation-bar:main"
        );
        RootModuleGraphManifest::from_path(&path).unwrap();
    }

    #[test]
    fn invalid_provider_write_leaves_original_file_untouched() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("module.json");
        let original = r#"{"name":"x","version":"1","mesh":{"schemaVersion":99}}"#;
        fs::write(&path, original).unwrap();

        assert!(write_active_provider_selection(&path, "mesh.audio", "@mesh/audio").is_err());
        assert_eq!(fs::read_to_string(path).unwrap(), original);
    }

    #[test]
    fn module_enabled_updates_auto_discovery_disabled_decisions() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("module.json");
        fs::write(
            &path,
            r#"{
  "name": "@mesh/local-config",
  "version": "0.1.0",
  "mesh": {
    "schemaVersion": 1,
    "modulesDir": "../modules",
    "disabled": ["@mesh/debug-inspector"]
  }
}"#,
        )
        .unwrap();

        write_module_enabled(&path, "@mesh/audio-popover", false).unwrap();
        let disabled = serde_json::from_str::<Value>(&fs::read_to_string(&path).unwrap()).unwrap()
            ["mesh"]["disabled"]
            .as_array()
            .unwrap()
            .clone();
        assert_eq!(
            disabled,
            vec![
                Value::String("@mesh/audio-popover".into()),
                Value::String("@mesh/debug-inspector".into()),
            ]
        );

        write_module_enabled(&path, "@mesh/audio-popover", true).unwrap();
        let updated: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(
            updated["mesh"]["disabled"],
            serde_json::json!(["@mesh/debug-inspector"])
        );
    }

    #[test]
    fn module_enabled_write_can_be_rolled_back_atomically() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("module.json");
        let original = r#"{
  "name": "@mesh/local-config",
  "version": "0.1.0",
  "mesh": {"schemaVersion": 1, "modulesDir": "../modules", "disabled": []}
}"#;
        fs::write(&path, original).unwrap();

        let rollback = write_module_enabled(&path, "@mesh/example", false).unwrap();
        assert!(fs::read_to_string(&path).unwrap().contains("@mesh/example"));
        rollback.restore().unwrap();

        assert_eq!(fs::read_to_string(path).unwrap(), original);
    }

    #[test]
    fn module_enabled_updates_explicit_inventory_entry() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("module.json");
        fs::write(
            &path,
            r#"{
  "name": "@mesh/local-config",
  "version": "0.1.0",
  "mesh": {
    "schemaVersion": 1,
    "modules": {
      "@mesh/panel": {"kind": "frontend", "path": "panel", "enabled": true}
    }
  }
}"#,
        )
        .unwrap();

        write_module_enabled(&path, "@mesh/panel", false).unwrap();
        let updated: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(updated["mesh"]["modules"]["@mesh/panel"]["enabled"], false);
    }

    #[test]
    fn module_enabled_updates_active_profile_without_touching_root_graph() {
        let directory = tempfile::tempdir().unwrap();
        let root_path = directory.path().join("module.json");
        let root = r#"{
  "name": "@mesh/local-config",
  "version": "0.1.0",
  "mesh": {"schemaVersion": 1, "modulesDir": "../modules", "disabled": []}
}"#;
        fs::write(&root_path, root).unwrap();
        let paths = ProfilePaths::from_root_graph(&root_path).unwrap();
        let mut profile = mesh_core_module::package::ShellProfile::new();
        profile.roots.insert(
            "@mesh/panel#default".into(),
            ProfileRootInstance {
                module: "@mesh/panel".into(),
                entrypoint: "main".into(),
                active: true,
                surface: None,
            },
        );
        paths.save("desktop", &profile).unwrap();
        paths.set_active("desktop").unwrap();

        let rollback =
            write_composed_module_enabled(&root_path, "@mesh/panel", ModuleKind::Frontend, false)
                .unwrap();
        assert!(!paths.load("desktop").unwrap().roots["@mesh/panel#default"].active);
        assert_eq!(fs::read_to_string(&root_path).unwrap(), root);

        rollback.restore().unwrap();
        assert!(paths.load("desktop").unwrap().roots["@mesh/panel#default"].active);
    }

    #[test]
    fn provider_selection_is_scoped_to_the_active_profile() {
        let directory = tempfile::tempdir().unwrap();
        let root_path = directory.path().join("module.json");
        let root = r#"{
  "name": "@mesh/local-config",
  "version": "0.1.0",
  "mesh": {"schemaVersion": 1, "modulesDir": "../modules", "providers": {}}
}"#;
        fs::write(&root_path, root).unwrap();
        let paths = ProfilePaths::from_root_graph(&root_path).unwrap();
        paths
            .save("work", &mesh_core_module::package::ShellProfile::new())
            .unwrap();
        paths.set_active("work").unwrap();

        write_composed_provider_selection(&root_path, "mesh.audio", "@mesh/work-audio").unwrap();

        assert_eq!(
            paths.load("work").unwrap().providers["mesh.audio"],
            "@mesh/work-audio"
        );
        assert_eq!(fs::read_to_string(root_path).unwrap(), root);
    }
}
