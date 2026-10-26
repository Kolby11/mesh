use crate::bindings::{
    FrontendIconBindings, IconPackBindings, parse_target, validate_canonical_identity,
    validate_reference,
};
use crate::xdg;
use anyhow::{Result, bail};
use mesh_core_resources::{SystemResourceCatalog, resource_revision};
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::mem::size_of;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// Result of resolving a logical icon name through the binding chain.
/// `Found` carries the resolved target; `Missing` lists every candidate
/// the resolver tried for diagnostics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IconResolution {
    Found {
        semantic_name: String,
        candidate: String,
        target: ResolvedTarget,
        multicolor: bool,
        provenance: IconResolutionProvenance,
    },
    Missing {
        semantic_name: String,
        tried: Vec<String>,
    },
}

/// Explain the effective snapshot path that produced a resolved icon.
/// Keeping this beside the resolution makes runtime, diagnostics, and future
/// CLI/LSP explanations consume the same owner and fallback facts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IconResolutionProvenance {
    pub owner_module: Option<String>,
    pub pack_id: Option<String>,
    pub candidate: String,
    pub fallback_stage: String,
}

/// Where a resolved icon's pixels come from. `File` is an SVG/PNG/JPEG
/// on disk; `Glyph` is a single glyph in a font pack identified by
/// codepoint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvedTarget {
    File(PathBuf),
    Glyph {
        font_path: PathBuf,
        font_bytes: Option<std::sync::Arc<[u8]>>,
        font_fingerprint: Option<mesh_core_resources::ResourceFingerprint>,
        codepoint: u32,
        supported_axes: SupportedAxes,
    },
}

/// Variable-font axes a font pack declares support for.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SupportedAxes {
    pub fill: bool,
    pub weight: bool,
    pub grade: bool,
    pub optical_size: bool,
}

#[derive(Debug)]
pub struct IconRegistry {
    /// Host resources captured by the graph/profile candidate. Resolution
    /// must never consult a process-wide discovery result because that could
    /// mix generations while a profile or graph is being replaced.
    host_catalog: std::sync::Arc<SystemResourceCatalog>,
    /// Loaded icon-pack binding modules keyed by their short pack id
    /// (`mesh.contributes.icons[].id`).
    icon_packs: Arc<BTreeMap<String, IconPackBindings>>,
    /// Lookup index: full module id → short pack id.
    pack_id_by_module: Arc<BTreeMap<String, String>>,
    /// Per-frontend resolution context.
    frontends: Arc<BTreeMap<String, FrontendIconBindings>>,
    /// Shell-wide default icon-pack module id.
    shell_default_pack_module: Option<String>,
    generation: u64,
    allow_io: bool,
    deferred_io: AtomicBool,
    cache: HashMap<IconCacheKey, IconResolution>,
    cache_order: VecDeque<IconCacheKey>,
    cache_bytes: usize,
    warned_misses: HashSet<(String, String)>,
    warned_miss_order: VecDeque<(String, String)>,
    warned_miss_bytes: usize,
}

impl Clone for IconRegistry {
    fn clone(&self) -> Self {
        let mut snapshot = self.resolution_snapshot();
        snapshot.allow_io = self.allow_io;
        snapshot
            .deferred_io
            .store(self.deferred_io.load(Ordering::Relaxed), Ordering::Relaxed);
        snapshot.cache = self.cache.clone();
        snapshot.cache_order = self.cache_order.clone();
        snapshot.cache_bytes = self.cache_bytes;
        snapshot.warned_misses = self.warned_misses.clone();
        snapshot.warned_miss_order = self.warned_miss_order.clone();
        snapshot.warned_miss_bytes = self.warned_miss_bytes;
        snapshot
    }
}

const ICON_REGISTRY_CACHE_CAPACITY: usize = 2048;
const ICON_REGISTRY_CACHE_MAX_BYTES: usize = 8 * 1024 * 1024;
const ICON_WARNED_MISS_CAPACITY: usize = 2048;
const ICON_WARNED_MISS_MAX_BYTES: usize = 256 * 1024;

/// Borrowed declaration shared by manifest, tooling and prepared binding validation.
pub struct IconPackValidation<'a> {
    pub pack_id: &'a str,
    pub mappings: Vec<(&'a str, &'a str)>,
    pub vocabularies: Vec<(&'a str, Vec<(&'a str, &'a str)>)>,
    pub fonts: Vec<(&'a str, &'a str)>,
    pub theme_count: usize,
}

/// Validate identities, uniqueness, bounded counts and mapping syntax.
pub fn validate_icon_pack(pack: &IconPackValidation<'_>) -> Result<()> {
    validate_canonical_identity(pack.pack_id, "icon pack id")?;
    if pack.mappings.len() > crate::MAX_ICON_PACK_MAPPINGS
        || pack.mappings.len().saturating_add(
            pack.vocabularies
                .iter()
                .map(|(_, mappings)| mappings.len())
                .fold(0_usize, usize::saturating_add),
        ) > crate::MAX_ICON_PACK_MAPPINGS
    {
        bail!(
            "icon pack '{}' contains more than {} total mappings",
            pack.pack_id,
            crate::MAX_ICON_PACK_MAPPINGS
        );
    }
    if pack.vocabularies.len() > crate::MAX_ICON_PACK_VOCABULARY_OWNERS {
        bail!(
            "icon pack '{}' contains more than {} vocabulary owners",
            pack.pack_id,
            crate::MAX_ICON_PACK_VOCABULARY_OWNERS
        );
    }
    if pack.fonts.len() > crate::MAX_ICON_PACK_FONT_REQUIREMENTS {
        bail!(
            "icon pack '{}' contains more than {} font requirements",
            pack.pack_id,
            crate::MAX_ICON_PACK_FONT_REQUIREMENTS
        );
    }
    if pack.theme_count > crate::MAX_ICON_PACK_THEME_REQUIREMENTS {
        bail!(
            "icon pack '{}' contains more than {} theme requirements",
            pack.pack_id,
            crate::MAX_ICON_PACK_THEME_REQUIREMENTS
        );
    }
    let mut aliases = HashSet::new();
    for &(alias, family) in &pack.fonts {
        validate_canonical_identity(alias, "icon font alias")?;
        if family.trim().is_empty() {
            bail!("icon font alias '{alias}' has an empty family");
        }
        if !aliases.insert(alias) {
            bail!("duplicate icon font alias '{alias}'");
        }
    }
    for &(name, target) in &pack.mappings {
        validate_icon_mapping(name, target)?;
    }
    for (owner, mappings) in &pack.vocabularies {
        validate_reference(owner, "icon vocabulary owner")?;
        if owner.len() > crate::MAX_ICON_MAPPING_NAME_BYTES {
            bail!(
                "icon vocabulary owner exceeds {} bytes",
                crate::MAX_ICON_MAPPING_NAME_BYTES
            );
        }
        for &(name, target) in mappings {
            validate_icon_mapping(name, target)?;
        }
    }
    Ok(())
}

pub fn validate_icon_mapping(name: &str, target: &str) -> Result<()> {
    validate_mapping_size(name, target)?;
    validate_reference(name, "icon mapping name")?;
    validate_mapping_target(name, target)
}

fn validate_bindings(bindings: &IconPackBindings) -> Result<()> {
    validate_reference(&bindings.module_id, "icon pack module id")?;
    validate_icon_pack(&IconPackValidation {
        pack_id: &bindings.pack_id,
        mappings: bindings
            .mappings
            .iter()
            .map(|(name, mapping)| (name.as_str(), mapping.target.as_str()))
            .collect(),
        vocabularies: bindings
            .vocabularies
            .iter()
            .map(|(owner, mappings)| {
                (
                    owner.as_str(),
                    mappings
                        .iter()
                        .map(|(name, mapping)| (name.as_str(), mapping.target.as_str()))
                        .collect(),
                )
            })
            .collect(),
        fonts: bindings
            .font_aliases
            .iter()
            .map(|(alias, font)| (alias.as_str(), font.family.as_str()))
            .collect(),
        theme_count: 0,
    })
}

