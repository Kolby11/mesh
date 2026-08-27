use super::super::types::watched_source_mtime;
use super::super::*;

const FRONTEND_RELOAD_POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(250);

impl Shell {
    pub(in crate::shell) fn reload_frontend_components_if_changed(
        &mut self,
    ) -> Result<(), ShellRunError> {
        let now = std::time::Instant::now();
        if now < self.next_frontend_reload_check {
            return Ok(());
        }
        self.next_frontend_reload_check = now
            + if self.file_watcher_active {
                super::FILE_WATCHER_RELOAD_PARK
            } else {
                FRONTEND_RELOAD_POLL_INTERVAL
            };

        for component_index in 0..self.components.len() {
            let Some(trigger_index) = self.components[component_index]
                .source_paths
                .iter()
                .position(|(path, last_mtime)| watched_source_mtime(path) != *last_mtime)
            else {
                continue;
            };
            let trigger_display = self.components[component_index].source_paths[trigger_index]
                .0
                .display()
                .to_string();
            let reload_result = self.components[component_index].component.reload_source();
            match reload_result {
                Ok(reloaded) => {
                    let runtime = &mut self.components[component_index];
                    runtime.source_paths = runtime
                        .component
                        .watched_source_paths()
                        .into_iter()
                        .map(|path| {
                            let mtime = watched_source_mtime(&path);
                            (path, mtime)
                        })
                        .collect();
                    if reloaded {
                        tracing::info!(
                            "recompiled frontend component '{}' (triggered by change in {})",
                            runtime.component.id(),
                            trigger_display
                        );
                        self.clear_component_failure(component_index);
                        self.sync_frontend_catalog_components();
                    }
                }
                Err(error) => {
                    let trigger_mtime = watched_source_mtime(
                        &self.components[component_index].source_paths[trigger_index].0,
                    );
                    self.contain_component_failure(component_index, "reload", &error);
                    // A quarantined component waits for a new source edit or
                    // activation replacement instead of retrying the same
                    // broken source on every polling interval.
                    if self.component_is_quarantined(component_index) {
                        self.components[component_index].source_paths[trigger_index].1 =
                            trigger_mtime;
                    }
                }
            }
        }

        Ok(())
    }

    pub(in crate::shell) fn tick_components(
        &mut self,
    ) -> Result<VecDeque<CoreRequest>, ShellRunError> {
        let mut requests = VecDeque::new();
        for component_index in 0..self.components.len() {
            if self.component_is_quarantined(component_index) {
                continue;
            }
            let wants_tick = self.components[component_index].component.wants_tick();
            if !wants_tick {
                continue;
            }
            match self.components[component_index].component.tick() {
                Ok(component_requests) => requests.extend(component_requests),
                Err(error) => {
                    self.contain_component_failure(component_index, "tick", &error);
                }
            }
        }
        Ok(requests)
    }
}
