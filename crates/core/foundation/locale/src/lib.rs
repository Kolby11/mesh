//! System-wide locale management with per-module translation catalogs,
//! fallback chains, and runtime locale switching.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
#[cfg(unix)]
use std::{
    ffi::CString,
    os::unix::ffi::OsStrExt,
    os::unix::io::{AsRawFd, FromRawFd},
};

const MAX_CATALOG_ENTRIES: usize = 4096;
const MAX_VARIANTS_PER_ENTRY: usize = 32;
const MAX_MESSAGE_LENGTH: usize = 16 * 1024;
pub const DEFAULT_MAX_CATALOG_SOURCE_BYTES: usize = 2 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocaleDirection {
    Ltr,
    Rtl,
}

impl LocaleDirection {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ltr => "ltr",
            Self::Rtl => "rtl",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LocaleError {
    #[error("locale tag is empty")]
    EmptyTag,
    #[error("invalid locale tag '{tag}'")]
    InvalidTag { tag: String },
}

/// The normalized locale decision shared by all consumers.
///
/// Fields are private so a caller cannot construct a selection with an
/// incomplete chain, inconsistent direction, or a stale revision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocaleSelection {
    active: String,
    fallback: String,
    chain: Vec<String>,
    direction: LocaleDirection,
    revision: u64,
}

impl LocaleSelection {
    pub fn try_new(
        active: impl AsRef<str>,
        fallback: impl AsRef<str>,
        revision: u64,
    ) -> Result<Self, LocaleError> {
        let active = canonicalize_locale_tag(active.as_ref())?;
        let fallback = canonicalize_locale_tag(fallback.as_ref())?;
        Ok(Self {
            direction: direction_for(&active),
            chain: complete_fallback_chain(&active, &fallback),
            active,
            fallback,
            revision,
        })
    }

    pub fn active(&self) -> &str {
        &self.active
    }

    pub fn fallback(&self) -> &str {
        &self.fallback
    }

    pub fn chain(&self) -> &[String] {
        &self.chain
    }

    pub fn direction(&self) -> LocaleDirection {
        self.direction
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }
}

/// Normalize a locale advertised by the host environment. POSIX locale
/// modifiers and encodings are not BCP 47 subtags, so they are removed before
/// the same canonical validator used by catalog selection is applied.
pub fn normalize_system_locale(raw: &str) -> Option<String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    let raw = raw.split(':').next().unwrap_or_default();
    let raw = raw.split(['.', '@']).next().unwrap_or_default();
    if raw.is_empty() || raw.eq_ignore_ascii_case("c") || raw.eq_ignore_ascii_case("posix") {
        return None;
    }
    canonicalize_locale_tag(&raw.replace('_', "-")).ok()
}

