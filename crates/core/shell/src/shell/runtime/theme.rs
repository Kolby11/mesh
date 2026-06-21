use super::super::*;

const THEME_RELOAD_POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(500);
const SHELL_SETTINGS_RELOAD_POLL_INTERVAL: std::time::Duration =
    std::time::Duration::from_millis(500);

impl Shell {
    pub(in crate::shell) fn reload_theme_if_changed(
        &mut self,
    ) -> Result<VecDeque<CoreRequest>, ShellRunError> {
        let now = std::time::Instant::now();
        if now < self.next_theme_reload_check {
            return Ok(VecDeque::new());
        }
        self.next_theme_reload_check = now
            + if self.file_watcher_active {
                super::FILE_WATCHER_RELOAD_PARK
            } else {
                THEME_RELOAD_POLL_INTERVAL
            };

        let Ok(metadata) = std::fs::metadata(&self.theme_watch.path) else {
            return Ok(VecDeque::new());
        };
        let Ok(modified_at) = metadata.modified() else {
            return Ok(VecDeque::new());
        };

        if self.theme_watch.modified_at == Some(modified_at) {
            return Ok(VecDeque::new());
        }

        let old_theme_id = self.theme.active().id.clone();
        let theme = mesh_core_theme::load_theme_from_path(&self.theme_watch.path)
            .map_err(ShellRunError::Theme)?;
        tracing::info!(
            "reloaded active theme '{}' from {}",
            theme.id,
            self.theme_watch.path.display()
        );
        self.theme.replace_active(theme);
        self.theme_watch.modified_at = Some(modified_at);
        self.mark_components_theme_changed()?;
        let new_theme_id = self.theme.active().id.clone();
        if new_theme_id != old_theme_id {
            return self.sync_theme_service_state(&new_theme_id);
        }
        Ok(VecDeque::new())
    }

    pub(in crate::shell) fn mark_components_theme_changed(&mut self) -> Result<(), ShellRunError> {
        let theme_id = self.theme.active().id.clone();
        let is_dark = theme_id.contains("dark");
        for runtime in &mut self.components {
            runtime
                .component
                .theme_changed()
                .map_err(ShellRunError::Component)?;
            runtime.force_full_present = true;
        }
        // Broadcast event so script-side subscribers can react. Painter-level
        // invalidation is handled by `theme_changed()` above; the event is
        // additive — components that opt in via `handle_core_event` can use it
        // for non-visual derived state (e.g. icon name based on dark mode).
        let _ = self.broadcast_core_event(CoreEvent::ThemeChanged { theme_id, is_dark })?;
        Ok(())
    }

    pub(in crate::shell) fn apply_set_theme(
        &mut self,
        theme_id: &str,
    ) -> Result<VecDeque<CoreRequest>, ShellRunError> {
        if self.theme.set_active(theme_id).is_err() {
            let path = mesh_core_theme::theme_path_for_id(theme_id);
            match mesh_core_theme::load_theme_from_path(&path) {
                Ok(theme) => {
                    self.theme.register_theme(theme);
                    if let Err(e) = self.theme.set_active(theme_id) {
                        tracing::warn!("failed to activate theme '{theme_id}': {e}");
                        return Ok(VecDeque::new());
                    }
                }
                Err(e) => {
                    tracing::warn!("cannot load theme '{theme_id}': {e}");
                    return Ok(VecDeque::new());
                }
            }
        }
        tracing::info!("active theme changed to '{theme_id}'");
        self.settings.theme.active = theme_id.to_string();
        let path = mesh_core_theme::theme_path_for_id(theme_id);
        let modified_at = std::fs::metadata(&path)
            .ok()
            .and_then(|metadata| metadata.modified().ok());
        self.theme_watch = ThemeWatchState { path, modified_at };
        self.mark_components_theme_changed()?;
        self.sync_theme_service_state(theme_id)
    }

    pub(in crate::shell) fn sync_theme_service_state(
        &mut self,
        theme_id: &str,
    ) -> Result<VecDeque<CoreRequest>, ShellRunError> {
        let is_dark = theme_id.contains("dark");
        let payload =
            serde_json::json!({ "current": theme_id, "theme_id": theme_id, "is_dark": is_dark });
        if let Some(tx) = self.service_handlers.get("mesh.theme") {
            let _ = tx.send(ServiceCommandMsg {
                command: "set-current".to_string(),
                payload: payload.clone(),
                coalesce: true,
            });
        }
        self.broadcast_service_event(ServiceEvent::Updated {
            service: "mesh.theme".into(),
            source_module: "@mesh/shell".into(),
            payload,
        })
    }

