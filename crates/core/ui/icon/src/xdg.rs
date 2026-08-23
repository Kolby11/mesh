use crate::config::{IconPackKind, IconPackRoot};
use crate::registry::{ResolvedTarget, SupportedAxes};
use mesh_core_resources::{
    ResourceFingerprint, ResourcePreparationToken, resource_fingerprint, resource_revision,
};
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

pub const MAX_GLYPH_MAP_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_GLYPH_MAP_ENTRIES: usize = 100_000;

static SUPPORTED_AXES_CACHE: OnceLock<Mutex<SupportedAxesCache>> = OnceLock::new();
static XDG_ICON_LOOKUP_CACHE: OnceLock<Mutex<XdgIconLookupCache>> = OnceLock::new();
const SUPPORTED_AXES_CACHE_CAPACITY: usize = 128;
const XDG_ICON_LOOKUP_CACHE_CAPACITY: usize = 2048;

type FontFreshness = ResourceFingerprint;

#[derive(Debug, Clone, Copy)]
struct CachedSupportedAxes {
    revision: u64,
    freshness: FontFreshness,
    axes: SupportedAxes,
}

#[derive(Debug, Default)]
struct SupportedAxesCache {
    entries: HashMap<PathBuf, CachedSupportedAxes>,
    order: VecDeque<PathBuf>,
}

