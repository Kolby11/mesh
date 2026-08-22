use crate::bindings::{FrontendIconBindings, IconPackBindings, parse_target};
use crate::config::{IconConfig, IconPackRoot};
use crate::xdg;
use anyhow::{Result, bail};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;

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

#[derive(Debug, Clone)]
pub struct IconRegistry {
    /// Underlying file/font pack roots discovered from XDG dirs and
    /// registered ad-hoc by modules. Used by the resolver to look up
    /// `<asset-pack>/<asset-name>` targets.
    config: IconConfig,
    /// Loaded icon-pack binding modules keyed by their short pack id
    /// (`mesh.icon_pack.id`).
    icon_packs: HashMap<String, IconPackBindings>,
    /// Lookup index: full module id → short pack id.
    pack_id_by_module: HashMap<String, String>,
    /// Per-frontend resolution context.
    frontends: HashMap<String, FrontendIconBindings>,
    /// Shell-wide default icon-pack module id.
    shell_default_pack_module: Option<String>,
    generation: u64,
    cache: HashMap<IconCacheKey, IconResolution>,
    cache_order: VecDeque<IconCacheKey>,
    warned_misses: HashSet<(String, String)>,
    warned_miss_order: VecDeque<(String, String)>,
}

const ICON_REGISTRY_CACHE_CAPACITY: usize = 2048;
const ICON_WARNED_MISS_CAPACITY: usize = 2048;

fn canonical_pack_id(value: &str) -> Result<&str> {
    if value.is_empty()
        || value.trim() != value
        || value != value.to_ascii_lowercase()
        || value.contains('/')
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        bail!("icon pack id '{value}' is not canonical");
    }
    Ok(value)
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct IconCacheKey {
    generation: u64,
    module_id: String,
    semantic_name: String,
    size: u32,
}

impl IconRegistry {
    pub fn from_config(config: IconConfig) -> Result<Self> {
        config.validate()?;
        Ok(Self {
            config,
            icon_packs: HashMap::new(),
            pack_id_by_module: HashMap::new(),
            frontends: HashMap::new(),
            shell_default_pack_module: None,
            generation: 0,
            cache: HashMap::new(),
            cache_order: VecDeque::new(),
            warned_misses: HashSet::new(),
            warned_miss_order: VecDeque::new(),
        })
    }

    pub fn set_config(&mut self, config: IconConfig) -> Result<()> {
        config.validate()?;
        self.config = config;
        self.bump_generation();
        Ok(())
    }

    pub fn register_pack(&mut self, pack: IconPackRoot) -> Result<bool> {
        if pack.id.trim().is_empty() {
            bail!("pack id must not be empty");
        }
        if self.config.pack(&pack.id).is_some() {
            return Ok(false);
        }
        if let Some(root) = &pack.root
            && root.as_os_str().is_empty()
        {
            bail!("pack '{}' root must not be empty", pack.id);
        }
        if pack.theme.trim().is_empty() {
            bail!("pack '{}' theme must not be empty", pack.id);
        }
        self.config.packs.push(pack);
        self.bump_generation();
        Ok(true)
    }

    /// Register or replace an icon-pack binding module's bindings.
    pub fn set_icon_pack(&mut self, bindings: IconPackBindings) {
        let pack_id = bindings.pack_id.clone();
        let module_id = bindings.module_id.clone();
        self.pack_id_by_module
            .insert(module_id.clone(), pack_id.clone());
        // If the same module previously claimed a different pack id,
        // remove that stale alias.
        for (other_module, other_pack) in self.pack_id_by_module.clone() {
            if other_module != module_id && other_pack == pack_id {
                self.pack_id_by_module.remove(&other_module);
            }
        }
        self.icon_packs.insert(pack_id, bindings);
        self.bump_generation();
    }

