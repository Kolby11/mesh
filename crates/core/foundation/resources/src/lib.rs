//! Host resource discovery shared by icon, font, and settings systems.
//!
//! This crate describes what the desktop has installed. MESH resource-pack
//! modules remain semantic mapping and composition units layered above it.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::sync::OnceLock;
#[cfg(unix)]
use std::{
    ffi::CString,
    os::unix::ffi::OsStrExt,
    os::unix::io::{AsRawFd, FromRawFd},
};

pub const DEFAULT_MAX_RESOURCE_BYTES: usize = 64 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum ResourceAssetError {
    #[error("resource path is unsafe {path}: {reason}")]
    UnsafePath { path: PathBuf, reason: String },
    #[error("resource is not a regular file: {path}")]
    NotRegularFile { path: PathBuf },
    #[error("resource exceeds {max_bytes} bytes: {path}")]
    TooLarge { path: PathBuf, max_bytes: usize },
    #[error("resource read failed for {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("safe resource opening is unavailable on this platform: {path}")]
    UnsupportedPlatform { path: PathBuf },
}

/// A module-rooted resource file whose path cannot escape through traversal or
/// symlink components. The candidate path is for provenance only; reads use a
/// descriptor-relative no-follow walk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceAssetHandle {
    module_root: PathBuf,
    relative_path: PathBuf,
}

impl ResourceAssetHandle {
    pub fn new(
        module_root: impl Into<PathBuf>,
        relative_path: impl AsRef<Path>,
    ) -> Result<Self, ResourceAssetError> {
        let module_root = module_root.into();
        let relative_path = relative_path.as_ref();
        if module_root.as_os_str().is_empty() {
            return Err(ResourceAssetError::UnsafePath {
                path: module_root,
                reason: "module root is empty".into(),
            });
        }
        if relative_path.as_os_str().is_empty() || relative_path.is_absolute() {
            return Err(ResourceAssetError::UnsafePath {
                path: relative_path.to_path_buf(),
                reason: "resource path must be non-empty and relative".into(),
            });
        }
        if relative_path.components().any(|component| {
            matches!(
                component,
                Component::CurDir
                    | Component::ParentDir
                    | Component::RootDir
                    | Component::Prefix(_)
            )
        }) {
            return Err(ResourceAssetError::UnsafePath {
                path: relative_path.to_path_buf(),
                reason: "resource path contains an unsafe component".into(),
            });
        }
        Ok(Self {
            module_root,
            relative_path: relative_path.to_path_buf(),
        })
    }

    pub fn module_root(&self) -> &Path {
        &self.module_root
    }

    pub fn relative_path(&self) -> &Path {
        &self.relative_path
    }

    pub fn candidate_path(&self) -> PathBuf {
        self.module_root.join(&self.relative_path)
    }

    pub fn read_bounded(&self, max_bytes: usize) -> Result<Vec<u8>, ResourceAssetError> {
        #[cfg(unix)]
        {
            let mut directory = open_resource_directory(&self.module_root)?;
            let mut components = self.relative_path.components().peekable();
            while let Some(component) = components.next() {
                let Component::Normal(component) = component else {
                    return Err(ResourceAssetError::UnsafePath {
                        path: self.candidate_path(),
                        reason: "non-normal path component".into(),
                    });
                };
                if components.peek().is_some() {
                    directory =
                        open_resource_directory_at(&directory, component, &self.candidate_path())?;
                } else {
                    let file =
                        open_resource_file_at(&directory, component, &self.candidate_path())?;
                    return read_resource_bounded(file, &self.candidate_path(), max_bytes);
                }
            }
            Err(ResourceAssetError::UnsafePath {
                path: self.candidate_path(),
                reason: "empty relative path".into(),
            })
        }
        #[cfg(not(unix))]
        {
            let _ = max_bytes;
            Err(ResourceAssetError::UnsupportedPlatform {
                path: self.candidate_path(),
            })
        }
    }
}

#[cfg(unix)]
fn open_resource_directory(path: &Path) -> Result<std::fs::File, ResourceAssetError> {
    let name =
        CString::new(path.as_os_str().as_bytes()).map_err(|_| ResourceAssetError::UnsafePath {
            path: path.to_path_buf(),
            reason: "path contains NUL".into(),
        })?;
    let fd = unsafe {
        libc::open(
            name.as_ptr(),
            libc::O_RDONLY | libc::O_CLOEXEC | libc::O_DIRECTORY | libc::O_NOFOLLOW,
        )
    };
    if fd < 0 {
        return Err(ResourceAssetError::Read {
            path: path.to_path_buf(),
            source: std::io::Error::last_os_error(),
        });
    }
    Ok(unsafe { std::fs::File::from_raw_fd(fd) })
}

