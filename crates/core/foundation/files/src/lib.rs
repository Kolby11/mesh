//! Descriptor-relative, bounded reads for module-owned files.

use std::path::{Component, Path, PathBuf};
#[cfg(unix)]
use std::{
    ffi::CString,
    io::Read,
    os::unix::{
        ffi::OsStrExt,
        io::{AsRawFd, FromRawFd},
    },
};

#[derive(Debug, thiserror::Error)]
pub enum ContainedFileError {
    #[error("unsafe source path {path}: {reason}")]
    UnsafePath { path: PathBuf, reason: String },
    #[error("failed to read {path}: {source}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("source is not a regular file: {path}")]
    NotRegularFile { path: PathBuf },
    #[error("source exceeds {max_bytes} bytes: {path}")]
    TooLarge { path: PathBuf, max_bytes: usize },
    #[error("source is not valid UTF-8: {path}")]
    InvalidUtf8 { path: PathBuf },
    #[error("source read cancelled: {path}")]
    Cancelled { path: PathBuf },
    #[error("contained file reads are unavailable on this platform: {path}")]
    UnsupportedPlatform { path: PathBuf },
}

/// A lexical module root and a relative path containing only normal components.
/// Reads reject symlinked roots, intermediate directories, and final files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainedFile {
    module_root: PathBuf,
    relative_path: PathBuf,
}

