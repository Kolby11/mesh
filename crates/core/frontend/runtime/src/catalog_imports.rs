use mesh_core_frontend::CompiledFrontendModule;
use mesh_core_module::Manifest;

pub fn validate_interface_import_declarations(
    module_id: &str,
    compiled: &CompiledFrontendModule,
    manifest: &Manifest,
) -> Result<(), crate::FrontendCatalogError> {
    for (interface, requested_version) in compiled_interface_imports(compiled) {
        let declared = manifest.dependencies.interfaces.iter().any(|dependency| {
            mesh_core_service::canonical_interface_name(&dependency.name) == interface
        });
        if !declared {
            return Err(crate::FrontendCatalogError::FrontendComposition {
                message: format!(
                    "module '{module_id}' imports interface '{interface}' but does not declare it in mesh.uses.interfaces or mesh.uses.optionalInterfaces"
                ),
            });
        }
        if let Some(version) = requested_version {
            if mesh_core_service::parse_version_req(&version).is_none() {
                return Err(crate::FrontendCatalogError::FrontendComposition {
                    message: format!(
                        "module '{module_id}' imports interface '{interface}' with invalid version range '{version}'"
                    ),
                });
            }
        }
    }
    Ok(())
}

pub fn compiled_interface_imports(
    compiled: &CompiledFrontendModule,
) -> Vec<(String, Option<String>)> {
    let mut imports = std::collections::BTreeSet::new();
    compiled
        .all_local_components()
        .into_iter()
        .chain(std::iter::once(&compiled.component))
        .flat_map(|component| {
            component
                .imports
                .iter()
                .filter_map(|import| match &import.target {
                    mesh_core_component::ComponentImportTarget::InterfaceApi {
                        interface,
                        version,
                    } => Some((
                        mesh_core_service::canonical_interface_name(interface),
                        version.clone(),
                    )),
                    _ => None,
                })
        })
        .for_each(|import| {
            imports.insert(import);
        });
    imports.into_iter().collect()
}
