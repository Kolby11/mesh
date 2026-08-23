//! Host resource discovery shared by icon, font, and settings systems.
//!
//! This crate describes what the desktop has installed. MESH resource-pack
//! modules remain semantic mapping and composition units layered above it.

mod coverage;
mod font;

pub use coverage::{
    ResourceChainSuggestion, ResourceCoverageAdvice, ResourceCoverageAdvisor, ResourceCoverageKind,
    ResourceCoverageRequest, ResourceFontScriptGap, ResourceFontScriptNeed, ResourceSemanticGap,
    ResourceSemanticNeed,
};

pub use font::{
    FontFaceBinding, FontFrontendBindings, FontPackBindings, FontRegistry, FontRegistryError,
    FontResolution, FontResolutionSource,
};

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock, RwLock};
#[cfg(unix)]
use std::{
    ffi::CString,
    os::unix::ffi::OsStrExt,
    os::unix::fs::MetadataExt,
    os::unix::io::{AsRawFd, FromRawFd},
};

pub const DEFAULT_MAX_RESOURCE_BYTES: usize = 64 * 1024 * 1024;
const RESOURCE_READ_CHUNK_BYTES: usize = 64 * 1024;

/// A shared byte budget for bounded derived-resource work.
///
/// Reservations cover work that is queued or waiting for the render-thread
/// handoff. The reservation is released automatically when the job or result
/// is dropped, so queue length cannot hide a collection of large assets.
#[derive(Debug, Clone)]
pub struct ResourceByteBudget {
    max_bytes: usize,
    used_bytes: Arc<AtomicU64>,
}

#[derive(Debug)]
pub struct ResourceByteReservation {
    budget: ResourceByteBudget,
    bytes: usize,
}

