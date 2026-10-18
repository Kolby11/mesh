//! System-wide locale management with per-module translation catalogs,
//! fallback chains, and runtime locale switching.

use fixed_decimal::Decimal;
use icu_decimal::DecimalFormatter;
use icu_locale_core::Locale as IcuLocale;
use icu_plurals::{PluralCategory, PluralRules};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

const MAX_CATALOG_ENTRIES: usize = 4096;
const MAX_VARIANTS_PER_ENTRY: usize = 32;
const MAX_MESSAGE_LENGTH: usize = 16 * 1024;
pub const DEFAULT_MAX_CATALOG_SOURCE_BYTES: usize = 2 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LocaleFormatError {
    #[error("invalid locale '{locale}' for formatting")]
    InvalidLocale { locale: String },
    #[error("invalid number '{value}' for locale formatting")]
    InvalidNumber { value: String },
    #[error("number is not finite")]
    NonFiniteNumber,
    #[error("unsupported date style '{style}'")]
    InvalidDateStyle { style: String },
    #[error("date timestamp is outside the supported year range")]
    DateOutOfRange,
    #[error("locale formatter data is unavailable: {message}")]
    FormatterData { message: String },
}

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
        let active = normalize_locale_tag(active.as_ref())?;
        let fallback = normalize_locale_tag(fallback.as_ref())?;
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

/// Normalize one user- or module-supplied BCP 47 locale tag into the casing
/// used by catalog keys and fallback-chain comparisons.
pub fn normalize_locale_tag(raw: &str) -> Result<String, LocaleError> {
    canonicalize_locale_tag(raw)
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
    normalize_locale_tag(&raw.replace('_', "-")).ok()
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
    source: mesh_core_files::ContainedFile,
}

impl CatalogSourceHandle {
    pub fn new(
        module_root: impl Into<PathBuf>,
        relative_path: impl AsRef<Path>,
    ) -> Result<Self, String> {
        mesh_core_files::ContainedFile::new(module_root, relative_path)
            .map(|source| Self { source })
            .map_err(|error| error.to_string())
    }

    pub fn module_root(&self) -> &Path {
        self.source.module_root()
    }

    pub fn relative_path(&self) -> &Path {
        self.source.relative_path()
    }

    pub fn candidate_path(&self) -> PathBuf {
        self.source.candidate_path()
    }

    pub fn read_utf8_bounded(&self, max_bytes: usize) -> Result<String, LocaleCatalogLoadError> {
        self.source.read_utf8_bounded(max_bytes).map_err(Into::into)
    }
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