/// Return the first usable locale from the standard POSIX environment
/// precedence. `LANGUAGE` may contain a colon-separated preference list.
pub fn system_locale() -> Option<String> {
    for key in ["LC_ALL", "LC_MESSAGES", "LANG", "LANGUAGE"] {
        let Ok(value) = std::env::var(key) else {
            continue;
        };
        if key == "LANGUAGE" {
            for candidate in value.split(':') {
                if let Some(locale) = normalize_system_locale(candidate) {
                    return Some(locale);
                }
            }
        } else if let Some(locale) = normalize_system_locale(&value) {
            return Some(locale);
        }
    }
    None
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranslationSet {
    pub locale: String,
    pub messages: HashMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CatalogEntry {
    Text(String),
    Plural(BTreeMap<String, String>),
    Select(BTreeMap<String, String>),
}

impl CatalogEntry {
    pub fn render(&self, locale: &str, args: &HashMap<String, String>) -> Option<String> {
        let template = self.resolve(args, locale)?;
        interpolate(template, args)
    }

    fn default_text(&self) -> Option<&str> {
        match self {
            Self::Text(value) => Some(value),
            Self::Plural(variants) | Self::Select(variants) => {
                variants.get("other").map(String::as_str)
            }
        }
    }

    fn resolve(&self, args: &HashMap<String, String>, locale: &str) -> Option<&str> {
        match self {
            Self::Text(value) => Some(value),
            Self::Plural(variants) => {
                let category = args
                    .get("count")
                    .and_then(|count| count.parse::<f64>().ok())
                    .map(|count| plural_category(locale, count))
                    .unwrap_or("other");
                variants
                    .get(category)
                    .or_else(|| variants.get("other"))
                    .map(String::as_str)
            }
            Self::Select(variants) => args
                .get("select")
                .or_else(|| args.get("variant"))
                .or_else(|| args.get("kind"))
                .and_then(|value| variants.get(value))
                .or_else(|| variants.get("other"))
                .map(String::as_str),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogDiagnostic {
    pub key: String,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct CompiledCatalog {
    pub locale: String,
    pub messages: HashMap<String, CatalogEntry>,
    pub diagnostics: Vec<CatalogDiagnostic>,
}

#[derive(Debug, thiserror::Error)]
pub enum LocaleCatalogLoadError {
    #[error("failed to read locale catalog {path}: {source}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("failed to parse locale catalog {path}: {source}")]
    Parse {
        path: PathBuf,
        source: serde_json::Error,
    },
    #[error("catalog source path is unsafe {path}: {reason}")]
    UnsafePath { path: PathBuf, reason: String },
    #[error("catalog source is not a regular file: {path}")]
    NotRegularFile { path: PathBuf },
    #[error("catalog source exceeds {max_bytes} bytes: {path}")]
    TooLarge { path: PathBuf, max_bytes: usize },
    #[error("catalog source is not valid UTF-8: {path}")]
    InvalidUtf8 { path: PathBuf },
    #[error("safe catalog source opening is unavailable on this platform: {path}")]
    UnsupportedPlatform { path: PathBuf },
    #[error("failed to start the locale catalog worker: {source}")]
    WorkerSpawn { source: std::io::Error },
    #[error("locale catalog worker panicked")]
    WorkerPanic,
    #[error("invalid locale '{locale}' in {context}")]
    InvalidLocale { locale: String, context: String },
}

#[derive(Debug, Clone)]
pub struct CatalogSourceDiagnostics {
    pub module_id: String,
    pub locale: String,
    pub path: PathBuf,
    pub diagnostics: Vec<CatalogDiagnostic>,
}

/// The layer a catalog contributes to a module's lookup chain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CatalogSourceKind {
    ModuleBundled,
    LanguagePack,
}

/// A graph-authorized catalog file rooted at one module directory.
///
/// The relative path is validated before it becomes a handle. Reads traverse
/// the root and every relative component with `O_NOFOLLOW`, so a catalog
/// cannot escape through `..` or a replaced symlink. The handle exposes the
/// candidate path for provenance only; opening always uses the contained
/// descriptor walk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogSourceHandle {
    module_root: PathBuf,
    relative_path: PathBuf,
}

impl CatalogSourceHandle {
    pub fn new(
        module_root: impl Into<PathBuf>,
        relative_path: impl AsRef<Path>,
    ) -> Result<Self, String> {
        let module_root = module_root.into();
        if module_root.as_os_str().is_empty() {
            return Err("catalog source module root cannot be empty".into());
        }
        let relative_path = relative_path.as_ref();
        if relative_path.as_os_str().is_empty() || relative_path.is_absolute() {
            return Err(format!(
                "catalog source path must be a non-empty relative path: {}",
                relative_path.display()
            ));
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
            return Err(format!(
                "catalog source path contains an unsafe component: {}",
                relative_path.display()
            ));
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

    pub fn read_utf8_bounded(&self, max_bytes: usize) -> Result<String, LocaleCatalogLoadError> {
        #[cfg(unix)]
        {
            let mut directory = open_catalog_directory(&self.module_root)?;
            let mut components = self.relative_path.components().peekable();
            while let Some(component) = components.next() {
                let Component::Normal(component) = component else {
                    return Err(LocaleCatalogLoadError::UnsafePath {
                        path: self.candidate_path(),
                        reason: "non-normal path component".into(),
                    });
                };
                if components.peek().is_some() {
                    directory =
                        open_catalog_directory_at(&directory, component, &self.candidate_path())?;
                } else {
                    let file = open_catalog_file_at(&directory, component, &self.candidate_path())?;
                    return read_catalog_utf8_bounded(file, &self.candidate_path(), max_bytes);
                }
            }
            Err(LocaleCatalogLoadError::UnsafePath {
                path: self.candidate_path(),
                reason: "empty relative path".into(),
            })
        }
        #[cfg(not(unix))]
        {
            let _ = max_bytes;
            Err(LocaleCatalogLoadError::UnsupportedPlatform {
                path: self.candidate_path(),
            })
        }
    }
}

#[cfg(unix)]
fn open_catalog_directory(path: &Path) -> Result<std::fs::File, LocaleCatalogLoadError> {
    let name = CString::new(path.as_os_str().as_bytes()).map_err(|_| {
        LocaleCatalogLoadError::UnsafePath {
            path: path.to_path_buf(),
            reason: "path contains NUL".into(),
        }
    })?;
    let fd = unsafe {
        libc::open(
            name.as_ptr(),
            libc::O_RDONLY | libc::O_CLOEXEC | libc::O_DIRECTORY | libc::O_NOFOLLOW,
        )
    };
    if fd < 0 {
        return Err(LocaleCatalogLoadError::Read {
            path: path.to_path_buf(),
            source: std::io::Error::last_os_error(),
        });
    }
    Ok(unsafe { std::fs::File::from_raw_fd(fd) })
}

#[cfg(unix)]
fn open_catalog_directory_at(
    parent: &std::fs::File,
    component: &std::ffi::OsStr,
    path: &Path,
) -> Result<std::fs::File, LocaleCatalogLoadError> {
    let name =
        CString::new(component.as_bytes()).map_err(|_| LocaleCatalogLoadError::UnsafePath {
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
        return Err(LocaleCatalogLoadError::Read {
            path: path.to_path_buf(),
            source: std::io::Error::last_os_error(),
        });
    }
    Ok(unsafe { std::fs::File::from_raw_fd(fd) })
}

#[cfg(unix)]
fn open_catalog_file_at(
    parent: &std::fs::File,
    component: &std::ffi::OsStr,
    path: &Path,
) -> Result<std::fs::File, LocaleCatalogLoadError> {
    let name =
        CString::new(component.as_bytes()).map_err(|_| LocaleCatalogLoadError::UnsafePath {
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
        return Err(LocaleCatalogLoadError::Read {
            path: path.to_path_buf(),
            source: std::io::Error::last_os_error(),
        });
    }
    Ok(unsafe { std::fs::File::from_raw_fd(fd) })
}

#[cfg(unix)]
fn read_catalog_utf8_bounded(
    file: std::fs::File,
    path: &Path,
    max_bytes: usize,
) -> Result<String, LocaleCatalogLoadError> {
    if !file
        .metadata()
        .map_err(|source| LocaleCatalogLoadError::Read {
            path: path.to_path_buf(),
            source,
        })?
        .file_type()
        .is_file()
    {
        return Err(LocaleCatalogLoadError::NotRegularFile {
            path: path.to_path_buf(),
        });
    }
    let mut bytes = Vec::new();
    file.take(max_bytes.saturating_add(1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|source| LocaleCatalogLoadError::Read {
            path: path.to_path_buf(),
            source,
        })?;
    if bytes.len() > max_bytes {
        return Err(LocaleCatalogLoadError::TooLarge {
            path: path.to_path_buf(),
            max_bytes,
        });
    }
    String::from_utf8(bytes).map_err(|_| LocaleCatalogLoadError::InvalidUtf8 {
        path: path.to_path_buf(),
    })
}

/// A graph-authorized catalog input. Language-pack sources target another
/// module, while module-bundled sources target their owning module.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogSource {
    pub owner_module_id: String,
    pub target_module_id: String,
    pub contribution_id: String,
    pub locale: String,
    pub path: PathBuf,
    pub kind: CatalogSourceKind,
    handle: CatalogSourceHandle,
}

impl CatalogSource {
    pub fn module(
        module_id: impl Into<String>,
        contribution_id: impl Into<String>,
        locale: impl Into<String>,
        path: PathBuf,
    ) -> Self {
        Self::try_module(module_id, contribution_id, locale, path)
            .expect("catalog source path must name a file")
    }

    pub fn try_module(
        module_id: impl Into<String>,
        contribution_id: impl Into<String>,
        locale: impl Into<String>,
        path: PathBuf,
    ) -> Result<Self, String> {
        let relative_path = path
            .file_name()
            .ok_or_else(|| format!("catalog source path has no filename: {}", path.display()))?;
        let module_root = path
            .parent()
            .filter(|root| !root.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        Self::module_from_root(
            module_id,
            contribution_id,
            locale,
            module_root.to_path_buf(),
            relative_path,
        )
    }

    pub fn module_from_root(
        module_id: impl Into<String>,
        contribution_id: impl Into<String>,
        locale: impl Into<String>,
        module_root: impl Into<PathBuf>,
        relative_path: impl AsRef<Path>,
    ) -> Result<Self, String> {
        let module_id = module_id.into();
        let handle = CatalogSourceHandle::new(module_root, relative_path)?;
        Ok(Self {
            owner_module_id: module_id.clone(),
            target_module_id: module_id,
            contribution_id: contribution_id.into(),
            locale: locale.into(),
            path: handle.candidate_path(),
            kind: CatalogSourceKind::ModuleBundled,
            handle,
        })
    }

    pub fn language_pack(
        pack_module_id: impl Into<String>,
        target_module_id: impl Into<String>,
        contribution_id: impl Into<String>,
        locale: impl Into<String>,
        path: PathBuf,
    ) -> Self {
        Self::try_language_pack(
            pack_module_id,
            target_module_id,
            contribution_id,
            locale,
            path,
        )
        .expect("catalog source path must name a file")
    }

    pub fn try_language_pack(
        pack_module_id: impl Into<String>,
        target_module_id: impl Into<String>,
        contribution_id: impl Into<String>,
        locale: impl Into<String>,
        path: PathBuf,
    ) -> Result<Self, String> {
        let relative_path = path
            .file_name()
            .ok_or_else(|| format!("catalog source path has no filename: {}", path.display()))?;
        let module_root = path
            .parent()
            .filter(|root| !root.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        Self::language_pack_from_root(
            pack_module_id,
            target_module_id,
            contribution_id,
            locale,
            module_root.to_path_buf(),
            relative_path,
        )
    }

    pub fn language_pack_from_root(
        pack_module_id: impl Into<String>,
        target_module_id: impl Into<String>,
        contribution_id: impl Into<String>,
        locale: impl Into<String>,
        module_root: impl Into<PathBuf>,
        relative_path: impl AsRef<Path>,
    ) -> Result<Self, String> {
        let handle = CatalogSourceHandle::new(module_root, relative_path)?;
        Ok(Self {
            owner_module_id: pack_module_id.into(),
            target_module_id: target_module_id.into(),
            contribution_id: contribution_id.into(),
            locale: locale.into(),
            path: handle.candidate_path(),
            kind: CatalogSourceKind::LanguagePack,
            handle,
        })
    }
}

/// The source that supplied an effective translation for a module/key pair.
/// This is retained in the immutable catalog snapshot for `which`/debug
/// consumers and diagnostics.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CatalogProvenance {
    pub kind: CatalogSourceKind,
    pub owner_module_id: String,
    pub target_module_id: String,
    pub contribution_id: String,
    pub locale: String,
    pub path: PathBuf,
}

impl CatalogProvenance {
    pub fn kind_name(&self) -> &'static str {
        match self.kind {
            CatalogSourceKind::ModuleBundled => "module",
            CatalogSourceKind::LanguagePack => "language-pack",
        }
    }
}

/// The one resolved representation shared by localized metadata and runtime
/// lookups. `text` is always display-safe: it is the winning translation, the
/// declared fallback, or the visible `!!key` miss marker. A missing result
/// retains the owner and catalog snapshot identity so callers can report it
/// without reconstructing lookup state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LocalizedTextResolution {
    pub owner_module_id: String,
    pub key: Option<String>,
    pub text: String,
    pub fallback: Option<String>,
    pub field_path: Option<String>,
    pub source: Option<CatalogProvenance>,
    pub snapshot_revision: u64,
    pub missing: bool,
}

impl LocalizedTextResolution {
    pub fn literal(
        owner_module_id: impl Into<String>,
        text: impl Into<String>,
        revision: u64,
    ) -> Self {
        Self {
            owner_module_id: owner_module_id.into(),
            key: None,
            text: text.into(),
            fallback: None,
            field_path: None,
            source: None,
            snapshot_revision: revision,
            missing: false,
        }
    }

    pub fn missing_marker(key: &str) -> String {
        format!("!!{key}")
    }

    /// Build the canonical visible result for a key that was not supplied by
    /// the active snapshot. Consumers can enqueue this value without having
    /// to reconstruct the owning module or snapshot identity later.
    pub fn missing(
        owner_module_id: impl Into<String>,
        key: impl Into<String>,
        fallback: Option<String>,
        snapshot_revision: u64,
    ) -> Self {
        let key = key.into();
        Self {
            owner_module_id: owner_module_id.into(),
            text: Self::missing_marker(&key),
            key: Some(key),
            fallback,
            field_path: None,
            source: None,
            snapshot_revision,
            missing: true,
        }
    }

    pub fn with_field_path(mut self, field_path: impl Into<String>) -> Self {
        self.field_path = Some(field_path.into());
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CatalogLayer {
    source: CatalogProvenance,
    messages: HashMap<String, CatalogEntry>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LocaleCatalogSnapshot {
    revision: u64,
    core: HashMap<String, HashMap<String, CatalogEntry>>,
    modules: HashMap<String, HashMap<String, Vec<CatalogLayer>>>,
    module_defaults: HashMap<String, String>,
}

impl LocaleCatalogSnapshot {
    pub fn revision(&self) -> u64 {
        self.revision
    }
}

/// One immutable locale decision and its graph-authorized catalog snapshot.
///
/// Shell surfaces retain this value instead of owning a `LocaleEngine`. The
/// engine is the coordinator's mutable preparation handle; this is the
/// read-only value shared with every mounted surface and runtime consumer.
#[derive(Debug, Clone)]
pub struct LocaleSnapshot {
    selection: LocaleSelection,
    catalogs: Arc<LocaleCatalogSnapshot>,
}

impl LocaleSnapshot {
    pub fn new(default_locale: impl Into<String>, fallback_locale: impl Into<String>) -> Self {
        match Self::try_new(default_locale, fallback_locale) {
            Ok(snapshot) => snapshot,
            Err(error) => {
                tracing::warn!(%error, "invalid locale snapshot selection; using en");
                Self::try_new("en", "en").expect("the built-in en locale must be valid")
            }
        }
    }

    pub fn try_new(
        default_locale: impl Into<String>,
        fallback_locale: impl Into<String>,
    ) -> Result<Self, LocaleError> {
        Ok(Self {
            selection: LocaleSelection::try_new(default_locale.into(), fallback_locale.into(), 1)?,
            catalogs: Arc::new(LocaleCatalogSnapshot::default()),
        })
    }

    pub fn selection(&self) -> &LocaleSelection {
        &self.selection
    }

    pub fn current(&self) -> &str {
        self.selection.active()
    }

    pub fn fallback_locale(&self) -> &str {
        self.selection.fallback()
    }

    pub fn direction(&self) -> LocaleDirection {
        self.selection.direction()
    }

    pub fn revision(&self) -> u64 {
        self.selection.revision()
    }

    pub fn catalog_snapshot(&self) -> Arc<LocaleCatalogSnapshot> {
        Arc::clone(&self.catalogs)
    }

    pub fn module_translator(&self, module_id: &str) -> ModuleTranslator<'_> {
        ModuleTranslator {
            snapshot: self,
            module_id: module_id.to_string(),
        }
    }

    pub fn core_translator(&self) -> CoreTranslator<'_> {
        CoreTranslator { snapshot: self }
    }

    fn translate_in_core(&self, key: &str) -> Option<&CatalogEntry> {
        for locale in self.selection.chain() {
            if let Some(messages) = self.catalogs.core.get(locale) {
                if let Some(value) = messages.get(key) {
                    return Some(value);
                }
            }
        }
        None
    }

    fn translate_in_module(&self, key: &str, module_id: &str) -> Option<&CatalogEntry> {
        for locale in self.selection.chain() {
            if let Some(value) = self.module_entry(module_id, locale, key, true) {
                return Some(value);
            }
        }
        if let Some(default_locale) = self.catalogs.module_defaults.get(module_id)
            && !self
                .selection
                .chain()
                .iter()
                .any(|locale| locale == default_locale)
            && let Some(value) = self.module_entry(module_id, default_locale, key, false)
        {
            return Some(value);
        }
        None
    }

    fn resolve_in_core<'a>(&'a self, key: &str, args: &HashMap<String, String>) -> Option<&'a str> {
        for locale in self.selection.chain() {
            if let Some(entry) = self.catalogs.core.get(locale).and_then(|m| m.get(key)) {
                return entry.resolve(args, self.current());
            }
        }
        None
    }

    fn resolve_in_module<'a>(
        &'a self,
        key: &str,
        module_id: &str,
        args: &HashMap<String, String>,
    ) -> Option<&'a str> {
        for locale in self.selection.chain() {
            if let Some(entry) = self.module_entry(module_id, locale, key, true) {
                return entry.resolve(args, self.current());
            }
        }
        if let Some(default_locale) = self.catalogs.module_defaults.get(module_id)
            && !self
                .selection
                .chain()
                .iter()
                .any(|locale| locale == default_locale)
            && let Some(entry) = self.module_entry(module_id, default_locale, key, false)
        {
            return entry.resolve(args, self.current());
        }
        None
    }

    fn module_entry(
        &self,
        module_id: &str,
        locale: &str,
        key: &str,
        include_language_packs: bool,
    ) -> Option<&CatalogEntry> {
        self.catalogs
            .modules
            .get(module_id)
            .and_then(|module_locales| module_locales.get(locale))
            .and_then(|layers| {
                layers.iter().find_map(|layer| {
                    if !include_language_packs
                        && layer.source.kind == CatalogSourceKind::LanguagePack
                    {
                        return None;
                    }
                    layer.messages.get(key)
                })
            })
    }

    fn source_for_module(&self, key: &str, module_id: &str) -> Option<&CatalogProvenance> {
        for locale in self.selection.chain() {
            if let Some(source) = self.module_source(module_id, locale, key, true) {
                return Some(source);
            }
        }
        if let Some(default_locale) = self.catalogs.module_defaults.get(module_id)
            && !self
                .selection
                .chain()
                .iter()
                .any(|locale| locale == default_locale)
        {
            return self.module_source(module_id, default_locale, key, false);
        }
        None
    }

    fn module_source(
        &self,
        module_id: &str,
        locale: &str,
        key: &str,
        include_language_packs: bool,
    ) -> Option<&CatalogProvenance> {
        self.catalogs
            .modules
            .get(module_id)
            .and_then(|module_locales| module_locales.get(locale))
            .and_then(|layers| {
                layers.iter().find_map(|layer| {
                    if !include_language_packs
                        && layer.source.kind == CatalogSourceKind::LanguagePack
                    {
                        return None;
                    }
                    layer.messages.contains_key(key).then_some(&layer.source)
                })
            })
    }

    fn effective_core_translations(&self) -> HashMap<String, String> {
        let mut messages = HashMap::new();
        for locale in self.selection.chain().iter().rev() {
            if let Some(catalog) = self.catalogs.core.get(locale) {
                messages.extend(catalog.iter().filter_map(|(key, entry)| {
                    entry
                        .default_text()
                        .map(|value| (key.clone(), value.to_string()))
                }));
            }
        }
        messages
    }

    fn effective_module_translations(&self, module_id: &str) -> HashMap<String, String> {
        let mut messages = HashMap::new();
        for key in self.module_keys(module_id) {
            if let Some(entry) = self.translate_in_module(&key, module_id)
                && let Some(value) = entry.default_text()
            {
                messages.insert(key, value.to_string());
            }
        }
        messages
    }

    fn effective_module_entries(&self, module_id: &str) -> HashMap<String, CatalogEntry> {
        let mut messages = HashMap::new();
        for key in self.module_keys(module_id) {
            if let Some(entry) = self.translate_in_module(&key, module_id) {
                messages.insert(key, entry.clone());
            }
        }
        messages
    }

    fn module_keys(&self, module_id: &str) -> BTreeSet<String> {
        self.catalogs
            .modules
            .get(module_id)
            .into_iter()
            .flat_map(|locales| locales.values())
            .flat_map(|layers| layers.iter())
            .flat_map(|layer| layer.messages.keys().cloned())
            .collect()
    }

    pub fn source_for(&self, module_id: &str, key: &str) -> Option<&CatalogProvenance> {
        self.source_for_module(key, module_id)
    }

    pub fn fallback_chain(&self) -> &[String] {
        self.selection.chain()
    }
}

#[derive(Debug, Clone)]
pub struct PreparedLocaleCatalogSnapshot {
    snapshot: Arc<LocaleCatalogSnapshot>,
    diagnostics: Vec<CatalogSourceDiagnostics>,
}

impl PreparedLocaleCatalogSnapshot {
    pub fn snapshot(&self) -> Arc<LocaleCatalogSnapshot> {
        Arc::clone(&self.snapshot)
    }

    pub fn diagnostics(&self) -> &[CatalogSourceDiagnostics] {
        &self.diagnostics
    }
}

/// Compile one JSON catalog entry-by-entry. Invalid entries are reported and
/// skipped without discarding valid siblings.
pub fn compile_catalog(locale: impl Into<String>, value: &serde_json::Value) -> CompiledCatalog {
    let locale = locale.into();
    let mut messages = HashMap::new();
    let mut diagnostics = Vec::new();
    let Some(object) = value.as_object() else {
        diagnostics.push(CatalogDiagnostic {
            key: "<catalog>".into(),
            message: "catalog root must be an object".into(),
        });
        return CompiledCatalog {
            locale,
            messages,
            diagnostics,
        };
    };

    if object.len() > MAX_CATALOG_ENTRIES {
        diagnostics.push(CatalogDiagnostic {
            key: "<catalog>".into(),
            message: format!("catalog exceeds {MAX_CATALOG_ENTRIES} entries"),
        });
    }
    for (index, (key, raw)) in object.iter().enumerate() {
        if index >= MAX_CATALOG_ENTRIES {
            break;
        }
        match compile_catalog_entry(key, raw) {
            Ok(entry) => {
                messages.insert(key.clone(), entry);
            }
            Err(message) => diagnostics.push(CatalogDiagnostic {
                key: key.clone(),
                message,
            }),
        }
    }
    CompiledCatalog {
        locale,
        messages,
        diagnostics,
    }
}

fn compile_catalog_entry(key: &str, raw: &serde_json::Value) -> Result<CatalogEntry, String> {
    if key.is_empty() || key.len() > 256 {
        return Err("translation key must be nonempty and at most 256 bytes".into());
    }
    if let Some(text) = raw.as_str() {
        validate_message(text)?;
        return Ok(CatalogEntry::Text(text.to_string()));
    }
    let Some(object) = raw.as_object() else {
        return Err("translation entry must be a string or variant object".into());
    };
    let plural = object.get("_plural").and_then(serde_json::Value::as_bool);
    let select = object.get("_select").and_then(serde_json::Value::as_bool);
    if plural == Some(true) && select == Some(true) {
        return Err("entry cannot be both plural and select".into());
    }
    let kind = if plural == Some(true) {
        "plural"
    } else if select == Some(true) {
        "select"
    } else {
        return Err("variant object must declare _plural or _select: true".into());
    };
    if object.len().saturating_sub(1) > MAX_VARIANTS_PER_ENTRY {
        return Err(format!(
            "{kind} entry exceeds {MAX_VARIANTS_PER_ENTRY} variants"
        ));
    }
    let mut variants = BTreeMap::new();
    let mut placeholders: Option<BTreeSet<String>> = None;
    for (variant, value) in object {
        if variant.starts_with('_') {
            continue;
        }
        let Some(text) = value.as_str() else {
            return Err(format!("{kind} variant '{variant}' must be a string"));
        };
        validate_message(text)?;
        let current = message_placeholders(text)?;
        if let Some(expected) = &placeholders {
            if expected != &current {
                return Err(format!("{kind} variants use inconsistent placeholders"));
            }
        } else {
            placeholders = Some(current);
        }
        variants.insert(variant.clone(), text.to_string());
    }
    if !variants.contains_key("other") {
        return Err(format!("{kind} entry requires an 'other' variant"));
    }
    if kind == "plural" {
        for variant in variants.keys() {
            if !matches!(
                variant.as_str(),
                "zero" | "one" | "two" | "few" | "many" | "other"
            ) {
                return Err(format!("unknown plural category '{variant}'"));
            }
        }
        Ok(CatalogEntry::Plural(variants))
    } else {
        Ok(CatalogEntry::Select(variants))
    }
}

fn validate_message(message: &str) -> Result<(), String> {
    if message.len() > MAX_MESSAGE_LENGTH {
        return Err(format!("message exceeds {MAX_MESSAGE_LENGTH} bytes"));
    }
    message_placeholders(message).map(|_| ())
}

fn message_placeholders(message: &str) -> Result<BTreeSet<String>, String> {
    let mut placeholders = BTreeSet::new();
    let mut remaining = message;
    while let Some(open) = remaining.find('{') {
        if let Some(close) = remaining[open + 1..].find('}') {
            let close = open + 1 + close;
            let name = &remaining[open + 1..close];
            if name.is_empty()
                || !name
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '.')
            {
                return Err("placeholder names must be nonempty identifiers".into());
            }
            placeholders.insert(name.to_string());
            remaining = &remaining[close + 1..];
        } else {
            return Err("message contains an unmatched '{'".into());
        }
    }
    if remaining.contains('}') {
        return Err("message contains an unmatched '}'".into());
    }
    Ok(placeholders)
}

fn plural_category(locale: &str, count: f64) -> &'static str {
    let language = locale.split('-').next().unwrap_or_default();
    let integer = count.fract() == 0.0;
    let absolute = count.abs();
    if language == "sk" && integer {
        if absolute == 1.0 {
            "one"
        } else if (2.0..=4.0).contains(&absolute) {
            "few"
        } else {
            "many"
        }
    } else if language == "fr" && (absolute == 0.0 || absolute == 1.0) {
        "one"
    } else if absolute == 1.0 {
        "one"
    } else {
        "other"
    }
}

/// A lookup handle restricted to one module's catalogs.
///
/// The handle deliberately borrows the immutable snapshot so callers cannot
/// accidentally retain a copied cross-module key pool. A new handle observes
/// the snapshot's locale chain and catalog contents immediately.
#[derive(Debug, Clone)]
pub struct ModuleTranslator<'a> {
    snapshot: &'a LocaleSnapshot,
    module_id: String,
}

impl ModuleTranslator<'_> {
    pub fn module_id(&self) -> &str {
        &self.module_id
    }

    pub fn locale(&self) -> &str {
        self.snapshot.current()
    }

    pub fn snapshot_revision(&self) -> u64 {
        self.snapshot.catalog_snapshot().revision()
    }

    pub fn translate(&self, key: &str) -> Option<&str> {
        self.snapshot
            .translate_in_module(key, &self.module_id)
            .and_then(CatalogEntry::default_text)
    }

    pub fn translate_with(&self, key: &str, args: &HashMap<String, String>) -> Option<String> {
        let template = self
            .snapshot
            .resolve_in_module(key, &self.module_id, args)?;
        interpolate(template, args)
    }

    /// Resolve a localized value with one shared fallback and miss policy.
    /// The source is cloned from the immutable snapshot so the result remains
    /// useful after a later catalog replacement.
    pub fn resolve(&self, key: &str, fallback: Option<&str>) -> LocalizedTextResolution {
        let source = self.source(key).cloned();
        let translated = self.translate(key).map(str::to_owned);
        let text = translated
            .or_else(|| fallback.map(str::to_owned))
            .unwrap_or_else(|| LocalizedTextResolution::missing_marker(key));
        LocalizedTextResolution {
            owner_module_id: self.module_id.clone(),
            key: Some(key.to_owned()),
            text,
            fallback: fallback.map(str::to_owned),
            field_path: None,
            missing: source.is_none(),
            source,
            snapshot_revision: self.snapshot_revision(),
        }
    }

    pub fn resolve_with(
        &self,
        key: &str,
        args: &HashMap<String, String>,
        fallback: Option<&str>,
    ) -> LocalizedTextResolution {
        let source = self.source(key).cloned();
        let translated = self
            .translate_with(key, args)
            .or_else(|| fallback.map(str::to_owned));
        let text = translated.unwrap_or_else(|| LocalizedTextResolution::missing_marker(key));
        LocalizedTextResolution {
            owner_module_id: self.module_id.clone(),
            key: Some(key.to_owned()),
            text,
            fallback: fallback.map(str::to_owned),
            field_path: None,
            missing: source.is_none(),
            source,
            snapshot_revision: self.snapshot_revision(),
        }
    }

    /// Copy the effective module catalog for a consumer that crosses an
    /// execution boundary, such as a Luau context.
    pub fn translations(&self) -> HashMap<String, String> {
        self.snapshot.effective_module_translations(&self.module_id)
    }

    pub fn entries(&self) -> HashMap<String, CatalogEntry> {
        self.snapshot.effective_module_entries(&self.module_id)
    }

    pub fn source(&self, key: &str) -> Option<&CatalogProvenance> {
        self.snapshot.source_for_module(key, &self.module_id)
    }
}

