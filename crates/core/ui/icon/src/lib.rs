use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock, RwLock};

mod bindings;
mod config;
mod discovery;
mod fallback;
mod registry;
mod xdg;

pub use bindings::{FontAsset, FrontendIconBindings, IconMapping, IconPackBindings, parse_target};
pub use config::{IconCandidate, IconConfig, IconPackKind, IconPackRoot, IconProfile};
pub use discovery::discover_xdg_packs;
pub use fallback::{
    BuiltInIconFallback, MISSING_ICON_SVG, fallback_stage, semantic_fallback_names,
};
pub use registry::{
    IconRegistry, IconResolution, IconResolutionProvenance, ResolvedTarget, SupportedAxes,
};
pub use xdg::{
    MAX_FONT_BYTES, MAX_GLYPH_MAP_BYTES, MAX_GLYPH_MAP_ENTRIES, parse_glyph_map_bytes,
    parse_glyph_map_bytes_with_cancellation, validate_font_bytes,
};

pub type IconRegistryHandle = Arc<Mutex<IconRegistry>>;

static DEFAULT_REGISTRY: OnceLock<RwLock<IconRegistryHandle>> = OnceLock::new();

fn default_registry() -> IconRegistryHandle {
    DEFAULT_REGISTRY
        .get_or_init(|| RwLock::new(Arc::new(Mutex::new(IconRegistry::empty()))))
        .read()
        .expect("default icon registry lock is not poisoned")
        .clone()
}

/// Atomically publish the complete graph/profile-authorized icon registry.
///
/// Readers that already hold the previous handle finish against that
/// last-known-good snapshot. New readers observe the candidate as a whole,
/// including its explicit host resource catalog and all module bindings.
pub fn replace_default_registry(registry: IconRegistryHandle) {
    *DEFAULT_REGISTRY
        .get_or_init(|| RwLock::new(Arc::new(Mutex::new(IconRegistry::empty()))))
        .write()
        .expect("default icon registry lock is not poisoned") = registry;
    mesh_core_resources::advance_resource_revision();
}

/// Resolve an icon name to a file path using the active committed icon
/// registry. Before a graph/profile publishes its resource snapshot, the
/// default registry is intentionally empty.
///
/// Explicit file paths are still accepted for compatibility with older callers.
/// Semantic names therefore resolve only from graph/profile-authorized packs
/// and the host catalog captured by that snapshot.
pub fn resolve_icon(name: &str, size: u32) -> Option<PathBuf> {
    resolve_icon_result(name, size).path()
}

