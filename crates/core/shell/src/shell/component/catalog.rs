use std::collections::HashMap;
use std::path::PathBuf;

use mesh_core_frontend::{CompiledFrontendModule, compile_frontend_module};
use mesh_core_module::ModuleType;
use mesh_core_module::lifecycle::ModuleInstance;
use mesh_core_module::package::InstalledModuleGraph;
use rayon::prelude::*;

use super::memo;
use crate::shell::ShellRunError;

#[derive(Debug, Clone)]
pub(in crate::shell) struct FrontendCatalog {
    pub(super) modules: HashMap<String, FrontendCatalogEntry>,
    pub(super) slot_contributions: HashMap<String, Vec<ResolvedSlotContribution>>,
}

#[derive(Debug, Clone)]
pub(in crate::shell) struct FrontendCatalogEntry {
    pub(in crate::shell) module_dir: PathBuf,
    pub(in crate::shell) compiled: CompiledFrontendModule,
}

#[derive(Debug, Clone)]
pub(super) struct ResolvedSlotContribution {
    pub(super) source_module_id: String,
    pub(super) widget_id: String,
    pub(super) contribution_id: String,
    pub(super) order: i64,
    pub(super) props_fingerprint: u64,
    pub(super) props: serde_json::Map<String, serde_json::Value>,
}