impl ContainedFile {
    pub fn new(
        module_root: impl Into<PathBuf>,
        relative_path: impl AsRef<Path>,
    ) -> Result<Self, ContainedFileError> {
        let module_root = module_root.into();
        let relative_path = relative_path.as_ref();
        if module_root.as_os_str().is_empty()
            || relative_path.as_os_str().is_empty()
            || relative_path
                .components()
                .any(|part| !matches!(part, Component::Normal(_)))
        {
            return Err(ContainedFileError::UnsafePath {
                path: module_root.join(relative_path),
                reason: "expected a module root and a non-empty normal relative path".into(),
            });
        }
        Ok(Self {
            module_root,
            relative_path: relative_path.into(),
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

    pub fn read_utf8_bounded(&self, max_bytes: usize) -> Result<String, ContainedFileError> {
        String::from_utf8(self.read_bounded(max_bytes, || false)?).map_err(|_| {
            ContainedFileError::InvalidUtf8 {
                path: self.candidate_path(),
            }
        })
    }

    /// Check cancellation before opening, between path components and read chunks,
    /// and before returning bytes. At most `max_bytes + 1` bytes are read.
    pub fn read_bounded(
        &self,
        max_bytes: usize,
        cancelled: impl Fn() -> bool,
    ) -> Result<Vec<u8>, ContainedFileError> {
        let path = self.candidate_path();
        let check = || {
            if cancelled() {
                Err(ContainedFileError::Cancelled { path: path.clone() })
            } else {
                Ok(())
            }
        };
        check()?;
        #[cfg(unix)]
        {
            let mut file = open_at(libc::AT_FDCWD, self.module_root.as_os_str(), true, &path)?;
            let mut components = self.relative_path.components().peekable();
            while let Some(Component::Normal(component)) = components.next() {
                check()?;
                file = open_at(
                    file.as_raw_fd(),
                    component,
                    components.peek().is_some(),
                    &path,
                )?;
            }
            if !file
                .metadata()
                .map_err(|source| ContainedFileError::Read {
                    path: path.clone(),
                    source,
                })?
                .is_file()
            {
                return Err(ContainedFileError::NotRegularFile { path });
            }
            const CHUNK: usize = 64 * 1024;
            let limit = max_bytes.saturating_add(1);
            let mut bytes = Vec::with_capacity(limit.min(CHUNK));
            let mut chunk = [0; CHUNK];
            while bytes.len() < limit {
                check()?;
                let amount = (limit - bytes.len()).min(CHUNK);
                let count =
                    file.read(&mut chunk[..amount])
                        .map_err(|source| ContainedFileError::Read {
                            path: path.clone(),
                            source,
                        })?;
                if count == 0 {
                    break;
                }
                bytes.extend_from_slice(&chunk[..count]);
            }
            check()?;
            if bytes.len() > max_bytes {
                return Err(ContainedFileError::TooLarge { path, max_bytes });
            }
            Ok(bytes)
        }
        #[cfg(not(unix))]
        {
            let _ = max_bytes;
            Err(ContainedFileError::UnsupportedPlatform { path })
        }
    }
}

/// A CSS `url(...)` asset path resolved against a declaring stylesheet's
/// directory, following GTK's model: styles and themes ship images next to
/// the stylesheet that names them.
///
/// `asset_base` is the stylesheet's containing directory (typically the
/// module root joined with the stylesheet's parent directory). Returns the
/// absolutized path when `url_path` is a relative path of normal components
/// (`url(assets/panel.png)`, `url("bg.webp")`). Returns `None` — leaving the
/// value untouched — for absolute paths, URL schemes (`http://`, `data:`), or
/// paths that escape `asset_base` with `..`, so the painter's existing
/// missing-asset diagnostics stay authoritative for them.
pub fn resolve_asset_url(asset_base: &Path, url_path: &str) -> Option<PathBuf> {
    let trimmed = url_path.trim();
    let inner = trimmed
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .or_else(|| {
            trimmed
                .strip_prefix('\'')
                .and_then(|rest| rest.strip_suffix('\''))
        })
        .map(str::trim)
        .unwrap_or(trimmed);
    if inner.is_empty()
        || inner.contains(':')
        || Path::new(inner)
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return None;
    }
    let joined = asset_base.join(inner);
    joined.is_absolute().then_some(joined)
}

#[cfg(unix)]
fn open_at(
    parent: i32,
    name: &std::ffi::OsStr,
    directory: bool,
    path: &Path,
) -> Result<std::fs::File, ContainedFileError> {
    let name = CString::new(name.as_bytes()).map_err(|_| ContainedFileError::UnsafePath {
        path: path.into(),
        reason: "path contains NUL".into(),
    })?;
    let flags = libc::O_RDONLY
        | libc::O_CLOEXEC
        | libc::O_NOFOLLOW
        | libc::O_NONBLOCK
        | if directory { libc::O_DIRECTORY } else { 0 };
    let fd = unsafe { libc::openat(parent, name.as_ptr(), flags, 0) };
    if fd < 0 {
        return Err(ContainedFileError::Read {
            path: path.into(),
            source: std::io::Error::last_os_error(),
        });
    }
    Ok(unsafe { std::fs::File::from_raw_fd(fd) })
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    #[test]
    fn contained_reads_bound_bytes_reject_symlinks_and_never_block_on_a_fifo() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("text"), "hello").unwrap();
        let file = ContainedFile::new(root.path(), "text").unwrap();
        assert_eq!(file.read_utf8_bounded(5).unwrap(), "hello");
        assert!(matches!(
            file.read_utf8_bounded(4),
            Err(ContainedFileError::TooLarge { .. })
        ));
        assert!(matches!(
            file.read_bounded(5, || true),
            Err(ContainedFileError::Cancelled { .. })
        ));
        assert!(ContainedFile::new(root.path(), "../escape").is_err());
        std::os::unix::fs::symlink(root.path().join("text"), root.path().join("link")).unwrap();
        assert!(
            ContainedFile::new(root.path(), "link")
                .unwrap()
                .read_utf8_bounded(5)
                .is_err()
        );
        let fifo = CString::new(root.path().join("fifo").as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
        assert!(matches!(
            ContainedFile::new(root.path(), "fifo")
                .unwrap()
                .read_utf8_bounded(5),
            Err(ContainedFileError::NotRegularFile { .. })
        ));
    }

    #[test]
    fn asset_urls_resolve_against_the_declaring_stylesheet_directory() {
        let base = Path::new("/modules/theme/pack");
        assert_eq!(
            resolve_asset_url(base, "assets/panel.png"),
            Some(PathBuf::from("/modules/theme/pack/assets/panel.png"))
        );
        assert_eq!(
            resolve_asset_url(base, " \"bg.webp\" "),
            Some(PathBuf::from("/modules/theme/pack/bg.webp"))
        );
        assert_eq!(
            resolve_asset_url(base, "'a b.png'"),
            Some(PathBuf::from("/modules/theme/pack/a b.png"))
        );
        assert_eq!(resolve_asset_url(base, "/etc/passwd"), None);
        assert_eq!(resolve_asset_url(base, "../escape.png"), None);
        assert_eq!(resolve_asset_url(base, "https://example.com/x.png"), None);
        assert_eq!(resolve_asset_url(base, "data:image/png;base64,AAA"), None);
        assert_eq!(resolve_asset_url(base, ""), None);
        assert_eq!(resolve_asset_url(base, ".."), None);
    }
}