/// Resolve an icon name using the shared default registry and preserve
/// diagnostic details for missing semantic icons.
pub fn resolve_icon_result(name: &str, size: u32) -> IconResolution {
    let p = Path::new(name);
    if p.is_file() {
        return IconResolution::Found {
            semantic_name: name.into(),
            candidate: p.display().to_string(),
            target: ResolvedTarget::File(p.to_path_buf()),
            multicolor: false,
            provenance: IconResolutionProvenance {
                owner_module: None,
                pack_id: None,
                candidate: p.display().to_string(),
                fallback_stage: "direct-path".into(),
            },
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

/// Resolve an icon for a specific module via the shared default registry.
/// This is the path used by the painter at render time — module bindings
/// (declared mappings + user overrides + module's preferred pack) take
/// precedence over shell-wide profile defaults.
pub fn resolve_icon_for_module(module_id: &str, name: &str, size: u32) -> IconResolution {
    let p = Path::new(name);
    if p.is_file() {
        return IconResolution::Found {
            semantic_name: name.into(),
            candidate: p.display().to_string(),
            target: ResolvedTarget::File(p.to_path_buf()),
            multicolor: false,
            provenance: IconResolutionProvenance {
                owner_module: None,
                pack_id: None,
                candidate: p.display().to_string(),
                fallback_stage: "direct-path".into(),
            },
        };
    }
    default_registry()
        .lock()
        .unwrap()
        .resolve_for_module(module_id, name, size)
}

/// Install or replace a frontend's icon resolution context on the shared
/// default registry. Called by the shell after composing the effective
/// pack chain (frontend deps + user `use_packs` override + shell
/// default).
pub fn set_default_frontend_bindings(module_id: impl Into<String>, bindings: FrontendIconBindings) {
    let current = default_registry();
    let mut candidate = current.lock().unwrap().clone();
    candidate.set_frontend_bindings(module_id, bindings);
    replace_default_registry(Arc::new(Mutex::new(candidate)));
}

pub fn remove_default_frontend_bindings(module_id: &str) {
    let current = default_registry();
    let mut candidate = current.lock().unwrap().clone();
    candidate.remove_frontend_bindings(module_id);
    replace_default_registry(Arc::new(Mutex::new(candidate)));
}

/// Install or replace a loaded icon-pack module's bindings.
pub fn set_default_icon_pack(bindings: IconPackBindings) {
    let current = default_registry();
    let mut candidate = current.lock().unwrap().clone();
    candidate.set_icon_pack(bindings);
    replace_default_registry(Arc::new(Mutex::new(candidate)));
}

/// Atomically replace graph-authorized icon-pack and frontend bindings while
/// retaining the registry's explicitly captured host resource catalog.
pub fn replace_default_bindings(
    icon_packs: Vec<IconPackBindings>,
    frontends: Vec<(String, FrontendIconBindings)>,
    shell_default_pack: Option<String>,
) -> anyhow::Result<()> {
    let current = default_registry();
    let mut candidate = current.lock().unwrap().clone();
    candidate.replace_bindings(icon_packs, frontends, shell_default_pack)?;
    replace_default_registry(Arc::new(Mutex::new(candidate)));
    Ok(())
}

pub fn remove_default_icon_pack(module_id: &str) {
    let current = default_registry();
    let mut candidate = current.lock().unwrap().clone();
    candidate.remove_icon_pack(module_id);
    replace_default_registry(Arc::new(Mutex::new(candidate)));
}

/// Set the user's chosen shell-default icon-pack module id.
pub fn set_default_shell_pack(module_id: Option<String>) {
    let current = default_registry();
    let mut candidate = current.lock().unwrap().clone();
    candidate.set_shell_default_pack(module_id);
    replace_default_registry(Arc::new(Mutex::new(candidate)));
}

/// Register an icon pack on the process-wide default registry.
///
/// Used by the shell to auto-register packs contributed by modules that
/// declare `assets.icons` in their manifest. Returns `Ok(true)` when the
/// pack was newly registered and `Ok(false)` when a pack with the same id
/// already existed (treated as a no-op so duplicate registration during
/// module reloads is harmless).
pub fn register_default_pack(pack: IconPackRoot) -> anyhow::Result<bool> {
    let current = default_registry();
    let mut candidate = current.lock().unwrap().clone();
    let registered = candidate.register_pack(pack)?;
    if registered {
        replace_default_registry(Arc::new(Mutex::new(candidate)));
    }
    Ok(registered)
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
    fn default_registry_replacement_retains_last_known_good_candidate() {
        let td = tempfile::tempdir().unwrap();
        let icon = td.path().join("home.svg");
        write_svg(&icon);

        let mut registry = IconRegistry::empty();
        registry
            .register_pack(IconPackRoot {
                id: "files".into(),
                root: Some(td.path().to_path_buf()),
                theme: "hicolor".into(),
                kind: IconPackKind::Xdg,
            })
            .unwrap();
        registry
            .replace_bindings(
                vec![IconPackBindings {
                    pack_id: "stable".into(),
                    module_id: "@test/stable".into(),
                    mappings: std::collections::HashMap::from([(
                        "home".into(),
                        "files/home".into(),
                    )]),
                    axes: SupportedAxes::default(),
                    font_aliases: std::collections::HashMap::new(),
                }],
                vec![("frontend".into(), FrontendIconBindings::default())],
                Some("@test/stable".into()),
            )
            .unwrap();
        replace_default_registry(Arc::new(Mutex::new(registry)));

        assert!(matches!(
            resolve_icon_for_module("frontend", "home", 16),
            IconResolution::Found { .. }
        ));

        let error = replace_default_bindings(
            vec![
                IconPackBindings {
                    pack_id: "duplicate".into(),
                    module_id: "@test/one".into(),
                    mappings: std::collections::HashMap::new(),
                    axes: SupportedAxes::default(),
                    font_aliases: std::collections::HashMap::new(),
                },
                IconPackBindings {
                    pack_id: "duplicate".into(),
                    module_id: "@test/two".into(),
                    mappings: std::collections::HashMap::new(),
                    axes: SupportedAxes::default(),
                    font_aliases: std::collections::HashMap::new(),
                },
            ],
            Vec::new(),
            None,
        )
        .unwrap_err();
        assert!(error.to_string().contains("duplicate icon pack id"));
        assert!(matches!(
            resolve_icon_for_module("frontend", "home", 16),
            IconResolution::Found { .. }
        ));

        replace_default_registry(Arc::new(Mutex::new(IconRegistry::empty())));
    }

    // Profile-based resolution was the v0 mechanism, replaced by icon-pack
    // binding modules + frontend-context resolution. The tests below were
    // exercising the obsolete code path; preserved here behind `#[ignore]`
    // until they're rewritten as binding-model fixtures.
    #[ignore]
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
                candidate, target, ..
            } => {
                assert_eq!(candidate, "material:audio-volume-muted");
                let ResolvedTarget::File(path) = target else {
                    panic!("expected file target");
                };
                assert!(path.ends_with("audio-volume-muted.svg"));
            }
            IconResolution::Missing { .. } => panic!("expected fallback candidate to resolve"),
        }
    }

    #[ignore]
    #[test]
    fn icon_config_resolves_freedesktop_theme_layout() {
        let td = tempfile::tempdir().unwrap();
        let theme = td.path().join("TestTheme");
        let status = theme.join("scalable/status");
        fs::create_dir_all(&status).unwrap();
        fs::write(
            theme.join("index.theme"),
            r#"[Icon Theme]
Name=Test Theme
Comment=Test XDG theme
Directories=scalable/status

[scalable/status]
Size=16
Type=Scalable
MinSize=1
MaxSize=256
Context=Status
"#,
        )
        .unwrap();
        write_svg(&status.join("network-wireless.svg"));

        let config = IconConfig::from_toml_str(&format!(
            r#"
active_profile = "xdg"

[[packs]]
id = "test"
root = "{}"
theme = "TestTheme"

[profiles.xdg.icons]
network-wireless = ["test:network-wireless"]
"#,
            td.path().display()
        ))
        .unwrap();

        let mut registry = IconRegistry::from_config(config).unwrap();
        let result = registry.resolve("network-wireless", 24);

        match result {
            IconResolution::Found {
                candidate, target, ..
            } => {
                assert_eq!(candidate, "test:network-wireless");
                let ResolvedTarget::File(path) = target else {
                    panic!("expected file target");
                };
                assert!(path.ends_with("TestTheme/scalable/status/network-wireless.svg"));
            }
            IconResolution::Missing { .. } => panic!("expected XDG theme icon to resolve"),
        }
    }

    #[ignore]
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