    pub fn module_translator<'a>(&'a self, module_id: &'a str) -> ModuleTranslator<'a> {
        ModuleTranslator {
            snapshot: self,
            module_id,
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

    /// The one precedence order for a module's catalogs, highest first: each
    /// locale of the fallback chain (all layers, language packs included),
    /// then the module's own default locale when the chain omits it (module
    /// layers only), then English as the last resort. Point lookups take the
    /// first match along this order and bulk projections apply it in reverse,
    /// so both agree by construction.
    fn module_lookup_order<'a>(
        &'a self,
        module_id: &str,
    ) -> impl DoubleEndedIterator<Item = (&'a str, bool)> + 'a {
        let in_chain = |candidate: &str| {
            self.selection
                .chain()
                .iter()
                .any(|locale| locale == candidate)
        };
        let default_locale = self
            .catalogs
            .module_defaults
            .get(module_id)
            .map(String::as_str)
            .filter(|default_locale| !in_chain(default_locale));
        let last_resort = (!in_chain(LAST_RESORT_LOCALE)
            && default_locale != Some(LAST_RESORT_LOCALE))
        .then_some((LAST_RESORT_LOCALE, true));
        self.selection
            .chain()
            .iter()
            .map(|locale| (locale.as_str(), true))
            .chain(default_locale.map(|locale| (locale, false)))
            .chain(last_resort)
    }

    fn translate_in_module(&self, key: &str, module_id: &str) -> Option<&CatalogEntry> {
        self.module_lookup_order(module_id)
            .find_map(|(locale, packs)| self.module_entry(module_id, locale, key, packs))
    }

    /// Visit every effective `(key, entry)` of a module once, lowest
    /// precedence first, so later visits for a key override earlier ones.
    fn for_each_module_entry_by_precedence<'a>(
        &'a self,
        module_id: &str,
        mut visit: impl FnMut(&'a str, &'a CatalogEntry, &'a str, &'a CatalogProvenance),
    ) {
        let Some(module_locales) = self.catalogs.modules.get(module_id) else {
            return;
        };
        for (locale, include_language_packs) in self.module_lookup_order(module_id).rev() {
            let Some(layers) = module_locales.get(locale) else {
                continue;
            };
            for layer in layers.iter().rev() {
                if !include_language_packs && layer.source.kind == CatalogSourceKind::LanguagePack {
                    continue;
                }
                for (key, entry) in &layer.messages {
                    visit(key.as_str(), entry, locale, &layer.source);
                }
            }
        }
    }

    fn resolve_in_core<'a>(&'a self, key: &str, args: &HashMap<String, String>) -> Option<&'a str> {
        let last_resort = (!self
            .selection
            .chain()
            .iter()
            .any(|locale| locale == LAST_RESORT_LOCALE))
        .then_some(LAST_RESORT_LOCALE);
        self.selection
            .chain()
            .iter()
            .map(String::as_str)
            .chain(last_resort)
            .find_map(|locale| {
                self.catalogs
                    .core
                    .get(locale)
                    .and_then(|m| m.get(key))
                    .map(|entry| (entry, locale))
            })
            .and_then(|(entry, locale)| entry.resolve(args, locale))
    }

    fn resolve_in_module<'a>(
        &'a self,
        key: &str,
        module_id: &str,
        args: &HashMap<String, String>,
    ) -> Option<&'a str> {
        self.module_lookup_order(module_id)
            .find_map(|(locale, packs)| {
                self.module_entry(module_id, locale, key, packs)
                    .map(|entry| (entry, locale))
            })
            .and_then(|(entry, locale)| entry.resolve(args, locale))
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
        self.module_lookup_order(module_id)
            .find_map(|(locale, packs)| self.module_source(module_id, locale, key, packs))
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
        let mut effective = HashMap::<&str, &CatalogEntry>::new();
        self.for_each_module_entry_by_precedence(module_id, |key, entry, _, _| {
            effective.insert(key, entry);
        });
        effective
            .into_iter()
            .filter_map(|(key, entry)| {
                entry
                    .default_text()
                    .map(|value| (key.to_string(), value.to_string()))
            })
            .collect()
    }

    fn effective_module_entries(&self, module_id: &str) -> HashMap<String, CatalogEntry> {
        let mut effective = HashMap::<&str, &CatalogEntry>::new();
        self.for_each_module_entry_by_precedence(module_id, |key, entry, _, _| {
            effective.insert(key, entry);
        });
        effective
            .into_iter()
            .map(|(key, entry)| (key.to_string(), entry.clone()))
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
    // `serde_json::Map` is ordered in the default build, but sorting here
    // keeps the bounded admission decision deterministic for callers that
    // construct a Value with an order-preserving map feature enabled.
    let mut entries = object.iter().collect::<Vec<_>>();
    entries.sort_by(|left, right| left.0.cmp(right.0));
    for (index, (key, raw)) in entries.into_iter().enumerate() {
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

fn parse_catalog_document(
    input: &str,
) -> Result<(serde_json::Value, Vec<String>), serde_json::Error> {
    struct CatalogDocumentVisitor;

    impl<'de> serde::de::Visitor<'de> for CatalogDocumentVisitor {
        type Value = (serde_json::Value, Vec<String>);

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("a JSON object catalog")
        }

        fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
        where
            A: serde::de::MapAccess<'de>,
        {
            let mut object = serde_json::Map::new();
            let mut duplicates = BTreeSet::new();
            while let Some((key, value)) = map.next_entry::<String, serde_json::Value>()? {
                if object.contains_key(&key) {
                    duplicates.insert(key);
                } else {
                    object.insert(key, value);
                }
            }
            Ok((
                serde_json::Value::Object(object),
                duplicates.into_iter().collect(),
            ))
        }
    }

    let mut deserializer = serde_json::Deserializer::from_str(input);
    let document =
        serde::de::Deserializer::deserialize_map(&mut deserializer, CatalogDocumentVisitor)?;
    deserializer.end()?;
    Ok(document)
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

/// Select a cardinal CLDR plural category for a decimal value.
///
/// The value is accepted as a string so visible fraction digits are retained:
/// `1`, `1.0`, and `1.00` can have different CLDR categories. Invalid values
/// deliberately fall back to `other`, which is the required safe catalog
/// variant.
pub fn plural_category(locale: &str, value: &str) -> &'static str {
    let Ok(value) = value.parse::<Decimal>() else {
        return "other";
    };
    let category = with_cached_formatter(&PLURAL_RULES, locale, |locale| {
        let locale = IcuLocale::try_from_str(locale).ok()?;
        PluralRules::try_new_cardinal(locale.into()).ok()
    }, |rules| rules.category_for(&value));
    match category {
        Some(PluralCategory::Zero) => "zero",
        Some(PluralCategory::One) => "one",
        Some(PluralCategory::Two) => "two",
        Some(PluralCategory::Few) => "few",
        Some(PluralCategory::Many) => "many",
        Some(PluralCategory::Other) | None => "other",
    }
}

/// Locales whose formatters each thread keeps; a shell usually needs one or
/// two, and an unbounded key set would come from untrusted script input.
const FORMATTER_CACHE_LOCALES: usize = 8;

type FormatterCache<T> = std::cell::RefCell<Vec<(String, Option<T>)>>;

thread_local! {
    static PLURAL_RULES: FormatterCache<PluralRules> = const { std::cell::RefCell::new(Vec::new()) };
    static DECIMAL_FORMATTERS: FormatterCache<Result<DecimalFormatter, LocaleFormatError>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// Run `use_formatter` with this thread's formatter for `locale`, building it
/// with `build` on first use. ICU construction loads and validates locale
/// data, which costs far more than the formatting call itself.
fn with_cached_formatter<T, R>(
    cache: &'static std::thread::LocalKey<FormatterCache<T>>,
    locale: &str,
    build: impl FnOnce(&str) -> Option<T>,
    use_formatter: impl FnOnce(&T) -> R,
) -> Option<R> {
    cache.with(|cache| {
        let mut cache = cache.borrow_mut();
        let index = match cache.iter().position(|(cached, _)| cached == locale) {
            Some(index) => index,
            None => {
                if cache.len() >= FORMATTER_CACHE_LOCALES {
                    cache.remove(0);
                }
                cache.push((locale.to_owned(), build(locale)));
                cache.len() - 1
            }
        };
        cache[index].1.as_ref().map(use_formatter)
    })
}

/// Format a decimal using the locale's CLDR decimal symbols and grouping.
///
/// The string form is useful for script arguments because it preserves the
/// number of visible fraction digits. `format_number` below is the convenient
/// floating-point wrapper for Rust callers.
pub fn format_number_value(locale: &str, value: &str) -> Result<String, LocaleFormatError> {
    let build = |locale: &str| {
        Some(normalized_icu_locale(locale).and_then(|locale| {
            DecimalFormatter::try_new(locale.into(), Default::default()).map_err(|error| {
                LocaleFormatError::FormatterData {
                    message: error.to_string(),
                }
            })
        }))
    };
    let formatted = with_cached_formatter(&DECIMAL_FORMATTERS, locale, build, |formatter| {
        let formatter = formatter.as_ref().map_err(Clone::clone)?;
        let value = value
            .parse::<Decimal>()
            .map_err(|_| LocaleFormatError::InvalidNumber {
                value: value.to_string(),
            })?;
        Ok(formatter.format_to_string(&value))
    });
    formatted.expect("decimal formatter cache stores every build result")
}

/// Format a finite floating-point number using locale-sensitive decimal
/// symbols. Callers that need trailing-zero-sensitive plural or number input
/// should use [`format_number_value`] instead.
pub fn format_number(locale: &str, value: f64) -> Result<String, LocaleFormatError> {
    if !value.is_finite() {
        return Err(LocaleFormatError::NonFiniteNumber);
    }
    let value = value.to_string();
    format_number_value(locale, &value)
}

/// A wall-clock reading in some time zone, already resolved from a Unix
/// timestamp. [`format_date`] and [`format_time`] read the host's local zone;
/// the `_at` variants take one directly so results are reproducible.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WallClock {
    pub year: i32,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
}

impl WallClock {
    /// The reading in the host's local zone (`TZ`, then `/etc/localtime`).
    pub fn local(timestamp_seconds: i64) -> Result<Self, LocaleFormatError> {
        check_timestamp_range(timestamp_seconds)?;
        let time: libc::time_t = timestamp_seconds
            .try_into()
            .map_err(|_| LocaleFormatError::DateOutOfRange)?;
        // SAFETY: `localtime_r` writes only into the caller-owned `tm` and
        // returns null on failure; both pointers are valid for the call.
        let mut tm: libc::tm = unsafe { std::mem::zeroed() };
        let result = unsafe { libc::localtime_r(&time, &mut tm) };
        if result.is_null() {
            return Self::utc(timestamp_seconds);
        }
        Ok(Self {
            year: tm.tm_year + 1900,
            month: (tm.tm_mon + 1) as u8,
            day: tm.tm_mday as u8,
            hour: tm.tm_hour as u8,
            minute: tm.tm_min as u8,
            second: tm.tm_sec.min(59) as u8,
        })
    }

    pub fn utc(timestamp_seconds: i64) -> Result<Self, LocaleFormatError> {
        check_timestamp_range(timestamp_seconds)?;
        let (year, month, day) = civil_date_from_days(timestamp_seconds.div_euclid(86_400));
        let seconds = timestamp_seconds.rem_euclid(86_400);
        Ok(Self {
            year: year as i32,
            month: month as u8,
            day: day as u8,
            hour: (seconds / 3_600) as u8,
            minute: (seconds % 3_600 / 60) as u8,
            second: (seconds % 60) as u8,
        })
    }
}

fn check_timestamp_range(timestamp_seconds: i64) -> Result<(), LocaleFormatError> {
    const MIN_TIMESTAMP: i64 = -62_135_596_800; // 0001-01-01T00:00:00Z
    const MAX_TIMESTAMP: i64 = 253_402_300_799; // 9999-12-31T23:59:59Z
    if (MIN_TIMESTAMP..=MAX_TIMESTAMP).contains(&timestamp_seconds) {
        Ok(())
    } else {
        Err(LocaleFormatError::DateOutOfRange)
    }
}

fn formatter_data_error(error: impl fmt::Display) -> LocaleFormatError {
    LocaleFormatError::FormatterData {
        message: error.to_string(),
    }
}

/// Format a Unix timestamp as a date in the host's local zone with CLDR
/// patterns for `locale`.
///
/// Styles: `short` (`10/10/26`), `medium` (`Oct 10, 2026`), `long`
/// (`October 10, 2026`), `full` (`Saturday, October 10, 2026`) and
/// `month_day` (`Oct 10`). The examples are `en`; every locale gets its own
/// order, separators and month forms (Slovak `10. októbra 2026`).
pub fn format_date(
    locale: &str,
    timestamp_seconds: i64,
    style: &str,
) -> Result<String, LocaleFormatError> {
    format_date_at(locale, WallClock::local(timestamp_seconds)?, style)
}

/// [`format_date`] for an already-resolved wall clock.
pub fn format_date_at(
    locale: &str,
    clock: WallClock,
    style: &str,
) -> Result<String, LocaleFormatError> {
    use icu_datetime::FixedCalendarDateTimeFormatter;
    use icu_datetime::fieldsets::{MD, YMD, YMDE};
    use icu_datetime::input::Date;

    let locale = normalized_icu_locale(locale)?;
    let date = Date::try_new_gregorian(clock.year, clock.month, clock.day)
        .map_err(|_| LocaleFormatError::DateOutOfRange)?;
    let prefs: icu_datetime::DateTimeFormatterPreferences = (&locale).into();
    macro_rules! render {
        ($fields:expr) => {
            FixedCalendarDateTimeFormatter::<icu_calendar::Gregorian, _>::try_new(prefs, $fields)
                .map(|formatter| formatter.format(&date).to_string())
                .map_err(formatter_data_error)
        };
    }
    match style {
        "short" => render!(YMD::short()),
        "medium" => render!(YMD::medium()),
        "long" => render!(YMD::long()),
        "full" => render!(YMDE::long()),
        "month_day" => render!(MD::medium()),
        _ => Err(LocaleFormatError::InvalidDateStyle {
            style: style.to_string(),
        }),
    }
}

/// Format the time of day of a Unix timestamp in the host's local zone,
/// with the locale's hour cycle. Styles: `short` (hours and minutes) and
/// `medium` (with seconds).
pub fn format_time(
    locale: &str,
    timestamp_seconds: i64,
    style: &str,
) -> Result<String, LocaleFormatError> {
    format_time_at(locale, WallClock::local(timestamp_seconds)?, style)
}

/// [`format_time`] for an already-resolved wall clock.
pub fn format_time_at(
    locale: &str,
    clock: WallClock,
    style: &str,
) -> Result<String, LocaleFormatError> {
    use icu_datetime::NoCalendarFormatter;
    use icu_datetime::fieldsets::T;
    use icu_datetime::input::Time;

    let locale = normalized_icu_locale(locale)?;
    let fields = match style {
        "short" => T::hm(),
        "medium" => T::hms(),
        _ => {
            return Err(LocaleFormatError::InvalidDateStyle {
                style: style.to_string(),
            });
        }
    };
    let time = Time::try_new(clock.hour, clock.minute, clock.second, 0)
        .map_err(|_| LocaleFormatError::DateOutOfRange)?;
    let formatter =
        NoCalendarFormatter::try_new((&locale).into(), fields).map_err(formatter_data_error)?;
    Ok(formatter.format(&time).to_string())
}

/// Format a percentage given in percent units (`55` is "55 %"), with the
/// locale's decimal symbols and its percent sign placement.
///
/// The placement table covers the shipped locales from CLDR's percent
/// patterns: a no-break space before the sign in most of continental
/// Europe, a narrow one in French, a leading sign in Turkish, and a plain
/// suffix elsewhere.
pub fn format_percent(locale: &str, value: &str) -> Result<String, LocaleFormatError> {
    let number = format_number_value(locale, value)?;
    let language = normalized_icu_locale(locale)?.id.language;
    Ok(match language.as_str() {
        "sk" | "cs" | "de" | "es" | "sv" | "ru" | "fi" | "nb" | "da" => {
            format!("{number}\u{a0}%")
        }
        "fr" => format!("{number}\u{202f}%"),
        "tr" => format!("%{number}"),
        _ => format!("{number}%"),
    })
}

/// Format a duration in seconds with localized, compact unit labels.
pub fn format_duration(locale: &str, seconds: f64) -> Result<String, LocaleFormatError> {
    if !seconds.is_finite() {
        return Err(LocaleFormatError::NonFiniteNumber);
    }
    let locale = normalized_icu_locale(locale)?;
    let sign = if seconds.is_sign_negative() { "-" } else { "" };
    let mut remaining = seconds.abs().trunc() as u64;
    let hours = remaining / 3_600;
    remaining %= 3_600;
    let minutes = remaining / 60;
    let secs = remaining % 60;
    let language = locale.id.language.to_string();
    let (hour, minute, second) = match language.as_str() {
        "sk" => ("h", "min", "s"),
        "cs" => ("h", "min", "s"),
        "de" => ("Std.", "Min.", "Sek."),
        "fr" => ("h", "min", "s"),
        _ => ("h", "min", "s"),
    };
    let mut result = String::new();
    if hours > 0 {
        let _ = write!(result, "{hours} {hour}");
    }
    if minutes > 0 {
        if !result.is_empty() {
            result.push(' ');
        }
        let _ = write!(result, "{minutes} {minute}");
    }
    if (hours == 0 && minutes == 0 && secs > 0) || result.is_empty() {
        if !result.is_empty() {
            result.push(' ');
        }
        let _ = write!(result, "{secs} {second}");
    }
    Ok(format!("{sign}{result}"))
}

fn normalized_icu_locale(locale: &str) -> Result<IcuLocale, LocaleFormatError> {
    let locale = normalize_locale_tag(locale).map_err(|_| LocaleFormatError::InvalidLocale {
        locale: locale.to_string(),
    })?;
    IcuLocale::try_from_str(&locale).map_err(|_| LocaleFormatError::InvalidLocale { locale })
}

fn civil_date_from_days(days: i64) -> (i64, u32, u32) {
    // Howard Hinnant's proleptic Gregorian civil-from-days algorithm.
    let days = days + 719_468;
    let era = if days >= 0 { days } else { days - 146_096 } / 146_097;
    let day_of_era = days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_part = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_part + 2) / 5 + 1;
    let month = month_part + if month_part < 10 { 3 } else { -9 };
    let year = year + i64::from(month <= 2);
    (year, month as u32, day as u32)
}

