//! Token-based theme engine.
//!
//! Themes define design tokens across standard groups — colors, typography,
//! spacing, radius, elevation, borders, motion, shadows — which components
//! inherit from the active theme.
pub mod css;

use serde::de::{MapAccess, Visitor};
use serde::ser::SerializeMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

#[cfg(unix)]
use std::ffi::CString;
#[cfg(unix)]
use std::os::unix::ffi::OsStrExt;
#[cfg(unix)]
use std::os::unix::io::{AsRawFd, FromRawFd};

const LEGACY_DEFAULT_SHELL_ANIMATION_PREFIX: &str = "animation.default.";
pub const DEFAULT_MAX_THEME_SOURCE_BYTES: usize = 2 * 1024 * 1024;
static NEXT_THEME_REVISION: AtomicU64 = AtomicU64::new(1);

fn next_theme_revision() -> u64 {
    NEXT_THEME_REVISION.fetch_add(1, Ordering::Relaxed)
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ComponentDefaults {
    declarations: Vec<(String, String)>,
}

impl ComponentDefaults {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, property: String, value: String) -> Option<String> {
        let previous = self.remove(&property);
        self.declarations.push((property, value));
        previous
    }

    pub fn get(&self, property: &str) -> Option<&String> {
        self.declarations
            .iter()
            .find_map(|(name, value)| (name == property).then_some(value))
    }

    pub fn contains_key(&self, property: &str) -> bool {
        self.get(property).is_some()
    }

    pub fn is_empty(&self) -> bool {
        self.declarations.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&String, &String)> {
        self.declarations
            .iter()
            .map(|(property, value)| (property, value))
    }

    fn remove(&mut self, property: &str) -> Option<String> {
        let index = self
            .declarations
            .iter()
            .position(|(name, _)| name == property)?;
        Some(self.declarations.remove(index).1)
    }
}

impl Extend<(String, String)> for ComponentDefaults {
    fn extend<T: IntoIterator<Item = (String, String)>>(&mut self, iter: T) {
        for (property, value) in iter {
            self.insert(property, value);
        }
    }
}

impl FromIterator<(String, String)> for ComponentDefaults {
    fn from_iter<T: IntoIterator<Item = (String, String)>>(iter: T) -> Self {
        let mut defaults = Self::new();
        defaults.extend(iter);
        defaults
    }
}

impl IntoIterator for ComponentDefaults {
    type Item = (String, String);
    type IntoIter = std::vec::IntoIter<(String, String)>;

    fn into_iter(self) -> Self::IntoIter {
        self.declarations.into_iter()
    }
}

pub struct ComponentDefaultsIter<'a> {
    iter: std::slice::Iter<'a, (String, String)>,
}

impl<'a> Iterator for ComponentDefaultsIter<'a> {
    type Item = (&'a String, &'a String);

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next().map(|(property, value)| (property, value))
    }
}

impl<'a> IntoIterator for &'a ComponentDefaults {
    type Item = (&'a String, &'a String);
    type IntoIter = ComponentDefaultsIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        ComponentDefaultsIter {
            iter: self.declarations.iter(),
        }
    }
}

impl Serialize for ComponentDefaults {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut map = serializer.serialize_map(Some(self.declarations.len()))?;
        for (property, value) in &self.declarations {
            map.serialize_entry(property, value)?;
        }
        map.end()
    }
}

impl<'de> Deserialize<'de> for ComponentDefaults {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct ComponentDefaultsVisitor;

        impl<'de> Visitor<'de> for ComponentDefaultsVisitor {
            type Value = ComponentDefaults;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a map of CSS properties to values")
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                let mut defaults = ComponentDefaults::new();
                while let Some((property, value)) = map.next_entry::<String, String>()? {
                    defaults.insert(property, value);
                }
                Ok(defaults)
            }
        }

        deserializer.deserialize_map(ComponentDefaultsVisitor)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TokenValue {
    String(String),
    Number(f64),
    Bool(bool),
}

impl std::fmt::Display for TokenValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::String(s) => write!(f, "{s}"),
            Self::Number(n) => write!(f, "{n}"),
            Self::Bool(b) => write!(f, "{b}"),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ThemeDefaults {
    #[serde(default)]
    pub components: HashMap<String, ComponentDefaults>,
}

/// A timeline offset in `[0.0, 1.0]` plus the raw declarations at that stop.
/// Stored uninterpreted; consumers resolve `var()` and lower the properties.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ThemeKeyframeStop {
    pub offset: f32,
    #[serde(default)]
    pub declarations: ComponentDefaults,
    /// Optional segment-local timing function, kept raw so the shell can
    /// resolve theme token references before lowering it to the animation
    /// engine's validated easing type.
    #[serde(default)]
    pub easing: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ThemeModule {
    #[serde(default)]
    pub tokens: HashMap<String, TokenValue>,
    #[serde(default)]
    pub defaults: ThemeDefaults,
    #[serde(default)]
    pub rules: Vec<ThemeStyleRule>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeStyleRule {
    pub selector: css::Selector,
    pub declarations: ComponentDefaults,
}

/// The layer which supplied an effective theme value.
///
/// Provenance is kept on the composed snapshot rather than inferred from the
/// winning value. This makes diagnostics and tooling able to explain why a
/// value won, even when two layers happen to contain the same value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThemeProvenance {
    BaseRecovery,
    ThemePack { id: String, mode: String },
    ModuleContribution { module_id: String },
    UserOverride,
}

/// Rendering metadata selected together with a theme mode.
///
/// These values are explicit inputs to the rendered snapshot. Consumers must
/// not infer color scheme or contrast from a theme identifier or display name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThemeMetadata {
    pub mode: String,
    pub color_scheme: String,
    pub contrast: String,
}

impl Default for ThemeMetadata {
    fn default() -> Self {
        Self {
            mode: "default".into(),
            color_scheme: "unknown".into(),
            contrast: "normal".into(),
        }
    }
}

impl ThemeMetadata {
    pub fn new(
        mode: impl Into<String>,
        color_scheme: impl Into<String>,
        contrast: impl Into<String>,
    ) -> Self {
        Self {
            mode: normalize_metadata_value(mode.into(), "default"),
            color_scheme: normalize_metadata_value(color_scheme.into(), "unknown"),
            contrast: normalize_metadata_value(contrast.into(), "normal"),
        }
    }
}

/// Policy used to choose a declared theme mode before the normal theme
/// prepare/commit transaction. The policy is configuration, not a second
/// rendering path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ThemeModePolicy {
    Manual,
    FollowSystem,
    Scheduled { entries: Vec<ThemeModeSchedule> },
}

impl Default for ThemeModePolicy {
    fn default() -> Self {
        Self::Manual
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThemeModeSchedule {
    /// Local wall-clock time in `HH:MM` form at which this mode starts.
    pub at: String,
    pub mode: String,
}

impl ThemeModePolicy {
    /// Resolve one declared mode from the policy inputs. Keeping this pure
    /// makes manual settings, system changes, and schedule ticks share the
    /// same candidate preparation path in the shell.
    pub fn select_mode(
        &self,
        modes: &BTreeMap<String, ThemeModeDescriptor>,
        default_mode: &str,
        explicit_mode: Option<&str>,
        system_color_scheme: Option<&str>,
        local_minute: u16,
    ) -> Result<String, String> {
        if !modes.contains_key(default_mode) {
            return Err(format!(
                "theme policy default mode '{default_mode}' is not declared"
            ));
        }

        match self {
            Self::Manual => {
                let mode = explicit_mode.unwrap_or(default_mode);
                modes
                    .contains_key(mode)
                    .then(|| mode.to_string())
                    .ok_or_else(|| format!("theme has no mode '{mode}'"))
            }
            Self::FollowSystem => {
                let Some(system_color_scheme) = system_color_scheme
                    .map(str::trim)
                    .filter(|scheme| !scheme.is_empty())
                else {
                    return Ok(default_mode.to_string());
                };
                Ok(modes
                    .iter()
                    .find(|(_, descriptor)| {
                        descriptor
                            .metadata
                            .color_scheme
                            .eq_ignore_ascii_case(system_color_scheme)
                    })
                    .map(|(mode, _)| mode.clone())
                    .unwrap_or_else(|| default_mode.to_string()))
            }
            Self::Scheduled { entries } => {
                if entries.is_empty() {
                    return Err("theme schedule must contain at least one entry".into());
                }
                let mut parsed = entries
                    .iter()
                    .map(|entry| {
                        let minute = parse_clock_minute(&entry.at)?;
                        if !modes.contains_key(&entry.mode) {
                            return Err(format!(
                                "theme schedule mode '{}' is not declared",
                                entry.mode
                            ));
                        }
                        Ok((minute, entry))
                    })
                    .collect::<Result<Vec<_>, String>>()?;
                parsed.sort_by_key(|(minute, _)| *minute);
                for pair in parsed.windows(2) {
                    if pair[0].0 == pair[1].0 {
                        return Err(format!(
                            "theme schedule contains duplicate time {:02}:{:02}",
                            pair[0].0 / 60,
                            pair[0].0 % 60
                        ));
                    }
                }
                let Some((_, selected)) = parsed
                    .iter()
                    .rev()
                    .find(|(minute, _)| *minute <= local_minute % (24 * 60))
                    .or_else(|| parsed.last())
                else {
                    return Ok(default_mode.to_string());
                };
                Ok(selected.mode.clone())
            }
        }
    }
}

fn parse_clock_minute(value: &str) -> Result<u16, String> {
    let (hour, minute) = value
        .trim()
        .split_once(':')
        .ok_or_else(|| format!("theme schedule time '{value}' must use HH:MM"))?;
    let hour = hour
        .parse::<u16>()
        .map_err(|_| format!("theme schedule time '{value}' has an invalid hour"))?;
    let minute = minute
        .parse::<u16>()
        .map_err(|_| format!("theme schedule time '{value}' has an invalid minute"))?;
    if hour >= 24 || minute >= 60 {
        return Err(format!(
            "theme schedule time '{value}' is outside 24-hour time"
        ));
    }
    Ok(hour * 60 + minute)
}

/// Current local wall-clock minute for schedule policy evaluation.
#[cfg(unix)]
pub fn local_minutes_since_midnight() -> u16 {
    let now = unsafe { libc::time(std::ptr::null_mut()) };
    let mut local = unsafe { std::mem::zeroed::<libc::tm>() };
    let result = unsafe { libc::localtime_r(&now, &mut local) };
    if result.is_null() {
        return 0;
    }
    (local.tm_hour.clamp(0, 23) as u16) * 60 + local.tm_min.clamp(0, 59) as u16
}

#[cfg(not(unix))]
pub fn local_minutes_since_midnight() -> u16 {
    0
}

/// Immutable, serializable description of the exact theme state used by the
/// renderer. The shell publishes this value as the authoritative `mesh.theme`
/// state and as the payload of revisioned theme-change events.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ThemeSnapshot {
    pub id: String,
    pub name: String,
    pub mode: String,
    pub color_scheme: String,
    pub contrast: String,
    pub revision: u64,
    pub tokens: HashMap<String, TokenValue>,
    pub provenance: BTreeMap<String, ThemeProvenance>,
}

impl ThemeSnapshot {
    /// Return token identities whose effective value or winning layer differs
    /// from `previous` in deterministic order.
    pub fn changed_token_names(&self, previous: &ThemeSnapshot) -> Vec<String> {
        let names = self
            .tokens
            .keys()
            .chain(previous.tokens.keys())
            .cloned()
            .collect::<std::collections::BTreeSet<_>>();
        names
            .into_iter()
            .filter(|name| {
                self.tokens.get(name) != previous.tokens.get(name)
                    || self.provenance.get(name) != previous.provenance.get(name)
            })
            .collect()
    }
}

fn normalize_metadata_value(value: String, fallback: &str) -> String {
    let value = value.trim();
    if value.is_empty() {
        fallback.into()
    } else {
        value.into()
    }
}

#[derive(Debug, Clone)]
pub struct ThemeModuleLayer {
    pub module_id: String,
    pub module: ThemeModule,
}

/// A source selected by an installed theme contribution.
///
/// The handle deliberately stores the owning module root separately from the
/// manifest-relative path. Callers must obtain it from the graph catalog;
/// free-form theme IDs never become filesystem paths. Opening the handle
/// safely against symlink and race attacks is a separate I/O boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThemeSourceHandle {
    module_root: PathBuf,
    relative_path: PathBuf,
}

