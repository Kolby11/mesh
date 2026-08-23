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

/// Resolve an existing regular file declared relative to a module root.
///
/// The returned path is canonical, so callers can use it as a stable identity
/// for dependency tracking and watching. Every existing path component is
/// checked before the final canonicalization; symlinked files and directories
/// are rejected even when their targets remain inside the module root.
pub fn resolve_contained_module_file(
    module_root: &Path,
    relative: &str,
    label: &str,
) -> Result<PathBuf, ModuleManifestError> {
    let canonical_root = canonical_module_root(module_root, label)?;
    let target = contained_path(&canonical_root, relative, label)?;
    canonical_regular_file(&canonical_root, &target, label)
}

/// Resolve an existing local component import relative to its owning source
/// file while keeping it inside the owning module root.
///
/// `@src/foo.mesh` is rooted at the module's `src` directory. Other local
/// imports are relative to the importing file, so `../../shared/foo.mesh` is
/// valid when it normalizes to a path inside the module. Absolute paths,
/// backslash-separated paths, and paths that normalize outside the module are
/// rejected before any source is read.
pub fn resolve_contained_component_file(
    module_root: &Path,
    owner_file: &Path,
    source: &str,
    label: &str,
) -> Result<PathBuf, ModuleManifestError> {
    let target = resolve_contained_component_path(module_root, owner_file, source, label)?;
    let canonical_root = canonical_module_root(module_root, label)?;
    canonical_regular_file(&canonical_root, &target, label)
}

/// Resolve a local component path with containment and symlink checks but
/// without requiring the final file to exist. Compiler validation uses this
/// form when it only needs a stable path for an already-parsed component.
pub fn resolve_contained_component_path(
    module_root: &Path,
    owner_file: &Path,
    source: &str,
    label: &str,
) -> Result<PathBuf, ModuleManifestError> {
    if source.is_empty()
        || source.contains('\\')
        || source.contains('\0')
        || Path::new(source).is_absolute()
    {
        return Err(ModuleManifestError::Validation(format!(
            "{label} '{source}' must be relative to its owning module"
        )));
    }

    let canonical_root = canonical_module_root(module_root, label)?;
    let owner = absolute_path(owner_file)?;
    validate_no_symlink_path(&owner, label)?;
    let owner_parent = owner.parent().ok_or_else(|| {
        ModuleManifestError::Validation(format!(
            "{label} owner {} has no parent directory",
            owner.display()
        ))
    })?;
    let canonical_owner_parent =
        fs::canonicalize(owner_parent).map_err(|source| ModuleManifestError::Io {
            path: owner_parent.to_path_buf(),
            source,
        })?;
    if !canonical_owner_parent.starts_with(&canonical_root) {
        return Err(ModuleManifestError::Validation(format!(
            "{label} owner {} is outside module root {}",
            owner.display(),
            canonical_root.display()
        )));
    }

    let raw_target = if let Some(rest) = source.strip_prefix("@src/") {
        canonical_root.join("src").join(rest)
    } else {
        canonical_owner_parent.join(source)
    };
    let mut target = normalize_absolute_path(&raw_target, label)?;
    if target.extension().is_none() {
        target.set_extension("mesh");
    }

    let relative = target.strip_prefix(&canonical_root).map_err(|_| {
        ModuleManifestError::Validation(format!(
            "{label} {} escapes module root {}",
            target.display(),
            canonical_root.display()
        ))
    })?;
    let relative = relative.to_str().ok_or_else(|| {
        ModuleManifestError::Validation(format!("{label} {} is not valid UTF-8", target.display()))
    })?;
    contained_path(&canonical_root, relative, label)
}

fn canonical_module_root(root: &Path, label: &str) -> Result<PathBuf, ModuleManifestError> {
    let metadata = fs::symlink_metadata(root).map_err(|source| ModuleManifestError::Io {
        path: root.to_path_buf(),
        source,
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(ModuleManifestError::Validation(format!(
            "{label} module root {} must be a real directory",
            root.display()
        )));
    }
    fs::canonicalize(root).map_err(|source| ModuleManifestError::Io {
        path: root.to_path_buf(),
        source,
    })
}

fn canonical_regular_file(
    canonical_root: &Path,
    target: &Path,
    label: &str,
) -> Result<PathBuf, ModuleManifestError> {
    validate_no_symlink_path(target, label)?;
    let metadata = fs::symlink_metadata(target).map_err(|source| ModuleManifestError::Io {
        path: target.to_path_buf(),
        source,
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(ModuleManifestError::Validation(format!(
            "{label} {} must be a regular, non-symlink file",
            target.display()
        )));
    }
    let canonical = fs::canonicalize(target).map_err(|source| ModuleManifestError::Io {
        path: target.to_path_buf(),
        source,
    })?;
    validate_no_symlink_path(target, label)?;
    if !canonical.starts_with(canonical_root) {
        return Err(ModuleManifestError::Validation(format!(
            "{label} {} escapes module root {}",
            target.display(),
            canonical_root.display()
        )));
    }
    Ok(canonical)
}

fn absolute_path(path: &Path) -> Result<PathBuf, ModuleManifestError> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(std::env::current_dir()
            .map_err(|source| ModuleManifestError::Io {
                path: PathBuf::from("."),
                source,
            })?
            .join(path))
    }
}

fn normalize_absolute_path(path: &Path, label: &str) -> Result<PathBuf, ModuleManifestError> {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => normalized.push(component.as_os_str()),
            Component::CurDir => {}
            Component::Normal(name) => normalized.push(name),
            Component::ParentDir => {
                if !normalized.pop() {
                    return Err(ModuleManifestError::Validation(format!(
                        "{label} {} escapes its root",
                        path.display()
                    )));
                }
            }
        }
    }
    Ok(normalized)
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
    fn component_imports_are_owner_relative_and_return_canonical_files() {
        let root = temp_dir("component-import");
        let owner = root.join("src/main.mesh");
        let target = root.join("shared/button.mesh");
        fs::create_dir_all(owner.parent().unwrap()).unwrap();
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(&owner, "").unwrap();
        fs::write(&target, "").unwrap();

        let resolved = resolve_contained_component_file(
            &root,
            &owner,
            "../shared/./button",
            "component import",
        )
        .unwrap();
        assert_eq!(resolved, target.canonicalize().unwrap());
        assert_eq!(
            resolve_contained_component_file(&root, &owner, "@src/../shared/button.mesh", "import")
                .unwrap(),
            resolved
        );
    }

    #[test]
    fn component_imports_reject_absolute_escape_and_symlink_paths() {
        let root = temp_dir("component-import-safety");
        let owner = root.join("src/main.mesh");
        fs::create_dir_all(owner.parent().unwrap()).unwrap();
        fs::write(&owner, "").unwrap();
        fs::write(root.join("inside.mesh"), "").unwrap();

        assert!(
            resolve_contained_component_file(&root, &owner, "/tmp/outside.mesh", "import").is_err()
        );
        assert!(
            resolve_contained_component_file(&root, &owner, "@src/../../outside", "import")
                .is_err()
        );
        assert!(
            resolve_contained_component_file(&root, &owner, "../../outside", "import").is_err()
        );

        #[cfg(unix)]
        {
            let outside = temp_dir("component-import-outside");
            let outside_file = outside.join("secret.mesh");
            fs::write(&outside_file, "").unwrap();
            std::os::unix::fs::symlink(&outside_file, root.join("linked.mesh")).unwrap();
            assert!(
                resolve_contained_component_file(&root, &owner, "../linked.mesh", "import")
                    .is_err()
            );
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