fn validate_mapping_size(name: &str, target: &str) -> Result<()> {
    if name.len() > crate::MAX_ICON_MAPPING_NAME_BYTES {
        bail!(
            "icon mapping name exceeds {} bytes",
            crate::MAX_ICON_MAPPING_NAME_BYTES
        );
    }
    if target.len() > crate::MAX_ICON_MAPPING_TARGET_BYTES {
        bail!(
            "icon mapping target exceeds {} bytes",
            crate::MAX_ICON_MAPPING_TARGET_BYTES
        );
    }
    Ok(())
}

fn validate_mapping_target(name: &str, target: &str) -> Result<()> {
    if target.trim().is_empty() {
        bail!("icon mapping '{name}' target must not be empty");
    }
    if std::path::Path::new(target).is_absolute() || target.trim_start().starts_with("~/") {
        bail!("icon mapping '{name}' target must be a pack/name reference");
    }
    if parse_target(target).is_none() {
        bail!("icon mapping '{name}' target '{target}' is not pack/name");
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct IconCacheKey {
    generation: u64,
    resource_revision: u64,
    module_id: String,
    semantic_name: String,
    size: u32,
}

fn icon_resolution_cache_weight(key: &IconCacheKey, resolution: &IconResolution) -> usize {
    let key_bytes = size_of::<IconCacheKey>()
        .saturating_add(key.module_id.len())
        .saturating_add(key.semantic_name.len());
    let resolution_bytes = match resolution {
        IconResolution::Found {
            semantic_name,
            candidate,
            target,
            provenance,
            ..
        } => size_of::<IconResolution>()
            .saturating_add(semantic_name.len())
            .saturating_add(candidate.len())
            .saturating_add(icon_target_cache_weight(target))
            .saturating_add(provenance.owner_module.as_deref().map_or(0, str::len))
            .saturating_add(provenance.pack_id.as_deref().map_or(0, str::len))
            .saturating_add(provenance.candidate.len())
            .saturating_add(provenance.fallback_stage.len()),
        IconResolution::Missing {
            semantic_name,
            tried,
        } => size_of::<IconResolution>()
            .saturating_add(semantic_name.len())
            .saturating_add(size_of::<Vec<String>>())
            .saturating_add(
                tried
                    .iter()
                    .map(|candidate| size_of::<String>().saturating_add(candidate.len()))
                    .fold(0usize, usize::saturating_add),
            ),
    };
    key_bytes.saturating_add(resolution_bytes).max(1)
}

fn icon_target_cache_weight(target: &ResolvedTarget) -> usize {
    match target {
        ResolvedTarget::File(path) => {
            size_of::<ResolvedTarget>().saturating_add(path.as_os_str().len())
        }
        // The binding snapshot owns prepared font bytes; cached targets only
        // clone their Arc, so charging the entire font per glyph double-counts it.
        ResolvedTarget::Glyph { font_path, .. } => {
            size_of::<ResolvedTarget>().saturating_add(font_path.as_os_str().len())
        }
    }
}

impl IconRegistry {
    /// Create the empty registry used before the first graph has committed or
    /// after the shell enters recovery. It has no host lookup authority.
    pub fn empty() -> Self {
        Self::from_catalog(std::sync::Arc::new(SystemResourceCatalog::empty()))
            .expect("builtin empty icon registry should be valid")
    }

    pub fn from_catalog(catalog: std::sync::Arc<SystemResourceCatalog>) -> Result<Self> {
        Ok(Self {
            host_catalog: catalog,
            icon_packs: Arc::new(BTreeMap::new()),
            pack_id_by_module: Arc::new(BTreeMap::new()),
            frontends: Arc::new(BTreeMap::new()),
            shell_default_pack_module: None,
            generation: 0,
            allow_io: true,
            deferred_io: AtomicBool::new(false),
            cache: HashMap::new(),
            cache_order: VecDeque::new(),
            cache_bytes: 0,
            warned_misses: HashSet::new(),
            warned_miss_order: VecDeque::new(),
            warned_miss_bytes: 0,
        })
    }

    /// Register or replace an icon-pack binding module's bindings.
    pub fn set_icon_pack(&mut self, bindings: IconPackBindings) {
        if let Err(error) = validate_bindings(&bindings) {
            tracing::warn!("rejecting icon pack '{}': {error}", bindings.pack_id);
            return;
        }
        let pack_id = bindings.pack_id.clone();
        let module_id = bindings.module_id.clone();
        if let Some(existing) = self.icon_packs.get(&pack_id)
            && existing.module_id != module_id
        {
            tracing::warn!(
                "rejecting icon pack '{}' from '{}': already owned by '{}'",
                pack_id,
                module_id,
                existing.module_id
            );
            return;
        }
        if let Some(previous_pack_id) = self.pack_id_by_module.get(&module_id).cloned()
            && previous_pack_id != pack_id
        {
            Arc::make_mut(&mut self.icon_packs).remove(&previous_pack_id);
        }
        Arc::make_mut(&mut self.pack_id_by_module).insert(module_id.clone(), pack_id.clone());
        Arc::make_mut(&mut self.icon_packs).insert(pack_id, bindings);
        self.bump_generation();
    }

    /// Atomically replace all graph-authorized binding modules and frontend
    /// contexts as one candidate so a failed graph/profile
    /// preparation cannot leave a mixed registry.
    pub fn replace_bindings(
        &mut self,
        icon_packs: Vec<IconPackBindings>,
        frontends: Vec<(String, FrontendIconBindings)>,
        shell_default_pack: Option<String>,
    ) -> Result<()> {
        let mut next_packs = BTreeMap::new();
        let mut next_pack_by_module = BTreeMap::new();
        for bindings in icon_packs {
            validate_bindings(&bindings)?;
            if next_packs
                .insert(bindings.pack_id.clone(), bindings.clone())
                .is_some()
            {
                bail!("duplicate icon pack id '{}'", bindings.pack_id);
            }
            if next_pack_by_module
                .insert(bindings.module_id.clone(), bindings.pack_id.clone())
                .is_some()
            {
                bail!("duplicate icon pack module '{}'", bindings.module_id);
            }
        }

        let mut next_frontends = BTreeMap::new();
        for (module_id, bindings) in frontends {
            if module_id.trim().is_empty() {
                bail!("frontend icon binding module id must not be empty");
            }
            bindings.validate_chain()?;
            if next_frontends.insert(module_id.clone(), bindings).is_some() {
                bail!("duplicate frontend icon binding module '{module_id}'");
            }
        }

        self.icon_packs = Arc::new(next_packs);
        self.pack_id_by_module = Arc::new(next_pack_by_module);
        self.frontends = Arc::new(next_frontends);
        self.shell_default_pack_module = shell_default_pack;
        self.bump_generation();
        Ok(())
    }

    pub fn remove_icon_pack(&mut self, module_id: &str) {
        if let Some(pack_id) = Arc::make_mut(&mut self.pack_id_by_module).remove(module_id) {
            Arc::make_mut(&mut self.icon_packs).remove(&pack_id);
            self.bump_generation();
        }
    }

    /// Register or replace a frontend's resolution context.
    pub fn set_frontend_bindings(
        &mut self,
        module_id: impl Into<String>,
        bindings: FrontendIconBindings,
    ) {
        Arc::make_mut(&mut self.frontends).insert(module_id.into(), bindings);
        self.bump_generation();
    }

    pub fn remove_frontend_bindings(&mut self, module_id: &str) {
        if Arc::make_mut(&mut self.frontends)
            .remove(module_id)
            .is_some()
        {
            self.bump_generation();
        }
    }

    pub fn set_shell_default_pack(&mut self, module_id: Option<String>) {
        self.shell_default_pack_module = module_id;
        self.bump_generation();
    }

    pub fn shell_default_pack(&self) -> Option<&str> {
        self.shell_default_pack_module.as_deref()
    }

    pub fn icon_pack(&self, pack_id: &str) -> Option<&IconPackBindings> {
        self.icon_packs.get(pack_id)
    }

    pub fn frontend_bindings(&self, module_id: &str) -> Option<&FrontendIconBindings> {
        self.frontends.get(module_id)
    }

    fn bump_generation(&mut self) {
        self.generation = self.generation.saturating_add(1);
        self.cache.clear();
        self.cache_order.clear();
        self.cache_bytes = 0;
        self.warned_misses.clear();
        self.warned_miss_order.clear();
        self.warned_miss_bytes = 0;
    }

    /// Legacy compat path — resolves a name with no module context. Used
    /// only by callers that haven't been migrated to the binding model;
    /// they get the shell-default pack chain only (no per-frontend
    /// overrides). Bare absolute paths still pass through.
    pub fn resolve(&mut self, semantic_name: &str, size: u32) -> IconResolution {
        self.resolve_for_module("", semantic_name, size)
    }

    /// Resolve a logical name in the context of a specific frontend.
    /// Walks the resolution order documented in `docs/spec/05-icons.md`.
    pub fn resolve_for_module(
        &mut self,
        module_id: &str,
        semantic_name: &str,
        size: u32,
    ) -> IconResolution {
        let cache_key = IconCacheKey {
            generation: self.generation,
            resource_revision: resource_revision(),
            module_id: module_id.to_string(),
            semantic_name: semantic_name.to_string(),
            size,
        };
        if let Some(cached) = self.cache.get(&cache_key).cloned() {
            self.cache_order.retain(|existing| existing != &cache_key);
            self.cache_order.push_back(cache_key);
            return cached;
        }

        let resolution = self.resolve_uncached(module_id, semantic_name, size);
        self.store_resolution(cache_key, &resolution);

        self.warn_missing_resolution(module_id, semantic_name, &resolution);

        resolution
    }

    fn warn_missing_resolution(
        &mut self,
        module_id: &str,
        semantic_name: &str,
        resolution: &IconResolution,
    ) {
        if matches!(resolution, IconResolution::Missing { .. })
            && self.record_warned_miss((module_id.to_string(), semantic_name.to_string()))
        {
            let frontend = self.frontends.get(module_id);
            let chain = frontend
                .map(|f| f.effective_chain(self.shell_default_pack_module.as_deref()))
                .unwrap_or_default();
            let known_packs: Vec<&str> = self.icon_packs.keys().map(String::as_str).collect();
            let pack_index: Vec<(&str, &str)> = self
                .pack_id_by_module
                .iter()
                .map(|(m, p)| (m.as_str(), p.as_str()))
                .collect();
            if let IconResolution::Missing { tried, .. } = resolution {
                tracing::warn!(
                    "icon '{}' requested by module '{}' could not be resolved; tried={:?} effective_chain={:?} registered_pack_ids={:?} pack_index={:?}; rendering built-in missing-icon glyph",
                    semantic_name,
                    module_id,
                    tried,
                    chain,
                    known_packs,
                    pack_index,
                );
            }
        }
    }

    fn store_resolution(&mut self, cache_key: IconCacheKey, resolution: &IconResolution) {
        let weight = icon_resolution_cache_weight(&cache_key, resolution);
        if weight <= ICON_REGISTRY_CACHE_MAX_BYTES {
            self.cache_order.retain(|existing| existing != &cache_key);
            if let Some(previous) = self.cache.remove(&cache_key) {
                self.cache_bytes = self
                    .cache_bytes
                    .saturating_sub(icon_resolution_cache_weight(&cache_key, &previous));
            }
            while self.cache.len() >= ICON_REGISTRY_CACHE_CAPACITY
                || self.cache_bytes.saturating_add(weight) > ICON_REGISTRY_CACHE_MAX_BYTES
            {
                let Some(evicted) = self.cache_order.pop_front() else {
                    break;
                };
                if let Some(previous) = self.cache.remove(&evicted) {
                    self.cache_bytes = self
                        .cache_bytes
                        .saturating_sub(icon_resolution_cache_weight(&evicted, &previous));
                }
            }
            if self.cache.len() < ICON_REGISTRY_CACHE_CAPACITY
                && self.cache_bytes.saturating_add(weight) <= ICON_REGISTRY_CACHE_MAX_BYTES
            {
                self.cache_order.push_back(cache_key.clone());
                self.cache.insert(cache_key, resolution.clone());
                self.cache_bytes = self.cache_bytes.saturating_add(weight);
            }
        }
    }

    pub(crate) fn resolution_snapshot(&self) -> Self {
        Self {
            host_catalog: Arc::clone(&self.host_catalog),
            icon_packs: Arc::clone(&self.icon_packs),
            pack_id_by_module: Arc::clone(&self.pack_id_by_module),
            frontends: Arc::clone(&self.frontends),
            shell_default_pack_module: self.shell_default_pack_module.clone(),
            generation: self.generation,
            allow_io: true,
            deferred_io: AtomicBool::new(false),
            cache: HashMap::new(),
            cache_order: VecDeque::new(),
            cache_bytes: 0,
            warned_misses: HashSet::new(),
            warned_miss_order: VecDeque::new(),
            warned_miss_bytes: 0,
        }
    }

    pub(crate) fn prepared_resolution(
        &mut self,
        module_id: &str,
        name: &str,
        size: u32,
    ) -> Option<IconResolution> {
        let key = IconCacheKey {
            generation: self.generation,
            resource_revision: resource_revision(),
            module_id: module_id.into(),
            semantic_name: name.into(),
            size,
        };
        if let Some(resolution) = self.cache.get(&key).cloned() {
            self.cache_order.retain(|existing| existing != &key);
            self.cache_order.push_back(key);
            return Some(resolution);
        }
        let path = std::path::Path::new(name);
        let file_name = path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| {
                ["svg", "png", "jpg", "jpeg", "bmp"]
                    .iter()
                    .any(|known| extension.eq_ignore_ascii_case(known))
            });
        if path.is_absolute() || name.starts_with("./") || name.starts_with("../") || file_name {
            return None;
        }
        let mut snapshot = self.resolution_snapshot();
        snapshot.allow_io = false;
        let result = snapshot.resolve_uncached(module_id, name, size);
        if snapshot.deferred_io.load(Ordering::Relaxed) {
            return None;
        }
        self.store_resolution(key, &result);
        self.warn_missing_resolution(module_id, name, &result);
        Some(result)
    }

    pub(crate) fn publish_resolution(
        &mut self,
        snapshot: &Self,
        revision: u64,
        module_id: &str,
        name: &str,
        size: u32,
        resolution: &IconResolution,
    ) {
        if self.generation == snapshot.generation && resource_revision() == revision {
            self.store_resolution(
                IconCacheKey {
                    generation: self.generation,
                    resource_revision: revision,
                    module_id: module_id.into(),
                    semantic_name: name.into(),
                    size,
                },
                resolution,
            );
            self.warn_missing_resolution(module_id, name, resolution);
        }
    }

    fn find_system_icon(&self, theme: &str, name: &str, size: u32) -> Option<PathBuf> {
        if self.host_catalog.icon_dirs.is_empty() {
            return None;
        }
        if self.allow_io {
            return xdg::find_icon_in_theme_with_catalog(&self.host_catalog, theme, name, size);
        }
        match xdg::prepared_icon_in_theme(&self.host_catalog, theme, name, size) {
            Some(result) => result,
            None => {
                self.deferred_io.store(true, Ordering::Relaxed);
                None
            }
        }
    }

    fn record_warned_miss(&mut self, key: (String, String)) -> bool {
        if !self.warned_misses.insert(key.clone()) {
            return false;
        }
        let weight = size_of::<(String, String)>()
            .saturating_add(key.0.len())
            .saturating_add(key.1.len());
        if weight > ICON_WARNED_MISS_MAX_BYTES {
            self.warned_misses.remove(&key);
            return true;
        }
        while self.warned_misses.len() >= ICON_WARNED_MISS_CAPACITY
            || self.warned_miss_bytes.saturating_add(weight) > ICON_WARNED_MISS_MAX_BYTES
        {
            let Some(evicted) = self.warned_miss_order.pop_front() else {
                break;
            };
            if self.warned_misses.remove(&evicted) {
                self.warned_miss_bytes = self.warned_miss_bytes.saturating_sub(
                    size_of::<(String, String)>()
                        .saturating_add(evicted.0.len())
                        .saturating_add(evicted.1.len()),
                );
            }
        }
        self.warned_miss_order.push_back(key);
        self.warned_miss_bytes = self.warned_miss_bytes.saturating_add(weight);
        while self.warned_misses.len() > ICON_WARNED_MISS_CAPACITY {
            let Some(evicted) = self.warned_miss_order.pop_front() else {
                break;
            };
            if self.warned_misses.remove(&evicted) {
                self.warned_miss_bytes = self.warned_miss_bytes.saturating_sub(
                    size_of::<(String, String)>()
                        .saturating_add(evicted.0.len())
                        .saturating_add(evicted.1.len()),
                );
            }
        }
        true
    }

    pub(crate) fn resolve_uncached(
        &self,
        module_id: &str,
        semantic_name: &str,
        size: u32,
    ) -> IconResolution {
        let mut tried = Vec::new();
        let frontend = self.frontends.get(module_id);

        // 1. User override
        if let Some(frontend) = frontend {
            if let Some(target) = frontend.user_overrides.get(semantic_name)
                && let Some(found) = self.try_target(
                    target,
                    None,
                    semantic_name,
                    size,
                    &mut tried,
                    "user-override",
                    None,
                    "user-override",
                )
            {
                return found;
            }
            // 2. Author override
            if let Some(target) = frontend.author_overrides.get(semantic_name)
                && let Some(found) = self.try_target(
                    target,
                    None,
                    semantic_name,
                    size,
                    &mut tried,
                    "author-override",
                    None,
                    "author-override",
                )
            {
                return found;
            }
        }

        // 3. Pack-qualified template name (`<pack-id>/<logical-name>`)
        if let Some((pack_id, inner)) = parse_target(semantic_name)
            && let Some(found) = self.try_pack_lookup(
                pack_id,
                module_id,
                inner,
                semantic_name,
                size,
                &mut tried,
                "pack-qualified",
            )
        {
            return found;
        }

        // 4. Effective dependency chain. A chain entry may be a MESH icon-pack
        // module or a system XDG theme id selected directly by the user. Each
        // shorter freedesktop name retries the entire chain in order.
        let chain = frontend
            .map(|frontend| frontend.effective_chain(self.shell_default_pack_module.as_deref()))
            .unwrap_or_else(|| self.shell_default_pack_module.iter().cloned().collect());
        for fallback_name in crate::fallback::semantic_fallback_names(semantic_name) {
            for chain_id in &chain {
                if let Some(pack_id) = self.pack_id_by_module.get(chain_id)
                    && let Some(found) = self.try_pack_lookup(
                        pack_id,
                        module_id,
                        &fallback_name,
                        semantic_name,
                        size,
                        &mut tried,
                        crate::fallback::fallback_stage(semantic_name, &fallback_name),
                    )
                {
                    return found;
                }
                if let Some(found) = self.try_system_pack_lookup(
                    chain_id,
                    &fallback_name,
                    semantic_name,
                    size,
                    &mut tried,
                ) {
                    return found;
                }
            }

            // 5. Hicolor system fallback (bare logical name as freedesktop
            // name), after the active chain has had the same candidate.
            if let Some(found) = self.find_system_icon("hicolor", &fallback_name, size) {
                let mapping = format!("hicolor:{fallback_name}");
                tried.push(mapping.clone());
                return IconResolution::Found {
                    semantic_name: semantic_name.into(),
                    candidate: mapping.clone(),
                    multicolor: crate::file_icon_is_multicolor(&found),
                    target: ResolvedTarget::File(found),
                    provenance: IconResolutionProvenance {
                        owner_module: None,
                        pack_id: Some("hicolor".into()),
                        candidate: mapping,
                        fallback_stage: "hicolor".into(),
                    },
                };
            }
        }

        IconResolution::Missing {
            semantic_name: semantic_name.into(),
            tried,
        }
    }

    /// Resolve a logical name through one specific icon-pack's mapping
    /// table, then dispatch the resulting `<asset-pack>/<asset-name>`
    /// target to the underlying file/font path.
    fn try_pack_lookup(
        &self,
        pack_id: &str,
        consumer_module_id: &str,
        logical_name: &str,
        semantic_name: &str,
        size: u32,
        tried: &mut Vec<String>,
        fallback_stage: &str,
    ) -> Option<IconResolution> {
        let pack = self.icon_packs.get(pack_id)?;
        let target = pack
            .vocabularies
            .get(consumer_module_id)
            .and_then(|mappings| mappings.get(logical_name))
            .or_else(|| pack.mappings.get(logical_name))?;
        self.try_target(
            &target.target,
            Some(target.multicolor),
            semantic_name,
            size,
            tried,
            &format!("pack:{pack_id}"),
            Some(pack_id),
            fallback_stage,
        )
    }

    fn try_system_pack_lookup(
        &self,
        pack_id: &str,
        lookup_name: &str,
        semantic_name: &str,
        size: u32,
        tried: &mut Vec<String>,
    ) -> Option<IconResolution> {
        let mapping = format!("system-pack:{pack_id}/{lookup_name}");
        tried.push(mapping.clone());
        let path = self.find_system_icon(pack_id, lookup_name, size)?;
        let multicolor = crate::file_icon_is_multicolor(&path);
        let target = ResolvedTarget::File(path);
        Some(IconResolution::Found {
            semantic_name: semantic_name.into(),
            candidate: mapping.clone(),
            target,
            multicolor,
            provenance: IconResolutionProvenance {
                owner_module: None,
                pack_id: Some(pack_id.into()),
                candidate: mapping,
                fallback_stage: crate::fallback::fallback_stage(semantic_name, lookup_name).into(),
            },
        })
    }

    /// Resolve an `<asset-pack>/<asset-name>` (or bare path) target into
    /// a `ResolvedTarget`. Tries XDG theme lookup then absolute file
    /// path.
    fn try_target(
        &self,
        target: &str,
        multicolor: Option<bool>,
        semantic_name: &str,
        size: u32,
        tried: &mut Vec<String>,
        source: &str,
        owner_pack: Option<&str>,
        fallback_stage: &str,
    ) -> Option<IconResolution> {
        let mapping_label = format!("{source}:{target}");
        tried.push(mapping_label.clone());

        // Bare absolute path
        let p = std::path::Path::new(target);
        if p.is_absolute() && !self.allow_io {
            self.deferred_io.store(true, Ordering::Relaxed);
            return None;
        }
        if p.is_absolute() && p.is_file() {
            return Some(IconResolution::Found {
                semantic_name: semantic_name.into(),
                candidate: mapping_label.clone(),
                target: ResolvedTarget::File(p.to_path_buf()),
                multicolor: multicolor.unwrap_or_else(|| crate::file_icon_is_multicolor(p)),
                provenance: self.provenance(owner_pack, mapping_label, fallback_stage),
            });
        }

        let (asset_pack, asset_name) = parse_target(target)?;

        // Font aliases are scoped to the icon pack that owns the mapping.
        // Searching every pack would make an alias collision depend on
        // HashMap iteration order.
        if let Some(owner_pack) = owner_pack
            && let Some(icon_pack) = self.icon_packs.get(owner_pack)
            && let Some(font_asset) = icon_pack.font_aliases.get(asset_pack)
            && let Some(target) = self.try_font_glyph(font_asset, asset_name, icon_pack.axes)
        {
            return Some(IconResolution::Found {
                semantic_name: semantic_name.into(),
                candidate: mapping_label.clone(),
                target,
                multicolor: multicolor.unwrap_or(false),
                provenance: self.provenance(Some(owner_pack), mapping_label, fallback_stage),
            });
        }

        // Asset-pack as a bare XDG theme name in the captured host catalog.
        if let Some(path) = self.find_system_icon(asset_pack, asset_name, size) {
            return Some(IconResolution::Found {
                semantic_name: semantic_name.into(),
                candidate: mapping_label.clone(),
                target: ResolvedTarget::File(path.clone()),
                multicolor: multicolor.unwrap_or_else(|| crate::file_icon_is_multicolor(&path)),
                provenance: self.provenance(owner_pack, mapping_label, fallback_stage),
            });
        }

        None
    }

    fn provenance(
        &self,
        owner_pack: Option<&str>,
        candidate: String,
        fallback_stage: &str,
    ) -> IconResolutionProvenance {
        IconResolutionProvenance {
            owner_module: owner_pack
                .and_then(|pack_id| self.icon_packs.get(pack_id))
                .map(|pack| pack.module_id.clone()),
            pack_id: owner_pack.map(str::to_string),
            candidate,
            fallback_stage: fallback_stage.into(),
        }
    }

    fn try_font_glyph(
        &self,
        font_asset: &crate::bindings::FontAsset,
        glyph_name: &str,
        axes: SupportedAxes,
    ) -> Option<ResolvedTarget> {
        let font_path = font_asset.resolved_font_path.clone()?;
        if !self.allow_io
            && (font_asset.prepared_font.is_none() || font_asset.prepared_glyphs.is_none())
        {
            self.deferred_io.store(true, Ordering::Relaxed);
            return None;
        }
        if font_asset.prepared_font.is_none() && !font_path.is_file() {
            return None;
        }
        let codepoint = if let Some(glyphs) = &font_asset.prepared_glyphs {
            glyphs.get(glyph_name).copied()
        } else {
            let glyph_map_path = font_asset.glyph_map_path.as_ref()?;
            crate::xdg::lookup_glyph_codepoint(glyph_map_path, glyph_name)
        }?;
        Some(ResolvedTarget::Glyph {
            font_path,
            font_bytes: font_asset.prepared_font.clone(),
            font_fingerprint: font_asset.font_fingerprint,
            codepoint,
            supported_axes: axes,
        })
    }
}

impl IconResolution {
    pub fn path(&self) -> Option<PathBuf> {
        match self {
            Self::Found {
                target: ResolvedTarget::File(path),
                ..
            } => Some(path.clone()),
            _ => None,
        }
    }

    pub fn target(&self) -> Option<&ResolvedTarget> {
        match self {
            Self::Found { target, .. } => Some(target),
            Self::Missing { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bindings::{FontAsset, IconMapping};
    use std::collections::HashMap;
    use std::fs;
    use std::path::Path;
    use std::sync::Arc;
    use tempfile::tempdir;

    fn registry() -> IconRegistry {
        IconRegistry::empty()
    }

    fn registry_with_theme(root: &Path, theme: &str, icons: &[&str]) -> IconRegistry {
        let theme_root = root.join(theme);
        let scalable = theme_root.join("scalable");
        fs::create_dir_all(&scalable).unwrap();
        fs::write(
            theme_root.join("index.theme"),
            "[Icon Theme]\nName=Test theme\nDirectories=scalable\n\n[scalable]\nSize=16\nType=Scalable\nMinSize=1\nMaxSize=512\n",
        )
        .unwrap();
        for icon in icons {
            fs::write(
                scalable.join(format!("{icon}.svg")),
                r#"<svg xmlns="http://www.w3.org/2000/svg" width="8" height="8"/>"#,
            )
            .unwrap();
        }
        IconRegistry::from_catalog(Arc::new(
            mesh_core_resources::SystemResourceCatalog::from_icon_dirs(vec![root.to_path_buf()]),
        ))
        .unwrap()
    }

    #[test]
    fn system_icon_names_preserve_colour_except_symbolic_assets() {
        let root = tempdir().unwrap();
        let mut registry =
            registry_with_theme(root.path(), "ColourPolicy", &["logo", "logo-symbolic"]);
        registry.set_shell_default_pack(Some("ColourPolicy".into()));
        for (name, expected) in [("logo", true), ("logo-symbolic", false)] {
            assert!(
                matches!(registry.resolve_for_module("frontend", name, 16), IconResolution::Found { multicolor, .. } if multicolor == expected)
            );
        }
        registry.set_icon_pack(IconPackBindings {
            pack_id: "explicit-colour".into(),
            module_id: "@test/explicit-colour".into(),
            mappings: HashMap::from([
                (
                    "tinted".into(),
                    crate::IconMapping {
                        target: "ColourPolicy/logo".into(),
                        multicolor: false,
                    },
                ),
                (
                    "coloured".into(),
                    crate::IconMapping {
                        target: "ColourPolicy/logo-symbolic".into(),
                        multicolor: true,
                    },
                ),
            ]),
            vocabularies: HashMap::new(),
            axes: SupportedAxes::default(),
            font_aliases: HashMap::new(),
        });
        for (name, expected) in [
            ("explicit-colour/tinted", false),
            ("explicit-colour/coloured", true),
        ] {
            assert!(
                matches!(registry.resolve_for_module("frontend", name, 16), IconResolution::Found { multicolor, .. } if multicolor == expected)
            );
        }
        assert!(!crate::file_icon_is_multicolor(std::path::Path::new(
            "theme/symbolic/apps/logo.svg"
        )));
        assert!(!crate::file_icon_is_multicolor(std::path::Path::new(
            "logo-symbolic.symbolic.png"
        )));
    }

    #[test]
    fn prepared_lookup_defers_cold_host_search_and_preserves_catalog_identity() {
        let first = tempdir().unwrap();
        let second = tempdir().unwrap();
        let mut a = registry_with_theme(first.path(), "PreparedLookupTest", &["lookup-test"]);
        let mut b = registry_with_theme(second.path(), "PreparedLookupTest", &["lookup-test"]);
        a.set_shell_default_pack(Some("PreparedLookupTest".into()));
        b.set_shell_default_pack(Some("PreparedLookupTest".into()));
        assert!(
            a.prepared_resolution("frontend", "lookup-test", 16)
                .is_none()
        );
        let resolved = a.resolve_for_module("frontend", "lookup-test", 16);
        assert!(resolved.path().unwrap().starts_with(first.path()));
        assert!(
            b.prepared_resolution("frontend", "lookup-test", 16)
                .is_none()
        );
        assert!(
            b.resolve_for_module("frontend", "lookup-test", 16)
                .path()
                .unwrap()
                .starts_with(second.path())
        );
    }

    #[test]
    fn prepared_lookup_does_not_skip_an_unprepared_higher_priority_theme() {
        let temp = tempdir().unwrap();
        let mut registry = registry_with_theme(temp.path(), "PreparedPriorityTest", &["settings"]);
        registry.set_icon_pack(IconPackBindings {
            pack_id: "prepared-priority".into(),
            module_id: "@mesh/prepared-priority".into(),
            mappings: HashMap::from([("settings".into(), "font/settings".into())]),
            vocabularies: HashMap::new(),
            axes: SupportedAxes::default(),
            font_aliases: HashMap::from([(
                "font".into(),
                FontAsset {
                    family: "Prepared Priority".into(),
                    glyph_map_path: None,
                    resolved_font_path: Some(temp.path().join("not-read.ttf")),
                    prepared_font: Some(Arc::from(&b"prepared"[..])),
                    font_fingerprint: None,
                    prepared_glyphs: Some(Arc::new(HashMap::from([("settings".into(), 0xe000)]))),
                },
            )]),
        });
        registry.set_frontend_bindings(
            "frontend",
            FrontendIconBindings {
                declared_pack_chain: vec![
                    "PreparedPriorityTest".into(),
                    "@mesh/prepared-priority".into(),
                ],
                ..Default::default()
            },
        );
        assert!(
            registry
                .prepared_resolution("frontend", "settings", 16)
                .is_none()
        );
        let resolution = registry.resolve_for_module("frontend", "settings", 16);
        assert!(matches!(resolution.target(), Some(ResolvedTarget::File(_))));
        assert!(resolution.path().unwrap().starts_with(temp.path()));
    }

    #[test]
    fn worker_snapshots_share_bindings_and_reject_superseded_cache_publication() {
        let temp = tempdir().unwrap();
        let mut live = registry_with_theme(temp.path(), "WorkerSnapshotTest", &["snapshot-test"]);
        live.set_shell_default_pack(Some("WorkerSnapshotTest".into()));
        let mut snapshot = live.resolution_snapshot();
        assert!(Arc::ptr_eq(&snapshot.icon_packs, &live.icon_packs));
        assert!(Arc::ptr_eq(&snapshot.frontends, &live.frontends));
        let revision = resource_revision();
        let resolution = snapshot.resolve_for_module("frontend", "snapshot-test", 16);
        live.set_frontend_bindings("frontend", FrontendIconBindings::default());
        assert!(!Arc::ptr_eq(&snapshot.frontends, &live.frontends));
        assert!(snapshot.frontend_bindings("frontend").is_none());
        live.publish_resolution(
            &snapshot,
            revision,
            "frontend",
            "snapshot-test",
            16,
            &resolution,
        );
        assert!(live.cache.is_empty());
    }

    #[test]
    fn worker_missing_publication_keeps_warning_deduplication_on_the_live_registry() {
        let mut live = IconRegistry::empty();
        let snapshot = live.resolution_snapshot();
        let revision = resource_revision();
        for size in [16, 24] {
            let result = snapshot.resolve_uncached("frontend", "missing-worker-icon", size);
            live.publish_resolution(
                &snapshot,
                revision,
                "frontend",
                "missing-worker-icon",
                size,
                &result,
            );
        }
        assert!(snapshot.warned_misses.is_empty());
        assert_eq!(live.warned_misses.len(), 1);
        assert_eq!(live.cache.len(), 2);
    }

    #[test]
    fn bubble_options_resolves_night_icon_when_system_theme_lacks_it() {
        let workspace = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../..");
        let manifest: serde_json::Value = serde_json::from_slice(
            &fs::read(workspace.join("modules/frontend/shared/module.json")).unwrap(),
        )
        .unwrap();
        let pack_dir = workspace.join("modules/icon-packs/material-symbols");
        let pack_manifest: serde_json::Value =
            serde_json::from_slice(&fs::read(pack_dir.join("module.json")).unwrap()).unwrap();
        let mut reg = registry();
        reg.set_icon_pack(IconPackBindings {
            pack_id: "material-rounded".into(),
            module_id: "@mesh/icons-material-symbols".into(),
            mappings: serde_json::from_value::<HashMap<String, String>>(
                pack_manifest["mesh"]["contributes"]["icons"][0]["mappings"].clone(),
            )
            .unwrap()
            .into_iter()
            .map(|(name, target)| (name, target.into()))
            .collect(),
            vocabularies: HashMap::new(),
            axes: SupportedAxes::default(),
            font_aliases: HashMap::from([(
                "ms".into(),
                FontAsset {
                    family: "Material Symbols Rounded".into(),
                    glyph_map_path: Some(pack_dir.join("codepoints.json")),
                    resolved_font_path: Some(pack_dir.join("assets/MaterialSymbolsRounded.ttf")),
                    prepared_font: None,
                    font_fingerprint: None,
                    prepared_glyphs: None,
                },
            )]),
        });
        reg.set_shell_default_pack(Some("Bibata-Original-Amber".into()));
        reg.set_frontend_bindings(
            "@mesh/bubble-options",
            FrontendIconBindings {
                declared_pack_chain: serde_json::from_value(
                    manifest["mesh"]["uses"]["resources"]["icons"].clone(),
                )
                .unwrap(),
                ..Default::default()
            },
        );
        assert!(matches!(
            reg.resolve_for_module("@mesh/bubble-options", "weather-clear-night", 24),
            IconResolution::Found { target: ResolvedTarget::Glyph { .. }, provenance, .. }
                if provenance.pack_id.as_deref() == Some("material-rounded")
        ));
    }

    #[test]
    fn bundled_material_symbols_resolves_to_a_variable_font_glyph() {
        let pack_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../../modules/icon-packs/material-symbols");
        let font_path = pack_dir.join("assets/MaterialSymbolsRounded.ttf");
        let glyph_map_path = pack_dir.join("codepoints.json");
        assert!(font_path.is_file());
        assert!(glyph_map_path.is_file());
        assert_eq!(
            crate::xdg::lookup_glyph_codepoint(&glyph_map_path, "2d_2"),
            Some(0xfff0e),
            "supplementary-plane glyphs must survive JSON conversion"
        );

        let mut reg = registry();
        reg.set_icon_pack(IconPackBindings {
            pack_id: "material-rounded".into(),
            module_id: "@mesh/icons-material-symbols".into(),
            mappings: HashMap::from([("settings".into(), "ms/settings".into())]),
            vocabularies: HashMap::new(),
            axes: SupportedAxes {
                fill: true,
                weight: true,
                grade: true,
                optical_size: true,
            },
            font_aliases: HashMap::from([(
                "ms".into(),
                FontAsset {
                    family: "Material Symbols Rounded".into(),
                    glyph_map_path: Some(glyph_map_path),
                    resolved_font_path: Some(font_path.clone()),
                    prepared_font: None,
                    font_fingerprint: None,
                    prepared_glyphs: None,
                },
            )]),
        });
        reg.set_shell_default_pack(Some("@mesh/icons-material-symbols".into()));
        reg.set_frontend_bindings("frontend", FrontendIconBindings::default());

        let result = reg.resolve_for_module("frontend", "settings", 24);
        match result {
            IconResolution::Found {
                target:
                    ResolvedTarget::Glyph {
                        font_path: resolved_path,
                        codepoint,
                        supported_axes,
                        ..
                    },
                ..
            } => {
                assert_eq!(resolved_path, font_path);
                assert_eq!(codepoint, 0xe8b8);
                assert_eq!(
                    supported_axes,
                    SupportedAxes {
                        fill: true,
                        weight: true,
                        grade: true,
                        optical_size: true,
                    }
                );
            }
            other => panic!("expected bundled Material Symbols glyph, got {other:?}"),
        }
    }

    #[test]
    fn pack_qualified_template_name_resolves_through_named_pack() {
        let td = tempfile::tempdir().unwrap();
        let mut reg = registry_with_theme(td.path(), "lucide-files", &["home"]);

        let mut mappings = HashMap::new();
        mappings.insert("home".into(), "lucide-files/home".into());
        reg.set_icon_pack(IconPackBindings {
            pack_id: "lucide".into(),
            module_id: "@mesh/icons-lucide".into(),
            mappings,
            vocabularies: HashMap::new(),
            axes: SupportedAxes::default(),
            font_aliases: HashMap::new(),
        });

        // Pack-qualified template form bypasses the chain.
        let result = reg.resolve_for_module("frontend", "lucide/home", 16);
        match result {
            IconResolution::Found {
                target: ResolvedTarget::File(path),
                ..
            } => assert!(path.ends_with("home.svg")),
            other => panic!("expected file resolution, got {other:?}"),
        }
    }

    #[test]
    fn resource_revision_retries_registry_negative_resolution() {
        let td = tempdir().unwrap();
        let mut reg = registry_with_theme(td.path(), "files", &[]);
        reg.set_icon_pack(IconPackBindings {
            pack_id: "files-pack".into(),
            module_id: "@mesh/files-pack".into(),
            mappings: HashMap::from([("appears-later".into(), "files/appears-later".into())]),
            vocabularies: HashMap::new(),
            axes: SupportedAxes::default(),
            font_aliases: HashMap::new(),
        });
        reg.set_shell_default_pack(Some("@mesh/files-pack".into()));

        assert!(matches!(
            reg.resolve("appears-later", 24),
            IconResolution::Missing { .. }
        ));
        fs::write(
            td.path().join("files/scalable/appears-later.svg"),
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="8" height="8"/>"#,
        )
        .unwrap();

        mesh_core_resources::advance_resource_revision();
        assert!(matches!(
            reg.resolve("appears-later", 24),
            IconResolution::Found {
                target: ResolvedTarget::File(_),
                ..
            }
        ));
    }

    #[test]
    fn semantic_resolution_generalizes_dash_suffixes_in_chain_order() {
        let td = tempdir().unwrap();
        let mut registry = registry_with_theme(td.path(), "files", &["base"]);
        registry.set_icon_pack(IconPackBindings {
            pack_id: "fallbacks".into(),
            module_id: "@mesh/fallbacks".into(),
            mappings: HashMap::from([("network-wireless".into(), "files/base".into())]),
            vocabularies: HashMap::new(),
            axes: SupportedAxes::default(),
            font_aliases: HashMap::new(),
        });
        registry.set_shell_default_pack(Some("@mesh/fallbacks".into()));

        let result = registry.resolve("network-wireless-signal-weak", 24);
        assert!(matches!(result, IconResolution::Found { .. }));
        assert!(matches!(
            result,
            IconResolution::Found { candidate, .. } if candidate == "pack:fallbacks:files/base"
        ));
    }

    #[test]
    fn vocabulary_mappings_are_scoped_to_the_requesting_module() {
        let td = tempdir().unwrap();
        let mut registry = registry_with_theme(td.path(), "files", &["weather", "other"]);
        registry
            .replace_bindings(
                vec![IconPackBindings {
                    pack_id: "vocabulary".into(),
                    module_id: "@mesh/icons-vocabulary".into(),
                    mappings: HashMap::new(),
                    vocabularies: HashMap::from([
                        (
                            "@community/weather".into(),
                            HashMap::from([(
                                "weather-clear".into(),
                                IconMapping {
                                    target: "files/weather".into(),
                                    multicolor: true,
                                },
                            )]),
                        ),
                        (
                            "@community/other".into(),
                            HashMap::from([(
                                "weather-clear".into(),
                                IconMapping {
                                    target: "files/other".into(),
                                    multicolor: false,
                                },
                            )]),
                        ),
                    ]),
                    axes: SupportedAxes::default(),
                    font_aliases: HashMap::new(),
                }],
                vec![
                    (
                        "@community/weather".into(),
                        FrontendIconBindings {
                            declared_pack_chain: vec!["@mesh/icons-vocabulary".into()],
                            ..Default::default()
                        },
                    ),
                    (
                        "@community/other".into(),
                        FrontendIconBindings {
                            declared_pack_chain: vec!["@mesh/icons-vocabulary".into()],
                            ..Default::default()
                        },
                    ),
                ],
                None,
            )
            .unwrap();

        let weather_result = registry.resolve_for_module("@community/weather", "weather-clear", 8);
        let IconResolution::Found {
            target: ResolvedTarget::File(path),
            multicolor,
            provenance,
            ..
        } = weather_result
        else {
            panic!("expected weather vocabulary mapping");
        };
        assert!(path.ends_with("weather.svg"));
        assert!(multicolor);
        assert_eq!(
            provenance.owner_module.as_deref(),
            Some("@mesh/icons-vocabulary")
        );
        assert_eq!(provenance.pack_id.as_deref(), Some("vocabulary"));

        let other_result = registry.resolve_for_module("@community/other", "weather-clear", 8);
        let ResolvedTarget::File(path) = other_result.target().unwrap() else {
            panic!("expected other vocabulary mapping");
        };
        assert!(path.ends_with("other.svg"));
    }

    #[test]
    fn replacing_a_module_pack_removes_its_superseded_pack_identity() {
        let mut registry = registry();
        registry.set_icon_pack(IconPackBindings {
            pack_id: "old-pack".into(),
            module_id: "@mesh/icons-changing".into(),
            mappings: HashMap::new(),
            vocabularies: HashMap::new(),
            axes: SupportedAxes::default(),
            font_aliases: HashMap::new(),
        });
        registry.set_icon_pack(IconPackBindings {
            pack_id: "new-pack".into(),
            module_id: "@mesh/icons-changing".into(),
            mappings: HashMap::new(),
            vocabularies: HashMap::new(),
            axes: SupportedAxes::default(),
            font_aliases: HashMap::new(),
        });
        assert!(registry.icon_pack("old-pack").is_none());
        assert_eq!(
            registry
                .icon_pack("new-pack")
                .map(|pack| pack.module_id.as_str()),
            Some("@mesh/icons-changing")
        );
    }

    #[test]
    fn replacement_rejects_duplicate_chain_entries_without_touching_snapshot() {
        let mut registry = registry();
        registry
            .replace_bindings(
                vec![IconPackBindings {
                    pack_id: "stable".into(),
                    module_id: "@mesh/stable".into(),
                    mappings: HashMap::new(),
                    vocabularies: HashMap::new(),
                    axes: SupportedAxes::default(),
                    font_aliases: HashMap::new(),
                }],
                Vec::new(),
                None,
            )
            .unwrap();
        let result = registry.replace_bindings(
            Vec::new(),
            vec![(
                "frontend".into(),
                FrontendIconBindings {
                    declared_pack_chain: vec!["duplicate".into(), "duplicate".into()],
                    ..Default::default()
                },
            )],
            None,
        );
        assert!(result.is_err());
        assert!(registry.icon_pack("stable").is_some());
    }

    #[test]
    fn user_override_wins_over_author_and_pack_chain() {
        let td = tempdir().unwrap();
        let user_icon = td.path().join("user.svg");
        let pack_icon = td.path().join("pack.svg");
        for p in [&user_icon, &pack_icon] {
            fs::write(
                p,
                r#"<svg xmlns="http://www.w3.org/2000/svg" width="8" height="8"><rect width="8" height="8"/></svg>"#,
            )
            .unwrap();
        }

        let mut reg = registry_with_theme(td.path(), "files", &["pack"]);
        let mut mappings = HashMap::new();
        mappings.insert("home".into(), "files/pack".into());
        reg.set_icon_pack(IconPackBindings {
            pack_id: "p".into(),
            module_id: "@mesh/icons-p".into(),
            mappings,
            vocabularies: HashMap::new(),
            axes: SupportedAxes::default(),
            font_aliases: HashMap::new(),
        });

        let mut user_overrides = HashMap::new();
        user_overrides.insert("home".into(), user_icon.to_string_lossy().to_string());
        reg.set_frontend_bindings(
            "navigation-bar",
            FrontendIconBindings {
                declared_pack_chain: vec!["@mesh/icons-p".into()],
                user_overrides,
                ..Default::default()
            },
        );

        let result = reg.resolve_for_module("navigation-bar", "home", 8);
        let ResolvedTarget::File(path) = result.target().unwrap() else {
            panic!("expected file target");
        };
        assert!(path.ends_with("user.svg"));
    }

    #[test]
    fn shell_default_pack_is_prepended() {
        let td = tempdir().unwrap();
        let mut reg = registry_with_theme(td.path(), "files", &["default", "other"]);

        let mut default_map = HashMap::new();
        default_map.insert("home".into(), "files/default".into());
        reg.set_icon_pack(IconPackBindings {
            pack_id: "default".into(),
            module_id: "@mesh/icons-default".into(),
            mappings: default_map,
            vocabularies: HashMap::new(),
            axes: SupportedAxes::default(),
            font_aliases: HashMap::new(),
        });

        let mut other_map = HashMap::new();
        other_map.insert("home".into(), "files/other".into());
        reg.set_icon_pack(IconPackBindings {
            pack_id: "other".into(),
            module_id: "@mesh/icons-other".into(),
            mappings: other_map,
            vocabularies: HashMap::new(),
            axes: SupportedAxes::default(),
            font_aliases: HashMap::new(),
        });

        reg.set_shell_default_pack(Some("@mesh/icons-default".into()));
        reg.set_frontend_bindings(
            "frontend",
            FrontendIconBindings {
                declared_pack_chain: vec!["@mesh/icons-other".into()],
                ..Default::default()
            },
        );

        let result = reg.resolve_for_module("frontend", "home", 8);
        let ResolvedTarget::File(path) = result.target().unwrap() else {
            panic!("expected file target");
        };
        assert!(path.ends_with("default.svg"), "got {}", path.display());
    }

    #[test]
    fn replacement_rejects_duplicate_pack_ids_without_touching_live_bindings() {
        let mut registry = registry();
        registry.set_icon_pack(IconPackBindings {
            pack_id: "stable".into(),
            module_id: "@mesh/stable-icons".into(),
            mappings: HashMap::new(),
            vocabularies: HashMap::new(),
            axes: SupportedAxes::default(),
            font_aliases: HashMap::new(),
        });

        let result = registry.replace_bindings(
            vec![
                IconPackBindings {
                    pack_id: "duplicate".into(),
                    module_id: "@mesh/one".into(),
                    mappings: HashMap::new(),
                    vocabularies: HashMap::new(),
                    axes: SupportedAxes::default(),
                    font_aliases: HashMap::new(),
                },
                IconPackBindings {
                    pack_id: "duplicate".into(),
                    module_id: "@mesh/two".into(),
                    mappings: HashMap::new(),
                    vocabularies: HashMap::new(),
                    axes: SupportedAxes::default(),
                    font_aliases: HashMap::new(),
                },
            ],
            Vec::new(),
            None,
        );

        assert!(result.is_err());
        assert!(registry.icon_pack("stable").is_some());
        assert!(registry.icon_pack("duplicate").is_none());

        let result = registry.replace_bindings(
            vec![IconPackBindings {
                pack_id: "Not-Canonical".into(),
                module_id: "@mesh/not-canonical".into(),
                mappings: HashMap::new(),
                vocabularies: HashMap::new(),
                axes: SupportedAxes::default(),
                font_aliases: HashMap::new(),
            }],
            Vec::new(),
            None,
        );
        assert!(result.is_err());
        assert!(registry.icon_pack("stable").is_some());
    }

    #[test]
    fn font_aliases_resolve_only_from_the_mapping_owner_pack() {
        let pack_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../../modules/icon-packs/material-symbols");
        let font_path = pack_dir.join("assets/MaterialSymbolsRounded.ttf");
        let glyph_map_path = pack_dir.join("codepoints.json");
        let other_map = tempdir().unwrap();
        let other_glyph_map = other_map.path().join("codepoints.json");
        fs::write(&other_glyph_map, r#"{"settings":""}"#).unwrap();

        let font_asset = |glyph_map_path| FontAsset {
            family: "Material Symbols Rounded".into(),
            glyph_map_path: Some(glyph_map_path),
            resolved_font_path: Some(font_path.clone()),
            prepared_font: None,
            font_fingerprint: None,
            prepared_glyphs: None,
        };
        let mut registry = registry();
        registry
            .replace_bindings(
                vec![
                    IconPackBindings {
                        pack_id: "pack-a".into(),
                        module_id: "@mesh/pack-a".into(),
                        mappings: HashMap::from([(
                            "settings".into(),
                            IconMapping {
                                target: "ms/settings".into(),
                                multicolor: true,
                            },
                        )]),
                        vocabularies: HashMap::new(),
                        axes: SupportedAxes::default(),
                        font_aliases: HashMap::from([("ms".into(), font_asset(glyph_map_path))]),
                    },
                    IconPackBindings {
                        pack_id: "pack-b".into(),
                        module_id: "@mesh/pack-b".into(),
                        mappings: HashMap::new(),
                        vocabularies: HashMap::new(),
                        axes: SupportedAxes::default(),
                        font_aliases: HashMap::from([("ms".into(), font_asset(other_glyph_map))]),
                    },
                ],
                vec![(
                    "frontend".into(),
                    FrontendIconBindings {
                        declared_pack_chain: vec!["@mesh/pack-a".into()],
                        ..Default::default()
                    },
                )],
                None,
            )
            .unwrap();

        let result = registry.resolve_for_module("frontend", "settings", 24);
        let IconResolution::Found {
            target: ResolvedTarget::Glyph { codepoint, .. },
            multicolor,
            provenance,
            ..
        } = result
        else {
            panic!("expected owner-scoped font alias resolution");
        };
        assert_eq!(codepoint, 0xe8b8);
        assert!(multicolor);
        assert_eq!(provenance.owner_module.as_deref(), Some("@mesh/pack-a"));
        assert_eq!(provenance.pack_id.as_deref(), Some("pack-a"));
        assert_eq!(provenance.fallback_stage, "exact");
    }

    #[test]
    fn prepared_font_alias_does_not_reopen_a_replaced_source_file() {
        let temp = tempdir().unwrap();
        let font_path = temp.path().join("prepared.ttf");
        fs::write(&font_path, b"old font bytes").unwrap();
        let mut registry = registry();
        registry
            .replace_bindings(
                vec![IconPackBindings {
                    pack_id: "prepared".into(),
                    module_id: "@mesh/prepared-icons".into(),
                    mappings: HashMap::from([("settings".into(), "font/settings".into())]),
                    vocabularies: HashMap::new(),
                    axes: SupportedAxes::default(),
                    font_aliases: HashMap::from([(
                        "font".into(),
                        FontAsset {
                            family: "Prepared Icons".into(),
                            glyph_map_path: None,
                            resolved_font_path: Some(font_path.clone()),
                            prepared_font: Some(std::sync::Arc::from(&b"prepared font bytes"[..])),
                            font_fingerprint: None,
                            prepared_glyphs: Some(std::sync::Arc::new(HashMap::from([(
                                "settings".into(),
                                0xe000,
                            )]))),
                        },
                    )]),
                }],
                vec![(
                    "frontend".into(),
                    FrontendIconBindings {
                        declared_pack_chain: vec!["@mesh/prepared-icons".into()],
                        ..Default::default()
                    },
                )],
                None,
            )
            .unwrap();
        fs::remove_file(&font_path).unwrap();

        let IconResolution::Found {
            target:
                ResolvedTarget::Glyph {
                    font_bytes: Some(bytes),
                    codepoint,
                    ..
                },
            ..
        } = registry
            .prepared_resolution("frontend", "settings", 24)
            .expect("prepared glyph must not need filesystem lookup")
        else {
            panic!("expected prepared font bytes to survive source replacement");
        };
        assert_eq!(bytes.as_ref(), b"prepared font bytes");
        assert_eq!(codepoint, 0xe000);
    }

    #[test]
    fn system_xdg_theme_can_be_the_shell_default_without_a_mesh_module() {
        let td = tempdir().unwrap();
        let mut reg = registry_with_theme(td.path(), "Ocean", &["settings"]);
        reg.set_shell_default_pack(Some("Ocean".into()));
        reg.set_frontend_bindings("settings", FrontendIconBindings::default());

        let result = reg.resolve_for_module("settings", "settings", 8);
        let ResolvedTarget::File(path) = result.target().unwrap() else {
            panic!("expected file target");
        };
        assert!(path.ends_with("settings.svg"), "got {}", path.display());
    }
}