impl ThemeSourceHandle {
    pub fn new(
        module_root: impl Into<PathBuf>,
        relative_path: impl AsRef<Path>,
    ) -> Result<Self, String> {
        let module_root = module_root.into();
        if module_root.as_os_str().is_empty() {
            return Err("theme source module root cannot be empty".into());
        }
        let relative_path = relative_path.as_ref();
        if relative_path.as_os_str().is_empty() || relative_path.is_absolute() {
            return Err(format!(
                "theme source path must be a non-empty relative path: {}",
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
                "theme source path contains an unsafe component: {}",
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

    /// Return the lexical candidate path for the later contained-open step.
    pub fn candidate_path(&self) -> PathBuf {
        self.module_root.join(&self.relative_path)
    }

    /// Open the selected source beneath its owning module root without
    /// following symlinks in the root, intermediate directories, or file.
    pub fn read_utf8_bounded(&self, max_bytes: usize) -> Result<String, ThemeSourceError> {
        #[cfg(unix)]
        {
            let mut directory = open_directory(&self.module_root)?;
            let mut components = self.relative_path.components().peekable();
            while let Some(component) = components.next() {
                let Component::Normal(component) = component else {
                    return Err(ThemeSourceError::UnsafePath {
                        path: self.candidate_path(),
                    });
                };
                if components.peek().is_some() {
                    directory = open_directory_at(&directory, component, &self.candidate_path())?;
                } else {
                    let file = open_file_at(&directory, component, &self.candidate_path())?;
                    return read_bounded_utf8(file, &self.candidate_path(), max_bytes);
                }
            }
            Err(ThemeSourceError::UnsafePath {
                path: self.candidate_path(),
            })
        }
        #[cfg(not(unix))]
        {
            let _ = max_bytes;
            Err(ThemeSourceError::UnsupportedPlatform {
                path: self.candidate_path(),
            })
        }
    }

    /// Return a stable content fingerprint for change detection without
    /// turning the source handle into a caller-controlled filesystem path.
    pub fn fingerprint(&self) -> Result<u64, ThemeSourceError> {
        Ok(fingerprint_bytes(
            self.read_utf8_bounded(DEFAULT_MAX_THEME_SOURCE_BYTES)?
                .as_bytes(),
        ))
    }
}

/// Stable non-cryptographic fingerprint for watcher/change-detection use.
/// Module provenance and safe source opening provide the trust boundary; this
/// value only avoids treating an unchanged mtime as an unchanged source.
pub fn fingerprint_bytes(bytes: &[u8]) -> u64 {
    const FNV_OFFSET: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;
    bytes.iter().fold(FNV_OFFSET, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(FNV_PRIME)
    })
}

#[derive(Debug, thiserror::Error)]
pub enum ThemeSourceError {
    #[error("I/O error opening theme source {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("theme source is not a regular file: {path}")]
    NotRegularFile { path: PathBuf },
    #[error("theme source exceeds {max_bytes} bytes: {path}")]
    TooLarge { path: PathBuf, max_bytes: usize },
    #[error("theme source is not valid UTF-8: {path}")]
    InvalidUtf8 { path: PathBuf },
    #[error("theme source path is unsafe: {path}")]
    UnsafePath { path: PathBuf },
    #[error("safe theme source opening is unavailable on this platform: {path}")]
    UnsupportedPlatform { path: PathBuf },
}

#[cfg(unix)]
fn open_directory(path: &Path) -> Result<std::fs::File, ThemeSourceError> {
    let name =
        CString::new(path.as_os_str().as_bytes()).map_err(|_| ThemeSourceError::UnsafePath {
            path: path.to_path_buf(),
        })?;
    let fd = unsafe {
        libc::open(
            name.as_ptr(),
            libc::O_RDONLY | libc::O_CLOEXEC | libc::O_DIRECTORY | libc::O_NOFOLLOW,
        )
    };
    if fd < 0 {
        return Err(ThemeSourceError::Io {
            path: path.to_path_buf(),
            source: std::io::Error::last_os_error(),
        });
    }
    Ok(unsafe { std::fs::File::from_raw_fd(fd) })
}

#[cfg(unix)]
fn open_directory_at(
    parent: &std::fs::File,
    component: &std::ffi::OsStr,
    path: &Path,
) -> Result<std::fs::File, ThemeSourceError> {
    let name = CString::new(component.as_bytes()).map_err(|_| ThemeSourceError::UnsafePath {
        path: path.to_path_buf(),
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
        return Err(ThemeSourceError::Io {
            path: path.to_path_buf(),
            source: std::io::Error::last_os_error(),
        });
    }
    Ok(unsafe { std::fs::File::from_raw_fd(fd) })
}

#[cfg(unix)]
fn open_file_at(
    parent: &std::fs::File,
    component: &std::ffi::OsStr,
    path: &Path,
) -> Result<std::fs::File, ThemeSourceError> {
    let name = CString::new(component.as_bytes()).map_err(|_| ThemeSourceError::UnsafePath {
        path: path.to_path_buf(),
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
        return Err(ThemeSourceError::Io {
            path: path.to_path_buf(),
            source: std::io::Error::last_os_error(),
        });
    }
    Ok(unsafe { std::fs::File::from_raw_fd(fd) })
}

#[cfg(unix)]
fn read_bounded_utf8(
    file: std::fs::File,
    path: &Path,
    max_bytes: usize,
) -> Result<String, ThemeSourceError> {
    if !file
        .metadata()
        .map_err(|source| ThemeSourceError::Io {
            path: path.to_path_buf(),
            source,
        })?
        .file_type()
        .is_file()
    {
        return Err(ThemeSourceError::NotRegularFile {
            path: path.to_path_buf(),
        });
    }
    let mut bytes = Vec::new();
    file.take(max_bytes.saturating_add(1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|source| ThemeSourceError::Io {
            path: path.to_path_buf(),
            source,
        })?;
    if bytes.len() > max_bytes {
        return Err(ThemeSourceError::TooLarge {
            path: path.to_path_buf(),
            max_bytes,
        });
    }
    String::from_utf8(bytes).map_err(|_| ThemeSourceError::InvalidUtf8 {
        path: path.to_path_buf(),
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThemeModeDescriptor {
    pub name: String,
    pub source: ThemeSourceHandle,
    pub metadata: ThemeMetadata,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThemePackDescriptor {
    /// Canonical graph-scoped identity (`owner-module:local-id`).
    pub id: String,
    pub owner_module: String,
    pub local_id: String,
    pub label: Option<String>,
    pub modes: BTreeMap<String, ThemeModeDescriptor>,
    pub default_mode: String,
}

impl ThemePackDescriptor {
    pub fn new(
        id: impl Into<String>,
        owner_module: impl Into<String>,
        local_id: impl Into<String>,
        label: Option<String>,
        module_root: impl Into<PathBuf>,
        modes: impl IntoIterator<Item = (String, String)>,
        default_mode: Option<String>,
    ) -> Result<Self, String> {
        let id = id.into();
        let owner_module = owner_module.into();
        let local_id = local_id.into();
        if id.trim().is_empty() || owner_module.trim().is_empty() || local_id.trim().is_empty() {
            return Err("theme descriptor identity and owner cannot be empty".into());
        }

        let module_root = module_root.into();
        let mut descriptors = BTreeMap::new();
        for (name, path) in modes {
            if name.trim().is_empty() {
                return Err(format!("theme {id} contains an empty mode name"));
            }
            if descriptors.contains_key(&name) {
                return Err(format!("theme {id} contains duplicate mode {name}"));
            }
            let source = ThemeSourceHandle::new(&module_root, path)?;
            descriptors.insert(
                name.clone(),
                ThemeModeDescriptor {
                    name,
                    source,
                    metadata: ThemeMetadata::default(),
                },
            );
        }
        if descriptors.is_empty() {
            return Err(format!("theme {id} must declare at least one mode"));
        }
        let default_mode = default_mode
            .or_else(|| descriptors.keys().next().cloned())
            .ok_or_else(|| format!("theme {id} has no default mode"))?;
        if !descriptors.contains_key(&default_mode) {
            return Err(format!(
                "theme {id} default mode {default_mode} is not declared"
            ));
        }

        Ok(Self {
            id,
            owner_module,
            local_id,
            label,
            modes: descriptors,
            default_mode,
        })
    }

    /// Attach explicit color-scheme and contrast metadata to declared modes.
    /// Unknown mode names are rejected so metadata cannot silently describe a
    /// different source than the one selected by the graph.
    pub fn with_mode_metadata(
        mut self,
        metadata: impl IntoIterator<Item = (String, ThemeMetadata)>,
    ) -> Result<Self, String> {
        for (mode, mut value) in metadata {
            let Some(descriptor) = self.modes.get_mut(&mode) else {
                return Err(format!(
                    "theme {} metadata names undeclared mode {mode}",
                    self.id
                ));
            };
            if value.color_scheme.trim().is_empty() {
                return Err(format!(
                    "theme {} mode {mode} has an empty color scheme",
                    self.id
                ));
            }
            if value.contrast.trim().is_empty() {
                return Err(format!(
                    "theme {} mode {mode} has an empty contrast",
                    self.id
                ));
            }
            value.mode = mode.clone();
            descriptor.metadata = value;
        }
        for (mode, descriptor) in &mut self.modes {
            if descriptor.metadata.mode == "default" {
                descriptor.metadata.mode = mode.clone();
            }
        }
        Ok(self)
    }

    pub fn mode(&self, name: &str) -> Option<&ThemeModeDescriptor> {
        self.modes.get(name)
    }

    pub fn default_source(&self) -> &ThemeSourceHandle {
        &self.modes[&self.default_mode].source
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ThemeCatalog {
    descriptors: BTreeMap<String, ThemePackDescriptor>,
}

impl ThemeCatalog {
    pub fn from_descriptors(
        descriptors: impl IntoIterator<Item = ThemePackDescriptor>,
    ) -> Result<Self, String> {
        let mut catalog = Self::default();
        for descriptor in descriptors {
            if catalog
                .descriptors
                .insert(descriptor.id.clone(), descriptor)
                .is_some()
            {
                return Err("theme catalog contains duplicate scoped identity".into());
            }
        }
        Ok(catalog)
    }

    pub fn get(&self, id: &str) -> Option<&ThemePackDescriptor> {
        self.descriptors.get(id)
    }

    pub fn iter(&self) -> impl Iterator<Item = &ThemePackDescriptor> {
        self.descriptors.values()
    }

    pub fn is_empty(&self) -> bool {
        self.descriptors.is_empty()
    }

    pub fn len(&self) -> usize {
        self.descriptors.len()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Theme {
    pub id: String,
    pub name: String,
    #[serde(default)]
    tokens: HashMap<String, TokenValue>,
    #[serde(default)]
    defaults: ThemeDefaults,
    #[serde(default)]
    pub keyframes: HashMap<String, Vec<ThemeKeyframeStop>>,
    #[serde(default)]
    metadata: ThemeMetadata,
    #[serde(default)]
    modules: HashMap<String, ThemeModule>,
    #[serde(default)]
    rules: Vec<ThemeStyleRule>,
    /// Effective-value provenance for a composed snapshot. It is deliberately
    /// not serialized because it describes the current graph/settings inputs,
    /// not portable theme-file content.
    #[serde(skip, default)]
    provenance: BTreeMap<String, ThemeProvenance>,
    /// Monotonic identity for the style-bearing data, retained across clones
    /// so consumers can share derived style caches. Every mutable accessor
    /// advances it, so an in-place edit cannot reuse stale lowered values.
    #[serde(skip, default = "next_theme_revision")]
    revision: u64,
}

impl Theme {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            tokens: HashMap::new(),
            defaults: ThemeDefaults::default(),
            keyframes: HashMap::new(),
            metadata: ThemeMetadata::default(),
            modules: HashMap::new(),
            rules: Vec::new(),
            provenance: BTreeMap::new(),
            revision: next_theme_revision(),
        }
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn metadata(&self) -> &ThemeMetadata {
        &self.metadata
    }

    pub fn set_render_metadata(
        &mut self,
        mode: impl Into<String>,
        color_scheme: impl Into<String>,
        contrast: impl Into<String>,
    ) {
        let metadata = ThemeMetadata::new(mode, color_scheme, contrast);
        if self.metadata != metadata {
            self.metadata = metadata;
            self.revision = next_theme_revision();
        }
    }

    pub fn snapshot(&self) -> ThemeSnapshot {
        ThemeSnapshot {
            id: self.id.clone(),
            name: self.name.clone(),
            mode: self.metadata.mode.clone(),
            color_scheme: self.metadata.color_scheme.clone(),
            contrast: self.metadata.contrast.clone(),
            revision: self.revision,
            tokens: self.tokens.clone(),
            provenance: self.provenance.clone(),
        }
    }

    pub fn tokens(&self) -> &HashMap<String, TokenValue> {
        &self.tokens
    }

    pub fn tokens_mut(&mut self) -> &mut HashMap<String, TokenValue> {
        self.revision = next_theme_revision();
        &mut self.tokens
    }

    pub fn set_token(
        &mut self,
        name: impl Into<String>,
        value: TokenValue,
        provenance: ThemeProvenance,
    ) {
        let name = name.into();
        self.tokens.insert(name.clone(), value);
        self.provenance.insert(name, provenance);
        self.revision = next_theme_revision();
    }

    /// Remove generated tokens owned by a runtime resource binding.
    ///
    /// Resource-derived tokens are recomputed when the active resource
    /// snapshot changes. Removing them as a group prevents a pack-qualified
    /// font reference from retaining a family after its pack is uninstalled.
    pub fn remove_tokens_with_prefix(&mut self, prefix: &str) -> bool {
        let names = self
            .tokens
            .keys()
            .filter(|name| name.starts_with(prefix))
            .cloned()
            .collect::<Vec<_>>();
        if names.is_empty() {
            return false;
        }
        for name in names {
            self.tokens.remove(&name);
            self.provenance.remove(&name);
        }
        self.revision = next_theme_revision();
        true
    }

    pub fn defaults(&self) -> &ThemeDefaults {
        &self.defaults
    }

    pub fn defaults_mut(&mut self) -> &mut ThemeDefaults {
        self.revision = next_theme_revision();
        &mut self.defaults
    }

    pub fn modules(&self) -> &HashMap<String, ThemeModule> {
        &self.modules
    }

    pub fn modules_mut(&mut self) -> &mut HashMap<String, ThemeModule> {
        self.revision = next_theme_revision();
        &mut self.modules
    }

    pub fn style_rules(&self) -> &[ThemeStyleRule] {
        &self.rules
    }

    pub fn provenance(&self) -> &BTreeMap<String, ThemeProvenance> {
        &self.provenance
    }

    pub fn provenance_for(&self, value: &str) -> Option<&ThemeProvenance> {
        self.provenance.get(value)
    }

    /// Compose one immutable style snapshot from the four runtime layers:
    /// recovery defaults, a selected graph theme pack/mode, module-owned
    /// contributions, and sparse user token overrides.
    pub fn compose_layers(
        base: &Theme,
        pack: &Theme,
        pack_id: impl Into<String>,
        mode: impl Into<String>,
        module_layers: impl IntoIterator<Item = ThemeModuleLayer>,
        user_overrides: &HashMap<String, TokenValue>,
    ) -> Result<Self, ThemeError> {
        let pack_id = pack_id.into();
        let mode = mode.into();
        if pack_id.trim().is_empty() || mode.trim().is_empty() {
            return Err(ThemeError::Composition(
                "theme pack identity and mode must not be empty".into(),
            ));
        }

        let pack_provenance = ThemeProvenance::ThemePack {
            id: pack_id.clone(),
            mode: mode.clone(),
        };
        let mut composed = base.clone();
        composed.id = pack.id.clone();
        composed.name = pack.name.clone();
        composed.metadata = pack.metadata.clone();
        composed.metadata.mode = mode.clone();
        composed.provenance.clear();

        for token in composed.tokens.keys() {
            composed
                .provenance
                .insert(token.clone(), ThemeProvenance::BaseRecovery);
        }
        for (component, defaults) in &composed.defaults.components {
            for property in defaults.iter().map(|(property, _)| property) {
                composed.provenance.insert(
                    format!("defaults.{component}.{property}"),
                    ThemeProvenance::BaseRecovery,
                );
            }
        }
        for (name, module) in &composed.modules {
            for token in module.tokens.keys() {
                composed
                    .provenance
                    .insert(format!("{name}.{token}"), ThemeProvenance::BaseRecovery);
            }
            for (component, defaults) in &module.defaults.components {
                for property in defaults.iter().map(|(property, _)| property) {
                    composed.provenance.insert(
                        format!("module:{name}.defaults.{component}.{property}"),
                        ThemeProvenance::BaseRecovery,
                    );
                }
            }
        }

        merge_theme_layer(&mut composed, pack, &pack_provenance);

        for layer in module_layers {
            let module_id = layer.module_id.trim();
            if module_id.is_empty() {
                return Err(ThemeError::Composition(
                    "module theme contribution has an empty owner".into(),
                ));
            }
            let provenance = ThemeProvenance::ModuleContribution {
                module_id: module_id.to_string(),
            };
            let mut target = composed
                .modules
                .entry(module_id.to_string())
                .or_default()
                .clone();
            merge_theme_module(
                module_id,
                &mut target,
                &layer.module,
                &mut composed.provenance,
                &provenance,
            );
            composed.modules.insert(module_id.to_string(), target);
        }

        flatten_module_tokens_into(&mut composed.tokens, &composed.modules);
        for (token, value) in user_overrides {
            apply_user_token_override(&mut composed, token, value.clone())?;
        }
        composed.revision = next_theme_revision();
        Ok(composed)
    }

    /// Look up a token by dotted name, e.g. `color.primary`.
    pub fn token(&self, name: &str) -> Option<&TokenValue> {
        self.tokens
            .get(name)
            .or_else(|| match split_explicit_module_token(name) {
                Some((module_id, token_name)) => self
                    .modules
                    .get(module_id)
                    .and_then(|module| module.tokens.get(token_name)),
                None => None,
            })
    }

    /// Resolve a token and any `var(--...)` references it contains.
    ///
    /// Token references are resolved against the same immutable theme
    /// snapshot that supplied the token. This keeps aliases deterministic and
    /// makes cycles explicit instead of relying on a depth limit in each
    /// consumer.
    pub fn resolve_token_value(&self, name: &str) -> Result<Option<TokenValue>, ThemeTokenError> {
        let name = canonical_token_name(name);
        if self.token(&name).is_none() {
            return Ok(None);
        }
        let mut stack = Vec::new();
        self.resolve_named_token(&name, &mut stack).map(Some)
    }

    /// Resolve all `var(--...)` references embedded in a raw theme value.
    ///
    /// This is useful for consumers that parse a shorthand after token
    /// substitution, such as animation declarations. A missing reference may
    /// use the CSS fallback form `var(--missing, fallback)`.
    pub fn resolve_token_references(&self, raw: &str) -> Result<String, ThemeTokenError> {
        let mut stack = Vec::new();
        self.resolve_string_references(raw, &mut stack)
    }

    /// Every token whose dotted name starts with `group`.
    pub fn tokens_in_group(&self, group: &str) -> HashMap<&str, &TokenValue> {
        let prefix = format!("{group}.");
        self.tokens
            .iter()
            .filter(|(k, _)| k.starts_with(&prefix))
            .map(|(k, v)| (k.as_str(), v))
            .collect()
    }

    pub fn component_defaults(&self, component: &str) -> Option<&ComponentDefaults> {
        self.defaults.components.get(component)
    }

    /// Stops of a theme-CSS `@keyframes` rule, sorted by offset.
    pub fn keyframe_stops(&self, name: &str) -> Option<&[ThemeKeyframeStop]> {
        self.keyframes.get(name).map(Vec::as_slice)
    }

    pub fn module_component_defaults(
        &self,
        module_id: &str,
        component: &str,
    ) -> Option<&ComponentDefaults> {
        self.modules
            .get(module_id)
            .and_then(|module| module.defaults.components.get(component))
    }

    fn resolve_named_token(
        &self,
        name: &str,
        stack: &mut Vec<String>,
    ) -> Result<TokenValue, ThemeTokenError> {
        if let Some(position) = stack.iter().position(|entry| entry == name) {
            let mut cycle = stack[position..].to_vec();
            cycle.push(name.to_string());
            return Err(ThemeTokenError::Cycle(cycle));
        }

        let Some(value) = self.token(name).cloned() else {
            return Err(ThemeTokenError::Missing(name.to_string()));
        };
        stack.push(name.to_string());
        let resolved = match value {
            TokenValue::String(value) => {
                let trimmed = value.trim();
                if let Some(reference) = exact_var_reference(trimmed) {
                    self.resolve_reference_value(reference, stack)?
                } else {
                    TokenValue::String(self.resolve_string_references(&value, stack)?)
                }
            }
            value => value,
        };
        stack.pop();
        Ok(resolved)
    }

    fn resolve_reference_value(
        &self,
        reference: &str,
        stack: &mut Vec<String>,
    ) -> Result<TokenValue, ThemeTokenError> {
        let name = canonical_token_name(reference);
        if let Some(value) = self.token(&name) {
            let _ = value;
            return self.resolve_named_token(&name, stack);
        }
        Err(ThemeTokenError::Missing(name))
    }

    fn resolve_string_references(
        &self,
        raw: &str,
        stack: &mut Vec<String>,
    ) -> Result<String, ThemeTokenError> {
        let mut output = String::with_capacity(raw.len());
        let mut rest = raw;
        while let Some(start) = rest.find("var(") {
            output.push_str(&rest[..start]);
            let reference_start = start + "var(".len();
            let Some(end) = find_matching_parenthesis(&rest[reference_start..]) else {
                return Err(ThemeTokenError::Malformed(raw.to_string()));
            };
            let inside = rest[reference_start..reference_start + end].trim();
            let (reference, fallback) = inside
                .split_once(',')
                .map_or((inside, None), |(reference, fallback)| {
                    (reference.trim(), Some(fallback.trim()))
                });
            if reference.is_empty() {
                return Err(ThemeTokenError::Malformed(raw.to_string()));
            }
            match self.resolve_reference_value(reference, stack) {
                Ok(value) => output.push_str(&value.to_string()),
                Err(ThemeTokenError::Missing(_)) if fallback.is_some() => {
                    output.push_str(fallback.unwrap_or_default())
                }
                Err(error) => return Err(error),
            }
            rest = &rest[reference_start + end + 1..];
        }
        output.push_str(rest);
        Ok(output)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ThemeTokenError {
    #[error("missing theme token '{0}'")]
    Missing(String),
    #[error("theme token dependency cycle: {0:?}")]
    Cycle(Vec<String>),
    #[error("malformed theme token reference in '{0}'")]
    Malformed(String),
}

fn canonical_token_name(name: &str) -> String {
    let name = name.trim();
    name.strip_prefix("--")
        .map(css_custom_property_to_token_name)
        .unwrap_or_else(|| name.to_string())
}

fn exact_var_reference(value: &str) -> Option<&str> {
    let end = value.strip_prefix("var(")?.strip_suffix(')')?;
    if end.contains(',') {
        return None;
    }
    let reference = end.trim();
    (!reference.is_empty()).then_some(reference)
}

fn find_matching_parenthesis(value: &str) -> Option<usize> {
    let mut depth = 0usize;
    for (offset, byte) in value.bytes().enumerate() {
        match byte {
            b'(' => depth += 1,
            b')' => {
                if depth == 0 {
                    return Some(offset);
                }
                depth -= 1;
            }
            _ => {}
        }
    }
    None
}

#[derive(Debug, Deserialize)]
struct RawTheme {
    id: String,
    name: String,
    #[serde(default)]
    tokens: HashMap<String, TokenValue>,
    #[serde(default)]
    defaults: ThemeDefaults,
    #[serde(default)]
    modules: HashMap<String, ThemeModule>,
    #[serde(default)]
    metadata: ThemeMetadata,
    #[serde(default)]
    rules: Vec<ThemeStyleRule>,
    #[serde(default)]
    default_shell_animations: HashMap<String, String>,
}

impl From<RawTheme> for Theme {
    fn from(raw: RawTheme) -> Self {
        let mut theme = Self {
            id: raw.id,
            name: raw.name,
            tokens: raw.tokens,
            defaults: raw.defaults,
            keyframes: HashMap::new(),
            metadata: raw.metadata,
            modules: raw.modules,
            rules: raw.rules,
            provenance: BTreeMap::new(),
            revision: next_theme_revision(),
        };
        normalize_legacy_default_shell_animations(
            &mut theme,
            raw.default_shell_animations.into_iter().collect(),
        );
        flatten_module_tokens_into(&mut theme.tokens, &theme.modules);
        theme
    }
}

fn normalize_legacy_default_shell_animations(
    theme: &mut Theme,
    mut default_shell_animations: Vec<(String, String)>,
) {
    let mut base_defaults = theme.defaults.components.remove("base").unwrap_or_default();
    let mut legacy_transition_fragments = Vec::new();

    let mut legacy_animation_keys: Vec<String> = theme
        .tokens
        .keys()
        .filter_map(|key| {
            key.strip_prefix(LEGACY_DEFAULT_SHELL_ANIMATION_PREFIX)
                .map(str::to_owned)
        })
        .collect();
    legacy_animation_keys.sort();

    for animation_name in legacy_animation_keys {
        let legacy_key = format!("{LEGACY_DEFAULT_SHELL_ANIMATION_PREFIX}{animation_name}");
        let Some(TokenValue::String(value)) = theme.tokens.remove(&legacy_key) else {
            continue;
        };
        legacy_transition_fragments.push(value);
    }

    default_shell_animations.sort_by(|left, right| left.0.cmp(&right.0));
    for (_name, value) in default_shell_animations {
        legacy_transition_fragments.push(value);
    }

    if !legacy_transition_fragments.is_empty() && !base_defaults.contains_key("transition") {
        base_defaults.insert("transition".into(), legacy_transition_fragments.join(", "));
    }

    if !base_defaults.is_empty() {
        theme
            .defaults
            .components
            .insert("base".into(), base_defaults);
    }
}

fn merge_theme_layer(composed: &mut Theme, layer: &Theme, provenance: &ThemeProvenance) {
    for (token, value) in &layer.tokens {
        composed.tokens.insert(token.clone(), value.clone());
        composed
            .provenance
            .insert(token.clone(), provenance.clone());
    }
    for (component, defaults) in &layer.defaults.components {
        let target = composed
            .defaults
            .components
            .entry(component.clone())
            .or_default();
        for (property, value) in defaults {
            target.insert(property.clone(), value.clone());
            composed.provenance.insert(
                format!("defaults.{component}.{property}"),
                provenance.clone(),
            );
        }
    }
    for (name, stops) in &layer.keyframes {
        composed.keyframes.insert(name.clone(), stops.clone());
        composed
            .provenance
            .insert(format!("keyframes.{name}"), provenance.clone());
    }
    let rule_start = composed.rules.len();
    composed.rules.extend(layer.rules.clone());
    for (index, rule) in layer.rules.iter().enumerate() {
        for property in rule.declarations.iter().map(|(property, _)| property) {
            composed.provenance.insert(
                format!("rules.global.{0}.{property}", rule_start + index),
                provenance.clone(),
            );
        }
    }
    for (module_id, module) in &layer.modules {
        let mut target = composed.modules.get(module_id).cloned().unwrap_or_default();
        merge_theme_module(
            module_id,
            &mut target,
            module,
            &mut composed.provenance,
            provenance,
        );
        composed.modules.insert(module_id.clone(), target);
    }
}

fn merge_theme_module(
    module_id: &str,
    target: &mut ThemeModule,
    layer: &ThemeModule,
    provenance: &mut BTreeMap<String, ThemeProvenance>,
    source: &ThemeProvenance,
) {
    for (token, value) in &layer.tokens {
        target.tokens.insert(token.clone(), value.clone());
        provenance.insert(format!("{module_id}.{token}"), source.clone());
    }
    for (component, defaults) in &layer.defaults.components {
        let target_defaults = target
            .defaults
            .components
            .entry(component.clone())
            .or_default();
        for (property, value) in defaults {
            target_defaults.insert(property.clone(), value.clone());
            provenance.insert(
                format!("module:{module_id}.defaults.{component}.{property}"),
                source.clone(),
            );
        }
    }
    target.rules.extend(layer.rules.clone());
    for (index, rule) in layer.rules.iter().enumerate() {
        for property in rule.declarations.iter().map(|(property, _)| property) {
            provenance.insert(
                format!("rules.module:{module_id}.{index}.{property}"),
                source.clone(),
            );
        }
    }
}

fn apply_user_token_override(
    composed: &mut Theme,
    token: &str,
    value: TokenValue,
) -> Result<(), ThemeError> {
    let token = token.trim();
    if token.is_empty() || token.split('.').any(|part| part.trim().is_empty()) {
        return Err(ThemeError::Composition(format!(
            "user theme token '{token}' must use a dotted name"
        )));
    }
    if let Some((module_id, local_name)) = split_explicit_module_token(token) {
        let Some(module) = composed.modules.get_mut(module_id) else {
            return Err(ThemeError::Composition(format!(
                "user theme token '{token}' targets an unknown module"
            )));
        };
        module.tokens.insert(local_name.to_string(), value);
        composed
            .tokens
            .insert(token.to_string(), module.tokens[local_name].clone());
    } else if !token.contains('.') {
        return Err(ThemeError::Composition(format!(
            "user theme token '{token}' must use a dotted name"
        )));
    } else {
        composed.tokens.insert(token.to_string(), value);
    }
    composed
        .provenance
        .insert(token.to_string(), ThemeProvenance::UserOverride);
    Ok(())
}

fn flatten_module_tokens_into(
    tokens: &mut HashMap<String, TokenValue>,
    modules: &HashMap<String, ThemeModule>,
) {
    for (module_id, module) in modules {
        for (token_name, value) in &module.tokens {
            tokens.insert(format!("{module_id}.{token_name}"), value.clone());
        }
    }
}

#[derive(Debug, Clone)]
pub struct ThemeEngine {
    active: Theme,
    available: Vec<Theme>,
    active_snapshot: Arc<ThemeSnapshot>,
}

impl ThemeEngine {
    pub fn new(default_theme: Theme) -> Self {
        let active_snapshot = Arc::new(default_theme.snapshot());
        Self {
            active: default_theme,
            available: Vec::new(),
            active_snapshot,
        }
    }

    pub fn active(&self) -> &Theme {
        &self.active
    }

    /// Return the immutable snapshot used for shell publication and cache
    /// identity. It changes only when an active theme commit completes.
    pub fn active_snapshot(&self) -> &ThemeSnapshot {
        &self.active_snapshot
    }

    /// Apply a controlled edit to the active theme and publish one fresh
    /// immutable snapshot after the edit completes.
    pub fn update_active(&mut self, update: impl FnOnce(&mut Theme)) {
        update(&mut self.active);
        self.active_snapshot = Arc::new(self.active.snapshot());
    }

    pub fn register_theme(&mut self, theme: Theme) -> Result<(), ThemeError> {
        let identity = theme.id.trim();
        if identity.is_empty() {
            return Err(ThemeError::InvalidIdentity(theme.id));
        }
        if self.active.id.trim() == identity
            || self
                .available
                .iter()
                .any(|candidate| candidate.id.trim() == identity)
        {
            return Err(ThemeError::DuplicateIdentity(identity.to_string()));
        }
        self.available.push(theme);
        Ok(())
    }

    pub fn set_active(&mut self, theme_id: &str) -> Result<(), ThemeError> {
        let theme = self
            .available
            .iter()
            .find(|t| t.id.trim() == theme_id.trim())
            .ok_or_else(|| ThemeError::NotFound(theme_id.to_string()))?;
        self.active = theme.clone();
        self.active_snapshot = Arc::new(self.active.snapshot());
        Ok(())
    }

    pub fn available_themes(&self) -> &[Theme] {
        &self.available
    }

    pub fn replace_active(&mut self, theme: Theme) {
        self.active = theme;
        self.active_snapshot = Arc::new(self.active.snapshot());
    }

    pub fn with_active(&self, theme: Theme) -> Self {
        let active_snapshot = Arc::new(theme.snapshot());
        Self {
            active: theme,
            available: self.available.clone(),
            active_snapshot,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ThemeError {
    #[error("theme identity is empty: {0}")]
    InvalidIdentity(String),

    #[error("theme identity is already registered: {0}")]
    DuplicateIdentity(String),

    #[error("theme not found: {0}")]
    NotFound(String),

    #[error("failed to read theme file {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("failed to parse theme file {path}: {source}")]
    Parse {
        path: PathBuf,
        source: serde_json::Error,
    },

    #[error("failed to parse theme css {path}: {message}")]
    CssParse { path: PathBuf, message: String },

    #[error("failed to open graph-authorized theme source {path}: {message}")]
    Source { path: PathBuf, message: String },

    #[error("invalid theme composition: {0}")]
    Composition(String),
}

pub fn default_theme() -> Theme {
    match load_theme_from_path(&default_theme_path()) {
        Ok(mut theme) => {
            // The bundled recovery theme's semantics are configuration, not
            // an inference from its `tokyo-night` identifier.
            theme.set_render_metadata("default", "dark", "normal");
            theme
        }
        Err(err) => {
            tracing::warn!("failed to load default theme, using embedded fallback: {err}");
            let mut theme = embedded_default_theme();
            theme.set_render_metadata("default", "dark", "normal");
            theme
        }
    }
}

pub fn default_theme_path() -> PathBuf {
    theme_path_for_id("tokyo-night")
}

pub fn theme_dir_path() -> PathBuf {
    if let Ok(path) = std::env::var("MESH_THEME_DIR")
        && !path.trim().is_empty()
    {
        return PathBuf::from(path);
    }

    let repo_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../..")
        .join("config/themes");
    if repo_path.exists() {
        return repo_path;
    }

    mesh_home_path().join("themes")
}

pub fn theme_path_for_id(theme_id: &str) -> PathBuf {
    let package_css = theme_dir_path().join(theme_id).join("theme.css");
    if package_css.exists() {
        return package_css;
    }

    theme_dir_path().join(format!("{theme_id}.json"))
}

/// Unparseable files are skipped so one bad theme cannot block startup.
pub fn load_themes_from_dir(dir: &Path) -> Vec<Theme> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut themes: Vec<Theme> = entries
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let path = e.path();
            if path.is_dir() {
                let css_path = path.join("theme.css");
                return load_theme_from_path(&css_path).ok();
            }
            if path.extension().map(|x| x == "json").unwrap_or(false) {
                return load_theme_from_path(&path).ok();
            }
            None
        })
        .collect();
    themes.sort_by(|a, b| a.id.cmp(&b.id));
    themes
}

pub fn load_theme_from_path(path: &Path) -> Result<Theme, ThemeError> {
    if path.is_dir() {
        return load_theme_from_path(&path.join("theme.css"));
    }

    let content = std::fs::read_to_string(path).map_err(|source| ThemeError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    match path.extension().and_then(|extension| extension.to_str()) {
        Some("css") => parse_theme_css_file(path, &content),
        _ => parse_theme(&content).map_err(|source| ThemeError::Parse {
            path: path.to_path_buf(),
            source,
        }),
    }
}

/// Load CSS through a graph-provided source handle. Unlike the legacy path
/// loader, this cannot select JSON or construct a path from a theme ID.
pub fn load_theme_from_source(source: &ThemeSourceHandle) -> Result<Theme, ThemeError> {
    let path = source.candidate_path();
    let content = source
        .read_utf8_bounded(DEFAULT_MAX_THEME_SOURCE_BYTES)
        .map_err(|error| ThemeError::Source {
            path: path.clone(),
            message: error.to_string(),
        })?;
    parse_theme_css_file(&path, &content)
}

fn embedded_default_theme() -> Theme {
    parse_theme_css(
        "tokyo-night",
        "Tokyo Night",
        include_str!("../../../../../config/themes/tokyo-night/theme.css"),
    )
    .expect("embedded default theme css must be valid")
}

fn mesh_home_path() -> PathBuf {
    if let Ok(path) = std::env::var("MESH_HOME")
        && !path.trim().is_empty()
    {
        return PathBuf::from(path);
    }

    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(home).join(".mesh")
}

fn parse_theme(content: &str) -> Result<Theme, serde_json::Error> {
    serde_json::from_str::<RawTheme>(content).map(Theme::from)
}

#[derive(Debug, Deserialize)]
struct ThemePackageManifest {
    #[serde(default)]
    name: String,
    mesh: ThemePackageMesh,
}

#[derive(Debug, Deserialize)]
struct ThemePackageMesh {
    theme: ThemePackageTheme,
}

#[derive(Debug, Deserialize)]
struct ThemePackageTheme {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    label: Option<String>,
}

fn parse_theme_css_file(path: &Path, content: &str) -> Result<Theme, ThemeError> {
    let (id, name) = load_theme_package_metadata(path)?;
    parse_theme_css(&id, &name, content).map_err(|message| ThemeError::CssParse {
        path: path.to_path_buf(),
        message,
    })
}

fn load_theme_package_metadata(path: &Path) -> Result<(String, String), ThemeError> {
    let package_dir = path.parent().unwrap_or_else(|| Path::new("."));
    let manifest_path = package_dir.join("module.json");
    let manifest_content =
        std::fs::read_to_string(&manifest_path).map_err(|source| ThemeError::Io {
            path: manifest_path.clone(),
            source,
        })?;
    let manifest: ThemePackageManifest =
        serde_json::from_str(&manifest_content).map_err(|source| ThemeError::Parse {
            path: manifest_path,
            source,
        })?;

    let id = manifest
        .mesh
        .theme
        .id
        .unwrap_or_else(|| manifest.name.trim_start_matches("@mesh/").to_string());
    let name = manifest.mesh.theme.label.unwrap_or_else(|| id.clone());
    Ok((id, name))
}

fn parse_theme_css(id: &str, name: &str, content: &str) -> Result<Theme, String> {
    let content = strip_css_comments(content)?;
    let mut theme = Theme {
        id: id.to_string(),
        name: name.to_string(),
        tokens: HashMap::new(),
        defaults: ThemeDefaults::default(),
        keyframes: HashMap::new(),
        metadata: ThemeMetadata::default(),
        modules: HashMap::new(),
        rules: Vec::new(),
        provenance: BTreeMap::new(),
        revision: next_theme_revision(),
    };

    parse_theme_css_blocks(content.as_str(), &mut theme)?;
    normalize_legacy_default_shell_animations(&mut theme, Vec::new());
    flatten_module_tokens_into(&mut theme.tokens, &theme.modules);
    Ok(theme)
}

fn strip_css_comments(content: &str) -> Result<String, String> {
    let mut output = String::with_capacity(content.len());
    let mut rest = content;
    while let Some(start) = rest.find("/*") {
        output.push_str(&rest[..start]);
        let after_start = &rest[start + 2..];
        if let Some(end) = after_start.find("*/") {
            rest = &after_start[end + 2..];
        } else {
            return Err("unterminated CSS comment".into());
        }
    }
    output.push_str(rest);
    Ok(output)
}

fn parse_theme_css_blocks(mut rest: &str, theme: &mut Theme) -> Result<(), String> {
    while let Some(open) = rest.find('{') {
        let selector = rest[..open].trim();
        let body_start = open + 1;
        let close = find_matching_brace(rest, open)
            .ok_or_else(|| format!("missing closing brace for selector '{selector}'"))?;
        let body = &rest[body_start..close];
        parse_theme_css_block(selector, body, theme)?;
        rest = &rest[close + 1..];
    }
    if !rest.trim().is_empty() {
        return Err(format!("unexpected trailing CSS: '{}'", rest.trim()));
    }
    Ok(())
}

fn find_matching_brace(content: &str, open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (offset, byte) in content.as_bytes()[open..].iter().enumerate() {
        match byte {
            b'{' => depth += 1,
            b'}' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(open + offset);
                }
            }
            _ => {}
        }
    }
    None
}

fn parse_theme_css_block(selector: &str, body: &str, theme: &mut Theme) -> Result<(), String> {
    if selector.is_empty() {
        return Ok(());
    }

    if let Some(name) = selector.strip_prefix("@keyframes") {
        let name = name.trim();
        if name.is_empty() {
            return Err("@keyframes rule is missing a name".into());
        }
        let stops = parse_keyframes_body(name, body)?;
        theme.keyframes.insert(name.to_string(), stops);
        return Ok(());
    }

    if let Some(module_id) = parse_module_selector(selector) {
        parse_theme_module_css(module_id, body, theme)?;
        return Ok(());
    }

    let declarations = parse_css_declarations(body)?;
    if selector == ":root" {
        for (property, value) in declarations {
            let Some(token_name) = css_variable_to_token_name(&property) else {
                continue;
            };
            theme.tokens.insert(token_name, parse_token_value(&value));
        }
        return Ok(());
    }

    let component = if selector == "node" { "base" } else { selector };
    push_theme_rule(
        selector,
        component,
        declarations,
        &mut theme.defaults,
        &mut theme.rules,
    )?;
    Ok(())
}

fn push_theme_rule(
    selector_source: &str,
    simple_component: &str,
    declarations: ComponentDefaults,
    defaults: &mut ThemeDefaults,
    rules: &mut Vec<ThemeStyleRule>,
) -> Result<(), String> {
    let selector = css::parse_selector(selector_source)?;
    match selector {
        css::Selector::Tag(_) => defaults
            .components
            .entry(simple_component.to_string())
            .or_default()
            .extend(declarations),
        selector => rules.push(ThemeStyleRule {
            selector,
            declarations,
        }),
    }
    Ok(())
}

/// Parse `<stop-selector> { declarations }` blocks, where the selector is
/// `from`, `to`, `<percent>%`, or a comma list duplicating the declarations at
/// each offset. Returns stops sorted by offset.
fn parse_keyframes_body(name: &str, mut rest: &str) -> Result<Vec<ThemeKeyframeStop>, String> {
    let mut stops: Vec<ThemeKeyframeStop> = Vec::new();
    while let Some(open) = rest.find('{') {
        let stop_selector = rest[..open].trim();
        let close = find_matching_brace(rest, open)
            .ok_or_else(|| format!("missing closing brace in @keyframes '{name}'"))?;
        let declarations = parse_css_declarations(&rest[open + 1..close])?;
        for part in stop_selector.split(',') {
            let offset = parse_keyframe_offset(part.trim()).ok_or_else(|| {
                format!("invalid keyframe offset '{part}' in @keyframes '{name}'")
            })?;
            stops.push(ThemeKeyframeStop {
                offset,
                declarations: declarations
                    .iter()
                    .filter(|(property, _)| *property != "animation-timing-function")
                    .map(|(property, value)| (property.clone(), value.clone()))
                    .collect(),
                easing: declarations.get("animation-timing-function").cloned(),
            });
        }
        rest = &rest[close + 1..];
    }
    stops.sort_by(|a, b| a.offset.total_cmp(&b.offset));
    Ok(stops)
}

fn parse_keyframe_offset(selector: &str) -> Option<f32> {
    match selector {
        "from" => Some(0.0),
        "to" => Some(1.0),
        _ => {
            let percent = selector.strip_suffix('%')?.trim().parse::<f32>().ok()?;
            Some((percent / 100.0).clamp(0.0, 1.0))
        }
    }
}

fn parse_module_selector(selector: &str) -> Option<&str> {
    let selector = selector.strip_prefix("@module")?.trim();
    selector.strip_prefix('"')?.strip_suffix('"')
}

fn parse_theme_module_css(module_id: &str, content: &str, theme: &mut Theme) -> Result<(), String> {
    let mut module = theme.modules.remove(module_id).unwrap_or_default();
    let mut rest = content;
    while let Some(open) = rest.find('{') {
        let selector = rest[..open].trim();
        let close = find_matching_brace(rest, open)
            .ok_or_else(|| format!("missing closing brace for module selector '{selector}'"))?;
        let body = &rest[open + 1..close];
        parse_theme_module_css_block(selector, body, &mut module)?;
        rest = &rest[close + 1..];
    }
    theme.modules.insert(module_id.to_string(), module);
    Ok(())
}

fn parse_theme_module_css_block(
    selector: &str,
    body: &str,
    module: &mut ThemeModule,
) -> Result<(), String> {
    let declarations = parse_css_declarations(body)?;
    if selector == ":root" {
        for (property, value) in declarations {
            let Some(token_name) = css_variable_to_token_name(&property) else {
                continue;
            };
            module.tokens.insert(token_name, parse_token_value(&value));
        }
        return Ok(());
    }

    let component = if selector == "node" { "base" } else { selector };
    push_theme_rule(
        selector,
        component,
        declarations,
        &mut module.defaults,
        &mut module.rules,
    )?;
    Ok(())
}

fn parse_css_declarations(body: &str) -> Result<ComponentDefaults, String> {
    let mut declarations = ComponentDefaults::new();
    for raw in split_css_top_level(body, ';')? {
        let declaration = raw.trim();
        if declaration.is_empty() {
            continue;
        }
        let Some(colon) = find_css_top_level_delimiter(declaration, ':')? else {
            return Err(format!("invalid declaration '{declaration}'"));
        };
        let property = declaration[..colon].trim();
        let value = declaration[colon + 1..].trim();
        if property.is_empty() || value.is_empty() {
            return Err(format!("invalid declaration '{declaration}'"));
        }
        declarations.insert(property.to_string(), value.to_string());
    }
    Ok(declarations)
}

fn split_css_top_level(input: &str, delimiter: char) -> Result<Vec<&str>, String> {
    let mut pieces = Vec::new();
    let mut start = 0;
    let mut depth = 0usize;
    let mut quote = None;
    let mut escaped = false;
    for (index, character) in input.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if character == '\\' && quote.is_some() {
            escaped = true;
            continue;
        }
        if let Some(active_quote) = quote {
            if character == active_quote {
                quote = None;
            }
            continue;
        }
        match character {
            '\'' | '"' => quote = Some(character),
            '(' => depth += 1,
            ')' if depth == 0 => return Err("unmatched ')' in CSS declaration".into()),
            ')' => depth -= 1,
            value if value == delimiter && depth == 0 => {
                pieces.push(&input[start..index]);
                start = index + value.len_utf8();
            }
            _ => {}
        }
    }
    if quote.is_some() || depth != 0 || escaped {
        return Err("unterminated quote or function in CSS declaration".into());
    }
    pieces.push(&input[start..]);
    Ok(pieces)
}

fn find_css_top_level_delimiter(input: &str, delimiter: char) -> Result<Option<usize>, String> {
    let mut depth = 0usize;
    let mut quote = None;
    let mut escaped = false;
    for (index, character) in input.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if character == '\\' && quote.is_some() {
            escaped = true;
            continue;
        }
        if let Some(active_quote) = quote {
            if character == active_quote {
                quote = None;
            }
            continue;
        }
        match character {
            '\'' | '"' => quote = Some(character),
            '(' => depth += 1,
            ')' if depth == 0 => return Err("unmatched ')' in CSS declaration".into()),
            ')' => depth -= 1,
            value if value == delimiter && depth == 0 => return Ok(Some(index)),
            _ => {}
        }
    }
    if quote.is_some() || depth != 0 || escaped {
        return Err("unterminated quote or function in CSS declaration".into());
    }
    Ok(None)
}

fn css_variable_to_token_name(property: &str) -> Option<String> {
    let token = property.strip_prefix("--")?;
    if token.is_empty() {
        return None;
    }
    Some(css_custom_property_to_token_name(token))
}

fn css_custom_property_to_token_name(token: &str) -> String {
    let Some((group, rest)) = token.split_once('-') else {
        return token.to_string();
    };

    let rest = match group {
        "animation" => canonicalize_prefixed(
            rest,
            &["curves-bezier", "default", "duration", "opacity", "scale"],
        ),
        "border" => canonicalize_prefixed(rest, &["style", "width"]),
        "shadow" => canonicalize_prefixed(rest, &["colored", "umbra"]),
        "shape" => canonicalize_prefixed(rest, &["corner"]),
        "spacing" => canonicalize_prefixed(rest, &["inset"]),
        "state" => canonicalize_suffixed(rest, &["opacity"]),
        "icon" => canonicalize_prefixed(rest, &["size"]),
        "typography" => canonicalize_prefixed(
            rest,
            &[
                "family",
                "line-height",
                "scale-body-large",
                "scale-body-medium",
                "scale-body-small",
                "scale-display-large",
                "scale-display-medium",
                "scale-display-small",
                "scale-headline-large",
                "scale-headline-medium",
                "scale-headline-small",
                "scale-label-large",
                "scale-label-medium",
                "scale-label-small",
                "scale-title-large",
                "scale-title-medium",
                "scale-title-small",
                "size",
                "tracking",
                "weight",
            ],
        ),
        "color" | "elevation" | "radius" => rest.to_string(),
        _ => rest.replace('-', "."),
    };

    format!("{group}.{rest}")
}

fn canonicalize_prefixed(value: &str, prefixes: &[&str]) -> String {
    let mut prefixes = prefixes.to_vec();
    prefixes.sort_by_key(|prefix| std::cmp::Reverse(prefix.len()));
    for prefix in prefixes {
        if value == prefix {
            return prefix.to_string();
        }
        if let Some(rest) = value.strip_prefix(&format!("{prefix}-")) {
            return format!("{}.{}", prefix.replace('-', "."), rest);
        }
    }
    value.to_string()
}

fn canonicalize_suffixed(value: &str, suffixes: &[&str]) -> String {
    for suffix in suffixes {
        if let Some(rest) = value.strip_suffix(&format!("-{suffix}")) {
            return format!("{rest}.{suffix}");
        }
    }
    value.to_string()
}

fn parse_token_value(value: &str) -> TokenValue {
    match value {
        "true" => TokenValue::Bool(true),
        "false" => TokenValue::Bool(false),
        _ => value
            .parse::<f64>()
            .map(TokenValue::Number)
            .unwrap_or_else(|_| TokenValue::String(value.to_string())),
    }
}

fn split_explicit_module_token(name: &str) -> Option<(&str, &str)> {
    if !name.starts_with('@') {
        return None;
    }

    let (module_id, token_name) = name.split_once('.')?;
    if module_id.is_empty() || token_name.is_empty() {
        return None;
    }
    Some((module_id, token_name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theme_css_keyframes_parse_into_sorted_stops() {
        let theme = parse_theme_css(
            "kf",
            "KF",
            r#"
            tooltip {
              animation: tooltip-enter 150ms ease-out;
            }
            @keyframes tooltip-enter {
              to { opacity: 1; transform: scale(1); animation-timing-function: ease-out; }
              from { opacity: 0; transform: scale(0.85); }
              25%, 75% { opacity: 0.5; }
            }
            "#,
        )
        .expect("theme css parses");

        let stops = theme
            .keyframe_stops("tooltip-enter")
            .expect("keyframes stored");
        assert_eq!(
            stops.iter().map(|s| s.offset).collect::<Vec<_>>(),
            vec![0.0, 0.25, 0.75, 1.0]
        );
        assert_eq!(
            stops[0].declarations.get("transform").map(String::as_str),
            Some("scale(0.85)")
        );
        assert_eq!(stops[3].easing.as_deref(), Some("ease-out"));
        assert_eq!(
            stops[1].declarations.get("opacity").map(String::as_str),
            Some("0.5")
        );
        // The keyframes rule must not leak into component defaults.
        assert!(
            theme
                .component_defaults("@keyframes tooltip-enter")
                .is_none()
        );
        assert_eq!(
            theme
                .component_defaults("tooltip")
                .and_then(|d| d.get("animation"))
                .map(String::as_str),
            Some("tooltip-enter 150ms ease-out")
        );
    }

    #[test]
    fn theme_css_keeps_supported_state_rules_for_the_shared_matcher() {
        let theme = parse_theme_css(
            "selectors",
            "Selectors",
            r#"
            button:hover { color: var(--color-primary); }
            .warning { color: #f00; }
            "#,
        )
        .expect("restricted selectors parse");

        assert_eq!(theme.style_rules().len(), 2);
        assert!(matches!(
            &theme.style_rules()[0].selector,
            css::Selector::State(_, state) if state == "hover"
        ));
        assert!(matches!(
            &theme.style_rules()[1].selector,
            css::Selector::Class(class) if class == "warning"
        ));
    }

    #[test]
    fn theme_css_rejects_unterminated_comments_and_trailing_text() {
        assert!(parse_theme_css("bad", "Bad", "/* unfinished").is_err());
        assert!(parse_theme_css("bad", "Bad", "button { color: #fff; } trailing").is_err());
        assert!(parse_theme_css("bad", "Bad", "button { color: \"unterminated; }").is_err());
    }

    #[test]
    fn explicit_module_token_lookup_reads_module_subtree() {
        let theme = parse_theme(
            r##"{
              "id": "scoped",
              "name": "Scoped",
              "tokens": {
                "color.primary": "#000000"
              },
              "modules": {
                "@mesh/weather": {
                  "tokens": {
                    "weather.color.sunny": "#f6b73c"
                  }
                }
              }
            }"##,
        )
        .expect("theme parses");

        assert_eq!(
            theme
                .token("@mesh/weather.weather.color.sunny")
                .map(ToString::to_string),
            Some("#f6b73c".into())
        );
        assert!(theme.token("weather.color.sunny").is_none());
    }

    #[test]
    fn legacy_default_shell_animation_tokens_are_extracted_into_base_transition() {
        let theme = parse_theme(
            r##"{
              "id": "legacy",
              "name": "Legacy",
              "tokens": {
                "color.primary": "#000000",
                "animation.duration.fast": 90.0,
                "animation.default.border-radius": "border-radius 90ms ease-out",
                "animation.default.opacity": "opacity 90ms ease-out"
              }
            }"##,
        )
        .expect("legacy theme parses");

        assert!(theme.token("animation.default.opacity").is_none());
        assert_eq!(
            theme
                .component_defaults("base")
                .and_then(|defaults| defaults.get("transition"))
                .map(String::as_str),
            Some("border-radius 90ms ease-out, opacity 90ms ease-out")
        );
        assert!(
            theme
                .component_defaults("base")
                .is_none_or(|defaults| !defaults.contains_key("opacity"))
        );
        assert!(theme.token("animation.duration.fast").is_some());
    }

    #[test]
    fn explicit_base_component_defaults_are_preserved() {
        let theme = parse_theme(
            r##"{
              "id": "separated",
              "name": "Separated",
              "tokens": {
                "animation.duration.fast": 90.0
              },
              "defaults": {
                "components": {
                  "base": {
                    "transition": "all var(--animation-duration-fast) ease-out"
                  }
                }
              }
            }"##,
        )
        .expect("separated theme parses");

        assert_eq!(
            theme
                .component_defaults("base")
                .and_then(|defaults| defaults.get("transition"))
                .map(String::as_str),
            Some("all var(--animation-duration-fast) ease-out")
        );
        assert!(theme.token("animation.default.hover").is_none());
    }

    #[test]
    fn css_theme_parses_tokens_and_component_defaults() {
        let theme = parse_theme_css(
            "css-theme",
            "CSS Theme",
            r#"
            :root {
              --color-on-primary: #ffffff;
              --typography-size-md: 14;
              --feature-enabled: true;
            }

            node {
              color: var(--color-on-primary);
            }

            button {
              border-radius: var(--radius-md);
            }
            "#,
        )
        .expect("css theme parses");

        assert_eq!(
            theme.token("color.on-primary").map(ToString::to_string),
            Some("#ffffff".into())
        );
        assert_eq!(
            theme.token("typography.size.md").map(ToString::to_string),
            Some("14".into())
        );
        assert_eq!(
            theme.token("feature.enabled").map(ToString::to_string),
            Some("true".into())
        );
        assert_eq!(
            theme
                .component_defaults("base")
                .and_then(|defaults| defaults.get("color"))
                .map(String::as_str),
            Some("var(--color-on-primary)")
        );
        assert_eq!(
            theme
                .component_defaults("button")
                .and_then(|defaults| defaults.get("border-radius"))
                .map(String::as_str),
            Some("var(--radius-md)")
        );
    }

    #[test]
    fn theme_token_aliases_resolve_transitively_and_keep_scalar_types() {
        let mut theme = Theme::new("aliases", "Aliases");
        theme.tokens_mut().extend([
            ("color.primary".into(), TokenValue::String("#123456".into())),
            (
                "color.accent".into(),
                TokenValue::String("var(--color-primary)".into()),
            ),
            ("spacing.small".into(), TokenValue::Number(8.0)),
            (
                "spacing.medium".into(),
                TokenValue::String("var(--spacing-small)".into()),
            ),
            (
                "animation.curves.bezier.emphasized-decelerate".into(),
                TokenValue::String("cubic-bezier(0.05, 0.7, 0.1, 1)".into()),
            ),
        ]);

        assert!(matches!(
            theme.resolve_token_value("color.accent"),
            Ok(Some(TokenValue::String(value))) if value == "#123456"
        ));
        assert!(matches!(
            theme.resolve_token_value("spacing.medium"),
            Ok(Some(TokenValue::Number(value))) if value == 8.0
        ));
        assert_eq!(
            theme
                .resolve_token_references(
                    "linear-gradient(var(--color-accent), var(--color-primary))"
                )
                .expect("aliases resolve"),
            "linear-gradient(#123456, #123456)"
        );
        assert_eq!(
            theme
                .resolve_token_references("var(--animation-curves-bezier-emphasized-decelerate)")
                .expect("hyphenated animation token resolves"),
            "cubic-bezier(0.05, 0.7, 0.1, 1)"
        );
        assert!(matches!(
            theme.resolve_token_value("animation.curves.bezier.emphasized-decelerate"),
            Ok(Some(TokenValue::String(value))) if value == "cubic-bezier(0.05, 0.7, 0.1, 1)"
        ));
    }

    #[test]
    fn theme_token_alias_cycles_are_reported_with_the_cycle_path() {
        let mut theme = Theme::new("cycles", "Cycles");
        theme.tokens_mut().extend([
            (
                "color.one".into(),
                TokenValue::String("var(--color-two)".into()),
            ),
            (
                "color.two".into(),
                TokenValue::String("var(--color-one)".into()),
            ),
        ]);

        let error = theme
            .resolve_token_value("color.one")
            .expect_err("cycle must be rejected");
        assert!(matches!(error, ThemeTokenError::Cycle(path) if path
            .iter()
            .map(String::as_str)
            .eq(["color.one", "color.two", "color.one"])));
    }

    #[test]
    fn css_theme_preserves_component_default_declaration_order() {
        let theme = parse_theme_css(
            "css-theme",
            "CSS Theme",
            r#"
            button {
              background: #000000;
              background-color: #112233;
              color: #ffffff;
            }
            "#,
        )
        .expect("css theme parses");

        let defaults = theme
            .component_defaults("button")
            .expect("button defaults parsed");
        let properties = defaults
            .iter()
            .map(|(property, _)| property.as_str())
            .collect::<Vec<_>>();
        assert_eq!(properties, vec!["background", "background-color", "color"]);
    }

    #[test]
    fn css_theme_moves_duplicate_component_default_to_latest_position() {
        let theme = parse_theme_css(
            "css-theme",
            "CSS Theme",
            r#"
            button {
              background-color: #000000;
              background: #112233;
              background-color: #445566;
            }
            "#,
        )
        .expect("css theme parses");

        let defaults = theme
            .component_defaults("button")
            .expect("button defaults parsed");
        let declarations = defaults
            .iter()
            .map(|(property, value)| (property.as_str(), value.as_str()))
            .collect::<Vec<_>>();
        assert_eq!(
            declarations,
            vec![("background", "#112233"), ("background-color", "#445566")]
        );
    }

    #[test]
    fn css_theme_does_not_interpret_double_dash_as_token_separator() {
        let theme = parse_theme_css(
            "css-theme",
            "CSS Theme",
            r#"
            :root {
              --color--on-primary: #ffffff;
            }
            "#,
        )
        .expect("css theme parses");

        assert!(theme.token("color.on-primary").is_none());
        assert_eq!(
            theme.token("color.-on-primary").map(ToString::to_string),
            Some("#ffffff".into())
        );
    }

    #[test]
    fn css_theme_parses_module_scoped_contributions() {
        let theme = parse_theme_css(
            "css-theme",
            "CSS Theme",
            r#"
            :root {
              --color-primary: #000000;
            }

            @module "@mesh/weather" {
              :root {
                --weather-color-sunny: #f6b73c;
              }

              node {
                color: var(--weather-color-sunny);
              }

              weather-chip {
                background: var(--weather-color-sunny);
              }
            }
            "#,
        )
        .expect("css theme parses");

        assert_eq!(
            theme
                .token("@mesh/weather.weather.color.sunny")
                .map(ToString::to_string),
            Some("#f6b73c".into())
        );
        assert_eq!(
            theme
                .module_component_defaults("@mesh/weather", "base")
                .and_then(|defaults| defaults.get("color"))
                .map(String::as_str),
            Some("var(--weather-color-sunny)")
        );
        assert_eq!(
            theme
                .module_component_defaults("@mesh/weather", "weather-chip")
                .and_then(|defaults| defaults.get("background"))
                .map(String::as_str),
            Some("var(--weather-color-sunny)")
        );
    }

    #[test]
    fn shipped_default_css_theme_exposes_expected_tokens() {
        let theme = default_theme();

        assert_eq!(theme.id, "tokyo-night");
        assert_eq!(
            theme
                .token("color.surface-container")
                .map(ToString::to_string),
            Some("#24283b".into())
        );
        assert_eq!(
            theme.token("color.on-surface").map(ToString::to_string),
            Some("#c0caf5".into())
        );
        let transition = theme
            .component_defaults("base")
            .and_then(|defaults| defaults.get("transition"))
            .expect("base transition default");
        assert!(
            transition.contains(
                "background-color var(--animation-duration-short) var(--animation-curves-bezier-standard)"
            )
        );
        assert!(
            transition.contains(
                "transform var(--animation-duration-short) var(--animation-curves-bezier-emphasized-decelerate)"
            )
        );
    }

    #[test]
    fn shipped_default_css_theme_owns_primitive_styles() {
        let theme = embedded_default_theme();

        let base = theme.component_defaults("base").expect("base defaults");
        assert_eq!(base.get("padding").map(String::as_str), Some("0"));
        assert_eq!(
            base.get("background").map(String::as_str),
            Some("transparent")
        );
        assert_eq!(
            base.get("color").map(String::as_str),
            Some("var(--color-on-surface)")
        );

        let row = theme.component_defaults("row").expect("row defaults");
        assert_eq!(row.get("padding").map(String::as_str), Some("0"));
        assert_eq!(row.get("gap").map(String::as_str), Some("8"));
        assert_eq!(row.get("width").map(String::as_str), Some("fit"));

        let button = theme.component_defaults("button").expect("button defaults");
        assert_eq!(button.get("padding").map(String::as_str), Some("10"));
        assert_eq!(
            button.get("background").map(String::as_str),
            Some("#24283b")
        );
    }

    #[test]
    fn style_revision_is_shared_by_clones_and_advanced_by_mutation() {
        let mut theme = Theme::new("revision-test", "Revision test");
        let initial = theme.revision();
        assert_eq!(theme.clone().revision(), initial);

        theme
            .tokens_mut()
            .insert("color.primary".into(), TokenValue::String("#112233".into()));
        let after_tokens = theme.revision();
        assert_ne!(after_tokens, initial);

        theme
            .defaults_mut()
            .components
            .insert("button".into(), ComponentDefaults::new());
        let after_defaults = theme.revision();
        assert_ne!(after_defaults, after_tokens);

        theme
            .modules_mut()
            .insert("@mesh/example".into(), ThemeModule::default());
        assert_ne!(theme.revision(), after_defaults);
    }

    #[test]
    fn theme_engine_publishes_immutable_snapshots_and_rejects_ambiguous_ids() {
        let mut engine = ThemeEngine::new(Theme::new("active", "Active"));
        assert!(matches!(
            engine.register_theme(Theme::new("active", "Duplicate active")),
            Err(ThemeError::DuplicateIdentity(identity)) if identity == "active"
        ));
        engine
            .register_theme(Theme::new("pack", "Pack"))
            .expect("first pack registration succeeds");
        assert!(matches!(
            engine.register_theme(Theme::new(" pack ", "Ambiguous pack")),
            Err(ThemeError::DuplicateIdentity(identity)) if identity == "pack"
        ));

        let previous = engine.active_snapshot().clone();
        engine.update_active(|theme| {
            theme.set_token(
                "color.primary",
                TokenValue::String("#123456".into()),
                ThemeProvenance::UserOverride,
            );
        });

        assert!(previous.tokens.get("color.primary").is_none());
        assert_eq!(
            engine.active_snapshot().tokens.get("color.primary"),
            Some(&TokenValue::String("#123456".into()))
        );
        assert_ne!(previous.revision, engine.active_snapshot().revision);
    }

    #[test]
    fn theme_composer_applies_layers_with_scoped_provenance() {
        let mut base = Theme::new("recovery", "Recovery");
        base.tokens_mut()
            .insert("color.primary".into(), TokenValue::String("#000".into()));

        let mut pack = Theme::new("pack", "Pack");
        pack.tokens_mut()
            .insert("color.primary".into(), TokenValue::String("#111".into()));
        pack.tokens_mut()
            .insert("color.surface".into(), TokenValue::String("#fff".into()));

        let mut module = ThemeModule::default();
        module
            .tokens
            .insert("color.accent".into(), TokenValue::String("#f00".into()));
        let mut user = HashMap::new();
        user.insert("color.surface".into(), TokenValue::String("#eee".into()));
        user.insert(
            "@mesh/weather.color.accent".into(),
            TokenValue::String("#0f0".into()),
        );

        let composed = Theme::compose_layers(
            &base,
            &pack,
            "@mesh/pack:default",
            "dark",
            [ThemeModuleLayer {
                module_id: "@mesh/weather".into(),
                module,
            }],
            &user,
        )
        .expect("theme composition succeeds");

        assert_eq!(composed.token("color.primary").unwrap().to_string(), "#111");
        assert_eq!(composed.token("color.surface").unwrap().to_string(), "#eee");
        assert_eq!(
            composed
                .token("@mesh/weather.color.accent")
                .unwrap()
                .to_string(),
            "#0f0"
        );
        assert_eq!(
            composed.provenance_for("color.primary"),
            Some(&ThemeProvenance::ThemePack {
                id: "@mesh/pack:default".into(),
                mode: "dark".into(),
            })
        );
        assert_eq!(
            composed.provenance_for("@mesh/weather.color.accent"),
            Some(&ThemeProvenance::UserOverride)
        );
    }

    #[test]
    fn theme_catalog_scopes_owner_and_selects_a_deterministic_default_mode() {
        let descriptor = ThemePackDescriptor::new(
            "@mesh/example-theme:desk",
            "@mesh/example-theme",
            "desk",
            Some("Desk".into()),
            "/modules/@mesh/example-theme",
            [
                ("light".into(), "themes/light/theme.css".into()),
                ("dark".into(), "themes/dark/theme.css".into()),
            ],
            None,
        )
        .expect("descriptor is valid");
        let catalog = ThemeCatalog::from_descriptors([descriptor]).expect("catalog is valid");
        let descriptor = catalog.get("@mesh/example-theme:desk").unwrap();

        assert_eq!(descriptor.default_mode, "dark");
        assert_eq!(descriptor.owner_module, "@mesh/example-theme");
        assert_eq!(
            descriptor.default_source().relative_path(),
            Path::new("themes/dark/theme.css")
        );
        assert_eq!(
            catalog
                .iter()
                .map(|item| item.id.as_str())
                .collect::<Vec<_>>(),
            ["@mesh/example-theme:desk"]
        );
    }

    #[test]
    fn theme_mode_policies_share_declared_mode_selection_rules() {
        let descriptor = ThemePackDescriptor::new(
            "@mesh/example-theme:desk",
            "@mesh/example-theme",
            "desk",
            Some("Desk".into()),
            "/modules/@mesh/example-theme",
            [
                ("dark".into(), "themes/dark/theme.css".into()),
                ("light".into(), "themes/light/theme.css".into()),
            ],
            Some("dark".into()),
        )
        .unwrap()
        .with_mode_metadata([
            ("dark".into(), ThemeMetadata::new("dark", "dark", "normal")),
            (
                "light".into(),
                ThemeMetadata::new("light", "light", "normal"),
            ),
        ])
        .unwrap();

        assert_eq!(
            ThemeModePolicy::Manual
                .select_mode(
                    &descriptor.modes,
                    &descriptor.default_mode,
                    Some("light"),
                    None,
                    0
                )
                .unwrap(),
            "light"
        );
        assert_eq!(
            ThemeModePolicy::FollowSystem
                .select_mode(
                    &descriptor.modes,
                    &descriptor.default_mode,
                    None,
                    Some("light"),
                    0,
                )
                .unwrap(),
            "light"
        );
        let scheduled = ThemeModePolicy::Scheduled {
            entries: vec![
                ThemeModeSchedule {
                    at: "06:00".into(),
                    mode: "light".into(),
                },
                ThemeModeSchedule {
                    at: "18:00".into(),
                    mode: "dark".into(),
                },
            ],
        };
        assert_eq!(
            scheduled
                .select_mode(
                    &descriptor.modes,
                    &descriptor.default_mode,
                    None,
                    None,
                    12 * 60
                )
                .unwrap(),
            "light"
        );
        assert_eq!(
            scheduled
                .select_mode(
                    &descriptor.modes,
                    &descriptor.default_mode,
                    None,
                    None,
                    20 * 60
                )
                .unwrap(),
            "dark"
        );
    }

    #[test]
    fn theme_mode_schedule_rejects_invalid_and_ambiguous_entries() {
        let descriptor = ThemePackDescriptor::new(
            "@mesh/example-theme:desk",
            "@mesh/example-theme",
            "desk",
            None,
            "/modules/@mesh/example-theme",
            [("dark".into(), "themes/dark/theme.css".into())],
            Some("dark".into()),
        )
        .unwrap();
        for entries in [
            vec![ThemeModeSchedule {
                at: "noon".into(),
                mode: "dark".into(),
            }],
            vec![ThemeModeSchedule {
                at: "08:00".into(),
                mode: "missing".into(),
            }],
            vec![
                ThemeModeSchedule {
                    at: "08:00".into(),
                    mode: "dark".into(),
                },
                ThemeModeSchedule {
                    at: "08:00".into(),
                    mode: "dark".into(),
                },
            ],
        ] {
            assert!(
                ThemeModePolicy::Scheduled { entries }
                    .select_mode(&descriptor.modes, &descriptor.default_mode, None, None, 0)
                    .is_err()
            );
        }
    }

    #[test]
    fn theme_source_handle_rejects_escape_paths() {
        assert!(ThemeSourceHandle::new("/modules/theme", "../outside.css").is_err());
        assert!(ThemeSourceHandle::new("/modules/theme", "/outside.css").is_err());
        assert!(ThemeSourceHandle::new("/modules/theme", "themes/dark/theme.css").is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn graph_theme_source_open_is_bounded_utf8_and_symlink_safe() {
        use std::os::unix::fs::symlink;

        let root =
            std::env::temp_dir().join(format!("mesh-theme-source-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("themes")).unwrap();
        std::fs::write(root.join("themes/main.css"), "node { color: #fff; }").unwrap();

        let source = ThemeSourceHandle::new(&root, "themes/main.css").unwrap();
        assert_eq!(
            source.read_utf8_bounded(1024).unwrap(),
            "node { color: #fff; }"
        );
        assert!(source.read_utf8_bounded(4).is_err());

        std::fs::write(root.join("themes/binary.css"), [0xff, 0xfe]).unwrap();
        let binary = ThemeSourceHandle::new(&root, "themes/binary.css").unwrap();
        assert!(matches!(
            binary.read_utf8_bounded(1024),
            Err(ThemeSourceError::InvalidUtf8 { .. })
        ));

        symlink(root.join("themes"), root.join("link")).unwrap();
        let escaped = ThemeSourceHandle::new(&root, "link/main.css").unwrap();
        assert!(escaped.read_utf8_bounded(1024).is_err());
        let _ = std::fs::remove_dir_all(&root);
    }
}
