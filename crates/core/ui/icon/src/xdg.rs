use crate::config::{IconPackKind, IconPackRoot};
use crate::registry::{ResolvedTarget, SupportedAxes};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

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

    let path = search_for_pack(pack)
        .search()
        .icons()
        .find_icon(asset_name, size.max(1), 1, theme_name(pack))
        .map(|icon| icon.path().to_path_buf())
        .or_else(|| find_direct_file(pack, asset_name))?;

    Some(ResolvedTarget::File(path))
}

/// Look up a glyph codepoint by name from a font pack's codepoints file.
/// Used by the binding resolver when a mapping target points at a font
/// alias declared in `mesh.icon_pack.requires.fonts`.
pub fn lookup_glyph_codepoint(glyph_map_path: &Path, glyph_name: &str) -> Option<u32> {
    load_codepoints(glyph_map_path)?.get(glyph_name).copied()
}

/// Look up an icon in any installed theme on the system XDG search path.
/// Used as a last-resort fallback when neither module bindings nor the
/// active profile produce a hit.
pub fn find_icon_in_theme(theme: &str, asset_name: &str, size: u32) -> Option<PathBuf> {
    icon::IconSearch::new()
        .search()
        .icons()
        .find_icon(asset_name, size.max(1), 1, theme)
        .map(|icon| icon.path().to_path_buf())
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
    let codepoint = load_codepoints(&glyph_map_path)?.get(asset_name).copied()?;
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
    if let Some(rest) = trimmed.strip_prefix("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home).join(rest);
        }
    }
    let candidate = PathBuf::from(trimmed);
    if candidate.is_absolute() {
        return candidate;
    }
    root.join(candidate)
}

static CODEPOINTS_CACHE: OnceLock<Mutex<HashMap<PathBuf, HashMap<String, u32>>>> = OnceLock::new();

fn load_codepoints(path: &Path) -> Option<HashMap<String, u32>> {
    let cache = CODEPOINTS_CACHE.get_or_init(Default::default);
    {
        let guard = cache.lock().ok()?;
        if let Some(map) = guard.get(path) {
            return Some(map.clone());
        }
    }
    let parsed = parse_codepoints_file(path)?;
    if let Ok(mut guard) = cache.lock() {
        guard.insert(path.to_path_buf(), parsed.clone());
    }
    Some(parsed)
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
    axes
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
    if let Some(root) = &pack.root {
        if root.join("index.theme").is_file() {
            if let Some(name) = root.file_name().and_then(|name| name.to_str()) {
                return name;
            }
        }
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