impl ResourceByteBudget {
    pub fn new(max_bytes: usize) -> Self {
        Self {
            max_bytes,
            used_bytes: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn max_bytes(&self) -> usize {
        self.max_bytes
    }

    pub fn used_bytes(&self) -> usize {
        let max_usize = u64::try_from(usize::MAX).unwrap_or(u64::MAX);
        self.used_bytes.load(Ordering::Acquire).min(max_usize) as usize
    }

    pub fn try_reserve(&self, bytes: usize) -> Option<ResourceByteReservation> {
        let bytes_u64 = u64::try_from(bytes).ok()?;
        let max_bytes = u64::try_from(self.max_bytes).ok()?;
        if bytes_u64 > max_bytes {
            return None;
        }

        let mut used = self.used_bytes.load(Ordering::Acquire);
        loop {
            let next = used.checked_add(bytes_u64)?;
            if next > max_bytes {
                return None;
            }
            match self.used_bytes.compare_exchange_weak(
                used,
                next,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => {
                    return Some(ResourceByteReservation {
                        budget: self.clone(),
                        bytes,
                    });
                }
                Err(observed) => used = observed,
            }
        }
    }
}

impl ResourceByteReservation {
    pub fn bytes(&self) -> usize {
        self.bytes
    }
}

impl Drop for ResourceByteReservation {
    fn drop(&mut self) {
        let bytes = u64::try_from(self.bytes).unwrap_or(u64::MAX);
        self.budget.used_bytes.fetch_sub(bytes, Ordering::AcqRel);
    }
}

/// Cooperative cancellation shared by an in-flight resource candidate.
///
/// Resource work is deliberately bounded by input size, but a candidate may
/// still contain many files or a large valid asset. Callers that supersede a
/// candidate can set this token; preparation checks it between files, parser
/// entries, and bounded read chunks. Cancellation never publishes a partial
/// candidate because registries commit only after preparation succeeds.
#[derive(Debug, Clone, Default)]
pub struct ResourcePreparationToken {
    cancelled: Arc<AtomicBool>,
}

impl ResourcePreparationToken {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
}

/// Owns the currently active resource-preparation generation.
///
/// Starting a new lease cancels the previous one before publishing the new
/// generation. The lease remains cheap to clone and can be moved into a
/// worker; callers must check [`ResourcePreparationLease::is_current`] before
/// committing the resulting candidate so cancellation races cannot publish an
/// older result after a newer preparation has started.
#[derive(Debug, Clone, Default)]
pub struct ResourcePreparationCoordinator {
    inner: Arc<ResourcePreparationCoordinatorInner>,
}

#[derive(Debug, Default)]
struct ResourcePreparationCoordinatorInner {
    next_generation: AtomicU64,
    active: Mutex<Option<ActiveResourcePreparation>>,
}

#[derive(Debug, Clone)]
struct ActiveResourcePreparation {
    generation: u64,
    token: ResourcePreparationToken,
}

/// A generation-scoped resource preparation lease.
#[derive(Debug, Clone)]
pub struct ResourcePreparationLease {
    coordinator: ResourcePreparationCoordinator,
    generation: u64,
    token: ResourcePreparationToken,
}

impl ResourcePreparationCoordinator {
    /// Begin a new generation and cancel any older in-flight preparation.
    pub fn begin(&self) -> ResourcePreparationLease {
        let mut active = self
            .inner
            .active
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let generation = self
            .inner
            .next_generation
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |generation| {
                Some(generation.saturating_add(1))
            })
            .expect("resource preparation generation update always returns a value")
            .saturating_add(1);
        let token = ResourcePreparationToken::new();
        if let Some(previous) = active.as_ref() {
            previous.token.cancel();
        }
        *active = Some(ActiveResourcePreparation {
            generation,
            token: token.clone(),
        });
        ResourcePreparationLease {
            coordinator: self.clone(),
            generation,
            token,
        }
    }

    /// Cancel the current generation, if one exists.
    pub fn cancel_active(&self) {
        let active = self
            .inner
            .active
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(active) = active.as_ref() {
            active.token.cancel();
        }
    }

    pub fn is_current(&self, generation: u64) -> bool {
        let active = self
            .inner
            .active
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        active
            .as_ref()
            .is_some_and(|active| active.generation == generation && !active.token.is_cancelled())
    }

    /// Retire a completed generation without affecting a newer lease.
    pub fn retire(&self, generation: u64) {
        let mut active = self
            .inner
            .active
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if active
            .as_ref()
            .is_some_and(|active| active.generation == generation)
        {
            *active = None;
        }
    }
}

impl ResourcePreparationLease {
    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn token(&self) -> &ResourcePreparationToken {
        &self.token
    }

    pub fn cancel(&self) {
        self.token.cancel();
    }

    pub fn is_current(&self) -> bool {
        self.coordinator.is_current(self.generation)
    }

    /// Retire this generation without affecting a newer lease.
    pub fn retire(&self) {
        self.coordinator.retire(self.generation);
    }
}

/// A cheap identity for a resource file. The revision token below covers
/// changes that are not visible through a single file's metadata (for
/// example, a newly installed icon in a theme directory); this fingerprint
/// avoids retaining stale bytes when an existing file is replaced.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ResourceFingerprint {
    pub len: u64,
    pub modified_nanos: u128,
    #[cfg(unix)]
    pub device: u64,
    #[cfg(unix)]
    pub inode: u64,
}

/// Return the current process-wide resource revision used by resource and
/// renderer caches.
pub fn resource_revision() -> u64 {
    RESOURCE_REVISION.load(Ordering::Acquire)
}

/// Advance the resource revision after an atomic resource/catalog change.
/// Cache keys include this value so negative lookups and derived render data
/// cannot survive a committed resource replacement.
pub fn advance_resource_revision() -> u64 {
    RESOURCE_REVISION
        .fetch_update(Ordering::AcqRel, Ordering::Acquire, |revision| {
            Some(revision.saturating_add(1))
        })
        .expect("resource revision update always returns a value")
        .saturating_add(1)
}

/// Return metadata identifying the current contents at `path`.
pub fn resource_fingerprint(path: &Path) -> Option<ResourceFingerprint> {
    let metadata = std::fs::metadata(path).ok()?;
    let modified_nanos = metadata
        .modified()
        .ok()?
        .duration_since(std::time::SystemTime::UNIX_EPOCH)
        .ok()?
        .as_nanos();
    Some(ResourceFingerprint {
        len: metadata.len(),
        modified_nanos,
        #[cfg(unix)]
        device: metadata.dev(),
        #[cfg(unix)]
        inode: metadata.ino(),
    })
}

