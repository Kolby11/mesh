use super::super::*;
use crate::manifest::CapabilitiesSection;
use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};
use std::time::{SystemTime, UNIX_EPOCH};

static ENV_LOCK: Mutex<()> = Mutex::new(());

pub(super) struct EnvGuard {
    key: &'static str,
    old: Option<String>,
    _lock: MutexGuard<'static, ()>,
}

impl EnvGuard {
    pub(super) fn set(key: &'static str, value: Option<&str>) -> Self {
        let lock = ENV_LOCK.lock().unwrap();
        let old = std::env::var(key).ok();
        unsafe {
            match value {
                Some(value) => std::env::set_var(key, value),
                None => std::env::remove_var(key),
            }
        }
        Self {
            key,
            old,
            _lock: lock,
        }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        unsafe {
            match &self.old {
                Some(value) => std::env::set_var(self.key, value),
                None => std::env::remove_var(self.key),
            }
        }
    }
}

pub(super) fn temp_dir(name: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("mesh-{name}-{nonce}"));
    fs::create_dir_all(&dir).unwrap();
    dir
}

pub(super) fn interface_relationship_manifest(
    relationship: Option<&str>,
    extends: Option<&str>,
) -> String {
    let relationship_json = relationship
        .map(|relationship| format!(r#","relationship":"{relationship}""#))
        .unwrap_or_default();
    let extends_json = extends
        .map(|extends| format!(r#","extends":"{extends}""#))
        .unwrap_or_default();
    format!(
        r#"{{
  "name": "@alice/example-interface",
  "version": "1.0.0",
  "mesh": {{
    "apiVersion": "0.1",
    "kind": "interface",
    "interface": {{
      "name": "alice.example",
      "version": "1.0",
      "file": "interface.toml",
      "domain": "example"{extends_json}{relationship_json}
    }}
  }}
}}"#
    )
}

pub(super) fn obvious_semantic_icon_literals(source: &str) -> std::collections::HashSet<String> {
    let prefixes = [
        "audio-volume-",
        "battery-",
        "media-playback-",
        "preferences-",
        "weather-",
        "window-",
    ];
    source
        .split('"')
        .skip(1)
        .step_by(2)
        .filter(|literal| prefixes.iter().any(|prefix| literal.starts_with(prefix)))
        .filter(|literal| {
            !literal.ends_with("-widget")
                && !literal.ends_with("-button")
                && !literal.ends_with("-glyph")
                && !literal.ends_with("-value")
        })
        .map(str::to_string)
        .collect()
}

pub(super) fn loaded_module(
    name: &str,
    kind: ModuleKind,
    dependencies: MeshDependencies,
    provides: Vec<MeshProvidesDeclaration>,
    contributes: MeshContributes,
) -> LoadedModuleManifest {
    LoadedModuleManifest {
        manifest: ModuleManifest {
            name: name.into(),
            version: "0.1.0".into(),
            description: None,
            license: None,
            repository: None,
            mesh: MeshModuleSection {
                api_version: "0.1".into(),
                kind,
                entry: None,
                uses: MeshUses::default(),
                capabilities: CapabilitiesSection::default(),
                i18n: MeshI18nSupport::default(),
                entrypoints: MeshEntrypoints::default(),
                keybinds: crate::manifest::KeybindsSection::default(),
                dependencies,
                provides: MeshProvides::default(),
                implements: provides,
                interface: None,
                interfaces: Vec::new(),
                contributes,
                icons: None,
                icon_pack: None,
                icon_requirements: crate::manifest::IconRequirementsSection::default(),
                accessibility: None,
                surface: None,
                surface_layout: None,
                theme: None,
                experimental: serde_json::Value::Null,
            },
        },
        path: PathBuf::from(format!("{name}/module.json")),
        source: ModuleManifestSource::CanonicalModuleJson,
        diagnostics: Vec::new(),
    }
}

pub(super) fn declare_frontend_surface_contract(module: &mut LoadedModuleManifest) {
    module.manifest.mesh.accessibility = Some(crate::manifest::AccessibilitySection {
        role: Some("application".into()),
        label: None,
        description: None,
    });
    module.manifest.mesh.surface_layout = Some(crate::manifest::SurfaceLayoutSection {
        keyboard_mode: Some("on_demand".into()),
        ..Default::default()
    });
}

pub(super) fn root_with_modules(
    modules: &[(&str, ModuleKind)],
    providers: &[(&str, &str)],
    layout: Option<&str>,
) -> RootModuleGraphManifest {
    RootModuleGraphManifest {
        schema_version: 1,
        modules_dir: "modules".into(),
        modules: modules
            .iter()
            .map(|(id, kind)| {
                (
                    (*id).into(),
                    InstalledModuleEntry {
                        kind: *kind,
                        path: format!("modules/{id}"),
                        enabled: true,
                    },
                )
            })
            .collect(),
        disabled: Vec::new(),
        providers: providers
            .iter()
            .map(|(interface, module_id)| ((*interface).into(), (*module_id).into()))
            .collect(),
        layout: layout.map(|entrypoint| RootLayoutSelection {
            entrypoint: entrypoint.into(),
        }),
        theme: None,
    }
}

pub(super) fn audio_modules() -> Vec<LoadedModuleManifest> {
    vec![
        loaded_module(
            "@mesh/pipewire-audio",
            ModuleKind::Backend,
            MeshDependencies::default(),
            vec![MeshProvidesDeclaration {
                interface: "mesh.audio".into(),
                version: None,
                base_module: None,
                provider: Some("pipewire".into()),
                label: Some(crate::manifest::LocalizedText::Literal(
                    "PipeWire".to_string(),
                )),
                priority: 100,
            }],
            MeshContributes::default(),
        ),
        loaded_module(
            "@mesh/pulseaudio-audio",
            ModuleKind::Backend,
            MeshDependencies::default(),
            vec![MeshProvidesDeclaration {
                interface: "mesh.audio".into(),
                version: None,
                base_module: None,
                provider: Some("pulseaudio".into()),
                label: Some(crate::manifest::LocalizedText::Literal(
                    "PulseAudio".to_string(),
                )),
                priority: 50,
            }],
            MeshContributes::default(),
        ),
    ]
}

pub(super) fn interface_module(
    module_id: &str,
    name: &str,
    domain: &str,
    relationship: InterfaceRelationship,
    extends: Option<&str>,
) -> LoadedModuleManifest {
    let mut module = loaded_module(
        module_id,
        ModuleKind::Interface,
        MeshDependencies::default(),
        Vec::new(),
        MeshContributes::default(),
    );
    module.manifest.mesh.interface = Some(MeshInterfaceDeclaration {
        name: name.into(),
        version: Some("1.0".into()),
        contract: Some(serde_json::json!({})),
        domain: Some(domain.into()),
        extends: extends.map(str::to_string),
        relationship: Some(relationship),
        reason: None,
    });
    module
}

pub(super) fn collect_mesh_files(dir: &std::path::Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_mesh_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "mesh") {
            out.push(path);
        }
    }
}
