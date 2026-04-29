use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

/// Cache for resolved icons: (name, size) -> Option<PathBuf>
static ICON_CACHE: OnceLock<Mutex<HashMap<(String, u32), Option<PathBuf>>>> = OnceLock::new();

fn cache() -> &'static Mutex<HashMap<(String, u32), Option<PathBuf>>> {
    ICON_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn bundled_icon_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/material")
}

fn bundled_icon_path(name: &str) -> Option<PathBuf> {
    let candidate = bundled_icon_dir().join(format!("{name}.svg"));
    candidate.is_file().then_some(candidate)
}

/// Resolve an icon name to a file path using the XDG icon lookup heuristic.
/// Returns `Some(path)` when found, or `None` when not found.
pub fn resolve_icon(name: &str, size: u32) -> Option<PathBuf> {
    // If the input looks like an explicit path, return it directly.
    let p = Path::new(name);
    if p.is_file() {
        return Some(p.to_path_buf());
    }

    let key = (name.to_string(), size);
    {
        let guard = cache().lock().unwrap();
        if let Some(cached) = guard.get(&key) {
            return cached.clone();
        }
    }

    // Candidate base directories (user before system)
    let mut bases: Vec<PathBuf> = Vec::new();
    if let Some(home) = dirs::home_dir() {
        bases.push(home.join(".local/share/icons"));
        bases.push(home.join(".icons"));
    }
    bases.push(PathBuf::from("/usr/share/icons"));
    bases.push(PathBuf::from("/usr/share/pixmaps"));

    // Order of preferred themes: hicolor fallback last
    let mut themes: Vec<String> = Vec::new();

    for base in &bases {
        if !base.exists() {
            continue;
        }
        // collect theme dirs
        if let Ok(entries) = std::fs::read_dir(base) {
            for e in entries.flatten() {
                if e.path().is_dir() {
                    if let Some(name) = e.file_name().to_str() {
                        themes.push(name.to_string());
                    }
                }
            }
        }
    }
    // ensure hicolor is present as fallback
    if !themes.contains(&"hicolor".to_string()) {
        themes.push("hicolor".to_string());
    }

    // categories to try inside theme dirs
    let categories = [
        "apps",
        "devices",
        "status",
        "actions",
        "places",
        "mimetypes",
    ];

    let mut found: Option<PathBuf> = None;

    'base_loop: for base in &bases {
        if !base.exists() {
            continue;
        }

        // First check theme directories
        for theme in &themes {
            let theme_dir = base.join(theme);
            if !theme_dir.is_dir() {
                continue;
            }

            // try size-specific png in categories
            let size_dir = format!("{}x{}", size, size);
            for cat in &categories {
                let candidate = theme_dir
                    .join(&size_dir)
                    .join(cat)
                    .join(format!("{}.png", name));
                if candidate.is_file() {
                    found = Some(candidate);
                    break 'base_loop;
                }
            }

            // try scalable svg in categories
            for cat in &categories {
                let candidate = theme_dir
                    .join("scalable")
                    .join(cat)
                    .join(format!("{}.svg", name));
                if candidate.is_file() {
                    // prefer pngs that matched earlier; since none matched in this loop,
                    // accept svg if found.
                    found = Some(candidate);
                    break 'base_loop;
                }
            }

            // try direct files in theme root
            let png = theme_dir.join(format!("{}.png", name));
            if png.is_file() {
                found = Some(png);
                break 'base_loop;
            }
            let svg = theme_dir.join(format!("{}.svg", name));
            if svg.is_file() {
                found = Some(svg);
                break 'base_loop;
            }
        }

        // After themes, try base/<name> as pixmaps fallback
        let png = base.join(format!("{}.png", name));
        if png.is_file() {
            found = Some(png);
            break;
        }
        let svg = base.join(format!("{}.svg", name));
        if svg.is_file() {
            found = Some(svg);
            break;
        }
    }

    if found.is_none() {
        found = bundled_icon_path(name);
    }

    let mut guard = cache().lock().unwrap();
    guard.insert(key, found.clone());
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn resolves_local_png() {
        let td = tempfile::tempdir().unwrap();
        let icons = td.path().join("icons");
        fs::create_dir_all(&icons).unwrap();
        let file = icons.join("testicon.png");
        fs::write(&file, b"PNGDATA").unwrap();

        // create a file and call resolve_icon with the explicit path
        let user_icons = td.path().join(".icons");
        fs::create_dir_all(&user_icons).unwrap();
        let ui = user_icons.join("testicon.png");
        fs::write(&ui, b"PNG").unwrap();

        let got = resolve_icon(&ui.to_string_lossy(), 24);
        assert!(got.is_some());
    }

    #[test]
    fn resolves_bundled_material_fallback() {
        let got = resolve_icon("audio-volume-high", 24);
        assert!(got.is_some());
        let got = got.unwrap();
        assert!(got.ends_with("assets/material/audio-volume-high.svg"));
    }
}