impl SupportedAxesCache {
    fn get(
        &mut self,
        path: &Path,
        revision: u64,
        freshness: FontFreshness,
    ) -> Option<SupportedAxes> {
        let axes = self
            .entries
            .get(path)
            .filter(|cached| cached.revision == revision && cached.freshness == freshness)
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
    revision: u64,
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
        revision: resource_revision(),
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

/// Parse a complete glyph map during resource preparation. JSON maps contain
/// one Unicode scalar per glyph; the text fallback is Google's `name hex`
/// format. A malformed entry rejects the complete map so a candidate cannot
/// publish a pack whose aliases only fail later during rendering.
pub fn parse_glyph_map_bytes(bytes: &[u8]) -> Result<HashMap<String, u32>, String> {
    parse_glyph_map_bytes_with_cancellation(bytes, &ResourcePreparationToken::new())
}

/// Parse a glyph map while allowing a superseded resource candidate to stop
/// cooperatively. The input remains bounded by [`MAX_GLYPH_MAP_BYTES`], and
/// the token is checked for every entry/line so a large valid map does not
/// monopolize the preparation worker after cancellation.
pub fn parse_glyph_map_bytes_with_cancellation(
    bytes: &[u8],
    cancellation: &ResourcePreparationToken,
) -> Result<HashMap<String, u32>, String> {
    if cancellation.is_cancelled() {
        return Err("resource preparation cancelled".into());
    }
    if bytes.len() > MAX_GLYPH_MAP_BYTES {
        return Err(format!("glyph map exceeds {} bytes", MAX_GLYPH_MAP_BYTES));
    }
    let raw = std::str::from_utf8(bytes).map_err(|_| "glyph map is not UTF-8".to_string())?;
    let trimmed = raw.trim_start();
    if trimmed.starts_with('{') {
        let value: serde_json::Value = serde_json::from_str(raw)
            .map_err(|error| format!("glyph map JSON is invalid: {error}"))?;
        let object = value
            .as_object()
            .ok_or_else(|| "glyph map JSON must be an object".to_string())?;
        if object.len() > MAX_GLYPH_MAP_ENTRIES {
            return Err(format!(
                "glyph map contains more than {} entries",
                MAX_GLYPH_MAP_ENTRIES
            ));
        }
        let mut result = HashMap::with_capacity(object.len());
        for (name, value) in object {
            if cancellation.is_cancelled() {
                return Err("resource preparation cancelled".into());
            }
            if name.trim().is_empty() {
                return Err("glyph map contains an empty glyph name".into());
            }
            let value = value
                .as_str()
                .ok_or_else(|| format!("glyph '{name}' must map to one Unicode character"))?;
            let mut chars = value.chars();
            let Some(character) = chars.next() else {
                return Err(format!("glyph '{name}' maps to an empty value"));
            };
            if chars.next().is_some() {
                return Err(format!(
                    "glyph '{name}' must map to exactly one Unicode character"
                ));
            }
            if result.insert(name.clone(), character as u32).is_some() {
                return Err(format!("glyph map contains duplicate glyph '{name}'"));
            }
        }
        if result.is_empty() {
            return Err("glyph map contains no entries".into());
        }
        return Ok(result);
    }

    let mut result = HashMap::new();
    for (line_number, line) in raw.lines().enumerate() {
        if cancellation.is_cancelled() {
            return Err("resource preparation cancelled".into());
        }
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.split_whitespace();
        let name = parts
            .next()
            .ok_or_else(|| format!("glyph map line {} is missing a name", line_number + 1))?;
        let codepoint = parts
            .next()
            .ok_or_else(|| format!("glyph map line {} is missing a codepoint", line_number + 1))?;
        if parts.next().is_some() {
            return Err(format!(
                "glyph map line {} has more than two fields",
                line_number + 1
            ));
        }
        if result.len() >= MAX_GLYPH_MAP_ENTRIES {
            return Err(format!(
                "glyph map contains more than {} entries",
                MAX_GLYPH_MAP_ENTRIES
            ));
        }
        if name.trim().is_empty() {
            return Err(format!(
                "glyph map line {} has an empty name",
                line_number + 1
            ));
        }
        let codepoint = u32::from_str_radix(codepoint, 16).map_err(|_| {
            format!(
                "glyph map line {} has an invalid codepoint",
                line_number + 1
            )
        })?;
        if codepoint > char::MAX as u32
            || (0xD800..=0xDFFF).contains(&codepoint)
            || char::from_u32(codepoint).is_none()
        {
            return Err(format!(
                "glyph map line {} has an invalid Unicode codepoint",
                line_number + 1
            ));
        }
        if result.insert(name.to_string(), codepoint).is_some() {
            return Err(format!("glyph map contains duplicate glyph '{name}'"));
        }
    }
    if result.is_empty() {
        return Err("glyph map contains no entries".into());
    }
    Ok(result)
}

/// Validate a complete font file while the resource candidate is being
/// prepared. Rendering still owns rasterization, but it should never be the
/// first code path to discover that a published pack is not a font.
pub fn validate_font_bytes(bytes: &[u8]) -> Result<(), String> {
    let face = ttf_parser::Face::parse(bytes, 0)
        .map_err(|error| format!("font file is invalid: {error:?}"))?;
    if face.number_of_glyphs() == 0 {
        return Err("font file contains no glyphs".into());
    }
    Ok(())
}

/// Look up an icon in any installed theme on the system XDG search path.
/// Used as a last-resort fallback when neither module bindings nor the
/// active profile produce a hit.
pub fn find_icon_in_theme(theme: &str, asset_name: &str, size: u32) -> Option<PathBuf> {
    let key = XdgIconLookupKey {
        revision: resource_revision(),
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

    let catalog = mesh_core_resources::system_resource_catalog();
    let path = icon::IconSearch::new_from(catalog.icon_dirs.clone())
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
    let font_fingerprint = resource_fingerprint(&font_path);
    Some(ResolvedTarget::Glyph {
        font_path,
        font_bytes: None,
        font_fingerprint,
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

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct CodepointsCacheKey {
    path: PathBuf,
    revision: u64,
    fingerprint: FontFreshness,
}

#[derive(Debug, Default)]
struct CodepointsCache {
    entries: HashMap<CodepointsCacheKey, HashMap<String, u32>>,
    order: VecDeque<CodepointsCacheKey>,
}

impl CodepointsCache {
    fn get(&mut self, key: &CodepointsCacheKey, glyph_name: &str) -> Option<Option<u32>> {
        let value = self
            .entries
            .get(key)
            .map(|codepoints| codepoints.get(glyph_name).copied());
        if value.is_some() {
            self.order.retain(|existing| existing != key);
            self.order.push_back(key.clone());
        }
        value
    }

    fn insert(&mut self, key: CodepointsCacheKey, value: HashMap<String, u32>) {
        self.order.retain(|existing| existing != &key);
        self.order.push_back(key.clone());
        self.entries.insert(key, value);
        while self.entries.len() > CODEPOINTS_CACHE_CAPACITY {
            let Some(evicted) = self.order.pop_front() else {
                break;
            };
            self.entries.remove(&evicted);
        }
    }
}

fn lookup_codepoint(path: &Path, glyph_name: &str) -> Option<u32> {
    let fingerprint = font_freshness(path)?;
    let key = CodepointsCacheKey {
        path: path.to_path_buf(),
        revision: resource_revision(),
        fingerprint,
    };
    let cache = CODEPOINTS_CACHE.get_or_init(|| Mutex::new(CodepointsCache::default()));
    {
        let mut guard = cache.lock().ok()?;
        if let Some(codepoint) = guard.get(&key, glyph_name) {
            return codepoint;
        }
    }
    let parsed = parse_codepoints_file(path)?;
    let codepoint = parsed.get(glyph_name).copied();
    if let Ok(mut guard) = cache.lock() {
        guard.insert(key, parsed);
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
    let revision = resource_revision();
    let freshness = font_freshness(font_path);
    if let Some(freshness) = freshness {
        let cache = SUPPORTED_AXES_CACHE.get_or_init(|| Mutex::new(SupportedAxesCache::default()));
        if let Ok(mut guard) = cache.lock()
            && let Some(axes) = guard.get(font_path, revision, freshness)
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
                CachedSupportedAxes {
                    revision,
                    freshness,
                    axes,
                },
            );
        }
    }
    axes
}

fn font_freshness(path: &Path) -> Option<FontFreshness> {
    resource_fingerprint(path)
}

fn search_for_pack(pack: &IconPackRoot) -> icon::IconSearch {
    match &pack.root {
        Some(root) => icon::IconSearch::new_from(vec![xdg_base_dir_for_root(root)]),
        None => {
            let catalog = mesh_core_resources::system_resource_catalog();
            icon::IconSearch::new_from(catalog.icon_dirs.clone())
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_json_glyph_maps_without_truncating_unicode() {
        let glyphs =
            parse_glyph_map_bytes(r#"{"settings":"\uE000","wide":"😀"}"#.as_bytes()).unwrap();
        assert_eq!(glyphs.get("settings"), Some(&0xE000));
        assert_eq!(glyphs.get("wide"), Some(&0x1F600));
    }

    #[test]
    fn rejects_multi_character_json_values() {
        let error = parse_glyph_map_bytes(br#"{"settings":"ab"}"#).unwrap_err();
        assert!(error.contains("exactly one Unicode character"));
    }

    #[test]
    fn rejects_malformed_middle_text_entries() {
        let error =
            parse_glyph_map_bytes(b"settings e000\nvolume not-hex\nclose e001").unwrap_err();
        assert!(error.contains("line 2"));
    }

    #[test]
    fn cancelled_glyph_map_preparation_does_not_return_partial_entries() {
        let cancellation = ResourcePreparationToken::new();
        cancellation.cancel();
        let error = parse_glyph_map_bytes_with_cancellation(
            br#"{"settings":"\uE000","volume":"\uE001"}"#,
            &cancellation,
        )
        .unwrap_err();
        assert_eq!(error, "resource preparation cancelled");
    }

    #[test]
    fn rejects_invalid_font_bytes() {
        let error = validate_font_bytes(b"not a font").unwrap_err();
        assert!(error.contains("invalid"));
    }

    #[test]
    fn resource_revision_invalidates_negative_icon_lookup() {
        let temp = tempfile::tempdir().unwrap();
        let pack = IconPackRoot {
            id: "revision-test".into(),
            root: Some(temp.path().to_path_buf()),
            theme: "hicolor".into(),
            kind: IconPackKind::Xdg,
        };

        assert!(find_icon_in_pack(&pack, "appears-later", 24).is_none());
        std::fs::write(temp.path().join("appears-later.svg"), b"<svg/>").unwrap();

        mesh_core_resources::advance_resource_revision();
        assert_eq!(
            find_icon_in_pack(&pack, "appears-later", 24),
            Some(ResolvedTarget::File(temp.path().join("appears-later.svg")))
        );
    }

    #[test]
    fn resource_revision_invalidates_cached_glyph_maps() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("codepoints.json");
        std::fs::write(&path, r#"{"settings":"\uE000"}"#).unwrap();
        assert_eq!(lookup_glyph_codepoint(&path, "settings"), Some(0xE000));

        std::fs::write(&path, r#"{"settings":"\uE001"}"#).unwrap();
        mesh_core_resources::advance_resource_revision();
        assert_eq!(lookup_glyph_codepoint(&path, "settings"), Some(0xE001));
    }
}