#[cfg(unix)]
fn open_resource_directory_at(
    parent: &std::fs::File,
    component: &std::ffi::OsStr,
    path: &Path,
) -> Result<std::fs::File, ResourceAssetError> {
    let name = CString::new(component.as_bytes()).map_err(|_| ResourceAssetError::UnsafePath {
        path: path.to_path_buf(),
        reason: "path contains NUL".into(),
    })?;
    let fd = unsafe {
        libc::openat(
            parent.as_raw_fd(),
            name.as_ptr(),
            libc::O_RDONLY | libc::O_CLOEXEC | libc::O_DIRECTORY | libc::O_NOFOLLOW,
            0,
        )
    };
    if fd < 0 {
        return Err(ResourceAssetError::Read {
            path: path.to_path_buf(),
            source: std::io::Error::last_os_error(),
        });
    }
    Ok(unsafe { std::fs::File::from_raw_fd(fd) })
}

#[cfg(unix)]
fn open_resource_file_at(
    parent: &std::fs::File,
    component: &std::ffi::OsStr,
    path: &Path,
) -> Result<std::fs::File, ResourceAssetError> {
    let name = CString::new(component.as_bytes()).map_err(|_| ResourceAssetError::UnsafePath {
        path: path.to_path_buf(),
        reason: "path contains NUL".into(),
    })?;
    let fd = unsafe {
        libc::openat(
            parent.as_raw_fd(),
            name.as_ptr(),
            libc::O_RDONLY | libc::O_CLOEXEC | libc::O_NOFOLLOW,
            0,
        )
    };
    if fd < 0 {
        return Err(ResourceAssetError::Read {
            path: path.to_path_buf(),
            source: std::io::Error::last_os_error(),
        });
    }
    Ok(unsafe { std::fs::File::from_raw_fd(fd) })
}

