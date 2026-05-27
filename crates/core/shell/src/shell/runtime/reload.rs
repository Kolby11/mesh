use super::super::*;

const FRONTEND_RELOAD_POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(250);
const MODULE_SETTINGS_RELOAD_POLL_INTERVAL: std::time::Duration =
    std::time::Duration::from_millis(500);

impl Shell {
    pub(in crate::shell) fn reload_frontend_components_if_changed(
        &mut self,
    ) -> Result<(), ShellRunError> {
        let now = std::time::Instant::now();
        if now < self.next_frontend_reload_check {
            return Ok(());
        }
        self.next_frontend_reload_check = now + FRONTEND_RELOAD_POLL_INTERVAL;

        for runtime in &mut self.components {
            if runtime.source_paths.is_empty() {
                continue;
            }

            let mut changed_path_index: Option<usize> = None;
            for (index, (path, last_mtime)) in runtime.source_paths.iter().enumerate() {
                let Ok(metadata) = std::fs::metadata(path) else {
                    continue;
                };
                let Ok(modified_at) = metadata.modified() else {
                    continue;
                };
                if *last_mtime != Some(modified_at) {
                    changed_path_index = Some(index);
                    break;
                }
            }

            let Some(trigger_index) = changed_path_index else {
                continue;
            };
            let trigger_display = runtime.source_paths[trigger_index].0.display().to_string();

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
                    trigger_display
                );
            }
        }

        Ok(())
    }

    pub(in crate::shell) fn reload_module_settings_if_changed(
        &mut self,
    ) -> Result<(), ShellRunError> {
        let now = std::time::Instant::now();
        if now < self.next_module_settings_reload_check {
            return Ok(());
        }
        self.next_module_settings_reload_check = now + MODULE_SETTINGS_RELOAD_POLL_INTERVAL;

        for runtime in &mut self.components {
            let current_settings_path = runtime.component.module_settings_path();
            if runtime.module_settings_path.as_deref() != current_settings_path {
                runtime.module_settings_path = current_settings_path.map(PathBuf::from);
                runtime.module_settings_modified_at = None;
            }

            let Some(settings_path) = runtime.module_settings_path.as_ref() else {
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
            if !runtime.component.wants_tick() {
                continue;
            }
            requests.extend(runtime.component.tick().map_err(ShellRunError::Component)?);
        }
        Ok(requests)
    }
}
