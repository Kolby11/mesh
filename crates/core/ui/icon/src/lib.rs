use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

mod config;
mod fallback;
mod registry;
mod xdg;

pub use config::{IconCandidate, IconConfig, IconPackRoot, IconProfile};
pub use fallback::BuiltInIconFallback;
pub use registry::{IconRegistry, IconResolution};

fn bundled_icon_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/material")
}

fn default_icon_config() -> IconConfig {
    IconConfig::builtin_material(bundled_icon_dir())
        .expect("builtin material icon config should be valid")
}

static DEFAULT_REGISTRY: OnceLock<Mutex<IconRegistry>> = OnceLock::new();

fn default_registry() -> &'static Mutex<IconRegistry> {
    DEFAULT_REGISTRY
        .get_or_init(|| Mutex::new(IconRegistry::from_config(default_icon_config()).unwrap()))
}

/// Resolve an icon name to a file path using the default configured icon registry.
///
/// Explicit file paths are still accepted for compatibility with older callers.
/// Semantic names resolve through the built-in Material profile unless a caller
/// uses [`IconRegistry`] directly with a different config.
pub fn resolve_icon(name: &str, size: u32) -> Option<PathBuf> {
    match resolve_icon_result(name, size) {
        IconResolution::Found { path, .. } => Some(path),
        IconResolution::Missing { .. } => None,
    }
}

/// Resolve an icon name using the shared default registry and preserve
/// diagnostic details for missing semantic icons.
pub fn resolve_icon_result(name: &str, size: u32) -> IconResolution {
    let p = Path::new(name);
    if p.is_file() {
        return IconResolution::Found {
            semantic_name: name.into(),
            candidate: p.display().to_string(),
            path: p.to_path_buf(),
            multicolor: false,
        };
    }

    default_registry().lock().unwrap().resolve(name, size)
}

/// Resolve an icon using an explicit registry.
pub fn resolve_icon_with_registry(
    registry: &mut IconRegistry,
    name: &str,
    size: u32,
) -> IconResolution {
    registry.resolve(name, size)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write_svg(path: &Path) {
        fs::write(
            path,
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16"><rect width="16" height="16" fill="black"/></svg>"#,
        )
        .unwrap();
    }

    #[test]
    fn resolves_local_png() {
        let td = tempfile::tempdir().unwrap();
        let icons = td.path().join("icons");
        fs::create_dir_all(&icons).unwrap();
        let file = icons.join("testicon.png");
        fs::write(&file, b"PNGDATA").unwrap();

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

    #[test]
    fn icon_config_resolves_ordered_fallbacks() {
        let td = tempfile::tempdir().unwrap();
        let material = td.path().join("material");
        fs::create_dir_all(&material).unwrap();
        write_svg(&material.join("audio-volume-muted.svg"));
        write_svg(&material.join("volume-off.svg"));

        let config = IconConfig::from_toml_str(&format!(
            r#"
active_profile = "rounded"

[[packs]]
id = "material"
root = "{}"
theme = "hicolor"

[[packs]]
id = "missing"
root = "{}"
theme = "hicolor"

[profiles.rounded.icons]
audio-volume-muted = ["missing:nope", "material:audio-volume-muted", "material:volume-off"]
"#,
            material.display(),
            td.path().join("missing").display()
        ))
        .unwrap();

        let mut registry = IconRegistry::from_config(config).unwrap();
        let result = registry.resolve("audio-volume-muted", 18);

        match result {
            IconResolution::Found {
                candidate, path, ..
            } => {
                assert_eq!(candidate, "material:audio-volume-muted");
                assert!(path.ends_with("audio-volume-muted.svg"));
            }
            IconResolution::Missing { .. } => panic!("expected fallback candidate to resolve"),
        }
    }

    #[test]
    fn icon_profile_switch_invalidates_cache() {
        let td = tempfile::tempdir().unwrap();
        let rounded = td.path().join("rounded");
        let filled = td.path().join("filled");
        fs::create_dir_all(&rounded).unwrap();
        fs::create_dir_all(&filled).unwrap();
        write_svg(&rounded.join("audio.svg"));
        write_svg(&filled.join("audio.svg"));

        let config = IconConfig::from_toml_str(&format!(
            r#"
active_profile = "rounded"

[[packs]]
id = "rounded"
root = "{}"
theme = "hicolor"

[[packs]]
id = "filled"
root = "{}"
theme = "hicolor"

[profiles.rounded.icons]
audio-volume-muted = ["rounded:audio"]

[profiles.filled.icons]
audio-volume-muted = ["filled:audio"]
"#,
            rounded.display(),
            filled.display()
        ))
        .unwrap();

        let mut registry = IconRegistry::from_config(config.clone()).unwrap();
        let first = registry.resolve("audio-volume-muted", 18).path().unwrap();

        let mut switched = config;
        switched.active_profile = "filled".into();
        registry.set_config(switched).unwrap();
        let second = registry.resolve("audio-volume-muted", 18).path().unwrap();

        assert_ne!(first, second);
        assert!(first.ends_with("rounded/audio.svg"));
        assert!(second.ends_with("filled/audio.svg"));
    }
}
