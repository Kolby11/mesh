use super::super::*;
use super::is_bare_printable_key;

#[derive(Debug, Clone)]
pub(in crate::shell::component) struct ResolvedSurfaceShortcut {
    pub(in crate::shell::component) keybind_id: String,
    pub(in crate::shell::component) key: String,
    pub(in crate::shell::component) modifiers: Vec<String>,
    pub(in crate::shell::component) trigger_kind: mesh_core_module::KeybindTriggerKind,
    pub(in crate::shell::component) source: KeybindResolutionSource,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum KeybindResolutionSource {
    UserOverride,
    LocaleDefault { locale: String },
    ModuleDefault,
}

#[derive(Debug, Clone)]
struct SurfaceShortcutDeclaration {
    keybind_id: String,
    generic_trigger: mesh_core_module::KeybindTrigger,
    localized_triggers: HashMap<String, mesh_core_module::KeybindTrigger>,
}

#[derive(Debug, Clone)]
pub(in crate::shell::component) struct KeybindSubscriber {
    pub(in crate::shell::component) keybind_id: String,
    pub(in crate::shell::component) node_key: String,
    pub(in crate::shell::component) handler: String,
}

impl FrontendSurfaceComponent {
    pub(super) fn handle_key_pressed(
        &mut self,
        tree: &WidgetNode,
        key: String,
        modifiers: KeyModifiers,
    ) -> Result<Vec<CoreRequest>, ComponentError> {
        let keyboard_settings = self.current_keyboard_settings();
        if matches!(key.as_str(), "Tab") && !modifiers.ctrl && !modifiers.alt {
            self.clear_selection();
            self.invalidate_interaction_restyle();
            return self.handle_tab_with_cross_surface(tree, modifiers.shift);
        }
        if matches!(key.as_str(), "Escape") && !modifiers.ctrl && !modifiers.alt {
            if let Some(requests) = self.handle_escape_with_cross_surface()? {
                self.clear_selection();
                self.invalidate_interaction_restyle();
                return Ok(requests);
            }
        }
        if modifiers.ctrl
            && key.eq_ignore_ascii_case("c")
            && let Some(text) = self.selection_copy_payload(tree)
        {
            return Ok(vec![CoreRequest::WriteClipboard { text }]);
        }

        let focused_key = self.normalized_focused_key(tree);
        let focused_text_input_has_bare_printable_key = focused_key
            .as_deref()
            .is_some_and(|focused_key| is_input_key(tree, focused_key))
            && is_bare_printable_key(&key, modifiers);
        if !focused_text_input_has_bare_printable_key {
            if let Some(requests) =
                self.dispatch_surface_shortcut(tree, &key, modifiers, &keyboard_settings)?
            {
                return Ok(requests);
            }
        }
        self.focus_visible_key = self.focused_key.clone();
        if let Some(focused_key) = focused_key {
            let mut requests = self.dispatch_focused_keyboard_handler(
                tree,
                &focused_key,
                "keydown",
                "keydown",
                &key,
                modifiers,
            )?;
            if is_input_key(tree, &focused_key) {
                self.clear_selection();
                let value = self.input_values.entry(focused_key.clone()).or_default();
                match key.as_str() {
                    "Backspace" => {
                        value.pop();
                        let current = value.clone();
                        self.invalidate_text_state();
                        requests.extend(self.dispatch_text_input_value_handlers(
                            tree,
                            &focused_key,
                            &current,
                        )?);
                        return Ok(requests);
                    }
                    _ => {}
                }
            } else if (Self::key_matches_any_binding(
                &key,
                &keyboard_settings.slider_decrement_keys,
            ) || Self::key_matches_any_binding(
                &key,
                &keyboard_settings.slider_increment_keys,
            )) && is_slider_key(tree, &focused_key)
            {
                self.clear_selection();
                let delta = if Self::key_matches_any_binding(
                    &key,
                    &keyboard_settings.slider_decrement_keys,
                ) {
                    -1.0
                } else {
                    1.0
                };
                if let Some(value) = self.slider_step_value(tree, &focused_key, delta) {
                    self.preserve_slider_value(tree, &focused_key, value);
                    self.invalidate_interaction_restyle();
                    requests.extend(self.call_node_handler(
                        tree,
                        &focused_key,
                        "change",
                        &[serde_json::json!(value)],
                    )?);
                    return Ok(requests);
                }
            }

            if find_node_by_key(tree, &focused_key).is_some_and(|node| node.tag == "button")
                && Self::key_matches_any_binding(&key, &keyboard_settings.button_activation_keys)
            {
                self.clear_selection();
                self.keyboard_button_press_activations
                    .insert((focused_key.clone(), key.clone()));
                requests.extend(self.dispatch_keyboard_button_activation(
                    tree,
                    &focused_key,
                    &key,
                )?);
                return Ok(requests);
            }

            if matches!(key.as_str(), "ArrowDown" | "ArrowUp")
                && let Some(next_key) =
                    self.rove_focus_within_parent(tree, &focused_key, key == "ArrowUp")
            {
                requests.extend(self.set_focus_target(tree, Some(next_key), true)?);
                self.invalidate_interaction_restyle();
                return Ok(requests);
            }

            if !requests.is_empty() {
                return Ok(requests);
            }
        }

        Ok(Vec::new())
    }