/// An explicitly named core-domain translator. Core strings never participate
/// in module lookup implicitly.
#[derive(Debug, Clone, Copy)]
pub struct CoreTranslator<'a> {
    snapshot: &'a LocaleSnapshot,
}

impl CoreTranslator<'_> {
    pub fn translate(&self, key: &str) -> Option<&str> {
        self.snapshot
            .translate_in_core(key)
            .and_then(CatalogEntry::default_text)
    }

    pub fn translate_with(&self, key: &str, args: &HashMap<String, String>) -> Option<String> {
        let template = self.snapshot.resolve_in_core(key, args)?;
        interpolate(template, args)
    }

    pub fn translations(&self) -> HashMap<String, String> {
        self.snapshot.effective_core_translations()
    }
}

#[derive(Debug, Clone)]
pub struct LocaleEngine {
    snapshot: Arc<LocaleSnapshot>,
}

impl LocaleEngine {
    pub fn new(default_locale: impl Into<String>) -> Self {
        Self::with_fallback_locale(default_locale, "en")
    }

    pub fn try_with_fallback_locale(
        default_locale: impl Into<String>,
        fallback_locale: impl Into<String>,
    ) -> Result<Self, LocaleError> {
        Ok(Self {
            snapshot: Arc::new(LocaleSnapshot::try_new(default_locale, fallback_locale)?),
        })
    }

