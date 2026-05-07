use super::super::*;

impl Shell {
    pub(in crate::shell) fn reload_frontend_components_if_changed(
        &mut self,
    ) -> Result<(), ShellRunError> {
        for runtime in &mut self.components {
            if runtime.source_paths.is_empty() {
                continue;
            }

            let mut changed_path: Option<std::path::PathBuf> = None;
            for (path, last_mtime) in &runtime.source_paths {
                let Ok(metadata) = std::fs::metadata(path) else {
                    continue;
                };
                let Ok(modified_at) = metadata.modified() else {
                    continue;
                };
                if *last_mtime != Some(modified_at) {
                    changed_path = Some(path.clone());
                    break;
                }
            }

            let Some(trigger) = changed_path else {
                continue;
            };

            let reloaded = runtime
                .component
                .reload_source()
                .map_err(ShellRunError::Component)?;

            // Re-read the watched-paths list from the component because
            // imports may have changed between compilations, and refresh
            // every entry's mtime so we don't immediately reload again.
            runtime.source_paths = runtime
                .component
                .watched_source_paths()
                .into_iter()
                .map(|path| {
                    let mtime = std::fs::metadata(&path)
                        .ok()
                        .and_then(|m| m.modified().ok());
                    (path, mtime)
                })
                .collect();

            if reloaded {
                tracing::info!(
                    "recompiled frontend component '{}' (triggered by change in {})",
                    runtime.component.id(),
                    trigger.display()
                );
            }
        }

        Ok(())
    }

    pub(in crate::shell) fn reload_module_settings_if_changed(
        &mut self,
    ) -> Result<(), ShellRunError> {
        for runtime in &mut self.components {
            let current_settings_path = runtime.component.module_settings_path().map(PathBuf::from);
            if runtime.module_settings_path != current_settings_path {
                runtime.module_settings_path = current_settings_path.clone();
                runtime.module_settings_modified_at = None;
            }

            let Some(settings_path) = current_settings_path
                .as_ref()
                .or(runtime.module_settings_path.as_ref())
            else {
                continue;
            };

            let Ok(metadata) = std::fs::metadata(settings_path) else {
                continue;
            };
            let Ok(modified_at) = metadata.modified() else {
                continue;
            };

            if runtime.module_settings_modified_at == Some(modified_at) {
                continue;
            }

            runtime.module_settings_modified_at = Some(modified_at);

            let changed = runtime
                .component
                .reload_module_settings()
                .map_err(ShellRunError::Component)?;

            if changed {
                tracing::info!(
                    "module settings changed for component '{}'",
                    runtime.component.id()
                );
            }
        }
        Ok(())
    }

    pub(in crate::shell) fn tick_components(
        &mut self,
    ) -> Result<VecDeque<CoreRequest>, ShellRunError> {
        let mut requests = VecDeque::new();
        for runtime in &mut self.components {
            requests.extend(runtime.component.tick().map_err(ShellRunError::Component)?);
        }
        Ok(requests)
    }
}
