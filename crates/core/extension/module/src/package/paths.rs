use super::ModuleManifestError;
use std::collections::HashSet;
use std::fs;
use std::path::{Component, Path, PathBuf};

/// The canonical identity of an installable module.
///
/// Manifests and persisted graph records still use strings for serde
/// compatibility, but every string crosses this type before it is used as an
/// identity or as a path component.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ModuleId(String);

impl ModuleId {
    pub fn parse(value: &str) -> Result<Self, ModuleManifestError> {
        if value.trim() != value || !value.starts_with('@') {
            return Err(invalid_module_id(value));
        }
        let Some((scope, name)) = value[1..].split_once('/') else {
            return Err(invalid_module_id(value));
        };
        if scope.is_empty()
            || name.is_empty()
            || name.contains('/')
            || !valid_module_id_part(scope)
            || !valid_module_id_part(name)
        {
            return Err(invalid_module_id(value));
        }
        Ok(Self(value.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn relative_path(&self) -> &str {
        &self.0[1..]
    }
}

pub fn module_install_path(
    modules_dir: &Path,
    module_id: &str,
) -> Result<PathBuf, ModuleManifestError> {
    let id = ModuleId::parse(module_id)?;
    contained_path(modules_dir, id.relative_path(), "installed module path")
}

fn valid_module_id_part(value: &str) -> bool {
    value
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.'))
        && value != "."
        && value != ".."
}

fn invalid_module_id(value: &str) -> ModuleManifestError {
    ModuleManifestError::Validation(format!(
        "module id '{value}' must use the contained @scope/name form with ASCII letters, digits, '.', '-' or '_'"
    ))
}

/// A path that is relative to one module or package root and cannot escape it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleRelativePath(String);

impl ModuleRelativePath {
    pub fn parse(label: &str, value: &str) -> Result<Self, ModuleManifestError> {
        let path = Path::new(value);
        if value.trim() != value
            || value.is_empty()
            || value.contains('\\')
            || path.is_absolute()
            || path
                .components()
                .any(|component| matches!(component, Component::ParentDir | Component::Prefix(_)))
            || path
                .components()
                .any(|component| matches!(component, Component::CurDir))
        {
            return Err(ModuleManifestError::Validation(format!(
                "{label} '{value}' must be a contained relative path"
            )));
        }
        Ok(Self(value.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Resolve a metadata path under `root` without following a symlink.
///
/// Missing final components are permitted for a write destination, while all
/// existing components are checked. This keeps validation useful both before
/// creating a module and before reading or deleting one.
pub fn contained_path(
    root: &Path,
    relative: &str,
    label: &str,
) -> Result<PathBuf, ModuleManifestError> {
    let relative = ModuleRelativePath::parse(label, relative)?;
    let root_metadata = fs::symlink_metadata(root).map_err(|source| ModuleManifestError::Io {
        path: root.to_path_buf(),
        source,
    })?;
    if root_metadata.file_type().is_symlink() || !root_metadata.is_dir() {
        return Err(ModuleManifestError::Validation(format!(
            "{label} root {} must be a real directory",
            root.display()
        )));
    }

    let target = root.join(relative.as_str());
    reject_symlink_components(root, &target, label)?;
    let canonical_root = fs::canonicalize(root).map_err(|source| ModuleManifestError::Io {
        path: root.to_path_buf(),
        source,
    })?;
    let existing = existing_ancestor(&target);
    let canonical_existing =
        fs::canonicalize(&existing).map_err(|source| ModuleManifestError::Io {
            path: existing.clone(),
            source,
        })?;
    if !canonical_existing.starts_with(&canonical_root) {
        return Err(ModuleManifestError::Validation(format!(
            "{label} {} escapes {}",
            target.display(),
            root.display()
        )));
    }
    Ok(target)
}

pub(crate) fn validate_regular_file(path: &Path, label: &str) -> Result<(), ModuleManifestError> {
    validate_no_symlink_path(path, label)?;
    let metadata = fs::symlink_metadata(path).map_err(|source| ModuleManifestError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(ModuleManifestError::Validation(format!(
            "{label} {} must be a regular, non-symlink file",
            path.display()
        )));
    }
    Ok(())
}

pub(crate) fn validate_no_symlink_path(
    path: &Path,
    label: &str,
) -> Result<(), ModuleManifestError> {
    let mut current = if path.is_absolute() {
        PathBuf::from("/")
    } else {
        std::env::current_dir().map_err(|source| ModuleManifestError::Io {
            path: PathBuf::from("."),
            source,
        })?
    };
    for component in path.components() {
        match component {
            Component::Normal(name) => current.push(name),
            Component::ParentDir => {
                current.pop();
                continue;
            }
            Component::CurDir | Component::RootDir | Component::Prefix(_) => continue,
        }
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(ModuleManifestError::Validation(format!(
                    "{label} {} contains unsupported symlink {}",
                    path.display(),
                    current.display()
                )));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
            Err(source) => {
                return Err(ModuleManifestError::Io {
                    path: current,
                    source,
                });
            }
        }
    }
    Ok(())
}

fn existing_ancestor(path: &Path) -> PathBuf {
    let mut current = path.to_path_buf();
    while !current.exists() {
        if !current.pop() {
            break;
        }
    }
    current
}

fn reject_symlink_components(
    root: &Path,
    target: &Path,
    label: &str,
) -> Result<(), ModuleManifestError> {
    let relative = target.strip_prefix(root).map_err(|_| {
        ModuleManifestError::Validation(format!(
            "{label} {} is outside {}",
            target.display(),
            root.display()
        ))
    })?;
    let mut current = root.to_path_buf();
    for component in relative.components() {
        let Component::Normal(name) = component else {
            return Err(ModuleManifestError::Validation(format!(
                "{label} {} is not a normal relative path",
                target.display()
            )));
        };
        current.push(name);
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(ModuleManifestError::Validation(format!(
                    "{label} {} contains unsupported symlink {}",
                    target.display(),
                    current.display()
                )));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
            Err(source) => {
                return Err(ModuleManifestError::Io {
                    path: current,
                    source,
                });
            }
        }
    }
    Ok(())
}

/// Validate an entire module tree before it is installed or hashed.
pub fn validate_module_tree(root: &Path) -> Result<(), ModuleManifestError> {
    let metadata = fs::symlink_metadata(root).map_err(|source| ModuleManifestError::Io {
        path: root.to_path_buf(),
        source,
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(ModuleManifestError::Validation(format!(
            "module tree {} must be a real directory",
            root.display()
        )));
    }
    let canonical_root = fs::canonicalize(root).map_err(|source| ModuleManifestError::Io {
        path: root.to_path_buf(),
        source,
    })?;
    let mut visited = HashSet::new();
    validate_module_tree_at(root, &canonical_root, &mut visited)
}

fn validate_module_tree_at(
    directory: &Path,
    canonical_root: &Path,
    visited: &mut HashSet<PathBuf>,
) -> Result<(), ModuleManifestError> {
    let canonical = fs::canonicalize(directory).map_err(|source| ModuleManifestError::Io {
        path: directory.to_path_buf(),
        source,
    })?;
    if !canonical.starts_with(canonical_root) || !visited.insert(canonical) {
        return Ok(());
    }
    for entry in fs::read_dir(directory).map_err(|source| ModuleManifestError::Io {
        path: directory.to_path_buf(),
        source,
    })? {
        let entry = entry.map_err(|source| ModuleManifestError::Io {
            path: directory.to_path_buf(),
            source,
        })?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path).map_err(|source| ModuleManifestError::Io {
            path: path.clone(),
            source,
        })?;
        if metadata.file_type().is_symlink() {
            return Err(ModuleManifestError::Validation(format!(
                "module tree contains unsupported symlink {}",
                path.display()
            )));
        }
        if metadata.is_dir() {
            validate_module_tree_at(&path, canonical_root, visited)?;
        } else if !metadata.is_file() {
            return Err(ModuleManifestError::Validation(format!(
                "module tree contains unsupported non-file {}",
                path.display()
            )));
        }
    }
    Ok(())
}

pub fn mesh_home() -> Result<PathBuf, ModuleManifestError> {
    if let Ok(path) = std::env::var("MESH_HOME") {
        let trimmed = path.trim();
        if trimmed.is_empty() {
            return Err(ModuleManifestError::InvalidMeshHome(
                "MESH_HOME cannot be empty".into(),
            ));
        }
        let path = PathBuf::from(trimmed);
        if !path.is_absolute() {
            return Err(ModuleManifestError::InvalidMeshHome(format!(
                "MESH_HOME must be absolute: {}",
                path.display()
            )));
        }
        return Ok(path);
    }

    let home = std::env::var("HOME")
        .map(PathBuf::from)
        .map_err(|_| ModuleManifestError::InvalidMeshHome("HOME is not set".into()))?;
    Ok(home.join(".mesh"))
}

pub fn root_module_graph_manifest_path() -> Result<PathBuf, ModuleManifestError> {
    Ok(mesh_home()?.join("module.json"))
}

pub fn settings_path() -> Result<PathBuf, ModuleManifestError> {
    Ok(mesh_home()?.join("settings.json"))
}

pub fn modules_dir() -> Result<PathBuf, ModuleManifestError> {
    Ok(mesh_home()?.join("modules"))
}

/// Durable content-addressed module objects and activation generations.
pub fn module_store_dir(config_dir: &Path) -> PathBuf {
    config_dir.join(".mesh-store")
}

pub fn themes_dir() -> Result<PathBuf, ModuleManifestError> {
    Ok(mesh_home()?.join("themes"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("mesh-package-paths-{name}-{nonce}"));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn module_ids_are_typed_before_they_become_paths() {
        assert!(ModuleId::parse("@mesh/panel").is_ok());
        for invalid in [
            "",
            "mesh/panel",
            "@mesh",
            "@mesh/../panel",
            "@mesh/panel/extra",
            "@mesh/panel ",
        ] {
            assert!(ModuleId::parse(invalid).is_err(), "accepted {invalid:?}");
        }
    }

    #[test]
    fn contained_paths_reject_escape_and_symlink_components() {
        let root = temp_dir("contained");
        fs::create_dir_all(root.join("scope")).unwrap();
        assert!(contained_path(&root, "scope/module", "module").is_ok());
        assert!(contained_path(&root, "../outside", "module").is_err());

        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&root, root.join("link")).unwrap();
            assert!(contained_path(&root, "link/module", "module").is_err());
        }
    }

    #[test]
    fn module_trees_reject_symlinks_before_reads_or_hashes() {
        let root = temp_dir("tree");
        fs::write(root.join("module.json"), "{}").unwrap();
        #[cfg(unix)]
        {
            let outside = temp_dir("outside");
            fs::write(outside.join("secret.luau"), "return 1").unwrap();
            std::os::unix::fs::symlink(outside.join("secret.luau"), root.join("secret.luau"))
                .unwrap();
            assert!(validate_module_tree(&root).is_err());
            assert!(super::super::module_tree_digest(&root).is_err());
        }
    }
}