    pub fn from_snapshot(snapshot: Arc<LocaleSnapshot>) -> Self {
        Self { snapshot }
    }

    pub fn with_fallback_locale(
        default_locale: impl Into<String>,
        fallback_locale: impl Into<String>,
    ) -> Self {
        match Self::try_with_fallback_locale(default_locale, fallback_locale) {
            Ok(engine) => engine,
            Err(error) => {
                tracing::warn!(%error, "invalid locale selection; using en");
                Self::try_with_fallback_locale("en", "en")
                    .expect("the built-in en locale must be valid")
            }
        }
    }

    pub fn selection(&self) -> &LocaleSelection {
        self.snapshot.selection()
    }

    pub fn direction(&self) -> LocaleDirection {
        self.snapshot.direction()
    }

    pub fn revision(&self) -> u64 {
        self.snapshot.revision()
    }

    pub fn fallback_locale(&self) -> &str {
        self.snapshot.fallback_locale()
    }

    pub fn snapshot(&self) -> Arc<LocaleSnapshot> {
        Arc::clone(&self.snapshot)
    }

    /// Adopt an already validated selection while retaining loaded catalogs.
    pub fn replace_selection(&mut self, selection: &LocaleSelection) {
        let snapshot = Arc::make_mut(&mut self.snapshot);
        snapshot.selection = selection.clone();
    }