    pub(in crate::shell) fn reload_locale_if_settings_changed(
        &mut self,
    ) -> Result<VecDeque<CoreRequest>, ShellRunError> {
        let mut requests = VecDeque::new();
        let now = std::time::Instant::now();
        if now < self.next_shell_settings_reload_check {
            return Ok(requests);
        }
        self.next_shell_settings_reload_check = now
            + if self.file_watcher_active {
                super::FILE_WATCHER_RELOAD_PARK
            } else {
                SHELL_SETTINGS_RELOAD_POLL_INTERVAL
            };

        let Ok(metadata) = std::fs::metadata(&self.settings_watch.path) else {
            return Ok(requests);
        };
        let Ok(modified_at) = metadata.modified() else {
            return Ok(requests);
        };

        if self.settings_watch.modified_at == Some(modified_at) {
            return Ok(requests);
        }

        self.settings_watch.modified_at = Some(modified_at);

        let new_settings = match load_shell_settings() {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!("failed to reload shell settings: {e}");
                return Ok(requests);
            }
        };

        let old_theme = self.settings.theme.clone();
        let old_i18n = self.settings.i18n.clone();
        let new_i18n = &new_settings.i18n;
        let locale_changed = old_i18n.locale != new_i18n.locale
            || old_i18n.fallback_locale != new_i18n.fallback_locale;

        let theme_changed = old_theme.active != new_settings.theme.active;
        if theme_changed {
            let (theme, theme_watch) = load_active_theme(&new_settings);
            let active_theme_id = theme.active().id.clone();
            tracing::info!(
                "active theme changed: {} -> {}",
                old_theme.active,
                active_theme_id
            );
            self.theme = theme;
            self.theme_watch = theme_watch;
            self.mark_components_theme_changed()?;
            requests.extend(self.sync_theme_service_state(&active_theme_id)?);
        }

        if locale_changed {
            tracing::info!(
                "locale changed: {} (fallback: {}) -> {} (fallback: {})",
                old_i18n.locale,
                old_i18n.fallback_locale,
                new_i18n.locale,
                new_i18n.fallback_locale,
            );
            self.locale = LocaleEngine::with_fallback_locale(
                new_i18n.locale.clone(),
                new_i18n.fallback_locale.clone(),
            );
            self.mark_components_locale_changed()?;
            requests.extend(self.sync_locale_service_state()?);
        }

        self.settings = new_settings;

        Ok(requests)
    }

    pub(in crate::shell) fn mark_components_locale_changed(&mut self) -> Result<(), ShellRunError> {
        let locale = self.locale.clone();
        let locale_id = locale.current().to_string();
        for runtime in &mut self.components {
            runtime
                .component
                .locale_changed(&locale)
                .map_err(ShellRunError::Component)?;
            runtime.force_full_present = true;
        }
        let _ = self.broadcast_core_event(CoreEvent::LocaleChanged { locale: locale_id })?;
        Ok(())
    }

    pub(in crate::shell) fn apply_set_locale(
        &mut self,
        locale: &str,
    ) -> Result<VecDeque<CoreRequest>, ShellRunError> {
        let locale = locale.trim();
        if locale.is_empty() {
            tracing::warn!("ignoring empty locale request");
            return Ok(VecDeque::new());
        }
        if self.locale.current() == locale {
            return self.sync_locale_service_state();
        }
        tracing::info!("active locale changed to '{locale}'");
        self.locale.set_locale(locale);
        self.mark_components_locale_changed()?;
        self.sync_locale_service_state()
    }

    pub(in crate::shell) fn sync_locale_service_state(
        &mut self,
    ) -> Result<VecDeque<CoreRequest>, ShellRunError> {
        let locale = self.locale.current().to_string();
        self.broadcast_service_event(ServiceEvent::Updated {
            service: "mesh.locale".into(),
            source_module: "@mesh/shell".into(),
            payload: serde_json::json!({
                "locale": locale.clone(),
                "current": locale
            }),
        })
    }
}