    pub(super) fn handle_key_released(
        &mut self,
        tree: &WidgetNode,
        key: String,
        modifiers: KeyModifiers,
    ) -> Result<Vec<CoreRequest>, ComponentError> {
        let keyboard_settings = self.current_keyboard_settings();
        self.focus_visible_key = self.focused_key.clone();
        if let Some(focused_key) = self.normalized_focused_key(tree) {
            let mut requests = self.dispatch_focused_keyboard_handler(
                tree,
                &focused_key,
                "keyup",
                "keyup",
                &key,
                modifiers,
            )?;

            if find_node_by_key(tree, &focused_key).is_some_and(|node| node.tag == "button")
                && Self::key_matches_any_binding(&key, &keyboard_settings.button_activation_keys)
            {
                self.clear_selection();
                if self
                    .keyboard_button_press_activations
                    .remove(&(focused_key.clone(), key.clone()))
                {
                    return Ok(requests);
                }
                requests.extend(self.dispatch_keyboard_button_activation(
                    tree,
                    &focused_key,
                    &key,
                )?);
                return Ok(requests);
            }

            if Self::key_matches_any_binding(&key, &keyboard_settings.toggle_activation_keys)
                && (self.is_checkable_choice_key(tree, &focused_key)
                    || self.is_radio_key(tree, &focused_key)
                    || self.is_option_key(tree, &focused_key)
                    || self.is_menu_item_key(tree, &focused_key)
                    || self.is_container_collection_item_key(tree, &focused_key))
            {
                self.clear_selection();
                self.invalidate_interaction_restyle();
                if self.is_option_key(tree, &focused_key) {
                    requests.extend(self.activate_option_choice(tree, &focused_key)?);
                } else if self.is_radio_key(tree, &focused_key) {
                    requests.extend(self.activate_radio_choice(tree, &focused_key)?);
                } else if self.is_menu_item_key(tree, &focused_key)
                    || self.is_container_collection_item_key(tree, &focused_key)
                {
                    let click_event = self.build_click_event(tree, &focused_key, 0.0, 0.0);
                    requests.extend(self.dispatch_activation_handlers(
                        tree,
                        &focused_key,
                        click_event,
                    )?);
                } else {
                    let value = self.toggle_checked_value(tree, &focused_key);
                    requests.extend(self.call_node_handler(
                        tree,
                        &focused_key,
                        "change",
                        &[serde_json::json!(value)],
                    )?);
                }
                return Ok(requests);
            }

            if !requests.is_empty() {
                return Ok(requests);
            }
        }

        Ok(Vec::new())
    }