    pub fn catalog_snapshot(&self) -> Arc<LocaleCatalogSnapshot> {
        self.snapshot.catalog_snapshot()
    }

    /// Replace the catalog pointer after a complete candidate has been
    /// prepared. Existing readers retaining the prior Arc keep last-known-good
    /// data until the replacement is committed.
    pub fn replace_catalog_snapshot(&mut self, snapshot: Arc<LocaleCatalogSnapshot>) {
        let current = Arc::make_mut(&mut self.snapshot);
        current.catalogs = snapshot;
    }

    /// Prepare a complete replacement catalog without changing this engine.
    /// Every source must be readable and parseable before the returned Arc can
    /// be committed. Structured entry diagnostics are non-fatal so valid
    /// siblings remain available in the candidate.
    pub fn prepare_module_catalog_snapshot(
        &self,
        sources: &[(String, String, PathBuf)],
    ) -> Result<PreparedLocaleCatalogSnapshot, LocaleCatalogLoadError> {
        let sources = sources
            .iter()
            .map(|(module_id, locale, path)| {
                CatalogSource::try_module(
                    module_id.clone(),
                    locale.clone(),
                    locale.clone(),
                    path.clone(),
                )
                .map_err(|reason| LocaleCatalogLoadError::UnsafePath {
                    path: path.clone(),
                    reason,
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        self.prepare_catalog_snapshot(&sources, &HashMap::new())
    }

    /// Prepare a complete replacement from graph-authorized module and
    /// language-pack sources. Sources are expected in configured pack order;
    /// the first matching language pack wins, followed by the module-bundled
    /// catalog. Module defaults are terminal fallbacks for their target only.
    pub fn prepare_catalog_snapshot(
        &self,
        sources: &[CatalogSource],
        module_defaults: &HashMap<String, String>,
    ) -> Result<PreparedLocaleCatalogSnapshot, LocaleCatalogLoadError> {
        let mut snapshot = LocaleCatalogSnapshot {
            revision: self.catalog_snapshot().revision.saturating_add(1),
            core: HashMap::new(),
            modules: HashMap::new(),
            module_defaults: HashMap::new(),
        };
        let mut diagnostics = Vec::new();

        for (module_id, locale) in module_defaults {
            let locale = canonicalize_locale_tag(locale).map_err(|_| {
                LocaleCatalogLoadError::InvalidLocale {
                    locale: locale.clone(),
                    context: format!("default locale for module {module_id}"),
                }
            })?;
            snapshot.module_defaults.insert(module_id.clone(), locale);
        }

        for source in sources {
            let locale = canonicalize_locale_tag(&source.locale).map_err(|_| {
                LocaleCatalogLoadError::InvalidLocale {
                    locale: source.locale.clone(),
                    context: format!("catalog {}", source.path.display()),
                }
            })?;
            let content = source
                .handle
                .read_utf8_bounded(DEFAULT_MAX_CATALOG_SOURCE_BYTES)?;
            let value = serde_json::from_str(&content).map_err(|source_error| {
                LocaleCatalogLoadError::Parse {
                    path: source.path.clone(),
                    source: source_error,
                }
            })?;
            let catalog = compile_catalog(locale.clone(), &value);
            if !catalog.diagnostics.is_empty() {
                diagnostics.push(CatalogSourceDiagnostics {
                    module_id: source.owner_module_id.clone(),
                    locale: locale.clone(),
                    path: source.path.clone(),
                    diagnostics: catalog.diagnostics,
                });
            }
            let provenance = CatalogProvenance {
                kind: source.kind,
                owner_module_id: source.owner_module_id.clone(),
                target_module_id: source.target_module_id.clone(),
                contribution_id: source.contribution_id.clone(),
                locale: locale.clone(),
                path: source.path.clone(),
            };
            snapshot
                .modules
                .entry(source.target_module_id.clone())
                .or_default()
                .entry(locale)
                .or_default()
                .push(CatalogLayer {
                    source: provenance,
                    messages: catalog.messages,
                });
        }

        // Keep the caller's order among pack layers, but always inspect packs
        // before the target module's own catalog for a locale. `sort_by` is
        // stable, so the configured first-pack-wins order is preserved.
        for locales in snapshot.modules.values_mut() {
            for layers in locales.values_mut() {
                layers.sort_by_key(|layer| match layer.source.kind {
                    CatalogSourceKind::LanguagePack => 0,
                    CatalogSourceKind::ModuleBundled => 1,
                });
            }
        }

        Ok(PreparedLocaleCatalogSnapshot {
            snapshot: Arc::new(snapshot),
            diagnostics,
        })
    }

    /// Prepare a complete catalog candidate away from the shell thread.
    ///
    /// The engine is cheap to clone because its current snapshot is shared;
    /// the worker owns the graph-authorized source handles and returns an
    /// immutable candidate for the caller to commit atomically.
    pub fn prepare_catalog_snapshot_off_thread(
        &self,
        sources: Vec<CatalogSource>,
        module_defaults: HashMap<String, String>,
    ) -> Result<PreparedLocaleCatalogSnapshot, LocaleCatalogLoadError> {
        let engine = self.clone();
        std::thread::Builder::new()
            .name("mesh-locale-catalog".into())
            .spawn(move || engine.prepare_catalog_snapshot(&sources, &module_defaults))
            .map_err(|source| LocaleCatalogLoadError::WorkerSpawn { source })?
            .join()
            .map_err(|_| LocaleCatalogLoadError::WorkerPanic)?
    }

    pub fn try_set_locale(&mut self, locale: impl AsRef<str>) -> Result<bool, LocaleError> {
        let next = LocaleSelection::try_new(
            locale,
            self.snapshot.fallback_locale(),
            self.snapshot.revision().saturating_add(1),
        )?;
        if next.active == self.snapshot.selection.active
            && next.fallback == self.snapshot.selection.fallback
        {
            return Ok(false);
        }
        self.replace_selection(&next);
        Ok(true)
    }

    pub fn current(&self) -> &str {
        self.snapshot.current()
    }

    pub fn set_locale(&mut self, locale: impl Into<String>) {
        if let Err(error) = self.try_set_locale(locale.into()) {
            tracing::warn!(%error, "ignoring invalid locale selection");
        }
    }

    /// Load a catalog owned by the explicit core domain.
    pub fn load_core_translations(&mut self, set: TranslationSet) {
        let locale = catalog_locale(&set.locale);
        let snapshot = Arc::make_mut(&mut self.snapshot);
        let catalogs = Arc::make_mut(&mut snapshot.catalogs);
        catalogs.core.entry(locale).or_default().extend(
            set.messages
                .into_iter()
                .map(|(key, value)| (key, CatalogEntry::Text(value))),
        );
        catalogs.revision = catalogs.revision.saturating_add(1);
    }

    pub fn load_core_catalog(&mut self, catalog: CompiledCatalog) {
        let locale = catalog_locale(&catalog.locale);
        let snapshot = Arc::make_mut(&mut self.snapshot);
        let catalogs = Arc::make_mut(&mut snapshot.catalogs);
        catalogs
            .core
            .entry(locale)
            .or_default()
            .extend(catalog.messages);
        catalogs.revision = catalogs.revision.saturating_add(1);
    }

    pub fn module_translator(&self, module_id: &str) -> ModuleTranslator<'_> {
        self.snapshot.module_translator(module_id)
    }

    pub fn core_translator(&self) -> CoreTranslator<'_> {
        self.snapshot.core_translator()
    }

    /// Load a catalog owned by one module. It is never copied into the core
    /// domain, so one module cannot shadow another module's keys.
    pub fn load_module_translations(&mut self, module_id: &str, set: TranslationSet) {
        let locale = catalog_locale(&set.locale);
        let snapshot = Arc::make_mut(&mut self.snapshot);
        let catalogs = Arc::make_mut(&mut snapshot.catalogs);
        catalogs
            .modules
            .entry(module_id.to_string())
            .or_default()
            .entry(locale)
            .or_default()
            .push(CatalogLayer {
                source: CatalogProvenance {
                    kind: CatalogSourceKind::ModuleBundled,
                    owner_module_id: module_id.to_string(),
                    target_module_id: module_id.to_string(),
                    contribution_id: "runtime".into(),
                    locale: catalog_locale(&set.locale),
                    path: PathBuf::new(),
                },
                messages: set
                    .messages
                    .into_iter()
                    .map(|(key, value)| (key, CatalogEntry::Text(value)))
                    .collect(),
            });
        catalogs.revision = catalogs.revision.saturating_add(1);
    }

    pub fn load_module_catalog(&mut self, module_id: &str, catalog: CompiledCatalog) {
        let locale = catalog_locale(&catalog.locale);
        let snapshot = Arc::make_mut(&mut self.snapshot);
        let catalogs = Arc::make_mut(&mut snapshot.catalogs);
        catalogs
            .modules
            .entry(module_id.to_string())
            .or_default()
            .entry(locale)
            .or_default()
            .push(CatalogLayer {
                source: CatalogProvenance {
                    kind: CatalogSourceKind::ModuleBundled,
                    owner_module_id: module_id.to_string(),
                    target_module_id: module_id.to_string(),
                    contribution_id: "runtime".into(),
                    locale: catalog_locale(&catalog.locale),
                    path: PathBuf::new(),
                },
                messages: catalog.messages,
            });
        catalogs.revision = catalogs.revision.saturating_add(1);
    }

    pub fn source_for(&self, module_id: &str, key: &str) -> Option<&CatalogProvenance> {
        self.snapshot.source_for(module_id, key)
    }

    pub fn fallback_chain(&self) -> &[String] {
        self.snapshot.fallback_chain()
    }
}

fn catalog_locale(locale: &str) -> String {
    canonicalize_locale_tag(locale).unwrap_or_else(|_| locale.trim().to_string())
}

fn canonicalize_locale_tag(raw: &str) -> Result<String, LocaleError> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err(LocaleError::EmptyTag);
    }
    let parts = raw.split('-').collect::<Vec<_>>();
    if parts.iter().any(|part| part.is_empty()) {
        return Err(LocaleError::InvalidTag {
            tag: raw.to_string(),
        });
    }

