use crate::config::{IconPackKind, IconPackRoot};
use crate::registry::{ResolvedTarget, SupportedAxes};
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::SystemTime;

static SUPPORTED_AXES_CACHE: OnceLock<Mutex<SupportedAxesCache>> = OnceLock::new();
static XDG_ICON_LOOKUP_CACHE: OnceLock<Mutex<XdgIconLookupCache>> = OnceLock::new();
const SUPPORTED_AXES_CACHE_CAPACITY: usize = 128;
const XDG_ICON_LOOKUP_CACHE_CAPACITY: usize = 2048;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FontFreshness {
    len: u64,
    modified_nanos: u128,
}

#[derive(Debug, Clone, Copy)]
struct CachedSupportedAxes {
    freshness: FontFreshness,
    axes: SupportedAxes,
}

#[derive(Debug, Default)]
struct SupportedAxesCache {
    entries: HashMap<PathBuf, CachedSupportedAxes>,
    order: VecDeque<PathBuf>,
}

impl SupportedAxesCache {
    fn get(&mut self, path: &Path, freshness: FontFreshness) -> Option<SupportedAxes> {
        let axes = self
            .entries
            .get(path)
            .filter(|cached| cached.freshness == freshness)
            .map(|cached| cached.axes);
        if axes.is_some() {
            self.order.retain(|existing| existing != path);
            self.order.push_back(path.to_path_buf());
        }
        axes
    }