#[cfg(unix)]
fn read_resource_bounded(
    file: std::fs::File,
    path: &Path,
    max_bytes: usize,
) -> Result<Vec<u8>, ResourceAssetError> {
    if !file
        .metadata()
        .map_err(|source| ResourceAssetError::Read {
            path: path.to_path_buf(),
            source,
        })?
        .file_type()
        .is_file()
    {
        return Err(ResourceAssetError::NotRegularFile {
            path: path.to_path_buf(),
        });
    }
    let mut bytes = Vec::new();
    file.take(max_bytes.saturating_add(1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|source| ResourceAssetError::Read {
            path: path.to_path_buf(),
            source,
        })?;
    if bytes.len() > max_bytes {
        return Err(ResourceAssetError::TooLarge {
            path: path.to_path_buf(),
            max_bytes,
        });
    }
    Ok(bytes)
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SystemResourceCatalog {
    pub icon_themes: Vec<SystemIconTheme>,
    pub font_families: Vec<SystemFontFamily>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemIconTheme {
    pub id: String,
    pub name: String,
    pub path: PathBuf,
    pub inherits: Vec<String>,
    pub hidden: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemFontFamily {
    pub name: String,
    pub face_count: usize,
    pub monospace: bool,
}

static SYSTEM_RESOURCES: OnceLock<SystemResourceCatalog> = OnceLock::new();

/// Cached process-wide host catalog. Font scanning is intentionally performed
/// once: fontdb follows the platform font directories and can inspect hundreds
/// of files on a typical desktop.
pub fn system_resource_catalog() -> &'static SystemResourceCatalog {
    SYSTEM_RESOURCES.get_or_init(discover_system_resources)
}

pub fn discover_system_resources() -> SystemResourceCatalog {
    SystemResourceCatalog {
        icon_themes: discover_icon_themes_in(&xdg_icon_base_dirs()),
        font_families: discover_font_families(),
    }
}

fn discover_font_families() -> Vec<SystemFontFamily> {
    let mut database = fontdb::Database::new();
    database.load_system_fonts();

    let mut families: BTreeMap<String, (usize, bool)> = BTreeMap::new();
    for face in database.faces() {
        for (family, _) in &face.families {
            let entry = families.entry(family.clone()).or_default();
            entry.0 += 1;
            entry.1 |= face.monospaced;
        }
    }

    families
        .into_iter()
        .map(|(name, (face_count, monospace))| SystemFontFamily {
            name,
            face_count,
            monospace,
        })
        .collect()
}

/// FreeDesktop icon base directories in lookup precedence order.
/// Both catalog discovery and icon resolution use this one authority.
pub fn xdg_icon_base_dirs() -> Vec<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let mut dirs = Vec::new();
    if let Some(data_home) =
        non_empty_env_value(std::env::var_os("XDG_DATA_HOME")).map(PathBuf::from)
    {
        dirs.push(data_home.join("icons"));
    } else if let Some(home) = &home {
        dirs.push(home.join(".local/share/icons"));
    }
    if let Some(home) = home {
        dirs.push(home.join(".icons"));
    }
    let data_dirs = non_empty_env_value(std::env::var_os("XDG_DATA_DIRS"))
        .map(|value| std::env::split_paths(&value).collect::<Vec<_>>())
        .filter(|dirs| !dirs.is_empty())
        .unwrap_or_else(|| {
            vec![
                PathBuf::from("/usr/local/share"),
                PathBuf::from("/usr/share"),
            ]
        });
    dirs.extend(data_dirs.into_iter().map(|dir| dir.join("icons")));

    let mut seen = HashSet::new();
    dirs.into_iter()
        .filter(|path| path.is_dir() && seen.insert(path.clone()))
        .collect()
}

fn non_empty_env_value(value: Option<std::ffi::OsString>) -> Option<std::ffi::OsString> {
    value.filter(|value| !value.is_empty())
}

fn discover_icon_themes_in(base_dirs: &[PathBuf]) -> Vec<SystemIconTheme> {
    let mut themes = BTreeMap::new();
    for base in base_dirs {
        let Ok(entries) = std::fs::read_dir(base) else {
            continue;
        };
        for entry in entries.flatten() {
            let Some(theme) = read_icon_theme(&entry.path()) else {
                continue;
            };
            // XDG directory precedence is user first, then system. Preserve
            // the first definition when the same theme id occurs in both.
            themes.entry(theme.id.clone()).or_insert(theme);
        }
    }
    themes.into_values().collect()
}

fn read_icon_theme(path: &Path) -> Option<SystemIconTheme> {
    let id = path.file_name()?.to_str()?.to_owned();
    let raw = std::fs::read_to_string(path.join("index.theme")).ok()?;
    let mut in_icon_theme = false;
    let mut name = None;
    let mut inherits = BTreeSet::new();
    let mut hidden = false;

    for raw_line in raw.lines() {
        let line = raw_line.trim();
        if line.starts_with('[') && line.ends_with(']') {
            in_icon_theme = line == "[Icon Theme]";
            continue;
        }
        if !in_icon_theme || line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        match key.trim() {
            "Name" => name = Some(value.trim().to_owned()),
            "Inherits" => inherits.extend(
                value
                    .split(',')
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_owned),
            ),
            "Hidden" => hidden = value.trim().eq_ignore_ascii_case("true"),
            _ => {}
        }
    }

    Some(SystemIconTheme {
        name: name.unwrap_or_else(|| id.clone()),
        id,
        path: path.to_owned(),
        inherits: inherits.into_iter().collect(),
        hidden,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_xdg_directory_values_are_absent() {
        assert_eq!(non_empty_env_value(Some(std::ffi::OsString::new())), None);
        assert_eq!(
            non_empty_env_value(Some(std::ffi::OsString::from("/tmp/mesh-data"))),
            Some(std::ffi::OsString::from("/tmp/mesh-data"))
        );
    }

    #[test]
    fn icon_catalog_honors_precedence_and_metadata() {
        let temp = tempfile::tempdir().unwrap();
        let user = temp.path().join("user");
        let system = temp.path().join("system");
        let user_theme = user.join("ocean");
        let system_theme = system.join("ocean");
        std::fs::create_dir_all(&user_theme).unwrap();
        std::fs::create_dir_all(&system_theme).unwrap();
        std::fs::write(
            user_theme.join("index.theme"),
            "[Icon Theme]\nName=Ocean User\nInherits=hicolor,Adwaita\n",
        )
        .unwrap();
        std::fs::write(
            system_theme.join("index.theme"),
            "[Icon Theme]\nName=Ocean System\n",
        )
        .unwrap();

        let themes = discover_icon_themes_in(&[user, system]);
        assert_eq!(themes.len(), 1);
        assert_eq!(themes[0].name, "Ocean User");
        assert_eq!(themes[0].inherits, ["Adwaita", "hicolor"]);
    }

    #[test]
    fn resource_handles_reject_traversal_and_bound_reads() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("icon.svg"), b"svg").unwrap();
        let handle = ResourceAssetHandle::new(temp.path(), "icon.svg").unwrap();
        assert_eq!(handle.read_bounded(16).unwrap(), b"svg");
        assert!(ResourceAssetHandle::new(temp.path(), "../icon.svg").is_err());
        assert!(matches!(
            handle.read_bounded(2),
            Err(ResourceAssetError::TooLarge { .. })
        ));
    }

    #[cfg(unix)]
    #[test]
    fn resource_handles_do_not_follow_symlinked_components() {
        let temp = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("secret.ttf"), b"font").unwrap();
        std::os::unix::fs::symlink(
            outside.path().join("secret.ttf"),
            temp.path().join("font.ttf"),
        )
        .unwrap();

        let handle = ResourceAssetHandle::new(temp.path(), "font.ttf").unwrap();
        assert!(handle.read_bounded(64).is_err());
    }
}