    let language = parts[0];
    if !(2..=8).contains(&language.len()) || !language.chars().all(|ch| ch.is_ascii_alphabetic()) {
        return Err(LocaleError::InvalidTag {
            tag: raw.to_string(),
        });
    }
    let mut normalized = vec![language.to_ascii_lowercase()];
    let mut index = 1;

    if parts
        .get(index)
        .is_some_and(|part| part.len() == 4 && part.chars().all(|ch| ch.is_ascii_alphabetic()))
    {
        let script = parts[index];
        let mut chars = script.chars();
        let first = chars.next().expect("script is nonempty");
        normalized.push(format!(
            "{}{}",
            first.to_ascii_uppercase(),
            chars.as_str().to_ascii_lowercase()
        ));
        index += 1;
    }

    if parts.get(index).is_some_and(|part| {
        (part.len() == 2 && part.chars().all(|ch| ch.is_ascii_alphabetic()))
            || (part.len() == 3 && part.chars().all(|ch| ch.is_ascii_digit()))
    }) {
        normalized.push(if parts[index].chars().all(|ch| ch.is_ascii_digit()) {
            parts[index].to_string()
        } else {
            parts[index].to_ascii_uppercase()
        });
        index += 1;
    }

    while index < parts.len() {
        let part = parts[index];
        if part.len() == 1 {
            if !part.chars().all(|ch| ch.is_ascii_alphanumeric()) {
                return Err(LocaleError::InvalidTag {
                    tag: raw.to_string(),
                });
            }
            normalized.push(part.to_ascii_lowercase());
            index += 1;
            let extension_start = index;
            while index < parts.len() && parts[index].len() != 1 {
                let extension = parts[index];
                if !(2..=8).contains(&extension.len())
                    || !extension.chars().all(|ch| ch.is_ascii_alphanumeric())
                {
                    return Err(LocaleError::InvalidTag {
                        tag: raw.to_string(),
                    });
                }
                normalized.push(extension.to_ascii_lowercase());
                index += 1;
            }
            if index == extension_start {
                return Err(LocaleError::InvalidTag {
                    tag: raw.to_string(),
                });
            }
            continue;
        }

        let is_variant = (5..=8).contains(&part.len())
            && part.chars().all(|ch| ch.is_ascii_alphanumeric())
            || (part.len() == 4
                && part.chars().next().is_some_and(|ch| ch.is_ascii_digit())
                && part.chars().all(|ch| ch.is_ascii_alphanumeric()));
        if !is_variant {
            return Err(LocaleError::InvalidTag {
                tag: raw.to_string(),
            });
        }
        normalized.push(part.to_ascii_lowercase());
        index += 1;
    }

    Ok(normalized.join("-"))
}

fn locale_parents(locale: &str) -> Vec<String> {
    let mut parts = locale.split('-').collect::<Vec<_>>();
    if let Some(extension_index) = parts.iter().position(|part| part.len() == 1) {
        parts.truncate(extension_index);
    }
    let mut parents = Vec::new();
    while parts.len() > 1 {
        parts.pop();
        parents.push(parts.join("-"));
    }
    parents
}

fn complete_fallback_chain(active: &str, fallback: &str) -> Vec<String> {
    let mut chain = Vec::new();
    let mut append = |locale: &str| {
        if !chain.iter().any(|existing| existing == locale) {
            chain.push(locale.to_string());
        }
    };
    append(active);
    for parent in locale_parents(active) {
        append(&parent);
    }
    append(fallback);
    for parent in locale_parents(fallback) {
        append(&parent);
    }
    append("en");
    chain
}

fn direction_for(locale: &str) -> LocaleDirection {
    let language = locale.split('-').next().unwrap_or_default();
    let script = locale
        .split('-')
        .find(|part| part.len() == 4)
        .unwrap_or_default();
    if matches!(
        language,
        "ar" | "dv" | "fa" | "he" | "ku" | "ps" | "sd" | "ug" | "ur" | "yi"
    ) || matches!(script, "Arab" | "Hebr" | "Nkoo" | "Rohg" | "Syrc" | "Thaa")
    {
        LocaleDirection::Rtl
    } else {
        LocaleDirection::Ltr
    }
}