    /// Atomically replace all graph-authorized binding modules. The system XDG
    /// config remains installed, while module-owned packs and frontend
    /// contexts are swapped as one candidate so a failed graph/profile
    /// preparation cannot leave a mixed registry.
    pub fn replace_bindings(
        &mut self,
        icon_packs: Vec<IconPackBindings>,
        frontends: Vec<(String, FrontendIconBindings)>,
        shell_default_pack: Option<String>,
    ) -> Result<()> {
        let mut next_packs = HashMap::new();
        let mut next_pack_by_module = HashMap::new();
        for bindings in icon_packs {
            canonical_pack_id(&bindings.pack_id)?;
            if bindings.module_id.trim().is_empty() {
                bail!("icon pack module id must not be empty");
            }
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

        let mut next_frontends = HashMap::new();
        for (module_id, bindings) in frontends {
            if module_id.trim().is_empty() {
                bail!("frontend icon binding module id must not be empty");
            }
            if next_frontends.insert(module_id.clone(), bindings).is_some() {
                bail!("duplicate frontend icon binding module '{module_id}'");
            }
        }

        self.icon_packs = next_packs;
        self.pack_id_by_module = next_pack_by_module;
        self.frontends = next_frontends;
        self.shell_default_pack_module = shell_default_pack;
        self.bump_generation();
        Ok(())
    }

    pub fn remove_icon_pack(&mut self, module_id: &str) {
        if let Some(pack_id) = self.pack_id_by_module.remove(module_id) {
            self.icon_packs.remove(&pack_id);
            self.bump_generation();
        }
    }

    /// Register or replace a frontend's resolution context.
    pub fn set_frontend_bindings(
        &mut self,
        module_id: impl Into<String>,
        bindings: FrontendIconBindings,
    ) {
        self.frontends.insert(module_id.into(), bindings);
        self.bump_generation();
    }

    pub fn remove_frontend_bindings(&mut self, module_id: &str) {
        if self.frontends.remove(module_id).is_some() {
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
        self.warned_misses.clear();
        self.warned_miss_order.clear();
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
        self.cache_order.retain(|existing| existing != &cache_key);
        self.cache_order.push_back(cache_key.clone());
        self.cache.insert(cache_key, resolution.clone());
        while self.cache.len() > ICON_REGISTRY_CACHE_CAPACITY {
            let Some(evicted) = self.cache_order.pop_front() else {
                break;
            };
            self.cache.remove(&evicted);
        }

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
            if let IconResolution::Missing { tried, .. } = &resolution {
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

        resolution
    }

    fn record_warned_miss(&mut self, key: (String, String)) -> bool {
        if !self.warned_misses.insert(key.clone()) {
            return false;
        }
        self.warned_miss_order.push_back(key);
        while self.warned_misses.len() > ICON_WARNED_MISS_CAPACITY {
            let Some(evicted) = self.warned_miss_order.pop_front() else {
                break;
            };
            self.warned_misses.remove(&evicted);
        }
        true
    }

    fn resolve_uncached(&self, module_id: &str, semantic_name: &str, size: u32) -> IconResolution {
        let mut tried = Vec::new();
        let frontend = self.frontends.get(module_id);

        // 1. User override
        if let Some(frontend) = frontend {
            if let Some(target) = frontend.user_overrides.get(semantic_name)
                && let Some(found) = self.try_target(
                    target,
                    false,
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
                    false,
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
            if let Some(found) = xdg::find_icon_in_theme("hicolor", &fallback_name, size) {
                let mapping = format!("hicolor:{fallback_name}");
                tried.push(mapping.clone());
                return IconResolution::Found {
                    semantic_name: semantic_name.into(),
                    candidate: mapping.clone(),
                    target: ResolvedTarget::File(found),
                    multicolor: false,
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
        logical_name: &str,
        semantic_name: &str,
        size: u32,
        tried: &mut Vec<String>,
        fallback_stage: &str,
    ) -> Option<IconResolution> {
        let pack = self.icon_packs.get(pack_id)?;
        let target = pack.mappings.get(logical_name)?;
        self.try_target(
            &target.target,
            target.multicolor,
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
        let pack = self.config.pack(pack_id)?;
        let mapping = format!("system-pack:{pack_id}/{lookup_name}");
        tried.push(mapping.clone());
        let target = xdg::find_icon_in_pack(pack, lookup_name, size)?;
        Some(IconResolution::Found {
            semantic_name: semantic_name.into(),
            candidate: mapping.clone(),
            target,
            multicolor: false,
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
        multicolor: bool,
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
        if p.is_absolute() && p.is_file() {
            return Some(IconResolution::Found {
                semantic_name: semantic_name.into(),
                candidate: mapping_label.clone(),
                target: ResolvedTarget::File(p.to_path_buf()),
                multicolor,
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
                multicolor,
                provenance: self.provenance(Some(owner_pack), mapping_label, fallback_stage),
            });
        }

        // Asset-pack registered as an XDG/file pack in the IconConfig
        if let Some(pack) = self.config.pack(asset_pack)
            && let Some(target) = xdg::find_icon_in_pack(pack, asset_name, size)
        {
            return Some(IconResolution::Found {
                semantic_name: semantic_name.into(),
                candidate: mapping_label.clone(),
                target,
                multicolor,
                provenance: self.provenance(owner_pack, mapping_label, fallback_stage),
            });
        }

        // Asset-pack as a bare XDG theme name on the system
        if let Some(path) = xdg::find_icon_in_theme(asset_pack, asset_name, size) {
            return Some(IconResolution::Found {
                semantic_name: semantic_name.into(),
                candidate: mapping_label.clone(),
                target: ResolvedTarget::File(path),
                multicolor,
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
        if !font_path.is_file() {
            return None;
        }
        let glyph_map_path = font_asset.glyph_map_path.as_ref()?;
        let codepoint = crate::xdg::lookup_glyph_codepoint(glyph_map_path, glyph_name)?;
        Some(ResolvedTarget::Glyph {
            font_path,
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
    use tempfile::tempdir;

    fn empty_config() -> IconConfig {
        IconConfig::from_toml_str(
            r#"
active_profile = "system"

[[packs]]
id = "system"
theme = "hicolor"

[profiles.system.icons]
nothing = ["system:nothing"]
"#,
        )
        .unwrap()
    }

    fn registry() -> IconRegistry {
        IconRegistry::from_config(empty_config()).unwrap()
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
        let td = tempdir().unwrap();
        let icon = td.path().join("home.svg");
        fs::write(
            &icon,
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16"><rect width="16" height="16"/></svg>"#,
        )
        .unwrap();

        let mut reg = registry();
        reg.register_pack(IconPackRoot {
            id: "lucide-files".into(),
            root: Some(td.path().to_path_buf()),
            theme: "hicolor".into(),
            kind: crate::IconPackKind::Xdg,
        })
        .unwrap();

        let mut mappings = HashMap::new();
        mappings.insert("home".into(), "lucide-files/home".into());
        reg.set_icon_pack(IconPackBindings {
            pack_id: "lucide".into(),
            module_id: "@mesh/icons-lucide".into(),
            mappings,
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
    fn semantic_resolution_generalizes_dash_suffixes_in_chain_order() {
        let td = tempdir().unwrap();
        let icon = td.path().join("base.svg");
        fs::write(
            &icon,
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="8" height="8"/>"#,
        )
        .unwrap();
        let mut registry = registry();
        registry
            .register_pack(IconPackRoot {
                id: "files".into(),
                root: Some(td.path().to_path_buf()),
                theme: "hicolor".into(),
                kind: crate::IconPackKind::Xdg,
            })
            .unwrap();
        registry.set_icon_pack(IconPackBindings {
            pack_id: "fallbacks".into(),
            module_id: "@mesh/fallbacks".into(),
            mappings: HashMap::from([("network-wireless".into(), "files/base".into())]),
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

        let mut reg = registry();
        reg.register_pack(IconPackRoot {
            id: "files".into(),
            root: Some(td.path().to_path_buf()),
            theme: "hicolor".into(),
            kind: crate::IconPackKind::Xdg,
        })
        .unwrap();
        let mut mappings = HashMap::new();
        mappings.insert("home".into(), "files/pack".into());
        reg.set_icon_pack(IconPackBindings {
            pack_id: "p".into(),
            module_id: "@mesh/icons-p".into(),
            mappings,
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
        let default_icon = td.path().join("default.svg");
        let other_icon = td.path().join("other.svg");
        for p in [&default_icon, &other_icon] {
            fs::write(
                p,
                r#"<svg xmlns="http://www.w3.org/2000/svg" width="8" height="8"><rect width="8" height="8"/></svg>"#,
            )
            .unwrap();
        }

        let mut reg = registry();
        reg.register_pack(IconPackRoot {
            id: "files".into(),
            root: Some(td.path().to_path_buf()),
            theme: "hicolor".into(),
            kind: crate::IconPackKind::Xdg,
        })
        .unwrap();

        let mut default_map = HashMap::new();
        default_map.insert("home".into(), "files/default".into());
        reg.set_icon_pack(IconPackBindings {
            pack_id: "default".into(),
            module_id: "@mesh/icons-default".into(),
            mappings: default_map,
            axes: SupportedAxes::default(),
            font_aliases: HashMap::new(),
        });

        let mut other_map = HashMap::new();
        other_map.insert("home".into(), "files/other".into());
        reg.set_icon_pack(IconPackBindings {
            pack_id: "other".into(),
            module_id: "@mesh/icons-other".into(),
            mappings: other_map,
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
        let mut registry = IconRegistry::from_config(empty_config()).unwrap();
        registry.set_icon_pack(IconPackBindings {
            pack_id: "stable".into(),
            module_id: "@mesh/stable-icons".into(),
            mappings: HashMap::new(),
            axes: SupportedAxes::default(),
            font_aliases: HashMap::new(),
        });

        let result = registry.replace_bindings(
            vec![
                IconPackBindings {
                    pack_id: "duplicate".into(),
                    module_id: "@mesh/one".into(),
                    mappings: HashMap::new(),
                    axes: SupportedAxes::default(),
                    font_aliases: HashMap::new(),
                },
                IconPackBindings {
                    pack_id: "duplicate".into(),
                    module_id: "@mesh/two".into(),
                    mappings: HashMap::new(),
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
                        axes: SupportedAxes::default(),
                        font_aliases: HashMap::from([("ms".into(), font_asset(glyph_map_path))]),
                    },
                    IconPackBindings {
                        pack_id: "pack-b".into(),
                        module_id: "@mesh/pack-b".into(),
                        mappings: HashMap::new(),
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
    fn system_xdg_theme_can_be_the_shell_default_without_a_mesh_module() {
        let td = tempdir().unwrap();
        let theme = td.path().join("Ocean");
        fs::create_dir_all(&theme).unwrap();
        fs::write(
            theme.join("settings.svg"),
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="8" height="8"/>"#,
        )
        .unwrap();

        let mut reg = registry();
        reg.register_pack(IconPackRoot {
            id: "Ocean".into(),
            root: Some(theme),
            theme: "Ocean".into(),
            kind: crate::IconPackKind::Xdg,
        })
        .unwrap();
        reg.set_shell_default_pack(Some("Ocean".into()));
        reg.set_frontend_bindings("settings", FrontendIconBindings::default());

        let result = reg.resolve_for_module("settings", "settings", 8);
        let ResolvedTarget::File(path) = result.target().unwrap() else {
            panic!("expected file target");
        };
        assert!(path.ends_with("settings.svg"), "got {}", path.display());
    }
}
