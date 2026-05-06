use std::collections::HashMap;
use std::path::PathBuf;

use mesh_core_plugin::PluginType;
use mesh_core_plugin::lifecycle::PluginInstance;
use mesh_core_render::{CompiledFrontendPlugin, compile_frontend_plugin};

use crate::shell::ShellRunError;

#[derive(Debug, Clone)]
pub(in crate::shell) struct FrontendCatalog {
    pub(super) plugins: HashMap<String, FrontendCatalogEntry>,
    pub(super) slot_contributions: HashMap<String, Vec<ResolvedSlotContribution>>,
}

#[derive(Debug, Clone)]
pub(in crate::shell) struct FrontendCatalogEntry {
    pub(in crate::shell) plugin_dir: PathBuf,
    pub(in crate::shell) compiled: CompiledFrontendPlugin,
}

#[derive(Debug, Clone)]
pub(super) struct ResolvedSlotContribution {
    pub(super) source_plugin_id: String,
    pub(super) widget_id: String,
    pub(super) contribution_id: String,
    pub(super) order: i64,
    pub(super) props: serde_json::Map<String, serde_json::Value>,
}

impl FrontendCatalog {
    pub(in crate::shell) fn from_plugins(
        plugins: &HashMap<String, PluginInstance>,
    ) -> Result<Self, ShellRunError> {
        let mut plugin_ids: Vec<String> = plugins.keys().cloned().collect();
        plugin_ids.sort();

        let mut catalog = Self {
            plugins: HashMap::new(),
            slot_contributions: HashMap::new(),
        };

        for plugin_id in plugin_ids {
            let Some(plugin) = plugins.get(&plugin_id) else {
                continue;
            };

            if !mesh_core_render::is_frontend_plugin(&plugin.manifest) {
                continue;
            }

            let compiled =
                compile_frontend_plugin(&plugin.manifest, &plugin.path).map_err(|source| {
                    ShellRunError::FrontendCompile {
                        plugin_id: plugin_id.clone(),
                        source,
                    }
                })?;

            catalog.plugins.insert(
                plugin_id.clone(),
                FrontendCatalogEntry {
                    plugin_dir: plugin.path.clone(),
                    compiled,
                },
            );
        }

        for (plugin_id, entry) in &catalog.plugins {
            for (slot_id, contributions) in &entry.compiled.manifest.slot_contributions {
                let bucket = catalog
                    .slot_contributions
                    .entry(slot_id.clone())
                    .or_default();
                for (index, contribution) in contributions.iter().enumerate() {
                    bucket.push(ResolvedSlotContribution {
                        source_plugin_id: plugin_id.clone(),
                        widget_id: contribution
                            .widget
                            .clone()
                            .unwrap_or_else(|| plugin_id.clone()),
                        contribution_id: contribution
                            .id
                            .clone()
                            .unwrap_or_else(|| format!("{plugin_id}:{slot_id}:{index}")),
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

        for (plugin_id, entry) in &catalog.plugins {
            for (alias, target_plugin_id) in &entry.compiled.plugin_component_imports {
                catalog
                    .validate_component_plugin_import(&entry.compiled.manifest, target_plugin_id)
                    .map_err(|message| ShellRunError::FrontendComposition {
                        message: format!(
                            "plugin '{plugin_id}' cannot import {alias} from '{target_plugin_id}': {message}"
                        ),
                    })?;
            }
            for component_tag in entry.compiled.referenced_component_tags() {
                if entry.compiled.local_components.contains_key(&component_tag) {
                    continue;
                }
                if entry
                    .compiled
                    .plugin_component_imports
                    .contains_key(&component_tag)
                {
                    continue;
                }
                return Err(ShellRunError::FrontendComposition {
                    message: format!(
                        "plugin '{plugin_id}' references <{component_tag}> but no explicit component import was compiled for that tag"
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
            .plugins
            .values()
            .filter(|entry| entry.compiled.manifest.package.plugin_type == PluginType::Surface)
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

    fn validate_component_plugin_import(
        &self,
        host: &mesh_core_plugin::Manifest,
        plugin_id: &str,
    ) -> Result<(), String> {
        if !host
            .required_plugin_dependencies()
            .iter()
            .any(|dependency_id| dependency_id == plugin_id)
        {
            return Err("target plugin is not a required dependency".into());
        }
        let Some(entry) = self.plugins.get(plugin_id) else {
            return Err("target plugin is not loaded".into());
        };
        match entry.compiled.manifest.package.plugin_type {
            PluginType::Widget | PluginType::Surface => Ok(()),
            other => Err(format!(
                "target plugin must be a frontend widget or surface, got {other}"
            )),
        }
    }

    pub(super) fn imported_component_plugin_id(
        &self,
        host: &mesh_core_plugin::Manifest,
        alias: &str,
    ) -> Result<String, String> {
        let Some(entry) = self.plugins.get(&host.package.id) else {
            return Err("host plugin is not loaded".into());
        };
        let Some(plugin_id) = entry.compiled.plugin_component_imports.get(alias) else {
            return Err(format!(
                "no explicit plugin import for component alias '{alias}'"
            ));
        };
        self.validate_component_plugin_import(host, plugin_id)?;
        Ok(plugin_id.clone())
    }
}