/// Interpolates `{name}` placeholders in one walk, so cost is O(template_len)
/// regardless of how many args are supplied.
fn interpolate(template: &str, args: &HashMap<String, String>) -> Option<String> {
    let mut result = String::with_capacity(template.len());
    let mut remaining = template;
    while let Some(open) = remaining.find('{') {
        result.push_str(&remaining[..open]);
        let after_open = &remaining[open + 1..];
        if let Some(close) = after_open.find('}') {
            let name = &after_open[..close];
            match args.get(name) {
                Some(value) => result.push_str(value),
                None => {
                    result.push('{');
                    result.push_str(name);
                    result.push('}');
                }
            }
            remaining = &after_open[close + 1..];
        } else {
            result.push_str(&remaining[open..]);
            return Some(result);
        }
    }
    result.push_str(remaining);
    Some(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_translation() {
        let mut engine = LocaleEngine::new("en");
        engine.load_core_translations(TranslationSet {
            locale: "en".to_string(),
            messages: HashMap::from([
                ("greeting".to_string(), "Hello, {name}!".to_string()),
                ("bye".to_string(), "Goodbye".to_string()),
            ]),
        });

        assert_eq!(engine.core_translator().translate("bye"), Some("Goodbye"));

        let args = HashMap::from([("name".to_string(), "World".to_string())]);
        assert_eq!(
            engine.core_translator().translate_with("greeting", &args),
            Some("Hello, World!".to_string())
        );
    }

    #[test]
    fn fallback_chain() {
        let mut engine = LocaleEngine::new("fr");
        engine.load_core_translations(TranslationSet {
            locale: "en".to_string(),
            messages: HashMap::from([("ok".to_string(), "OK".to_string())]),
        });

        assert_eq!(engine.core_translator().translate("ok"), Some("OK"));
    }

    #[test]
    fn effective_module_catalog_matches_scoped_lookup_precedence() {
        let mut engine = LocaleEngine::with_fallback_locale("sk", "en");
        engine.load_core_translations(TranslationSet {
            locale: "sk".to_string(),
            messages: HashMap::from([
                ("shared".to_string(), "global-sk".to_string()),
                ("global".to_string(), "iba-global".to_string()),
            ]),
        });
        engine.load_core_translations(TranslationSet {
            locale: "en".to_string(),
            messages: HashMap::from([("shared".to_string(), "global-en".to_string())]),
        });
        engine.load_module_translations(
            "@mesh/example",
            TranslationSet {
                locale: "en".to_string(),
                messages: HashMap::from([("shared".to_string(), "module-en".to_string())]),
            },
        );

        let translator = engine.module_translator("@mesh/example");
        assert_eq!(translator.translate("shared"), Some("module-en"));
        assert_eq!(translator.translate("global"), None);
    }

    #[test]
    fn module_catalogs_never_leak_into_other_module_or_core_lookup() {
        let mut engine = LocaleEngine::new("en");
        engine.load_module_translations(
            "@mesh/one",
            TranslationSet {
                locale: "en".into(),
                messages: HashMap::from([(String::from("shared"), String::from("one"))]),
            },
        );
        engine.load_module_translations(
            "@mesh/two",
            TranslationSet {
                locale: "en".into(),
                messages: HashMap::from([(String::from("shared"), String::from("two"))]),
            },
        );

        assert_eq!(
            engine.module_translator("@mesh/one").translate("shared"),
            Some("one")
        );
        assert_eq!(
            engine.module_translator("@mesh/two").translate("shared"),
            Some("two")
        );
        assert_eq!(engine.core_translator().translate("shared"), None);
    }

    #[test]
    fn selection_normalizes_parents_direction_and_revision() {
        let selection = LocaleSelection::try_new(" zh-hant-tw ", "en-US", 7).unwrap();

        assert_eq!(selection.active(), "zh-Hant-TW");
        assert_eq!(selection.fallback(), "en-US");
        assert_eq!(
            selection.chain(),
            &["zh-Hant-TW", "zh-Hant", "zh", "en-US", "en"]
        );
        assert_eq!(selection.direction(), LocaleDirection::Ltr);
        assert_eq!(selection.revision(), 7);

        let rtl = LocaleSelection::try_new("ar-EG", "en", 1).unwrap();
        assert_eq!(rtl.direction(), LocaleDirection::Rtl);
    }

    #[test]
    fn locale_changes_replace_stale_chain_and_increment_revision() {
        let mut engine = LocaleEngine::with_fallback_locale("sk-SK", "de-DE");
        assert_eq!(
            engine.fallback_chain(),
            &["sk-SK", "sk", "de-DE", "de", "en"]
        );
        assert_eq!(engine.revision(), 1);

        assert!(engine.try_set_locale("fr-FR").unwrap());
        assert_eq!(engine.current(), "fr-FR");
        assert_eq!(
            engine.fallback_chain(),
            &["fr-FR", "fr", "de-DE", "de", "en"]
        );
        assert_eq!(engine.revision(), 2);
        assert!(!engine.try_set_locale("fr-fr").unwrap());
        assert_eq!(engine.revision(), 2);
    }

    #[test]
    fn invalid_locale_tags_are_rejected() {
        assert!(matches!(
            LocaleSelection::try_new("", "en", 1),
            Err(LocaleError::EmptyTag)
        ));
        assert!(matches!(
            LocaleSelection::try_new("en_US", "en", 1),
            Err(LocaleError::InvalidTag { .. })
        ));
        assert!(matches!(
            LocaleSelection::try_new("en-a", "en", 1),
            Err(LocaleError::InvalidTag { .. })
        ));
    }

    #[test]
    fn system_locale_normalization_removes_posix_details() {
        assert_eq!(
            normalize_system_locale("sk_SK.UTF-8@euro"),
            Some("sk-SK".to_string())
        );
        assert_eq!(normalize_system_locale("C.UTF-8"), None);
        assert_eq!(normalize_system_locale("POSIX"), None);
        assert_eq!(
            normalize_system_locale(" zh-Hant "),
            Some("zh-Hant".to_string())
        );
    }

    #[test]
    fn typed_catalogs_keep_valid_siblings_and_resolve_variants() {
        let compiled = compile_catalog(
            "sk",
            &serde_json::json!({
                "plain": "Ahoj {name}",
                "items": {
                    "_plural": true,
                    "one": "{count} položka",
                    "few": "{count} položky",
                    "many": "{count} položiek",
                    "other": "{count} položiek"
                },
                "greeting": {
                    "_select": true,
                    "formal": "Dobrý deň",
                    "other": "Ahoj"
                },
                "broken": {
                    "_plural": true,
                    "one": "{count} položka",
                    "other": "položky"
                }
            }),
        );
        assert_eq!(compiled.messages.len(), 3);
        assert_eq!(compiled.diagnostics.len(), 1);
        assert_eq!(compiled.diagnostics[0].key, "broken");

        let mut engine = LocaleEngine::new("sk");
        engine.load_module_catalog("@mesh/example", compiled);
        let translator = engine.module_translator("@mesh/example");
        let args = HashMap::from([
            ("count".into(), "3".into()),
            ("name".into(), "Mária".into()),
            ("select".into(), "formal".into()),
        ]);
        assert_eq!(
            translator.translate_with("plain", &args),
            Some("Ahoj Mária".into())
        );
        assert_eq!(
            translator.translate_with("items", &args),
            Some("3 položky".into())
        );
        assert_eq!(
            translator.translate_with("greeting", &args),
            Some("Dobrý deň".into())
        );
        assert_eq!(translator.translate("items"), Some("{count} položiek"));
    }

    #[test]
    fn ordered_language_packs_win_before_bundled_and_default_catalogs() {
        let module_path = test_catalog_path("layer-module");
        let first_pack_path = test_catalog_path("layer-pack-first");
        let second_pack_path = test_catalog_path("layer-pack-second");
        let default_path = test_catalog_path("layer-default");
        std::fs::write(
            &module_path,
            serde_json::json!({
                "shared": "module-cs",
                "bundled": "bundled-cs"
            })
            .to_string(),
        )
        .unwrap();
        std::fs::write(
            &first_pack_path,
            serde_json::json!({ "shared": "first-pack" }).to_string(),
        )
        .unwrap();
        std::fs::write(
            &second_pack_path,
            serde_json::json!({ "shared": "second-pack" }).to_string(),
        )
        .unwrap();
        std::fs::write(
            &default_path,
            serde_json::json!({ "defaulted": "module-default" }).to_string(),
        )
        .unwrap();

        let sources = vec![
            CatalogSource::module("@mesh/example", "cs", "cs", module_path.clone()),
            CatalogSource::language_pack(
                "@mesh/first-pack",
                "@mesh/example",
                "first-cs",
                "cs",
                first_pack_path.clone(),
            ),
            CatalogSource::language_pack(
                "@mesh/second-pack",
                "@mesh/example",
                "second-cs",
                "cs",
                second_pack_path.clone(),
            ),
            CatalogSource::module("@mesh/example", "default", "de", default_path.clone()),
        ];
        let defaults = HashMap::from([("@mesh/example".into(), "de".into())]);
        let mut engine = LocaleEngine::with_fallback_locale("cs-CZ", "en");
        let prepared = engine
            .prepare_catalog_snapshot(&sources, &defaults)
            .unwrap();
        engine.replace_catalog_snapshot(prepared.snapshot());

        let translator = engine.module_translator("@mesh/example");
        assert_eq!(translator.translate("shared"), Some("first-pack"));
        assert_eq!(translator.translate("bundled"), Some("bundled-cs"));
        assert_eq!(translator.translate("defaulted"), Some("module-default"));
        let source = translator.source("shared").unwrap();
        assert_eq!(source.kind, CatalogSourceKind::LanguagePack);
        assert_eq!(source.owner_module_id, "@mesh/first-pack");
        assert_eq!(source.contribution_id, "first-cs");
        assert_eq!(source.path, first_pack_path);
        assert_eq!(
            engine.module_translator("@mesh/other").translate("shared"),
            None
        );

        for path in [module_path, first_pack_path, second_pack_path, default_path] {
            let _ = std::fs::remove_file(path);
        }
    }

    #[test]
    fn catalog_snapshots_are_revisioned_and_replaced_atomically() {
        let mut engine = LocaleEngine::new("en");
        let empty = engine.catalog_snapshot();
        engine.load_module_translations(
            "@mesh/example",
            TranslationSet {
                locale: "en".into(),
                messages: HashMap::from([(String::from("hello"), String::from("Hello"))]),
            },
        );
        let committed = engine.catalog_snapshot();
        assert_eq!(empty.revision(), 0);
        assert_eq!(committed.revision(), 1);
        assert_eq!(
            engine.module_translator("@mesh/example").translate("hello"),
            Some("Hello")
        );

        engine.replace_catalog_snapshot(empty);
        assert_eq!(engine.catalog_snapshot().revision(), 0);
        assert_eq!(
            engine.module_translator("@mesh/example").translate("hello"),
            None
        );
    }

    #[test]
    fn locale_snapshots_are_shared_until_the_coordinator_replaces_one() {
        let mut engine = LocaleEngine::new("en");
        let first = engine.snapshot();
        assert!(Arc::ptr_eq(&first, &engine.snapshot()));

        let mut replacement = LocaleEngine::from_snapshot(Arc::clone(&first));
        replacement.set_locale("sk");

        assert_eq!(first.current(), "en");
        assert_eq!(replacement.current(), "sk");
        assert!(!Arc::ptr_eq(&first, &replacement.snapshot()));

        engine.load_module_translations(
            "@mesh/example",
            TranslationSet {
                locale: "en".into(),
                messages: HashMap::from([(String::from("hello"), String::from("Hello"))]),
            },
        );
        assert_eq!(
            first.module_translator("@mesh/example").translate("hello"),
            None
        );
        assert_eq!(
            engine.module_translator("@mesh/example").translate("hello"),
            Some("Hello")
        );
    }

    #[test]
    fn localized_resolution_carries_provenance_and_visible_miss_policy() {
        let mut engine = LocaleEngine::new("en");
        engine.load_module_translations(
            "@mesh/example",
            TranslationSet {
                locale: "en".into(),
                messages: HashMap::from([(String::from("hello"), String::from("Hello"))]),
            },
        );

        let translator = engine.module_translator("@mesh/example");
        let translated = translator.resolve("hello", Some("Fallback"));
        assert_eq!(translated.owner_module_id, "@mesh/example");
        assert_eq!(translated.key.as_deref(), Some("hello"));
        assert_eq!(translated.text, "Hello");
        assert_eq!(translated.fallback.as_deref(), Some("Fallback"));
        assert_eq!(translated.field_path, None);
        assert!(!translated.missing);
        assert_eq!(translated.snapshot_revision, 1);
        assert_eq!(
            translated
                .source
                .as_ref()
                .map(|source| source.owner_module_id.as_str()),
            Some("@mesh/example")
        );

        let fallback = translator.resolve("missing", Some("Fallback"));
        assert_eq!(fallback.text, "Fallback");
        assert!(fallback.missing);
        assert!(fallback.source.is_none());

        let marker = translator.resolve("missing", None);
        assert_eq!(marker.text, "!!missing");
        assert!(marker.missing);
        assert_eq!(marker.snapshot_revision, 1);
        assert_eq!(
            marker
                .with_field_path("mesh.contributes.keybinds.mute.label")
                .field_path,
            Some("mesh.contributes.keybinds.mute.label".into())
        );
    }

    #[test]
    fn catalog_source_handle_rejects_escape_paths() {
        assert!(CatalogSourceHandle::new("/modules/example", "../outside.json").is_err());
        assert!(CatalogSourceHandle::new("/modules/example", "/outside.json").is_err());
        assert!(CatalogSourceHandle::new("/modules/example", "config/i18n/en.json").is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn catalog_source_handle_is_bounded_utf8_and_symlink_safe() {
        use std::os::unix::fs::symlink;

        let root =
            std::env::temp_dir().join(format!("mesh-locale-source-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("config/i18n")).unwrap();
        std::fs::write(root.join("config/i18n/en.json"), "{\"hello\":\"Hello\"}").unwrap();

        let source = CatalogSourceHandle::new(&root, "config/i18n/en.json").unwrap();
        assert_eq!(
            source.read_utf8_bounded(1024).unwrap(),
            "{\"hello\":\"Hello\"}"
        );
        assert!(matches!(
            source.read_utf8_bounded(4),
            Err(LocaleCatalogLoadError::TooLarge { .. })
        ));

        std::fs::write(root.join("config/i18n/binary.json"), [0xff, 0xfe]).unwrap();
        let binary = CatalogSourceHandle::new(&root, "config/i18n/binary.json").unwrap();
        assert!(matches!(
            binary.read_utf8_bounded(1024),
            Err(LocaleCatalogLoadError::InvalidUtf8 { .. })
        ));

        symlink(root.join("config/i18n"), root.join("link")).unwrap();
        let escaped = CatalogSourceHandle::new(&root, "link/en.json").unwrap();
        assert!(escaped.read_utf8_bounded(1024).is_err());

        symlink(
            root.join("config/i18n/en.json"),
            root.join("config/i18n/link.json"),
        )
        .unwrap();
        let linked = CatalogSourceHandle::new(&root, "config/i18n/link.json").unwrap();
        assert!(linked.read_utf8_bounded(1024).is_err());

        let directory = CatalogSourceHandle::new(&root, "config/i18n").unwrap();
        assert!(matches!(
            directory.read_utf8_bounded(1024),
            Err(LocaleCatalogLoadError::NotRegularFile { .. })
        ));

        let _ = std::fs::remove_dir_all(&root);
    }

    fn test_catalog_path(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("mesh-locale-{}-{name}.json", std::process::id()))
    }

    #[test]
    fn catalog_preparation_keeps_last_known_good_on_read_or_parse_failure() {
        let mut engine = LocaleEngine::new("en");
        engine.load_module_translations(
            "@mesh/example",
            TranslationSet {
                locale: "en".into(),
                messages: HashMap::from([(String::from("hello"), String::from("Hello"))]),
            },
        );
        let before = engine.catalog_snapshot();

        let missing = test_catalog_path("missing");
        let _ = std::fs::remove_file(&missing);
        let sources = vec![("@mesh/example".into(), "en".into(), missing.clone())];
        assert!(matches!(
            engine.prepare_module_catalog_snapshot(&sources),
            Err(LocaleCatalogLoadError::Read { .. })
        ));
        assert_eq!(engine.catalog_snapshot().revision(), before.revision());
        assert_eq!(
            engine.module_translator("@mesh/example").translate("hello"),
            Some("Hello")
        );

        let malformed = test_catalog_path("malformed");
        std::fs::write(&malformed, "{ malformed").unwrap();
        let sources = vec![("@mesh/example".into(), "en".into(), malformed.clone())];
        assert!(matches!(
            engine.prepare_module_catalog_snapshot(&sources),
            Err(LocaleCatalogLoadError::Parse { .. })
        ));
        assert_eq!(engine.catalog_snapshot().revision(), before.revision());
        let _ = std::fs::remove_file(malformed);
    }

    #[test]
    fn catalog_preparation_preserves_valid_siblings_and_reports_entry_diagnostics() {
        let path = test_catalog_path("diagnostics");
        std::fs::write(
            &path,
            serde_json::json!({
                "valid": "Visible",
                "broken": { "_plural": true, "one": "{count}" }
            })
            .to_string(),
        )
        .unwrap();
        let sources = vec![("@mesh/example".into(), "en".into(), path.clone())];
        let mut engine = LocaleEngine::new("en");
        let prepared = engine.prepare_module_catalog_snapshot(&sources).unwrap();

        assert_eq!(prepared.snapshot().revision(), 1);
        assert_eq!(prepared.diagnostics().len(), 1);
        engine.replace_catalog_snapshot(prepared.snapshot());
        assert_eq!(
            engine.module_translator("@mesh/example").translate("valid"),
            Some("Visible")
        );
        assert_eq!(
            engine
                .module_translator("@mesh/example")
                .translate("broken"),
            None
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn catalog_preparation_off_thread_returns_an_atomic_candidate() {
        let path = test_catalog_path("worker");
        std::fs::write(&path, serde_json::json!({ "hello": "Hello" }).to_string()).unwrap();
        let engine = LocaleEngine::new("en");
        let prepared = engine
            .prepare_catalog_snapshot_off_thread(
                vec![CatalogSource::module(
                    "@mesh/example",
                    "en",
                    "en",
                    path.clone(),
                )],
                HashMap::new(),
            )
            .unwrap();

        assert_eq!(prepared.snapshot().revision(), 1);
        let mut committed = engine;
        committed.replace_catalog_snapshot(prepared.snapshot());
        assert_eq!(
            committed
                .module_translator("@mesh/example")
                .translate("hello"),
            Some("Hello")
        );
        let _ = std::fs::remove_file(path);
    }
}