    pub(super) fn build_keyboard_event(
        &self,
        tree: &WidgetNode,
        node_key: &str,
        event_type: &str,
        key: &str,
        modifiers: KeyModifiers,
    ) -> serde_json::Value {
        let target = find_node_by_key(tree, node_key);
        let (left, top, right, bottom) =
            find_node_bounds_by_key(tree, node_key, 0.0, 0.0).unwrap_or((0.0, 0.0, 0.0, 0.0));
        let width = (right - left).max(0.0);
        let height = (bottom - top).max(0.0);
        let bounds = serde_json::json!({
            "left": left,
            "top": top,
            "right": right,
            "bottom": bottom,
            "width": width,
            "height": height,
        });
        let position = serde_json::json!({
            "margin_left": left.round() as i32,
            "margin_bottom": bottom.round() as i32,
        });
        let tag = target.map(|node| node.tag.clone()).unwrap_or_default();
        let mut current_target = target
            .map(|node| element_snapshot_json(node, left - node.layout.x, top - node.layout.y))
            .unwrap_or_else(|| serde_json::json!({}));
        if let Some(object) = current_target.as_object_mut() {
            object.insert(
                "key".into(),
                serde_json::Value::String(node_key.to_string()),
            );
            object.insert("tag".into(), serde_json::Value::String(tag.clone()));
            object.insert("bounds".into(), bounds.clone());
            object.insert("position".into(), position.clone());
        }

        serde_json::json!({
            "type": event_type,
            "key": key,
            "modifiers": {
                "ctrl": modifiers.ctrl,
                "shift": modifiers.shift,
                "alt": modifiers.alt,
            },
            "surface": {
                "id": self.surface_id(),
                "width": tree.layout.width,
                "height": tree.layout.height,
            },
            "current": {
                "key": node_key,
                "tag": tag,
                "bounds": bounds,
                "position": position,
            },
            "current_target": current_target,
        })
    }

    pub(super) fn dispatch_focused_keyboard_handler(
        &mut self,
        tree: &WidgetNode,
        node_key: &str,
        handler_name: &str,
        event_type: &str,
        key: &str,
        modifiers: KeyModifiers,
    ) -> Result<Vec<CoreRequest>, ComponentError> {
        let event = self.build_keyboard_event(tree, node_key, event_type, key, modifiers);
        self.call_node_handler(tree, node_key, handler_name, &[event])
    }

    pub(super) fn dispatch_keyboard_button_activation(
        &mut self,
        tree: &WidgetNode,
        node_key: &str,
        key: &str,
    ) -> Result<Vec<CoreRequest>, ComponentError> {
        let Some(handler) = find_click_handler(tree, node_key) else {
            return Ok(Vec::new());
        };
        let (left, top, right, bottom) =
            find_node_bounds_by_key(tree, node_key, 0.0, 0.0).unwrap_or((0.0, 0.0, 0.0, 0.0));
        let center_x = (left + right) * 0.5;
        let center_y = (top + bottom) * 0.5;
        let mut event = self.build_click_event(tree, node_key, center_x, center_y);
        if let Some(object) = event.as_object_mut() {
            object.insert(
                "trigger".into(),
                serde_json::json!({
                    "type": "keyboard",
                    "key": key,
                }),
            );
        }
        self.call_namespaced_handler(&handler, &[event])
    }

    pub(super) fn current_keyboard_settings(&self) -> mesh_core_config::KeyboardSettings {
        mesh_core_config::load_shell_settings()
            .map(|settings| settings.keyboard)
            .unwrap_or_default()
    }

    pub(super) fn key_matches_binding(key: &str, binding: &str) -> bool {
        normalize_key_name(key) == normalize_key_name(binding)
    }

    pub(super) fn key_matches_any_binding(key: &str, bindings: &[String]) -> bool {
        bindings
            .iter()
            .any(|binding| Self::key_matches_binding(key, binding))
    }

    pub(in crate::shell::component) fn resolved_surface_shortcuts(
        &self,
        keyboard_settings: &mesh_core_config::KeyboardSettings,
    ) -> Vec<ResolvedSurfaceShortcut> {
        let declarations = self.surface_shortcut_declarations();
        let overrides = keyboard_settings.surface_shortcuts.get(self.surface_id());
        let active_locale = self.locale.current();
        let declared_ids = declarations
            .iter()
            .map(|declaration| declaration.keybind_id.as_str())
            .collect::<HashSet<_>>();

        if let Some(overrides) = overrides {
            for action_id in overrides.keys() {
                if !declared_ids.contains(action_id.as_str()) {
                    self.record_keybind_diagnostic(
                        action_id,
                        "user override references undeclared keybind action",
                    );
                }
            }
        }

        if declarations.is_empty() {
            return Vec::new();
        }

        let resolved = declarations
            .into_iter()
            .filter_map(|declaration| {
                let override_key = overrides
                    .and_then(|surface| surface.get(&declaration.keybind_id))
                    .and_then(|shortcut| shortcut.key.clone());
                self.resolve_surface_shortcut_declaration(declaration, override_key, active_locale)
            })
            .collect::<Vec<_>>();

        self.record_duplicate_surface_shortcut_diagnostics(&resolved);
        resolved
    }

