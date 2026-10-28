use mesh_core_frontend::compile_frontend_module;
use mesh_core_module::manifest::load_canonical_manifest;
use std::path::PathBuf;

#[test]
fn radial_launcher_compiles() {
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../modules/frontend/radial-launcher");
    let manifest = load_canonical_manifest(&directory).unwrap();
    compile_frontend_module(&manifest.manifest, &directory)
        .unwrap_or_else(|error| panic!("radial-launcher: {error}"));
}

#[test]
fn halo_orb_and_composition_manifests_load() {
    let modules = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../../modules");
    let orb = modules.join("frontend/halo-orb");
    let manifest = load_canonical_manifest(&orb).unwrap();
    compile_frontend_module(&manifest.manifest, &orb)
        .unwrap_or_else(|error| panic!("halo-orb: {error}"));
    load_canonical_manifest(&modules.join("compositions/halo")).unwrap();
}
