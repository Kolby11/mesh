use mesh_core_resources::{
    ResourceFingerprint, ResourcePreparationToken, SystemResourceCatalog, resource_fingerprint,
    resource_revision,
};
use std::collections::{HashMap, VecDeque};
use std::io::Read;
use std::mem::size_of;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

pub const MAX_GLYPH_MAP_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_GLYPH_MAP_ENTRIES: usize = 100_000;
pub const MAX_FONT_BYTES: usize = 64 * 1024 * 1024;

static XDG_ICON_LOOKUP_CACHE: OnceLock<Mutex<XdgIconLookupCache>> = OnceLock::new();
const XDG_ICON_LOOKUP_CACHE_CAPACITY: usize = 2048;
const XDG_ICON_LOOKUP_CACHE_MAX_BYTES: usize = 512 * 1024;

type FontFreshness = ResourceFingerprint;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct XdgIconLookupKey {
    revision: u64,
    theme: String,
    asset_name: String,
    size: u32,
}

#[derive(Debug, Default)]
struct XdgIconLookupCache {
    entries: HashMap<XdgIconLookupKey, Option<PathBuf>>,
    order: VecDeque<XdgIconLookupKey>,
    bytes: usize,
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
        let weight = xdg_lookup_cache_weight(&key, value.as_ref());
        if weight > XDG_ICON_LOOKUP_CACHE_MAX_BYTES {
            return;
        }
        if let Some(previous) = self.entries.remove(&key) {
            self.bytes = self
                .bytes
                .saturating_sub(xdg_lookup_cache_weight(&key, previous.as_ref()));
        }
        self.order.retain(|existing| existing != &key);
        while self.entries.len() >= XDG_ICON_LOOKUP_CACHE_CAPACITY
            || self.bytes.saturating_add(weight) > XDG_ICON_LOOKUP_CACHE_MAX_BYTES
        {
            let Some(evicted) = self.order.pop_front() else {
                break;
            };
            if let Some(previous) = self.entries.remove(&evicted) {
                self.bytes = self
                    .bytes
                    .saturating_sub(xdg_lookup_cache_weight(&evicted, previous.as_ref()));
            }
        }
        self.order.push_back(key.clone());
        self.entries.insert(key, value);
        self.bytes = self.bytes.saturating_add(weight);
    }
}

fn xdg_lookup_cache_weight(key: &XdgIconLookupKey, value: Option<&PathBuf>) -> usize {
    size_of::<XdgIconLookupKey>()
        .saturating_add(key.theme.len())
        .saturating_add(key.asset_name.len())
        .saturating_add(size_of::<Option<PathBuf>>())
        .saturating_add(value.map_or(0, |path| path.as_os_str().len()))
        .max(1)
}

/// Look up a glyph codepoint by name from a font pack's codepoints file.
/// Used by the binding resolver when a mapping target points at a font
/// alias declared in `mesh.contributes.icons[].requires.fonts`.
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
    if bytes.len() > MAX_FONT_BYTES {
        return Err(format!("font file exceeds {MAX_FONT_BYTES} bytes"));
    }
    let face = ttf_parser::Face::parse(bytes, 0)
        .map_err(|error| format!("font file is invalid: {error:?}"))?;
    if face.number_of_glyphs() == 0 {
        return Err("font file contains no glyphs".into());
    }
    Ok(())
}

pub fn find_icon_in_theme_with_catalog(
    catalog: &SystemResourceCatalog,
    theme: &str,
    asset_name: &str,
    size: u32,
) -> Option<PathBuf> {
    let key = XdgIconLookupKey {
        revision: resource_revision(),
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

static CODEPOINTS_CACHE: OnceLock<Mutex<CodepointsCache>> = OnceLock::new();
const CODEPOINTS_CACHE_CAPACITY: usize = 128;
const CODEPOINTS_CACHE_MAX_BYTES: usize = 32 * 1024 * 1024;

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
    bytes: usize,
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
        let weight = codepoints_cache_weight(&key, &value);
        if weight > CODEPOINTS_CACHE_MAX_BYTES {
            return;
        }
        if let Some(previous) = self.entries.remove(&key) {
            self.bytes = self
                .bytes
                .saturating_sub(codepoints_cache_weight(&key, &previous));
        }
        self.order.retain(|existing| existing != &key);
        while self.entries.len() >= CODEPOINTS_CACHE_CAPACITY
            || self.bytes.saturating_add(weight) > CODEPOINTS_CACHE_MAX_BYTES
        {
            let Some(evicted) = self.order.pop_front() else {
                break;
            };
            if let Some(value) = self.entries.remove(&evicted) {
                self.bytes = self
                    .bytes
                    .saturating_sub(codepoints_cache_weight(&evicted, &value));
            }
        }
        self.order.push_back(key.clone());
        self.entries.insert(key, value);
        self.bytes = self.bytes.saturating_add(weight);
    }
}

fn codepoints_cache_weight(key: &CodepointsCacheKey, value: &HashMap<String, u32>) -> usize {
    let key_bytes = size_of::<CodepointsCacheKey>()
        .saturating_add(key.path.as_os_str().len())
        .saturating_add(2 * size_of::<usize>());
    let map_bytes = size_of::<HashMap<String, u32>>()
        .saturating_add(value.capacity().saturating_mul(size_of::<(String, u32)>()));
    let names = value
        .keys()
        .map(|name| name.capacity().saturating_add(size_of::<String>()))
        .fold(0usize, usize::saturating_add);
    key_bytes
        .saturating_add(map_bytes)
        .saturating_add(names)
        .max(1)
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
    let bytes = read_bounded_file(path, MAX_GLYPH_MAP_BYTES)?;
    parse_glyph_map_bytes(&bytes).ok()
}

fn read_bounded_file(path: &Path, max_bytes: usize) -> Option<Vec<u8>> {
    let mut file = std::fs::File::open(path).ok()?;
    let mut bytes = Vec::new();
    file.by_ref()
        .take(u64::try_from(max_bytes.saturating_add(1)).ok()?)
        .read_to_end(&mut bytes)
        .ok()?;
    (bytes.len() <= max_bytes).then_some(bytes)
}

/// Inspect the font's `fvar` table to discover which variable-font axes
/// it actually exposes. Returns conservative defaults (everything off)
/// when the font can't be parsed; the painter then silently ignores
/// CSS `--icon-*` properties that don't match.
fn font_freshness(path: &Path) -> Option<FontFreshness> {
    resource_fingerprint(path)
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