static RESOURCE_REVISION: AtomicU64 = AtomicU64::new(1);

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
    #[error("font resource {path} has no face named '{family}'")]
    InvalidFont { path: PathBuf, family: String },
    #[error("resource preparation was cancelled while reading: {path}")]
    Cancelled { path: PathBuf },
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
        self.read_bounded_with_cancellation(max_bytes, &ResourcePreparationToken::new())
    }

    pub fn read_bounded_with_cancellation(
        &self,
        max_bytes: usize,
        cancellation: &ResourcePreparationToken,
    ) -> Result<Vec<u8>, ResourceAssetError> {
        #[cfg(unix)]
        {
            if cancellation.is_cancelled() {
                return Err(ResourceAssetError::Cancelled {
                    path: self.candidate_path(),
                });
            }
            let mut directory = open_resource_directory(&self.module_root)?;
            let mut components = self.relative_path.components().peekable();
            while let Some(component) = components.next() {
                if cancellation.is_cancelled() {
                    return Err(ResourceAssetError::Cancelled {
                        path: self.candidate_path(),
                    });
                }
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
                    return read_resource_bounded(
                        file,
                        &self.candidate_path(),
                        max_bytes,
                        cancellation,
                    );
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
            let _ = cancellation;
            Err(ResourceAssetError::UnsupportedPlatform {
                path: self.candidate_path(),
            })
        }
    }
}

/// Validate a module-bundled font face while the resource snapshot is being
/// prepared. The renderer later receives the contained handle and never has
/// to parse an untrusted font file on its paint path.
pub fn validate_font_face(
    handle: &ResourceAssetHandle,
    expected_family: &str,
) -> Result<(), ResourceAssetError> {
    validate_font_face_with_cancellation(handle, expected_family, &ResourcePreparationToken::new())
}

