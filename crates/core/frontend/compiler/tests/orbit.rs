use mesh_core_frontend::compile_frontend_module;
use mesh_core_module::manifest::load_canonical_manifest;
use std::path::PathBuf;

#[test]
fn orbit_components_compile() {
    for name in ["orbit-bar", "orbit-widgets"] {
        let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../../modules/frontend")
            .join(name);
        let manifest = load_canonical_manifest(&directory).unwrap();
        let compiled = compile_frontend_module(&manifest.manifest, &directory)
            .unwrap_or_else(|error| panic!("{name}: {error}"));
        let components: &[&str] = if name == "orbit-bar" {
            &["WorkspaceStrip", "Island", "StatusStrip"]
        } else {
            &["MarketCard", "ActivityCard", "UsageCard"]
        };
        for component in components {
            assert!(
                compiled.has_local_component(None, component),
                "{name}: {component}"
            );
        }
    }
}
