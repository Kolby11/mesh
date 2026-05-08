use std::collections::HashMap;
use std::path::PathBuf;

use mesh_core_module::ModuleType;
use mesh_core_module::lifecycle::ModuleInstance;
use mesh_core_render::{CompiledFrontendModule, compile_frontend_module};

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
    pub(super) props: serde_json::Map<String, serde_json::Value>,
}

impl FrontendCatalog {
    pub(in crate::shell) fn from_modules(
        modules: &HashMap<String, ModuleInstance>,
    ) -> Result<Self, ShellRunError> {
        let mut module_ids: Vec<String> = modules.keys().cloned().collect();
        module_ids.sort();

        let mut catalog = Self {
            modules: HashMap::new(),
            slot_contributions: HashMap::new(),
        };

        for module_id in module_ids {
            let Some(module) = modules.get(&module_id) else {
                continue;
            };

            if !mesh_core_render::is_frontend_module(&module.manifest) {
                continue;
            }

            let compiled =
                compile_frontend_module(&module.manifest, &module.path).map_err(|source| {
                    ShellRunError::FrontendCompile {
                        module_id: module_id.clone(),
                        source,
                    }
                })?;

            catalog.modules.insert(
                module_id.clone(),
                FrontendCatalogEntry {
                    module_dir: module.path.clone(),
                    compiled,
                },
            );
        }

        for (module_id, entry) in &catalog.modules {
            for (slot_id, contributions) in &entry.compiled.manifest.slot_contributions {
                let bucket = catalog
                    .slot_contributions
                    .entry(slot_id.clone())
                    .or_default();
                for (index, contribution) in contributions.iter().enumerate() {
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
                        props: contribution.props.clone(),
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
            return Err("target module is not a required dependency".into());
        }
        let Some(entry) = self.modules.get(module_id) else {
            return Err("target module is not loaded".into());
        };
        match entry.compiled.manifest.package.module_type {
            ModuleType::Widget | ModuleType::Surface => Ok(()),
            other => Err(format!(
                "target module must be a frontend widget or surface, got {other}"
            )),
        }
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
                "no explicit module import for component alias '{alias}'"
            ));
        };
        self.validate_component_module_import(host, module_id)?;
        Ok(module_id.clone())
    }
}
