use mesh_core_module::RootModuleGraphManifest;
use serde_json::{Map, Value};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static TEMP_FILE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

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
}