    fn insert(&mut self, path: PathBuf, value: CachedSupportedAxes) {
        self.order.retain(|existing| existing != &path);
        self.order.push_back(path.clone());
        self.entries.insert(path, value);
        while self.entries.len() > SUPPORTED_AXES_CACHE_CAPACITY {
            let Some(evicted) = self.order.pop_front() else {
                break;
            };
            self.entries.remove(&evicted);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct XdgIconLookupKey {
    root: Option<PathBuf>,
    theme: String,
    asset_name: String,
    size: u32,
}

#[derive(Debug, Default)]
struct XdgIconLookupCache {
    entries: HashMap<XdgIconLookupKey, Option<PathBuf>>,
    order: VecDeque<XdgIconLookupKey>,
}

impl XdgIconLookupCache {
    fn get(&mut self, key: &XdgIconLookupKey) -> Option<Option<PathBuf>> {
        let value = self.entries.get(key).cloned();
        if value.is_some() {
            self.order.retain(|existing| existing != key);
            self.order.push_back(key.clone());
        }
        value
    }

    fn insert(&mut self, key: XdgIconLookupKey, value: Option<PathBuf>) {
        self.order.retain(|existing| existing != &key);
        self.order.push_back(key.clone());
        self.entries.insert(key, value);
        while self.entries.len() > XDG_ICON_LOOKUP_CACHE_CAPACITY {
            let Some(evicted) = self.order.pop_front() else {
                break;
            };
            self.entries.remove(&evicted);
        }
    }
}

pub fn find_icon_in_pack(
    pack: &IconPackRoot,
    asset_name: &str,
    size: u32,
) -> Option<ResolvedTarget> {
    if let IconPackKind::Font {
        font_file,
        glyph_map,
    } = &pack.kind
    {
        return resolve_font_glyph(pack, font_file, glyph_map, asset_name);
    }

    let path = lookup_xdg_icon_in_pack(pack, asset_name, size)?;

    Some(ResolvedTarget::File(path))
}

fn lookup_xdg_icon_in_pack(pack: &IconPackRoot, asset_name: &str, size: u32) -> Option<PathBuf> {
    let key = XdgIconLookupKey {
        root: pack.root.clone(),
        theme: theme_name(pack).to_string(),
        asset_name: asset_name.to_string(),
        size: size.max(1),
    };
    let cache = XDG_ICON_LOOKUP_CACHE.get_or_init(|| Mutex::new(XdgIconLookupCache::default()));
    if let Ok(mut guard) = cache.lock()
        && let Some(cached) = guard.get(&key)
    {
        return cached;
    }

    let path = search_for_pack(pack)
        .search()
        .icons()
        .find_icon(asset_name, key.size, 1, &key.theme)
        .map(|icon| icon.path().to_path_buf())
        .or_else(|| find_direct_file(pack, asset_name));

    if let Ok(mut guard) = cache.lock() {
        guard.insert(key, path.clone());
    }
    path
}

/// Look up a glyph codepoint by name from a font pack's codepoints file.
/// Used by the binding resolver when a mapping target points at a font
/// alias declared in `mesh.icon_pack.requires.fonts`.
pub fn lookup_glyph_codepoint(glyph_map_path: &Path, glyph_name: &str) -> Option<u32> {
    lookup_codepoint(glyph_map_path, glyph_name)
}

/// Look up an icon in any installed theme on the system XDG search path.
/// Used as a last-resort fallback when neither module bindings nor the
/// active profile produce a hit.
pub fn find_icon_in_theme(theme: &str, asset_name: &str, size: u32) -> Option<PathBuf> {
    let key = XdgIconLookupKey {
        root: None,
        theme: theme.to_string(),
        asset_name: asset_name.to_string(),
        size: size.max(1),
    };
    let cache = XDG_ICON_LOOKUP_CACHE.get_or_init(|| Mutex::new(XdgIconLookupCache::default()));
    if let Ok(mut guard) = cache.lock()
        && let Some(cached) = guard.get(&key)
    {
        return cached;
    }

    let path = icon::IconSearch::new()
        .search()
        .icons()
        .find_icon(asset_name, key.size, 1, &key.theme)
        .map(|icon| icon.path().to_path_buf());

    if let Ok(mut guard) = cache.lock() {
        guard.insert(key, path.clone());
    }
    path
}

fn resolve_font_glyph(
    pack: &IconPackRoot,
    font_file: &str,
    glyph_map: &str,
    asset_name: &str,
) -> Option<ResolvedTarget> {
    let root = pack.root.as_ref()?;
    let font_path = resolve_pack_path(root, font_file);
    let glyph_map_path = resolve_pack_path(root, glyph_map);
    if !font_path.is_file() {
        return None;
    }
    let codepoint = lookup_codepoint(&glyph_map_path, asset_name)?;
    let supported_axes = detect_supported_axes(&font_path);
    Some(ResolvedTarget::Glyph {
        font_path,
        codepoint,
        supported_axes,
    })
}

/// Resolve a path declared inside `mesh-pack.json` against the pack root,
/// honoring shell-style `~` expansion and absolute paths.
fn resolve_pack_path(root: &Path, declared: &str) -> PathBuf {
    let trimmed = declared.trim();
    if let Some(rest) = trimmed.strip_prefix("~/")
        && let Some(home) = std::env::var_os("HOME")
    {
        return PathBuf::from(home).join(rest);
    }
    let candidate = PathBuf::from(trimmed);
    if candidate.is_absolute() {
        return candidate;
    }
    root.join(candidate)
}

static CODEPOINTS_CACHE: OnceLock<Mutex<CodepointsCache>> = OnceLock::new();
const CODEPOINTS_CACHE_CAPACITY: usize = 128;

#[derive(Debug, Default)]
struct CodepointsCache {
    entries: HashMap<PathBuf, HashMap<String, u32>>,
    order: VecDeque<PathBuf>,
}

impl CodepointsCache {
    fn get(&mut self, path: &Path, glyph_name: &str) -> Option<Option<u32>> {
        let value = self
            .entries
            .get(path)
            .map(|codepoints| codepoints.get(glyph_name).copied());
        if value.is_some() {
            self.order.retain(|existing| existing != path);
            self.order.push_back(path.to_path_buf());
        }
        value
    }

    fn insert(&mut self, path: PathBuf, value: HashMap<String, u32>) {
        self.order.retain(|existing| existing != &path);
        self.order.push_back(path.clone());
        self.entries.insert(path, value);
        while self.entries.len() > CODEPOINTS_CACHE_CAPACITY {
            let Some(evicted) = self.order.pop_front() else {
                break;
            };
            self.entries.remove(&evicted);
        }
    }
}

fn lookup_codepoint(path: &Path, glyph_name: &str) -> Option<u32> {
    let cache = CODEPOINTS_CACHE.get_or_init(|| Mutex::new(CodepointsCache::default()));
    {
        let mut guard = cache.lock().ok()?;
        if let Some(codepoint) = guard.get(path, glyph_name) {
            return codepoint;
        }
    }
    let parsed = parse_codepoints_file(path)?;
    let codepoint = parsed.get(glyph_name).copied();
    if let Ok(mut guard) = cache.lock() {
        guard.insert(path.to_path_buf(), parsed);
    }
    codepoint
}

fn parse_codepoints_file(path: &Path) -> Option<HashMap<String, u32>> {
    let raw = std::fs::read_to_string(path).ok()?;
    // Preferred form: JSON `{ "name": "\uXXXX", ... }`. Each value is a
    // single-character string whose code point is the glyph index in the
    // PUA region.
    if let Ok(map) = serde_json::from_str::<HashMap<String, String>>(&raw) {
        let codepoints = map
            .into_iter()
            .filter_map(|(name, value)| value.chars().next().map(|c| (name, c as u32)))
            .collect();
        return Some(codepoints);
    }
    // Fallback: Google's `name codepoint` text format (e.g. `volume_up e050`).
    let mut map = HashMap::new();
    for line in raw.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.split_whitespace();
        let name = parts.next()?;
        let cp_hex = parts.next()?;
        if let Ok(cp) = u32::from_str_radix(cp_hex, 16) {
            map.insert(name.to_string(), cp);
        }
    }
    if map.is_empty() {
        tracing::warn!(
            "icon pack codepoints file at {} could not be parsed as JSON or text",
            path.display()
        );
        None
    } else {
        Some(map)
    }
}

/// Inspect the font's `fvar` table to discover which variable-font axes
/// it actually exposes. Returns conservative defaults (everything off)
/// when the font can't be parsed; the painter then silently ignores
/// CSS `--icon-*` properties that don't match.
fn detect_supported_axes(font_path: &Path) -> SupportedAxes {
    let freshness = font_freshness(font_path);
    if let Some(freshness) = freshness {
        let cache = SUPPORTED_AXES_CACHE.get_or_init(|| Mutex::new(SupportedAxesCache::default()));
        if let Ok(mut guard) = cache.lock()
            && let Some(axes) = guard.get(font_path, freshness)
        {
            return axes;
        }
    }

    let bytes = match std::fs::read(font_path) {
        Ok(bytes) => bytes,
        Err(_) => return SupportedAxes::default(),
    };
    let face = match ttf_parser::Face::parse(&bytes, 0) {
        Ok(face) => face,
        Err(_) => return SupportedAxes::default(),
    };
    let mut axes = SupportedAxes::default();
    for axis in face.variation_axes() {
        let tag = axis.tag.to_bytes();
        match &tag {
            b"FILL" => axes.fill = true,
            b"wght" => axes.weight = true,
            b"GRAD" => axes.grade = true,
            b"opsz" => axes.optical_size = true,
            _ => {}
        }
    }
    if let Some(freshness) = freshness {
        let cache = SUPPORTED_AXES_CACHE.get_or_init(|| Mutex::new(SupportedAxesCache::default()));
        if let Ok(mut guard) = cache.lock() {
            guard.insert(
                font_path.to_path_buf(),
                CachedSupportedAxes { freshness, axes },
            );
        }
    }
    axes
}

fn font_freshness(path: &Path) -> Option<FontFreshness> {
    let metadata = std::fs::metadata(path).ok()?;
    let modified = metadata.modified().ok()?;
    let modified_nanos = modified
        .duration_since(SystemTime::UNIX_EPOCH)
        .ok()?
        .as_nanos();
    Some(FontFreshness {
        len: metadata.len(),
        modified_nanos,
    })
}

fn search_for_pack(pack: &IconPackRoot) -> icon::IconSearch {
    match &pack.root {
        Some(root) => icon::IconSearch::new_from(vec![xdg_base_dir_for_root(root)]),
        None => icon::IconSearch::new(),
    }
}

fn xdg_base_dir_for_root(root: &Path) -> PathBuf {
    if root.join("index.theme").is_file() {
        return root.parent().unwrap_or(root).to_path_buf();
    }
    root.to_path_buf()
}

fn theme_name(pack: &IconPackRoot) -> &str {
    if pack.theme != "hicolor" {
        return &pack.theme;
    }
    if let Some(root) = &pack.root
        && root.join("index.theme").is_file()
        && let Some(name) = root.file_name().and_then(|name| name.to_str())
    {
        return name;
    }
    &pack.theme
}

fn find_direct_file(pack: &IconPackRoot, asset_name: &str) -> Option<PathBuf> {
    let Some(root) = &pack.root else {
        return None;
    };
    ["svg", "png", "jpg", "jpeg", "bmp"]
        .into_iter()
        .map(|ext| root.join(format!("{asset_name}.{ext}")))
        .find(|candidate| candidate.is_file())
}