    fn surface_shortcut_declarations(&self) -> Vec<SurfaceShortcutDeclaration> {
        let mut declarations = self.manifest_surface_shortcut_declarations();
        for legacy in self.legacy_settings_surface_shortcut_declarations() {
            if declarations
                .iter()
                .any(|declaration| declaration.keybind_id == legacy.keybind_id)
            {
                self.record_keybind_diagnostic(
                    &legacy.keybind_id,
                    "legacy settings shortcut is ignored because mesh.keybinds declares this action",
                );
                continue;
            }
            self.record_keybind_diagnostic(
                &legacy.keybind_id,
                "legacy settings shortcut declarations are migration-only; declare this action in mesh.keybinds",
            );
        }
        declarations.sort_by(|left, right| left.keybind_id.cmp(&right.keybind_id));
        declarations
    }

    fn manifest_surface_shortcut_declarations(&self) -> Vec<SurfaceShortcutDeclaration> {
        self.compiled
            .manifest
            .keybinds
            .actions
            .iter()
            .map(|(keybind_id, action)| SurfaceShortcutDeclaration {
                keybind_id: keybind_id.clone(),
                generic_trigger: action.trigger.clone(),
                localized_triggers: action.localized_triggers.clone(),
            })
            .collect()
    }

    fn legacy_settings_surface_shortcut_declarations(&self) -> Vec<SurfaceShortcutDeclaration> {
        surface_shortcut_declarations_from_settings(&self.settings_json)
    }

    pub(in crate::shell::component) fn keybind_subscribers(
        &self,
        tree: &WidgetNode,
    ) -> Vec<KeybindSubscriber> {
        let mut subscribers = Vec::new();
        collect_keybind_subscribers(tree, &mut subscribers);
        subscribers
    }

    pub(super) fn dispatch_surface_shortcut(
        &mut self,
        tree: &WidgetNode,
        key: &str,
        modifiers: KeyModifiers,
        keyboard_settings: &mesh_core_config::KeyboardSettings,
    ) -> Result<Option<Vec<CoreRequest>>, ComponentError> {
        let matched = self
            .resolved_surface_shortcuts(keyboard_settings)
            .into_iter()
            .find(|shortcut| {
                Self::key_matches_binding(key, &shortcut.key)
                    && shortcut_modifiers_match(&shortcut.modifiers, modifiers)
            });
        let Some(shortcut) = matched else {
            return Ok(None);
        };

        let subscribers = self
            .keybind_subscribers(tree)
            .into_iter()
            .filter(|subscriber| subscriber.keybind_id == shortcut.keybind_id)
            .collect::<Vec<_>>();
        if subscribers.is_empty() {
            self.record_keybind_diagnostic(
                &shortcut.keybind_id,
                "resolved keybind has no runtime subscribers on focused surface",
            );
            return Ok(None);
        }

        let mut requests = Vec::new();
        for subscriber in subscribers {
            let mut event =
                self.build_keyboard_event(tree, &subscriber.node_key, "keybind", key, modifiers);
            if let Some(object) = event.as_object_mut() {
                object.insert(
                    "keybind".into(),
                    serde_json::json!({
                        "id": shortcut.keybind_id.clone(),
                        "trigger_kind": match shortcut.trigger_kind {
                            mesh_core_module::KeybindTriggerKind::Shortcut => "shortcut",
                            mesh_core_module::KeybindTriggerKind::AccessKey => "access_key",
                        },
                        "source": match shortcut.source.clone() {
                            KeybindResolutionSource::UserOverride => "user_override".to_string(),
                            KeybindResolutionSource::LocaleDefault { locale } => {
                                format!("locale:{locale}")
                            }
                            KeybindResolutionSource::ModuleDefault => "module_default".to_string(),
                        },
                    }),
                );
            }
            requests.extend(self.call_namespaced_handler(&subscriber.handler, &[event])?);
        }
        Ok(Some(requests))
    }

    pub(in crate::shell::component) fn annotate_surface_shortcuts(&self, tree: &mut WidgetNode) {
        let keyboard_settings = self.current_keyboard_settings();
        for shortcut in self.resolved_surface_shortcuts(&keyboard_settings) {
            let accessibility_shortcut = format_shortcut_for_accessibility(&shortcut);
            for node in find_nodes_by_keybind_mut(tree, &shortcut.keybind_id) {
                match node.accessibility.keyboard_shortcut.as_deref() {
                    Some(existing) if existing == accessibility_shortcut => {}
                    Some(existing) if existing == shortcut.keybind_id => {
                        node.accessibility.keyboard_shortcut = Some(accessibility_shortcut.clone());
                    }
                    Some(existing) => {
                        node.accessibility.keyboard_shortcut =
                            Some(format!("{existing}, {accessibility_shortcut}"));
                    }
                    None => {
                        node.accessibility.keyboard_shortcut = Some(accessibility_shortcut.clone());
                    }
                }
            }
        }
    }

