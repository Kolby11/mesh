use crate::config::{IconPackKind, IconPackRoot};
use serde::Deserialize;
use std::path::{Path, PathBuf};

/// Standard XDG base directories where icon packs may be installed.
/// User-local entries come first so they win over system packs with the
/// same id when both are present.
fn xdg_icon_base_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(home) = dirs_home() {
        dirs.push(home.join(".local/share/icons"));
        dirs.push(home.join(".icons"));
    }
    dirs.push(PathBuf::from("/usr/share/icons"));
    dirs.push(PathBuf::from("/usr/share/pixmaps"));
    dirs.into_iter().filter(|p| p.is_dir()).collect()
}

fn dirs_home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

/// Discover icon packs installed in the standard XDG locations. Returns
/// every pack found — packs with an explicit `mesh-pack.json` first, then
/// implicit XDG packs (directories with an `index.theme` but no MESH
/// manifest). Caller is responsible for registering them with the registry
/// and deciding what to do about duplicate ids.
pub fn discover_xdg_packs() -> Vec<IconPackRoot> {
    let mut packs = Vec::new();
    let mut seen_ids = std::collections::HashSet::new();

    for base in xdg_icon_base_dirs() {
        let entries = match std::fs::read_dir(&base) {
            Ok(entries) => entries,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            if let Some(pack) = read_mesh_pack_manifest(&path) {
                if seen_ids.insert(pack.id.clone()) {
                    packs.push(pack);
                }
                continue;
            }
            if let Some(pack) = read_implicit_xdg_pack(&path)
                && seen_ids.insert(pack.id.clone())
            {
                packs.push(pack);
            }
        }
    }

    packs
}

#[derive(Debug, Deserialize)]
struct MeshPackManifest {
    id: String,
    #[serde(default = "default_pack_kind")]
    kind: String,
    #[serde(default)]
    font_file: Option<String>,
    #[serde(default)]
    glyph_map: Option<String>,
    #[serde(default = "default_theme")]
    theme: String,
}

fn default_pack_kind() -> String {
    "xdg".into()
}

fn default_theme() -> String {
    "hicolor".into()
}

fn read_mesh_pack_manifest(dir: &Path) -> Option<IconPackRoot> {
    let manifest_path = dir.join("mesh-pack.json");
    if !manifest_path.is_file() {
        return None;
    }
    let raw = std::fs::read_to_string(&manifest_path).ok()?;
    let parsed: MeshPackManifest = serde_json::from_str(&raw)
        .map_err(|e| {
            tracing::warn!(
                "ignoring icon pack at {}: invalid mesh-pack.json: {e}",
                manifest_path.display()
            );
            e
        })
        .ok()?;
    let kind = match parsed.kind.as_str() {
        "xdg" => IconPackKind::Xdg,
        "font" => IconPackKind::Font {
            font_file: parsed.font_file.clone().unwrap_or_default(),
            glyph_map: parsed.glyph_map.clone().unwrap_or_default(),
        },
        other => {
            tracing::warn!(
                "ignoring icon pack at {}: unknown kind '{}'",
                manifest_path.display(),
                other
            );
            return None;
        }
    };
    Some(IconPackRoot {
        id: parsed.id,
        root: Some(dir.to_path_buf()),
        theme: parsed.theme,
        kind,
    })
}

fn read_implicit_xdg_pack(dir: &Path) -> Option<IconPackRoot> {
    if !dir.join("index.theme").is_file() {
        return None;
    }
    let id = dir.file_name()?.to_str()?.to_string();
    Some(IconPackRoot {
        id,
        root: Some(dir.to_path_buf()),
        theme: dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("hicolor")
            .to_string(),
        kind: IconPackKind::Xdg,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn reads_mesh_pack_manifest() {
        let td = tempdir().unwrap();
        let pack_dir = td.path().join("material-symbols");
        fs::create_dir_all(&pack_dir).unwrap();
        fs::write(
            pack_dir.join("mesh-pack.json"),
            r#"{"id":"material-symbols","kind":"font","font_file":"f.ttf","glyph_map":"map.json"}"#,
        )
        .unwrap();

        let pack = read_mesh_pack_manifest(&pack_dir).unwrap();
        assert_eq!(pack.id, "material-symbols");
        assert!(matches!(pack.kind, IconPackKind::Font { .. }));
    }

    #[test]
    fn detects_implicit_xdg_pack_from_index_theme() {
        let td = tempdir().unwrap();
        let pack_dir = td.path().join("Arc");
        fs::create_dir_all(&pack_dir).unwrap();
        fs::write(pack_dir.join("index.theme"), "[Icon Theme]\nName=Arc\n").unwrap();

        let pack = read_implicit_xdg_pack(&pack_dir).unwrap();
        assert_eq!(pack.id, "Arc");
        assert!(matches!(pack.kind, IconPackKind::Xdg));
    }
}
