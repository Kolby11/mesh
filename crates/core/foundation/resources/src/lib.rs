//! Host resource discovery shared by icon, font, and settings systems.
//!
//! This crate describes what the desktop has installed. MESH resource-pack
//! modules remain semantic mapping and composition units layered above it.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SystemResourceCatalog {
    pub icon_themes: Vec<SystemIconTheme>,
    pub font_families: Vec<SystemFontFamily>,
}

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

static SYSTEM_RESOURCES: OnceLock<SystemResourceCatalog> = OnceLock::new();

/// Cached process-wide host catalog. Font scanning is intentionally performed
/// once: fontdb follows the platform font directories and can inspect hundreds
/// of files on a typical desktop.
pub fn system_resource_catalog() -> &'static SystemResourceCatalog {
    SYSTEM_RESOURCES.get_or_init(discover_system_resources)
}

pub fn discover_system_resources() -> SystemResourceCatalog {
    SystemResourceCatalog {
        icon_themes: discover_icon_themes_in(&xdg_icon_base_dirs()),
        font_families: discover_font_families(),
    }
}

fn discover_font_families() -> Vec<SystemFontFamily> {
    let mut database = fontdb::Database::new();
    database.load_system_fonts();

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

/// FreeDesktop icon base directories in lookup precedence order.
/// Both catalog discovery and icon resolution use this one authority.
pub fn xdg_icon_base_dirs() -> Vec<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let mut dirs = Vec::new();
    if let Some(data_home) = std::env::var_os("XDG_DATA_HOME").map(PathBuf::from) {
        dirs.push(data_home.join("icons"));
    } else if let Some(home) = &home {
        dirs.push(home.join(".local/share/icons"));
    }
    if let Some(home) = home {
        dirs.push(home.join(".icons"));
    }
    let data_dirs = std::env::var_os("XDG_DATA_DIRS")
        .map(|value| std::env::split_paths(&value).collect::<Vec<_>>())
        .filter(|dirs| !dirs.is_empty())
        .unwrap_or_else(|| {
            vec![
                PathBuf::from("/usr/local/share"),
                PathBuf::from("/usr/share"),
            ]
        });
    dirs.extend(data_dirs.into_iter().map(|dir| dir.join("icons")));

    let mut seen = HashSet::new();
    dirs.into_iter()
        .filter(|path| path.is_dir() && seen.insert(path.clone()))
        .collect()
}

fn discover_icon_themes_in(base_dirs: &[PathBuf]) -> Vec<SystemIconTheme> {
    let mut themes = BTreeMap::new();
    for base in base_dirs {
        let Ok(entries) = std::fs::read_dir(base) else {
            continue;
        };
        for entry in entries.flatten() {
            let Some(theme) = read_icon_theme(&entry.path()) else {
                continue;
            };
            // XDG directory precedence is user first, then system. Preserve
            // the first definition when the same theme id occurs in both.
            themes.entry(theme.id.clone()).or_insert(theme);
        }
    }
    themes.into_values().collect()
}

fn read_icon_theme(path: &Path) -> Option<SystemIconTheme> {
    let id = path.file_name()?.to_str()?.to_owned();
    let raw = std::fs::read_to_string(path.join("index.theme")).ok()?;
    let mut in_icon_theme = false;
    let mut name = None;
    let mut inherits = BTreeSet::new();
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
            "Inherits" => inherits.extend(
                value
                    .split(',')
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_owned),
            ),
            "Hidden" => hidden = value.trim().eq_ignore_ascii_case("true"),
            _ => {}
        }
    }

    Some(SystemIconTheme {
        name: name.unwrap_or_else(|| id.clone()),
        id,
        path: path.to_owned(),
        inherits: inherits.into_iter().collect(),
        hidden,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(themes[0].inherits, ["Adwaita", "hicolor"]);
    }
}