    pub(in crate::shell::component) fn debug_surface_keybinds(
        &self,
    ) -> Vec<mesh_core_debug::DebugKeybindEntry> {
        let keyboard_settings = self.current_keyboard_settings();
        self.resolved_surface_shortcuts(&keyboard_settings)
            .into_iter()
            .map(|shortcut| {
                let action = self
                    .compiled
                    .manifest
                    .keybinds
                    .actions
                    .get(&shortcut.keybind_id);
                let label = action.and_then(|action| {
                    action.label.as_ref().map(|text| {
                        self.resolve_manifest_text(
                            &self.compiled.manifest.package.id,
                            &format!("mesh.keybinds.{}.label", shortcut.keybind_id),
                            text,
                        )
                    })
                });
                let description = action.and_then(|action| {
                    action.description.as_ref().map(|text| {
                        self.resolve_manifest_text(
                            &self.compiled.manifest.package.id,
                            &format!("mesh.keybinds.{}.description", shortcut.keybind_id),
                            text,
                        )
                    })
                });
                let category = action.and_then(|action| {
                    action.category.as_ref().map(|text| {
                        self.resolve_manifest_text(
                            &self.compiled.manifest.package.id,
                            &format!("mesh.keybinds.{}.category", shortcut.keybind_id),
                            text,
                        )
                    })
                });
                mesh_core_debug::DebugKeybindEntry {
                    surface_id: self.surface_id().to_string(),
                    module_id: self.compiled.manifest.package.id.clone(),
                    action_id: shortcut.keybind_id.clone(),
                    label: label.as_ref().map(|text| text.text.clone()),
                    description: description.as_ref().map(|text| text.text.clone()),
                    category: category.as_ref().map(|text| text.text.clone()),
                    label_key: label.and_then(|text| text.key),
                    description_key: description.and_then(|text| text.key),
                    category_key: category.and_then(|text| text.key),
                    key: shortcut.key.clone(),
                    modifiers: shortcut.modifiers.clone(),
                    trigger_kind: match shortcut.trigger_kind {
                        mesh_core_module::KeybindTriggerKind::Shortcut => "shortcut".to_string(),
                        mesh_core_module::KeybindTriggerKind::AccessKey => "access_key".to_string(),
                    },
                    source: match shortcut.source.clone() {
                        KeybindResolutionSource::UserOverride => "user_override".to_string(),
                        KeybindResolutionSource::LocaleDefault { locale } => {
                            format!("locale:{locale}")
                        }
                        KeybindResolutionSource::ModuleDefault => "module_default".to_string(),
                    },
                    accessibility_shortcut: format_shortcut_for_accessibility(&shortcut),
                }
            })
            .collect()
    }

    fn resolve_surface_shortcut_declaration(
        &self,
        declaration: SurfaceShortcutDeclaration,
        override_key: Option<String>,
        active_locale: &str,
    ) -> Option<ResolvedSurfaceShortcut> {
        if let Some(key) = override_key {
            let kind = declaration.generic_trigger.kind;
            let modifiers = declaration.generic_trigger.modifiers.clone();
            if let Some(reason) = unsafe_override_reason(&key, &modifiers) {
                self.record_keybind_diagnostic(&declaration.keybind_id, reason);
                return resolve_surface_shortcut_declaration_without_override(
                    declaration,
                    active_locale,
                    self,
                );
            }
            return resolved_surface_shortcut(
                declaration,
                key,
                modifiers,
                kind,
                KeybindResolutionSource::UserOverride,
                self,
            );
        }

        resolve_surface_shortcut_declaration_without_override(declaration, active_locale, self)
    }