impl FrontendCatalog {
    pub(in crate::shell) fn from_modules(
        modules: &HashMap<String, ModuleInstance>,
        graph: Option<&InstalledModuleGraph>,
    ) -> Result<Self, ShellRunError> {
        let mut module_ids: Vec<String> = modules.keys().cloned().collect();
        module_ids.sort();

        let frontend_modules: Vec<_> = module_ids
            .iter()
            .filter_map(|module_id| {
                let module = modules.get(module_id)?;
                mesh_core_frontend::is_frontend_module(&module.manifest)
                    .then_some((module_id, module))
            })
            .collect();
        let compiled_entries = frontend_modules
            .par_iter()
            .map(|(module_id, module)| {
                compile_frontend_module(&module.manifest, &module.path)
                    .map(|compiled| {
                        (
                            (*module_id).clone(),
                            FrontendCatalogEntry {
                                module_dir: module.path.clone(),
                                compiled,
                            },
                        )
                    })
                    .map_err(|source| ShellRunError::FrontendCompile {
                        module_id: (*module_id).clone(),
                        source,
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;

        let mut catalog = Self {
            modules: compiled_entries.into_iter().collect(),
            slot_contributions: HashMap::new(),
        };

        for (module_id, entry) in &catalog.modules {
            for (slot_id, contributions) in &entry.compiled.manifest.slot_contributions {
                let bucket = catalog
                    .slot_contributions
                    .entry(slot_id.clone())
                    .or_default();
                for (index, contribution) in contributions.iter().enumerate() {
                    let props = contribution.props.clone();
                    bucket.push(ResolvedSlotContribution {
                        source_module_id: module_id.clone(),
                        widget_id: contribution
                            .widget
                            .clone()
                            .unwrap_or_else(|| module_id.clone()),
                        contribution_id: contribution
                            .id
                            .clone()
                            .unwrap_or_else(|| format!("{module_id}:{slot_id}:{index}")),
                        order: contribution.order.unwrap_or(0),
                        props_fingerprint: memo::slot_props_fingerprint(&props),
                        props,
                    });
                }
            }
        }

        for contributions in catalog.slot_contributions.values_mut() {
            contributions.sort_by(|left, right| {
                left.order
                    .cmp(&right.order)
                    .then_with(|| left.widget_id.cmp(&right.widget_id))
                    .then_with(|| left.contribution_id.cmp(&right.contribution_id))
            });
        }

        for (module_id, entry) in &catalog.modules {
            for (alias, target_module_id) in &entry.compiled.module_component_imports {
                catalog
                    .validate_component_module_import(&entry.compiled.manifest, target_module_id)
                    .map_err(|message| ShellRunError::FrontendComposition {
                        message: format!(
                            "module '{module_id}' cannot import {alias} from '{target_module_id}': {message}"
                        ),
                    })?;
            }
            for component_tag in entry.compiled.referenced_component_tags() {
                if entry.compiled.local_components.contains_key(&component_tag) {
                    continue;
                }
                if entry
                    .compiled
                    .module_component_imports
                    .contains_key(&component_tag)
                {
                    continue;
                }
                return Err(ShellRunError::FrontendComposition {
                    message: format!(
                        "module '{module_id}' references <{component_tag}> but no explicit component import was compiled for that tag"
                    ),
                });
            }
            if let Some(graph) = graph {
                catalog
                    .validate_interface_imports(module_id, &entry.compiled, graph)
                    .map_err(|message| ShellRunError::FrontendComposition { message })?;
            }
        }

        Ok(catalog)
    }

    pub(super) fn slot_contributions_for(&self, slot_id: &str) -> &[ResolvedSlotContribution] {
        self.slot_contributions
            .get(slot_id)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    pub(in crate::shell) fn top_level_surfaces(&self) -> Vec<FrontendCatalogEntry> {
        let mut entries: Vec<FrontendCatalogEntry> = self
            .modules
            .values()
            .filter(|entry| entry.compiled.manifest.package.module_type == ModuleType::Surface)
            .cloned()
            .collect();
        entries.sort_by(|left, right| {
            left.compiled
                .manifest
                .package
                .id
                .cmp(&right.compiled.manifest.package.id)
        });
        entries
    }

    pub(in crate::shell) fn top_level_surfaces_filtered(
        &self,
        enabled_frontends: Option<&std::collections::HashSet<String>>,
    ) -> Vec<FrontendCatalogEntry> {
        let mut entries = self.top_level_surfaces();
        if let Some(enabled_frontends) = enabled_frontends {
            entries.retain(|entry| enabled_frontends.contains(&entry.compiled.manifest.package.id));
        }
        entries
    }

    fn validate_component_module_import(
        &self,
        host: &mesh_core_module::Manifest,
        module_id: &str,
    ) -> Result<(), String> {
        if !host
            .required_module_dependencies()
            .iter()
            .any(|dependency_id| dependency_id == module_id)
        {
            return Err(format!(
                "target module '{module_id}' is not declared in mesh.uses.modules as a required module dependency"
            ));
        }
        let Some(entry) = self.modules.get(module_id) else {
            return Err("target module is not loaded".into());
        };
        match entry.compiled.manifest.package.module_type {
            ModuleType::Widget | ModuleType::Surface | ModuleType::Component => Ok(()),
            other => Err(format!(
                "target module must be a frontend widget, component, or surface, got {other}"
            )),
        }
    }

    fn validate_interface_imports(
        &self,
        module_id: &str,
        compiled: &CompiledFrontendModule,
        graph: &InstalledModuleGraph,
    ) -> Result<(), String> {
        let Some(requirements) = graph.requirements_for_frontend(module_id) else {
            return Ok(());
        };
        let declared = requirements
            .backend
            .keys()
            .chain(requirements.optional_backend.keys())
            .collect::<std::collections::HashSet<_>>();

        for interface in compiled_interface_imports(compiled) {
            if !declared.contains(&interface) {
                return Err(format!(
                    "module '{module_id}' imports interface '{interface}' but does not declare it in mesh.uses.interfaces or mesh.uses.optionalInterfaces"
                ));
            }
        }

        Ok(())
    }

    pub(super) fn imported_component_module_id(
        &self,
        host: &mesh_core_module::Manifest,
        alias: &str,
    ) -> Result<String, String> {
        let Some(entry) = self.modules.get(&host.package.id) else {
            return Err("host module is not loaded".into());
        };
        let Some(module_id) = entry.compiled.module_component_imports.get(alias) else {
            return Err(format!(
                "no explicit component import for alias '{alias}'; add a script import such as local {alias} = require(\"@scope/module\")"
            ));
        };
        self.validate_component_module_import(host, module_id)?;
        Ok(module_id.clone())
    }
}

#[cfg(test)]
mod performance_tests {
    use super::*;
    use mesh_core_module::lifecycle::ModuleInstance;

    fn shipped_frontend_modules() -> HashMap<String, ModuleInstance> {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
        std::fs::read_dir(root.join("modules/frontend"))
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
            .filter_map(|entry| {
                let module_dir = entry.path();
                let loaded = mesh_core_module::manifest::load_manifest(&module_dir).ok()?;
                let module_id = loaded.manifest.package.id.clone();
                Some((
                    module_id,
                    ModuleInstance::new(loaded.manifest, module_dir, loaded.path, loaded.source),
                ))
            })
            .collect()
    }

    fn compile_sequentially(
        modules: &HashMap<String, ModuleInstance>,
    ) -> Result<Vec<(String, FrontendCatalogEntry)>, ShellRunError> {
        let mut module_ids: Vec<_> = modules.keys().cloned().collect();
        module_ids.sort();
        module_ids
            .into_iter()
            .filter_map(|module_id| {
                let module = modules.get(&module_id)?;
                mesh_core_frontend::is_frontend_module(&module.manifest)
                    .then_some((module_id, module))
            })
            .map(|(module_id, module)| {
                compile_frontend_module(&module.manifest, &module.path)
                    .map(|compiled| {
                        (
                            module_id.clone(),
                            FrontendCatalogEntry {
                                module_dir: module.path.clone(),
                                compiled,
                            },
                        )
                    })
                    .map_err(|source| ShellRunError::FrontendCompile { module_id, source })
            })
            .collect()
    }

    #[test]
    fn parallel_catalog_compilation_matches_sequential_module_set() {
        let modules = shipped_frontend_modules();
        let sequential: std::collections::HashSet<_> = compile_sequentially(&modules)
            .unwrap()
            .into_iter()
            .map(|(id, _)| id)
            .collect();
        let parallel: std::collections::HashSet<_> = FrontendCatalog::from_modules(&modules, None)
            .unwrap()
            .modules
            .into_keys()
            .collect();
        assert_eq!(parallel, sequential);
    }

    #[test]
    #[ignore = "release-only frontend compilation benchmark"]
    fn parallel_frontend_compilation_beats_sequential_startup() {
        use std::hint::black_box;
        use std::time::Instant;

        let modules = shipped_frontend_modules();
        let iterations = 20;

        let started = Instant::now();
        for _ in 0..iterations {
            black_box(compile_sequentially(&modules).unwrap());
        }
        let sequential = started.elapsed();

        let started = Instant::now();
        for _ in 0..iterations {
            black_box(FrontendCatalog::from_modules(&modules, None).unwrap());
        }
        let parallel = started.elapsed();

        eprintln!(
            "frontend compilation over {iterations} shipped-catalog builds: sequential {sequential:?}, parallel {parallel:?}"
        );
    }
}

fn compiled_interface_imports(
    compiled: &CompiledFrontendModule,
) -> std::collections::HashSet<String> {
    compiled
        .local_components
        .values()
        .chain(std::iter::once(&compiled.component))
        .flat_map(|component| {
            component
                .imports
                .iter()
                .filter_map(|import| match &import.target {
                    mesh_core_component::ComponentImportTarget::InterfaceApi {
                        interface, ..
                    } => Some(interface.clone()),
                    _ => None,
                })
        })
        .collect()
}