pub fn validate_font_face_with_cancellation(
    handle: &ResourceAssetHandle,
    expected_family: &str,
    cancellation: &ResourcePreparationToken,
) -> Result<(), ResourceAssetError> {
    if cancellation.is_cancelled() {
        return Err(ResourceAssetError::Cancelled {
            path: handle.candidate_path(),
        });
    }
    let bytes = handle.read_bounded_with_cancellation(DEFAULT_MAX_RESOURCE_BYTES, cancellation)?;
    let mut database = fontdb::Database::new();
    database.load_font_data(bytes);
    if cancellation.is_cancelled() {
        return Err(ResourceAssetError::Cancelled {
            path: handle.candidate_path(),
        });
    }
    if database.faces().any(|face| {
        face.families
            .iter()
            .any(|(family, _)| family.eq_ignore_ascii_case(expected_family))
    }) {
        Ok(())
    } else {
        Err(ResourceAssetError::InvalidFont {
            path: handle.candidate_path(),
            family: expected_family.to_owned(),
        })
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
    mut file: std::fs::File,
    path: &Path,
    max_bytes: usize,
    cancellation: &ResourcePreparationToken,
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
    let limit = max_bytes.saturating_add(1);
    let mut bytes = Vec::with_capacity(limit.min(RESOURCE_READ_CHUNK_BYTES));
    let mut chunk = [0_u8; RESOURCE_READ_CHUNK_BYTES];
    while bytes.len() < limit {
        if cancellation.is_cancelled() {
            return Err(ResourceAssetError::Cancelled {
                path: path.to_path_buf(),
            });
        }
        let amount = (limit - bytes.len()).min(chunk.len());
        let read = file
            .read(&mut chunk[..amount])
            .map_err(|source| ResourceAssetError::Read {
                path: path.to_path_buf(),
                source,
            })?;
        if read == 0 {
            break;
        }
        bytes.extend_from_slice(&chunk[..read]);
    }
    if cancellation.is_cancelled() {
        return Err(ResourceAssetError::Cancelled {
            path: path.to_path_buf(),
        });
    }
    if bytes.len() > max_bytes {
        return Err(ResourceAssetError::TooLarge {
            path: path.to_path_buf(),
            max_bytes,
        });
    }
    Ok(bytes)
}

#[derive(Debug, Clone)]
pub struct SystemResourceCatalog {
    pub revision: u64,
    /// Ordered XDG data roots used to derive icon and font lookup roots.
    pub data_dirs: Vec<PathBuf>,
    /// Ordered icon roots. Earlier roots have precedence over later roots.
    pub icon_dirs: Vec<PathBuf>,
    /// Ordered font roots used to build the shared host font database.
    pub font_dirs: Vec<PathBuf>,
    pub icon_themes: Vec<SystemIconTheme>,
    pub font_families: Vec<SystemFontFamily>,
    fingerprints: BTreeMap<PathBuf, ResourceFingerprint>,
    font_database: Arc<fontdb::Database>,
}

impl PartialEq for SystemResourceCatalog {
    fn eq(&self, other: &Self) -> bool {
        self.data_dirs == other.data_dirs
            && self.icon_dirs == other.icon_dirs
            && self.font_dirs == other.font_dirs
            && self.icon_themes == other.icon_themes
            && self.font_families == other.font_families
            && self.fingerprints == other.fingerprints
    }
}

impl Eq for SystemResourceCatalog {}

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

/// The diagnostic view of the effective resource candidate.
///
/// This is deliberately owned by the foundational resources crate rather than
/// by the shell, CLI, or LSP. Each of those consumers can therefore display
/// the same ordered chains, owner records, asset fingerprints, and structured
/// diagnostics without inventing a second resource authority.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceExplanationSnapshot {
    pub revision: u64,
    pub host_revision: u64,
    pub host: ResourceHostExplanation,
    pub icons: ResourceChainExplanation,
    pub fonts: ResourceChainExplanation,
    pub frontends: Vec<ResourceFrontendExplanation>,
    pub diagnostics: Vec<ResourceExplanationDiagnostic>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceHostExplanation {
    pub data_dirs: Vec<String>,
    pub icon_dirs: Vec<String>,
    pub font_dirs: Vec<String>,
    pub icon_themes: Vec<ResourceHostIconTheme>,
    pub font_families: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceHostIconTheme {
    pub id: String,
    pub name: String,
    pub inherits: Vec<String>,
    pub hidden: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceChainExplanation {
    /// The active ordered chain. `chain_position` is stable and starts at 0.
    pub chain: Vec<ResourcePackExplanation>,
    /// Accepted identifiers for this resource kind in the current snapshot.
    /// This includes active module packs and, for icons, visible host themes.
    pub available: Vec<String>,
    /// Contribution assets that were prepared for the active graph but are not
    /// owned by a pack mapping.
    pub contributions: Vec<ResourceAssetExplanation>,
    /// Runtime semantic-name resolutions made against this same snapshot.
    pub resolutions: Vec<ResourceResolutionExplanation>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourcePackExplanation {
    pub module_id: String,
    pub pack_id: String,
    pub chain_position: usize,
    pub status: String,
    pub assets: Vec<ResourceAssetExplanation>,
    pub mappings: Vec<ResourceMappingExplanation>,
    /// Font script coverage declared by this pack's `covers` and bundled
    /// faces. Icon packs leave this empty; semantic icon coverage is carried
    /// by `mappings`.
    #[serde(default)]
    pub script_coverage: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceAssetExplanation {
    pub id: String,
    pub path: String,
    pub fingerprint: Option<ResourceFingerprint>,
    pub prepared: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceMappingExplanation {
    pub semantic_name: String,
    pub target: String,
    pub multicolor: bool,
    pub owner_module: String,
    pub fallback_stage: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceResolutionExplanation {
    pub module_id: String,
    pub semantic_name: String,
    pub required: bool,
    pub status: String,
    pub owner_module: Option<String>,
    pub pack_id: Option<String>,
    pub candidate: Option<String>,
    pub fallback_stage: Option<String>,
    pub tried: Vec<String>,
    pub asset: Option<ResourceAssetExplanation>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceFrontendExplanation {
    pub module_id: String,
    pub icon_chain: Vec<String>,
    pub font_chain: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceExplanationDiagnostic {
    pub severity: String,
    pub code: String,
    pub module_id: Option<String>,
    pub pack_id: Option<String>,
    pub message: String,
}

impl ResourceExplanationSnapshot {
    /// Start a tooling snapshot from the same host catalog used by resource
    /// preparation. Callers then add the graph-owned chains and diagnostics.
    pub fn from_catalog(catalog: &SystemResourceCatalog) -> Self {
        let host = ResourceHostExplanation {
            data_dirs: catalog
                .data_dirs
                .iter()
                .map(|path| path.display().to_string())
                .collect(),
            icon_dirs: catalog
                .icon_dirs
                .iter()
                .map(|path| path.display().to_string())
                .collect(),
            font_dirs: catalog
                .font_dirs
                .iter()
                .map(|path| path.display().to_string())
                .collect(),
            icon_themes: catalog
                .icon_themes
                .iter()
                .map(|theme| ResourceHostIconTheme {
                    id: theme.id.clone(),
                    name: theme.name.clone(),
                    inherits: theme.inherits.clone(),
                    hidden: theme.hidden,
                })
                .collect(),
            font_families: catalog
                .font_families
                .iter()
                .map(|family| family.name.clone())
                .collect(),
        };
        let available_icons = host
            .icon_themes
            .iter()
            .filter(|theme| !theme.hidden)
            .map(|theme| theme.id.clone())
            .collect();
        Self {
            revision: catalog.revision,
            host_revision: catalog.revision,
            host,
            icons: ResourceChainExplanation {
                available: available_icons,
                ..ResourceChainExplanation::default()
            },
            fonts: ResourceChainExplanation::default(),
            frontends: Vec::new(),
            diagnostics: Vec::new(),
        }
    }

    /// Return the identifiers accepted by the runtime for a shell icon-pack
    /// setting. The order is the effective host/module discovery order.
    pub fn icon_pack_ids(&self) -> &[String] {
        &self.icons.available
    }

    pub fn font_pack_ids(&self) -> &[String] {
        &self.fonts.available
    }
}

static SYSTEM_RESOURCES: OnceLock<RwLock<Arc<SystemResourceCatalog>>> = OnceLock::new();

/// Return the current immutable host catalog. Consumers clone the `Arc`, so a
/// refresh can publish a new snapshot without invalidating an in-flight
/// lookup or renderer preparation.
pub fn system_resource_catalog() -> Arc<SystemResourceCatalog> {
    SYSTEM_RESOURCES
        .get_or_init(|| RwLock::new(Arc::new(discover_system_resources())))
        .read()
        .expect("host resource catalog lock is not poisoned")
        .clone()
}

pub fn discover_system_resources() -> SystemResourceCatalog {
    let roots = host_resource_roots();
    let font_database = discover_font_database(&roots.font_dirs);
    let icon_themes = discover_icon_themes_in(&roots.icon_dirs);
    let fingerprints = catalog_fingerprints(&roots, &icon_themes, &font_database);
    SystemResourceCatalog {
        revision: resource_revision(),
        data_dirs: roots.data_dirs,
        icon_themes,
        font_families: discover_font_families(&font_database),
        icon_dirs: roots.icon_dirs,
        font_dirs: roots.font_dirs,
        fingerprints,
        font_database,
    }
}

/// Refresh the process-wide host snapshot when the ordered roots, installed
/// themes, or available font faces changed. An unchanged refresh retains the
/// existing revision and allocation.
pub fn refresh_system_resource_catalog() -> Arc<SystemResourceCatalog> {
    let store = SYSTEM_RESOURCES.get_or_init(|| RwLock::new(Arc::new(discover_system_resources())));
    let mut current = store
        .write()
        .expect("host resource catalog lock is not poisoned");
    let mut candidate = discover_system_resources();
    if current.as_ref() == &candidate {
        return current.clone();
    }
    candidate.revision = advance_resource_revision();
    let next = Arc::new(candidate);
    *current = next.clone();
    next
}

impl SystemResourceCatalog {
    /// Reuse the exact font database that produced `font_families` rather than
    /// invoking font discovery again in a consumer-specific path.
    pub fn font_database(&self) -> fontdb::Database {
        self.font_database.as_ref().clone()
    }

    /// Resolve an installed family to one of the file-backed faces in this
    /// snapshot. Missing families remain missing; there is no fontconfig
    /// fallback subprocess hidden behind this lookup.
    pub fn font_path_for_family(&self, family: &str) -> Option<PathBuf> {
        self.font_database.faces().find_map(|face| {
            if !face
                .families
                .iter()
                .any(|(candidate, _)| candidate.eq_ignore_ascii_case(family))
            {
                return None;
            }
            match &face.source {
                fontdb::Source::File(path) => Some(path.clone()),
                _ => None,
            }
        })
    }
}

fn discover_font_database(font_dirs: &[PathBuf]) -> Arc<fontdb::Database> {
    let mut database = fontdb::Database::new();
    // Load explicit XDG roots first so a user-local face wins when the same
    // family is also present in a system directory. `load_system_fonts` then
    // fills in platform-specific roots that are not represented by XDG.
    for font_dir in font_dirs {
        database.load_fonts_dir(font_dir);
    }
    database.load_system_fonts();
    Arc::new(database)
}

fn discover_font_families(database: &fontdb::Database) -> Vec<SystemFontFamily> {
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

fn catalog_fingerprints(
    roots: &HostResourceRoots,
    icon_themes: &[SystemIconTheme],
    font_database: &fontdb::Database,
) -> BTreeMap<PathBuf, ResourceFingerprint> {
    let mut paths = roots
        .data_dirs
        .iter()
        .chain(&roots.icon_dirs)
        .chain(&roots.font_dirs)
        .cloned()
        .collect::<HashSet<_>>();
    paths.extend(
        icon_themes
            .iter()
            .map(|theme| theme.path.join("index.theme")),
    );
    paths.extend(font_database.faces().filter_map(|face| match &face.source {
        fontdb::Source::File(path) => Some(path.clone()),
        _ => None,
    }));
    paths
        .into_iter()
        .filter_map(|path| resource_fingerprint(&path).map(|fingerprint| (path, fingerprint)))
        .collect()
}

/// FreeDesktop icon base directories in the current host-catalog order.
/// Both catalog discovery and icon resolution use this one authority.
pub fn xdg_icon_base_dirs() -> Vec<PathBuf> {
    host_resource_roots().icon_dirs
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostResourceRoots {
    pub data_dirs: Vec<PathBuf>,
    pub icon_dirs: Vec<PathBuf>,
    pub font_dirs: Vec<PathBuf>,
}

fn host_resource_roots() -> HostResourceRoots {
    let home = absolute_env_path("HOME");
    let data_home = absolute_env_path("XDG_DATA_HOME")
        .or_else(|| home.as_ref().map(|home| home.join(".local/share")));
    let data_dirs = absolute_env_paths("XDG_DATA_DIRS").unwrap_or_else(|| {
        vec![
            PathBuf::from("/usr/local/share"),
            PathBuf::from("/usr/share"),
        ]
    });

    let mut ordered_data_dirs = Vec::new();
    if let Some(data_home) = data_home {
        ordered_data_dirs.push(data_home);
    }
    ordered_data_dirs.extend(data_dirs);
    let data_dirs = existing_unique_paths(ordered_data_dirs);

    let mut icon_candidates = Vec::new();
    if let Some(data_home) = data_dirs.first() {
        icon_candidates.push(data_home.join("icons"));
    }
    if let Some(home) = &home {
        icon_candidates.push(home.join(".icons"));
    }
    icon_candidates.extend(data_dirs.iter().map(|dir| dir.join("icons")));

    let mut font_candidates = Vec::new();
    if let Some(data_home) = data_dirs.first() {
        font_candidates.push(data_home.join("fonts"));
    }
    if let Some(home) = &home {
        font_candidates.push(home.join(".fonts"));
    }
    font_candidates.extend(data_dirs.iter().map(|dir| dir.join("fonts")));

    HostResourceRoots {
        data_dirs,
        icon_dirs: existing_unique_paths(icon_candidates),
        font_dirs: existing_unique_paths(font_candidates),
    }
}

fn existing_unique_paths(paths: impl IntoIterator<Item = PathBuf>) -> Vec<PathBuf> {
    let mut seen = HashSet::new();
    paths
        .into_iter()
        .filter(|path| path.is_dir() && seen.insert(path.clone()))
        .collect()
}

fn absolute_env_path(name: &str) -> Option<PathBuf> {
    let path = PathBuf::from(non_empty_env_value(std::env::var_os(name))?);
    path.is_absolute().then_some(path)
}

fn absolute_env_paths(name: &str) -> Option<Vec<PathBuf>> {
    let value = non_empty_env_value(std::env::var_os(name))?;
    let paths = std::env::split_paths(&value)
        .filter(|path| path.is_absolute())
        .collect::<Vec<_>>();
    (!paths.is_empty()).then_some(paths)
}

fn non_empty_env_value(value: Option<std::ffi::OsString>) -> Option<std::ffi::OsString> {
    value.filter(|value| !value.is_empty())
}

fn discover_icon_themes_in(base_dirs: &[PathBuf]) -> Vec<SystemIconTheme> {
    let mut themes = Vec::new();
    let mut seen_ids = HashSet::new();
    for base in base_dirs {
        let Ok(mut entries) = std::fs::read_dir(base) else {
            continue;
        };
        let mut paths = entries
            .by_ref()
            .flatten()
            .map(|entry| entry.path())
            .collect::<Vec<_>>();
        paths.sort();
        for path in paths {
            let Some(theme) = read_icon_theme(&path) else {
                continue;
            };
            // XDG directory precedence is user first, then system. Preserve
            // the first definition when the same theme id occurs in both.
            if seen_ids.insert(theme.id.clone()) {
                themes.push(theme);
            }
        }
    }
    themes
}

fn read_icon_theme(path: &Path) -> Option<SystemIconTheme> {
    let id = path.file_name()?.to_str()?.to_owned();
    let raw = std::fs::read_to_string(path.join("index.theme")).ok()?;
    let mut in_icon_theme = false;
    let mut name = None;
    let mut inherits = Vec::new();
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
            "Inherits" => {
                for inherited in value
                    .split(',')
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                {
                    if !inherits.iter().any(|existing| existing == inherited) {
                        inherits.push(inherited.to_owned());
                    }
                }
            }
            "Hidden" => hidden = value.trim().eq_ignore_ascii_case("true"),
            _ => {}
        }
    }

    Some(SystemIconTheme {
        name: name.unwrap_or_else(|| id.clone()),
        id,
        path: path.to_owned(),
        inherits,
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
    fn resource_revision_advances_and_fingerprints_existing_files() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("resource.bin");
        std::fs::write(&path, b"resource").unwrap();

        let first_revision = resource_revision();
        let first_fingerprint = resource_fingerprint(&path).unwrap();
        let next_revision = advance_resource_revision();

        assert_eq!(next_revision, first_revision.saturating_add(1));
        assert_eq!(resource_revision(), next_revision);
        assert_eq!(resource_fingerprint(&path), Some(first_fingerprint));
        assert!(resource_fingerprint(&temp.path().join("missing")).is_none());
    }

    #[test]
    fn discovered_catalog_carries_the_resource_revision() {
        let catalog = discover_system_resources();
        assert!(catalog.revision <= resource_revision());
    }

    #[test]
    fn unchanged_host_catalog_refresh_reuses_the_snapshot() {
        let first = system_resource_catalog();
        let second = refresh_system_resource_catalog();
        assert!(Arc::ptr_eq(&first, &second));
    }

    #[test]
    fn catalog_font_lookup_uses_a_file_backed_face() {
        let catalog = discover_system_resources();
        let (family, path) = catalog
            .font_database()
            .faces()
            .find_map(|face| match (&face.families.first(), &face.source) {
                (Some((family, _)), fontdb::Source::File(path)) => {
                    Some((family.clone(), path.clone()))
                }
                _ => None,
            })
            .expect("test environment should provide a file-backed font");
        assert_eq!(catalog.font_path_for_family(&family), Some(path));
    }

    #[test]
    fn catalog_fingerprints_detect_changed_theme_metadata() {
        let temp = tempfile::tempdir().unwrap();
        let icon_root = temp.path().join("icons");
        let theme = icon_root.join("ocean");
        std::fs::create_dir_all(&theme).unwrap();
        let index = theme.join("index.theme");
        std::fs::write(&index, "[Icon Theme]\nName=Ocean\n").unwrap();
        let roots = HostResourceRoots {
            data_dirs: vec![temp.path().to_path_buf()],
            icon_dirs: vec![icon_root],
            font_dirs: Vec::new(),
        };
        let database = fontdb::Database::new();
        let before = catalog_fingerprints(
            &roots,
            &discover_icon_themes_in(&roots.icon_dirs),
            &database,
        );
        std::fs::write(&index, "[Icon Theme]\nName=Ocean Updated\n").unwrap();
        let after = catalog_fingerprints(
            &roots,
            &discover_icon_themes_in(&roots.icon_dirs),
            &database,
        );
        assert_ne!(before, after);
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
        assert_eq!(themes[0].inherits, ["hicolor", "Adwaita"]);
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

    #[test]
    fn cancelled_resource_reads_fail_before_candidate_bytes_are_returned() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("asset.bin"), vec![0_u8; 128 * 1024]).unwrap();
        let handle = ResourceAssetHandle::new(temp.path(), "asset.bin").unwrap();
        let cancellation = ResourcePreparationToken::new();
        cancellation.cancel();

        assert!(matches!(
            handle.read_bounded_with_cancellation(256 * 1024, &cancellation),
            Err(ResourceAssetError::Cancelled { .. })
        ));
    }

    #[test]
    fn preparation_coordinator_supersedes_only_the_active_generation() {
        let coordinator = ResourcePreparationCoordinator::default();
        let first = coordinator.begin();
        assert_eq!(first.generation(), 1);
        assert!(first.is_current());

        let second = coordinator.begin();
        assert!(first.token().is_cancelled());
        assert!(!first.is_current());
        assert!(second.generation() > first.generation());
        assert!(second.is_current());

        first.retire();
        assert!(second.is_current());

        coordinator.cancel_active();
        assert!(second.token().is_cancelled());
        assert!(!second.is_current());
    }

    #[test]
    fn byte_budget_rejects_overcommit_and_releases_reservations() {
        let budget = ResourceByteBudget::new(10);
        let first = budget.try_reserve(6).expect("first reservation fits");
        assert_eq!(first.bytes(), 6);
        assert_eq!(budget.used_bytes(), 6);
        assert!(budget.try_reserve(5).is_none());

        let second = budget.try_reserve(4).expect("remaining budget fits");
        assert_eq!(budget.used_bytes(), 10);
        drop(first);
        assert_eq!(budget.used_bytes(), 4);
        drop(second);
        assert_eq!(budget.used_bytes(), 0);
    }

    #[test]
    fn byte_budget_zero_reservations_are_accounted_safely() {
        let budget = ResourceByteBudget::new(0);
        let reservation = budget.try_reserve(0).expect("zero work is harmless");
        assert_eq!(reservation.bytes(), 0);
        assert_eq!(budget.used_bytes(), 0);
        assert!(budget.try_reserve(1).is_none());
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

    #[test]
    fn bundled_font_validation_rejects_malformed_face_data() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("font.ttf"), b"not a font").unwrap();
        let handle = ResourceAssetHandle::new(temp.path(), "font.ttf").unwrap();
        assert!(matches!(
            validate_font_face(&handle, "Inter"),
            Err(ResourceAssetError::InvalidFont { .. })
        ));
    }
}