    fn record_duplicate_surface_shortcut_diagnostics(&self, shortcuts: &[ResolvedSurfaceShortcut]) {
        let mut seen = HashMap::<(String, Vec<String>), String>::new();
        for shortcut in shortcuts {
            let key = (
                normalize_key_name(shortcut.key.trim()),
                normalized_modifiers(&shortcut.modifiers),
            );
            if let Some(first_action_id) = seen.get(&key) {
                self.record_keybind_diagnostic(
                    &shortcut.keybind_id,
                    &format!(
                        "duplicate effective binding with action '{first_action_id}' for {}",
                        format_binding(&shortcut.key, &shortcut.modifiers)
                    ),
                );
            } else {
                seen.insert(key, shortcut.keybind_id.clone());
            }
        }
    }
}

fn surface_shortcut_declarations_from_settings(
    settings_json: &serde_json::Value,
) -> Vec<SurfaceShortcutDeclaration> {
    let Some(shortcuts) = settings_json
        .get("keyboard")
        .and_then(|value| value.get("shortcuts"))
        .and_then(serde_json::Value::as_object)
    else {
        return Vec::new();
    };

    shortcuts
        .iter()
        .filter_map(|(shortcut_id, value)| {
            let default_key = value.get("key")?.as_str()?.to_string();
            if default_key.trim().is_empty() {
                return None;
            }

            Some(SurfaceShortcutDeclaration {
                keybind_id: shortcut_id.clone(),
                generic_trigger: mesh_core_module::KeybindTrigger {
                    kind: mesh_core_module::KeybindTriggerKind::Shortcut,
                    key: Some(default_key),
                    modifiers: Vec::new(),
                },
                localized_triggers: HashMap::new(),
            })
        })
        .collect()
}

fn resolve_surface_shortcut_declaration_without_override(
    declaration: SurfaceShortcutDeclaration,
    active_locale: &str,
    component: &FrontendSurfaceComponent,
) -> Option<ResolvedSurfaceShortcut> {
    if declaration.generic_trigger.kind == mesh_core_module::KeybindTriggerKind::AccessKey {
        for locale in keybind_locale_candidates(active_locale) {
            let Some(trigger) = declaration.localized_triggers.get(&locale).cloned() else {
                continue;
            };
            let Some(key) = trigger.key.clone() else {
                component.record_keybind_diagnostic(
                    &declaration.keybind_id,
                    &format!("localized trigger '{locale}' has no key"),
                );
                continue;
            };
            if key.trim().is_empty() {
                component.record_keybind_diagnostic(
                    &declaration.keybind_id,
                    &format!("localized trigger '{locale}' has empty key"),
                );
                continue;
            }
            return resolved_surface_shortcut(
                declaration,
                key,
                trigger.modifiers,
                trigger.kind,
                KeybindResolutionSource::LocaleDefault { locale },
                component,
            );
        }
    }

    let kind = declaration.generic_trigger.kind;
    let Some(key) = declaration.generic_trigger.key.clone() else {
        component.record_keybind_diagnostic(&declaration.keybind_id, "trigger has no key");
        return None;
    };
    let modifiers = declaration.generic_trigger.modifiers.clone();
    resolved_surface_shortcut(
        declaration,
        key,
        modifiers,
        kind,
        KeybindResolutionSource::ModuleDefault,
        component,
    )
}

fn resolved_surface_shortcut(
    declaration: SurfaceShortcutDeclaration,
    key: String,
    modifiers: Vec<String>,
    trigger_kind: mesh_core_module::KeybindTriggerKind,
    source: KeybindResolutionSource,
    component: &FrontendSurfaceComponent,
) -> Option<ResolvedSurfaceShortcut> {
    if key.trim().is_empty() {
        component.record_keybind_diagnostic(&declaration.keybind_id, "trigger has empty key");
        return None;
    }

    if let Some(modifier) = unsupported_modifier(&modifiers) {
        component.record_keybind_diagnostic(
            &declaration.keybind_id,
            &format!("trigger contains unsupported modifier '{modifier}'"),
        );
        return None;
    }

    Some(ResolvedSurfaceShortcut {
        keybind_id: declaration.keybind_id,
        key,
        modifiers,
        trigger_kind,
        source,
    })
}

fn unsupported_modifier(modifiers: &[String]) -> Option<String> {
    modifiers.iter().find_map(|modifier| {
        let trimmed = modifier.trim();
        match normalize_key_name(trimmed).as_str() {
            "ctrl" | "control" | "shift" | "alt" | "option" => None,
            _ => Some(trimmed.to_string()),
        }
    })
}

fn normalized_modifiers(modifiers: &[String]) -> Vec<String> {
    let mut normalized = modifiers
        .iter()
        .filter_map(
            |modifier| match normalize_key_name(modifier.trim()).as_str() {
                "ctrl" | "control" => Some("ctrl".to_string()),
                "shift" => Some("shift".to_string()),
                "alt" | "option" => Some("alt".to_string()),
                _ => None,
            },
        )
        .collect::<Vec<_>>();
    normalized.sort();
    normalized.dedup();
    normalized
}

fn format_binding(key: &str, modifiers: &[String]) -> String {
    let mut parts = normalized_modifiers(modifiers);
    parts.push(normalize_key_name(key.trim()));
    parts.join("+")
}

fn unsafe_override_reason(key: &str, modifiers: &[String]) -> Option<&'static str> {
    let key = normalize_key_name(key.trim());
    let modifiers = normalized_modifiers(modifiers);
    let has_ctrl = modifiers.iter().any(|modifier| modifier == "ctrl");
    let has_alt = modifiers.iter().any(|modifier| modifier == "alt");
    let shell_owned_without_modifier = !has_ctrl && !has_alt;