/// A lookup handle restricted to one module's catalogs.
///
/// The handle deliberately borrows the immutable snapshot so callers cannot
/// accidentally retain a copied cross-module key pool. A new handle observes
/// the snapshot's locale chain and catalog contents immediately.
#[derive(Debug, Clone)]
pub struct ModuleTranslator<'a> {
    snapshot: &'a LocaleSnapshot,
    module_id: &'a str,
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
        let present = self
            .snapshot
            .translate_in_module(key, &self.module_id)
            .is_some();
        let translated = self.translate(key).map(str::to_owned);
        let text = translated
            .or_else(|| fallback.map(str::to_owned))
            .unwrap_or_else(|| LocalizedTextResolution::missing_marker(key));
        LocalizedTextResolution {
            owner_module_id: self.module_id.to_string(),
            key: Some(key.to_owned()),
            text,
            fallback: fallback.map(str::to_owned),
            field_path: None,
            missing: !present,
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
        let present = self
            .snapshot
            .translate_in_module(key, &self.module_id)
            .is_some();
        let translated = self
            .translate_with(key, args)
            .or_else(|| fallback.map(str::to_owned));
        let text = translated.unwrap_or_else(|| LocalizedTextResolution::missing_marker(key));
        LocalizedTextResolution {
            owner_module_id: self.module_id.to_string(),
            key: Some(key.to_owned()),
            text,
            fallback: fallback.map(str::to_owned),
            field_path: None,
            missing: !present,
            source,
            snapshot_revision: self.snapshot_revision(),
        }
    }

    /// Resolve every localized-text object in a JSON value using this module's
    /// snapshot. This is the shared bridge for metadata-shaped surfaces such
    /// as generated settings schemas; callers receive the same owned
    /// resolution records used by runtime text so misses retain their owner,
    /// source, and snapshot revision.
    pub fn resolve_json(
        &self,
        value: &serde_json::Value,
        field_path: impl Into<String>,
    ) -> (serde_json::Value, Vec<LocalizedTextResolution>) {
        resolve_json_value(value, &field_path.into(), &mut |key, fallback| {
            self.resolve(key, fallback)
        })
    }

    /// Copy this module-scoped resolver across an execution boundary while
    /// retaining effective catalog provenance for diagnostics.
    pub fn owned(&self) -> OwnedModuleTranslator {
        let mut effective = HashMap::<&str, (&CatalogEntry, &str, &CatalogProvenance)>::new();
        self.snapshot.for_each_module_entry_by_precedence(
            self.module_id,
            |key, entry, locale, source| {
                effective.insert(key, (entry, locale, source));
            },
        );
        let mut entries = HashMap::with_capacity(effective.len());
        let mut entry_locales = HashMap::with_capacity(effective.len());
        let mut sources = HashMap::with_capacity(effective.len());
        for (key, (entry, locale, source)) in effective {
            entries.insert(key.to_string(), entry.clone());
            entry_locales.insert(key.to_string(), locale.to_string());
            sources.insert(key.to_string(), source.clone());
        }
        OwnedModuleTranslator {
            owner_module_id: self.module_id.to_string(),
            locale: self.locale().to_string(),
            entries,
            entry_locales,
            sources,
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

/// An owned module-scoped resolver for callers that cannot borrow a live
/// LocaleSnapshot, such as a Luau closure retained by a script context.
/// It is deliberately keyed by one owner module and never contains a global
/// translation pool.
#[derive(Debug, Clone)]
pub struct OwnedModuleTranslator {
    owner_module_id: String,
    locale: String,
    entries: HashMap<String, CatalogEntry>,
    entry_locales: HashMap<String, String>,
    sources: HashMap<String, CatalogProvenance>,
    snapshot_revision: u64,
}

impl OwnedModuleTranslator {
    pub fn new(
        owner_module_id: impl Into<String>,
        locale: impl Into<String>,
        entries: HashMap<String, CatalogEntry>,
        snapshot_revision: u64,
    ) -> Self {
        Self {
            owner_module_id: owner_module_id.into(),
            locale: locale.into(),
            entries,
            entry_locales: HashMap::new(),
            sources: HashMap::new(),
            snapshot_revision,
        }
    }

    pub fn module_id(&self) -> &str {
        &self.owner_module_id
    }

    pub fn locale(&self) -> &str {
        &self.locale
    }

    pub fn snapshot_revision(&self) -> u64 {
        self.snapshot_revision
    }

    pub fn resolve(&self, key: &str, fallback: Option<&str>) -> LocalizedTextResolution {
        let source = self.sources.get(key).cloned();
        let entry = self.entries.get(key);
        let translated = entry.and_then(|entry| {
            entry.render(
                self.entry_locales
                    .get(key)
                    .map(String::as_str)
                    .unwrap_or(&self.locale),
                &HashMap::new(),
            )
        });
        let text = translated
            .or_else(|| fallback.map(str::to_owned))
            .unwrap_or_else(|| LocalizedTextResolution::missing_marker(key));
        LocalizedTextResolution {
            owner_module_id: self.owner_module_id.clone(),
            key: Some(key.to_owned()),
            text,
            fallback: fallback.map(str::to_owned),
            field_path: None,
            missing: entry.is_none(),
            source,
            snapshot_revision: self.snapshot_revision,
        }
    }

    pub fn resolve_with(
        &self,
        key: &str,
        args: &HashMap<String, String>,
        fallback: Option<&str>,
    ) -> LocalizedTextResolution {
        let source = self.sources.get(key).cloned();
        let entry = self.entries.get(key);
        let translated = entry
            .and_then(|entry| {
                entry.render(
                    self.entry_locales
                        .get(key)
                        .map(String::as_str)
                        .unwrap_or(&self.locale),
                    args,
                )
            })
            .or_else(|| fallback.map(str::to_owned));
        let text = translated.unwrap_or_else(|| LocalizedTextResolution::missing_marker(key));
        LocalizedTextResolution {
            owner_module_id: self.owner_module_id.clone(),
            key: Some(key.to_owned()),
            text,
            fallback: fallback.map(str::to_owned),
            field_path: None,
            missing: entry.is_none(),
            source,
            snapshot_revision: self.snapshot_revision,
        }
    }

    pub fn resolve_json(
        &self,
        value: &serde_json::Value,
        field_path: impl Into<String>,
    ) -> (serde_json::Value, Vec<LocalizedTextResolution>) {
        resolve_json_value(value, &field_path.into(), &mut |key, fallback| {
            self.resolve(key, fallback)
        })
    }
}

fn resolve_json_value(
    value: &serde_json::Value,
    field_path: &str,
    resolve: &mut impl FnMut(&str, Option<&str>) -> LocalizedTextResolution,
) -> (serde_json::Value, Vec<LocalizedTextResolution>) {
    match value {
        serde_json::Value::Object(object)
            if object
                .get("t")
                .and_then(serde_json::Value::as_str)
                .is_some() =>
        {
            let key = object
                .get("t")
                .and_then(serde_json::Value::as_str)
                .expect("localized object has a string key");
            let fallback = object.get("fallback").and_then(serde_json::Value::as_str);
            let resolution = resolve(key, fallback).with_field_path(field_path);
            (
                serde_json::Value::String(resolution.text.clone()),
                vec![resolution],
            )
        }
        serde_json::Value::Object(object) => {
            let mut resolved = serde_json::Map::new();
            let mut resolutions = Vec::new();
            for (key, value) in object {
                let child_path = if field_path.is_empty() {
                    key.clone()
                } else {
                    format!("{field_path}.{key}")
                };
                let (value, mut child_resolutions) =
                    resolve_json_value(value, &child_path, resolve);
                resolved.insert(key.clone(), value);
                resolutions.append(&mut child_resolutions);
            }
            (serde_json::Value::Object(resolved), resolutions)
        }
        serde_json::Value::Array(values) => {
            let mut resolved = Vec::with_capacity(values.len());
            let mut resolutions = Vec::new();
            for (index, value) in values.iter().enumerate() {
                let child_path = format!("{field_path}[{index}]");
                let (value, mut child_resolutions) =
                    resolve_json_value(value, &child_path, resolve);
                resolved.push(value);
                resolutions.append(&mut child_resolutions);
            }
            (serde_json::Value::Array(resolved), resolutions)
        }
        _ => (value.clone(), Vec::new()),
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
            let (value, duplicate_keys) =
                parse_catalog_document(&content).map_err(|source_error| {
                    LocaleCatalogLoadError::Parse {
                        path: source.path.clone(),
                        source: source_error,
                    }
                })?;
            let mut catalog = compile_catalog(locale.clone(), &value);
            catalog
                .diagnostics
                .extend(duplicate_keys.into_iter().map(|key| CatalogDiagnostic {
                    key,
                    message: "duplicate catalog key; first entry wins".into(),
                }));
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

    pub fn module_translator<'a>(&'a self, module_id: &'a str) -> ModuleTranslator<'a> {
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
    chain
}

/// Catalogs every lookup falls back to after the user's chain and, for a
/// module, after its own `defaultLocale` (07 §5).
const LAST_RESORT_LOCALE: &str = "en";

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


    // cargo test -p mesh-core-locale --release --lib -- cached_icu_formatter_profile --ignored --nocapture
    #[test]
    #[ignore = "release-only ICU formatter cache profile"]
    fn cached_icu_formatter_profile() {
        // 200 plural `t()` lookups and 200 formatted numbers per frame.
        let frames = 50;
        let counts = (0..200).map(|count| count.to_string()).collect::<Vec<_>>();
        let started = std::time::Instant::now();
        let mut uncached = 0usize;
        for _ in 0..frames {
            for count in &counts {
                let locale = IcuLocale::try_from_str("sk").unwrap();
                let rules = PluralRules::try_new_cardinal(locale.into()).unwrap();
                uncached += rules.category_for(&count.parse::<Decimal>().unwrap()) as usize;
                let locale = normalized_icu_locale("sk").unwrap();
                let formatter = DecimalFormatter::try_new(locale.into(), Default::default()).unwrap();
                uncached += formatter
                    .format_to_string(&count.parse::<Decimal>().unwrap())
                    .len();
            }
        }
        let uncached_time = started.elapsed();
        let started = std::time::Instant::now();
        let mut cached = 0usize;
        for _ in 0..frames {
            for count in &counts {
                cached += plural_category("sk", count).len();
                cached += format_number_value("sk", count).unwrap().len();
            }
        }
        let cached_time = started.elapsed();
        std::hint::black_box((uncached, cached));
        let speedup = uncached_time.as_secs_f64() / cached_time.as_secs_f64();
        eprintln!(
            "{} plural + number calls: per-call ICU construction {uncached_time:?}; cached formatters {cached_time:?}; ratio {speedup:.2}x",
            frames * counts.len()
        );
        println!("MESH_PERF metric=cached_icu_formatter_speedup value={speedup:.6}");
        assert!(speedup >= 2.0, "cached ICU formatters should be at least 2x");
    }


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
            &["zh-Hant-TW", "zh-Hant", "zh", "en-US", "en"],
            "an English fallback still contributes its own parent"
        );
        assert_eq!(selection.direction(), LocaleDirection::Ltr);
        assert_eq!(selection.revision(), 7);

        let rtl = LocaleSelection::try_new("ar-EG", "en", 1).unwrap();
        assert_eq!(rtl.direction(), LocaleDirection::Rtl);
    }

    #[test]
    fn locale_changes_replace_stale_chain_and_increment_revision() {
        let mut engine = LocaleEngine::with_fallback_locale("sk-SK", "de-DE");
        assert_eq!(engine.fallback_chain(), &["sk-SK", "sk", "de-DE", "de"]);
        assert_eq!(engine.revision(), 1);

        assert!(engine.try_set_locale("fr-FR").unwrap());
        assert_eq!(engine.current(), "fr-FR");
        assert_eq!(engine.fallback_chain(), &["fr-FR", "fr", "de-DE", "de"]);
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
    fn plural_selection_uses_cldr_decimal_operands() {
        assert_eq!(plural_category("en", "1.0"), "other");
        assert_eq!(plural_category("sk", "1"), "one");
        assert_eq!(plural_category("sk", "1.0"), "many");
        assert_eq!(plural_category("ru", "2"), "few");
        assert_eq!(plural_category("ru", "5"), "many");
        assert_eq!(plural_category("ar", "0"), "zero");
        assert_eq!(plural_category("ar", "2"), "two");
        assert_eq!(plural_category("ar", "11"), "many");
    }

    #[test]
    fn fallback_catalog_uses_its_own_cldr_plural_locale() {
        let mut engine = LocaleEngine::try_with_fallback_locale("sk", "en").unwrap();
        engine.load_module_catalog(
            "@mesh/example",
            compile_catalog(
                "en",
                &serde_json::json!({
                    "items": {
                        "_plural": true,
                        "one": "{count} item",
                        "other": "{count} items"
                    }
                }),
            ),
        );
        let args = HashMap::from([("count".into(), "2".into())]);
        assert_eq!(
            engine
                .module_translator("@mesh/example")
                .translate_with("items", &args),
            Some("2 items".into())
        );
    }

    #[test]
    fn owned_translator_keeps_fallback_plural_locale_and_catalog_presence() {
        let mut engine = LocaleEngine::try_with_fallback_locale("sk", "en").unwrap();
        engine.load_module_catalog(
            "@mesh/example",
            compile_catalog(
                "en",
                &serde_json::json!({
                    "items": {
                        "_plural": true,
                        "one": "{count} item",
                        "other": "{count} items"
                    }
                }),
            ),
        );
        let translator = engine.module_translator("@mesh/example").owned();
        let args = HashMap::from([("count".into(), "1.0".into())]);
        let resolution = translator.resolve_with("items", &args, None);

        assert_eq!(resolution.text, "1.0 items");
        assert!(!resolution.missing);
    }

    #[test]
    fn dates_and_times_follow_cldr_patterns_per_locale() {
        let clock = WallClock {
            year: 2026,
            month: 10,
            day: 10,
            hour: 21,
            minute: 5,
            second: 9,
        };
        assert_eq!(format_date_at("en", clock, "month_day").unwrap(), "Oct 10");
        assert_eq!(format_date_at("sk", clock, "month_day").unwrap(), "10. 10.");
        assert_eq!(format_date_at("de", clock, "month_day").unwrap(), "10.10.");
        assert_eq!(
            format_date_at("en", clock, "full").unwrap(),
            "Saturday, October 10, 2026"
        );
        // RES-03: Slovak and Czech take the genitive month, and Czech no
        // longer borrows Slovak names; Russian is no longer English.
        assert_eq!(
            format_date_at("sk", clock, "long").unwrap(),
            "10. októbra 2026"
        );
        assert_eq!(
            format_date_at("cs", clock, "long").unwrap(),
            "10. října 2026"
        );
        assert_eq!(
            format_date_at("ru", clock, "long").unwrap(),
            "10 октября 2026\u{202f}г."
        );
        assert_eq!(format_time_at("sk", clock, "short").unwrap(), "21:05");
        assert_eq!(
            format_time_at("en", clock, "short").unwrap(),
            "9:05\u{202f}PM"
        );
        assert_eq!(
            format_time_at("en-GB", clock, "medium").unwrap(),
            "21:05:09"
        );
        assert_eq!(format_percent("en", "55").unwrap(), "55%");
        assert_eq!(format_percent("sk", "55").unwrap(), "55\u{a0}%");
        assert_eq!(format_percent("tr", "55").unwrap(), "%55");
    }

    #[test]
    fn local_wall_clock_agrees_with_utc_on_the_offset_libc_reports() {
        let timestamp = 1_791_665_021;
        let local = WallClock::local(timestamp).unwrap();
        let utc = WallClock::utc(timestamp).unwrap();
        let minutes = |clock: WallClock| i64::from(clock.hour) * 60 + i64::from(clock.minute);
        let offset = (minutes(local) - minutes(utc)).rem_euclid(24 * 60);
        assert_eq!(local.minute.abs_diff(utc.minute) % 15, 0);
        assert!(offset < 24 * 60);
        assert_eq!(WallClock::utc(0).unwrap().year, 1970);
    }

    #[test]
    fn locale_formatters_are_bounded_and_locale_aware() {
        assert_eq!(
            format_number_value("en-US", "1234567.89").unwrap(),
            "1,234,567.89"
        );
        assert_eq!(
            format_number_value("sk-SK", "1234567.89").unwrap(),
            "1 234 567,89"
        );
        assert_eq!(format_duration("en", 3_675.0).unwrap(), "1 h 1 min");
        assert!(matches!(
            format_date("en", 0, "weekday"),
            Err(LocaleFormatError::InvalidDateStyle { .. })
        ));
    }

    #[test]
    fn catalog_entry_admission_is_bounded_and_sorted() {
        let mut value = serde_json::Map::new();
        for index in 0..=MAX_CATALOG_ENTRIES {
            value.insert(format!("key-{index:04}"), serde_json::json!("value"));
        }

        let compiled = compile_catalog("en", &serde_json::Value::Object(value));
        assert_eq!(compiled.messages.len(), MAX_CATALOG_ENTRIES);
        assert!(compiled.messages.contains_key("key-0000"));
        assert!(!compiled.messages.contains_key("key-4096"));
        assert_eq!(compiled.diagnostics[0].key, "<catalog>");
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
            serde_json::json!({
                "shared": "second-pack",
                "second-only": "second-pack-value"
            })
            .to_string(),
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
        assert_eq!(
            translator.translate("second-only"),
            Some("second-pack-value")
        );
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
    fn module_default_locale_wins_before_the_english_last_resort() {
        let default_path = test_catalog_path("default-before-en-de");
        let english_path = test_catalog_path("default-before-en-en");
        std::fs::write(
            &default_path,
            serde_json::json!({ "greeting": "Hallo" }).to_string(),
        )
        .unwrap();
        std::fs::write(
            &english_path,
            serde_json::json!({ "greeting": "Hello", "english-only": "Only" }).to_string(),
        )
        .unwrap();
        let sources = vec![
            CatalogSource::module("@mesh/example", "default", "de", default_path.clone()),
            CatalogSource::module("@mesh/example", "en", "en", english_path.clone()),
        ];
        let defaults = HashMap::from([("@mesh/example".into(), "de".into())]);
        let mut engine = LocaleEngine::with_fallback_locale("sk-SK", "sk");
        let prepared = engine
            .prepare_catalog_snapshot(&sources, &defaults)
            .unwrap();
        engine.replace_catalog_snapshot(prepared.snapshot());

        let translator = engine.module_translator("@mesh/example");
        assert_eq!(translator.translate("greeting"), Some("Hallo"));
        assert_eq!(translator.translate("english-only"), Some("Only"));

        for path in [default_path, english_path] {
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
    fn json_resolution_preserves_owner_paths_and_visible_misses() {
        let mut engine = LocaleEngine::new("sk");
        engine.load_module_translations(
            "@mesh/settings-owner",
            TranslationSet {
                locale: "sk".into(),
                messages: HashMap::from([(
                    String::from("props.volume.label"),
                    String::from("Hlasitosť"),
                )]),
            },
        );

        let input = serde_json::json!({
            "properties": {
                "volume": {
                    "label": { "t": "props.volume.label", "fallback": "Volume" }
                },
                "missing": {
                    "description": { "t": "props.missing.description" }
                }
            }
        });
        let translator = engine.module_translator("@mesh/settings-owner");
        let (resolved, resolutions) =
            translator.resolve_json(&input, "mesh.settings.generated_props");

        assert_eq!(
            resolved["properties"]["volume"]["label"],
            serde_json::json!("Hlasitosť")
        );
        assert_eq!(
            resolved["properties"]["missing"]["description"],
            serde_json::json!("!!props.missing.description")
        );
        assert_eq!(resolutions.len(), 2);
        let translated = resolutions
            .iter()
            .find(|resolution| resolution.key.as_deref() == Some("props.volume.label"))
            .expect("translated resolution");
        assert_eq!(translated.owner_module_id, "@mesh/settings-owner");
        assert_eq!(
            translated.field_path.as_deref(),
            Some("mesh.settings.generated_props.properties.volume.label")
        );
        assert!(!translated.missing);
        assert!(
            resolutions
                .iter()
                .any(
                    |resolution| resolution.key.as_deref() == Some("props.missing.description")
                        && resolution.missing
                )
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
    fn catalog_preparation_reports_duplicate_keys_deterministically() {
        let path = test_catalog_path("duplicate-keys");
        std::fs::write(
            &path,
            r#"{"duplicate":"first","valid":"Visible","duplicate":"second"}"#,
        )
        .unwrap();
        let sources = vec![("@mesh/example".into(), "en".into(), path.clone())];
        let mut engine = LocaleEngine::new("en");
        let prepared = engine.prepare_module_catalog_snapshot(&sources).unwrap();

        assert_eq!(prepared.diagnostics().len(), 1);
        assert_eq!(prepared.diagnostics()[0].diagnostics[0].key, "duplicate");
        engine.replace_catalog_snapshot(prepared.snapshot());
        assert_eq!(
            engine
                .module_translator("@mesh/example")
                .translate("duplicate"),
            Some("first")
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

impl From<mesh_core_files::ContainedFileError> for LocaleCatalogLoadError {
    fn from(error: mesh_core_files::ContainedFileError) -> Self {
        use mesh_core_files::ContainedFileError as E;
        match error {
            E::Read { path, source } => Self::Read { path, source },
            E::NotRegularFile { path } => Self::NotRegularFile { path },
            E::TooLarge { path, max_bytes } => Self::TooLarge { path, max_bytes },
            E::UnsupportedPlatform { path } => Self::UnsupportedPlatform { path },
            E::UnsafePath { path, reason } => Self::UnsafePath { path, reason },
            E::InvalidUtf8 { path } => Self::InvalidUtf8 { path },
            E::Cancelled { path } => Self::Read {
                path,
                source: std::io::Error::new(std::io::ErrorKind::Interrupted, "read cancelled"),
            },
        }
    }
}