    if shell_owned_without_modifier && matches!(key.as_str(), "tab" | "escape" | "enter" | "space")
    {
        return Some("user override uses a shell-owned traversal, cancel, or activation key");
    }
    if has_ctrl && key == "c" {
        return Some("user override uses reserved selection-copy shortcut");
    }

    None
}

fn shortcut_modifiers_match(required: &[String], active: KeyModifiers) -> bool {
    let mut required_ctrl = false;
    let mut required_shift = false;
    let mut required_alt = false;

    for modifier in required {
        match normalize_key_name(modifier.trim()).as_str() {
            "ctrl" | "control" => required_ctrl = true,
            "shift" => required_shift = true,
            "alt" | "option" => required_alt = true,
            _ => return false,
        }
    }

    active.ctrl == required_ctrl && active.shift == required_shift && active.alt == required_alt
}

fn format_shortcut_for_accessibility(shortcut: &ResolvedSurfaceShortcut) -> String {
    let mut parts = Vec::new();
    for modifier in &shortcut.modifiers {
        match normalize_key_name(modifier.trim()).as_str() {
            "ctrl" | "control" => parts.push("Control".to_string()),
            "shift" => parts.push("Shift".to_string()),
            "alt" | "option" => parts.push("Alt".to_string()),
            _ => {}
        }
    }
    parts.push(shortcut.key.clone());
    parts.join("+")
}

fn keybind_locale_candidates(locale: &str) -> Vec<String> {
    let locale = locale.trim().replace('_', "-");
    if locale.is_empty() {
        return Vec::new();
    }

    let mut candidates = vec![locale.clone()];
    if let Some((parent, _)) = locale.split_once('-')
        && !parent.is_empty()
        && parent != locale
    {
        candidates.push(parent.to_string());
    }
    candidates
}

fn normalize_key_name(value: &str) -> String {
    match value {
        " " => "space".into(),
        other => other.to_ascii_lowercase(),
    }
}

fn collect_keybind_subscribers(node: &WidgetNode, subscribers: &mut Vec<KeybindSubscriber>) {
    if let (Some(keybind_id), Some(node_key), Some(handler)) = (
        node.attributes.get("keybind"),
        node.attributes.get("_mesh_key"),
        node.event_handlers.get("keybind"),
    ) {
        subscribers.push(KeybindSubscriber {
            keybind_id: keybind_id.clone(),
            node_key: node_key.clone(),
            handler: handler.clone(),
        });
    }

    for child in &node.children {
        collect_keybind_subscribers(child, subscribers);
    }
}

fn find_nodes_by_keybind_mut<'a>(
    node: &'a mut WidgetNode,
    keybind_id: &str,
) -> Vec<&'a mut WidgetNode> {
    let mut found = Vec::new();
    collect_nodes_by_keybind_mut(node, keybind_id, &mut found);
    found
}

fn collect_nodes_by_keybind_mut<'a>(
    node: &'a mut WidgetNode,
    keybind_id: &str,
    found: &mut Vec<&'a mut WidgetNode>,
) {
    if node
        .attributes
        .get("keybind")
        .is_some_and(|value| value == keybind_id)
    {
        found.push(node);
        return;
    }

    for child in &mut node.children {
        collect_nodes_by_keybind_mut(child, keybind_id, found);
    }
}
