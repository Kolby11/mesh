use super::super::*;
use super::is_bare_printable_key;
use mesh_core_elements::Prop;

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
    /// Assigned from the user's `shell.keyboard.key_priority`.
    Auto,
}

#[derive(Debug, Clone)]
struct SurfaceShortcutDeclaration {
    keybind_id: String,
    generic_trigger: mesh_core_module::KeybindTrigger,
    localized_triggers: HashMap<String, mesh_core_module::KeybindTrigger>,
}

/// A keyboard target resolved in one tree traversal. Keyboard event payloads
/// and dispatch both need the node's transformed bounds, so keeping these
/// together prevents each stage from searching the tree again.
struct ResolvedInputTarget<'a> {
    node: &'a WidgetNode,
    bounds: (f32, f32, f32, f32),
}

#[derive(Debug, Clone)]
pub(in crate::shell::component) struct KeybindSubscriber {
    pub(in crate::shell::component) keybind_id: String,
    pub(in crate::shell::component) node_key: String,
    pub(in crate::shell::component) handler: HandlerTarget,
}

impl FrontendSurfaceComponent {
    fn resolve_keyboard_target<'a>(
        tree: &'a WidgetNode,
        node_key: &str,
    ) -> Option<ResolvedInputTarget<'a>> {
        let (node, bounds) = find_node_with_bounds_by_key(tree, node_key).or_else(|| {
            mesh_core_interaction::find_focus_node_with_bounds_by_key(tree, node_key)
        })?;
        Some(ResolvedInputTarget { node, bounds })
    }

    pub(super) fn handle_key_pressed(
        &mut self,
        tree: &WidgetNode,
        key: String,
        modifiers: KeyModifiers,
    ) -> Result<Vec<CoreRequest>, ComponentError> {
        let keyboard_settings = self.current_keyboard_settings();
        if self.key_hint_press(&keyboard_settings, &key, modifiers) {
            return Ok(Vec::new());
        }
        let mut requests = self.handle_key_pressed_inner(tree, key, modifiers)?;
        requests.extend(self.take_assigned_write());
        Ok(requests)
    }

    fn handle_key_pressed_inner(
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
            if self.embedded_popover_return_focus.is_some() {
                self.pending_embedded_popover_focus_restore = true;
            }
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
        // A key aimed at the focused element makes its focus visible. A lone
        // modifier is not: it is usually half of a compositor shortcut, and
        // lighting the ring on a button the pointer just clicked is noise.
        if !is_modifier_key(&key) {
            let mut transaction = self.interaction_state.begin();
            transaction.focus(self.focused_id, true);
            self.commit_interaction_delta(transaction);
            self.focus_visible_key = self.focused_key.clone();
            self.focus_visible_id = self.focused_id;
        }
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
                let input_id = find_node_by_key(tree, &focused_key)
                    .expect("is_input_key verified the focused input exists")
                    .id;
                let editable = is_editable_input_key(tree, &focused_key);
                match key.as_str() {
                    "Backspace" if editable => {
                        let current = self.delete_input_text(
                            input_id,
                            self.input_previous_scalar_bytes(input_id),
                            0,
                        );
                        self.clear_selection();
                        self.invalidate_text_state();
                        requests.extend(self.dispatch_text_input_value_handlers(
                            tree,
                            &focused_key,
                            &current,
                        )?);
                        return Ok(requests);
                    }
                    "Delete" if editable => {
                        let current = self.delete_input_text(
                            input_id,
                            0,
                            self.input_next_scalar_bytes(input_id),
                        );
                        self.clear_selection();
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

            if let Some(focused_node) = find_node_by_key(tree, &focused_key)
                && matches!(focused_node.tag.as_str(), "button" | "shape")
                && Self::key_matches_any_binding(&key, &keyboard_settings.button_activation_keys)
            {
                self.clear_selection();
                self.keyboard_button_press_activations
                    .insert((focused_node.id, key.clone()));
                requests.extend(self.dispatch_keyboard_button_activation(
                    tree,
                    &focused_key,
                    &key,
                )?);
                return Ok(requests);
            }

            if let Some(next_key) = tab_roving_target(tree, &focused_key, &key) {
                self.clear_selection();
                requests.extend(self.set_focus_target(tree, Some(next_key.clone()), true)?);
                // Automatic activation: the focused tab is the shown one.
                let click_event = self.build_click_event(tree, &next_key, 0.0, 0.0);
                requests.extend(self.dispatch_activation_handlers(tree, &next_key, click_event)?);
                self.invalidate_interaction_restyle();
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
        // Release never changes focus visibility: the press already decided
        // it, and releasing the keys of a compositor shortcut after clicking
        // a button must not paint that button's focus ring.
        let keyboard_settings = self.current_keyboard_settings();
        self.key_hint_release(&keyboard_settings, &key);
        let normalized_focused_key = self.normalized_focused_key(tree);
        if let Some(focused_key) = normalized_focused_key {
            let mut requests = self.dispatch_focused_keyboard_handler(
                tree,
                &focused_key,
                "keyup",
                "keyup",
                &key,
                modifiers,
            )?;

            if let Some(focused_node) = find_node_by_key(tree, &focused_key)
                && matches!(focused_node.tag.as_str(), "button" | "shape")
                && Self::key_matches_any_binding(&key, &keyboard_settings.button_activation_keys)
            {
                self.clear_selection();
                if self
                    .keyboard_button_press_activations
                    .remove(&(focused_node.id, key.clone()))
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
        let target = Self::resolve_keyboard_target(tree, node_key);
        self.build_keyboard_event_for(tree, node_key, target, event_type, key, modifiers)
    }

    fn build_keyboard_event_for(
        &self,
        tree: &WidgetNode,
        node_key: &str,
        target: Option<ResolvedInputTarget<'_>>,
        event_type: &str,
        key: &str,
        modifiers: KeyModifiers,
    ) -> serde_json::Value {
        let (left, top, right, bottom) = target
            .as_ref()
            .map(|target| target.bounds)
            .unwrap_or((0.0, 0.0, 0.0, 0.0));
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
        let tag = target
            .as_ref()
            .map(|target| target.node.tag.clone())
            .unwrap_or_default();
        let mut current_target = target
            .map(|target| {
                element_snapshot_json(
                    target.node,
                    left - target.node.layout.x,
                    top - target.node.layout.y,
                )
            })
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
        let target = Self::resolve_keyboard_target(tree, node_key);
        let event = self.build_keyboard_event_for(
            tree,
            node_key,
            target.as_ref().map(|target| ResolvedInputTarget {
                node: target.node,
                bounds: target.bounds,
            }),
            event_type,
            key,
            modifiers,
        );
        let Some(target) = target else {
            return Ok(Vec::new());
        };
        self.call_resolved_node_handler(target.node, handler_name, &[event])
    }

    pub(super) fn dispatch_keyboard_button_activation(
        &mut self,
        tree: &WidgetNode,
        node_key: &str,
        key: &str,
    ) -> Result<Vec<CoreRequest>, ComponentError> {
        let Some(target) = Self::resolve_keyboard_target(tree, node_key) else {
            return Ok(Vec::new());
        };
        if !target.node.event_handlers.contains_key("click")
            && !target.node.event_handler_calls.contains_key("click")
        {
            return Ok(Vec::new());
        }
        let (left, top, right, bottom) = target.bounds;
        let center_x = (left + right) * 0.5;
        let center_y = (top + bottom) * 0.5;
        let mut event = self.build_click_event_for(
            tree,
            node_key,
            Some(target.node),
            target.bounds,
            center_x,
            center_y,
        );
        if let Some(object) = event.as_object_mut() {
            object.insert(
                "trigger".into(),
                serde_json::json!({
                    "type": "keyboard",
                    "key": key,
                }),
            );
        }
        if target
            .node
            .attributes
            .get(Prop::AriaHaspopup)
            .is_some_and(|value| value == "true" || value == "menu")
        {
            self.pending_embedded_popover_focus = Some(true);
        }
        self.call_resolved_node_handler(target.node, "click", &[event])
    }

    /// Activation keys and shortcut overrides for the current key event.
    ///
    /// Read straight off the shared settings store, which the shell's settings
    /// watcher already keeps current — the input path costs no syscalls and
    /// cannot go stale.
    pub(in crate::shell::component) fn current_keyboard_settings(
        &self,
    ) -> mesh_core_config::KeyboardSettings {
        self.settings.shell().keyboard.clone()
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
        let active_locale = self.locale.current().to_string();
        if let Some(cached) = self.resolved_surface_shortcuts_cache.borrow().as_ref()
            && cached.locale == active_locale
            && cached.keyboard_settings == *keyboard_settings
        {
            return cached.shortcuts.clone();
        }

        let declarations = self.surface_shortcut_declarations();
        let overrides = keyboard_settings.surface_shortcuts.get(self.surface_id());
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

        let mut resolved = Vec::new();
        let mut auto_actions = Vec::new();
        for declaration in declarations {
            let override_key = overrides
                .and_then(|surface| surface.get(&declaration.keybind_id))
                .and_then(|shortcut| shortcut.key.clone());
            if declaration.generic_trigger.kind == mesh_core_module::KeybindTriggerKind::Auto
                && override_key.is_none()
            {
                auto_actions.push(declaration.keybind_id);
                continue;
            }
            resolved.extend(self.resolve_surface_shortcut_declaration(
                &declaration,
                override_key,
                &active_locale,
            ));
        }
        self.assign_auto_shortcuts(&mut resolved, &auto_actions, keyboard_settings);

        self.record_duplicate_surface_shortcut_diagnostics(&resolved);
        let mut shortcuts_by_keybind = HashMap::with_capacity(resolved.len());
        for shortcut in &resolved {
            shortcuts_by_keybind
                .entry(shortcut.keybind_id.clone())
                .or_insert_with(Vec::new)
                .push(format_shortcut_for_accessibility(shortcut));
        }
        static GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        *self.resolved_surface_shortcuts_cache.borrow_mut() = Some(ResolvedSurfaceShortcutsCache {
            generation: GENERATION.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            keyboard_settings: keyboard_settings.clone(),
            locale: active_locale,
            shortcuts: resolved.clone(),
            shortcuts_by_keybind,
        });
        resolved
    }

    /// Gives each auto action the first free key of the user's priority list
    /// and queues the result for saving when it differs from the saved
    /// assignment.
    fn assign_auto_shortcuts(
        &self,
        resolved: &mut Vec<ResolvedSurfaceShortcut>,
        auto_actions: &[String],
        keyboard_settings: &mesh_core_config::KeyboardSettings,
    ) {
        use mesh_core_config::keybinds::{assign_auto_keys, normalize_key, parse_key_priority};
        if auto_actions.is_empty() {
            return;
        }
        let taken = resolved
            .iter()
            .filter(|shortcut| shortcut.modifiers.is_empty())
            .map(|shortcut| normalize_key(&shortcut.key))
            .collect::<HashSet<_>>();
        let cached = keyboard_settings
            .assigned
            .get(self.surface_id())
            .cloned()
            .unwrap_or_default();
        let assignment = assign_auto_keys(
            auto_actions,
            &taken,
            &parse_key_priority(&keyboard_settings.key_priority),
            &cached,
        );
        for action in auto_actions {
            match assignment.keys.get(action) {
                Some(key) => resolved.push(ResolvedSurfaceShortcut {
                    keybind_id: action.clone(),
                    key: key.clone(),
                    modifiers: Vec::new(),
                    trigger_kind: mesh_core_module::KeybindTriggerKind::Auto,
                    source: KeybindResolutionSource::Auto,
                }),
                None => {
                    self.record_keybind_diagnostic(
                        action,
                        "no free key left in shell.keyboard.key_priority for this auto action",
                    );
                }
            }
        }
        if assignment.keys != cached {
            *self.pending_assigned_write.borrow_mut() = Some(assignment.keys);
        }
    }

    /// The saved-assignment write the last resolution queued, once per distinct
    /// value.
    pub(in crate::shell::component) fn take_assigned_write(&self) -> Option<CoreRequest> {
        let keys = self.pending_assigned_write.borrow_mut().take()?;
        if self.last_assigned_write.borrow().as_ref() == Some(&keys) {
            return None;
        }
        *self.last_assigned_write.borrow_mut() = Some(keys.clone());
        Some(CoreRequest::SetKeyboardSetting {
            field: "assigned".into(),
            value: serde_json::json!({ self.surface_id(): keys }),
        })
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
                    "legacy settings shortcut is ignored because mesh.contributes.keybinds declares this action",
                );
                continue;
            }
            self.record_keybind_diagnostic(
                &legacy.keybind_id,
                "legacy settings shortcut declarations are migration-only; declare this action in mesh.contributes.keybinds",
            );
        }
        declarations.sort_by(|left, right| left.keybind_id.cmp(&right.keybind_id));
        declarations
    }

    /// The module declaring `keybind_id` for this surface and its action,
    /// in the precedence of [`Self::manifest_surface_shortcut_declarations`].
    fn keybind_declaration(
        &self,
        keybind_id: &str,
    ) -> Option<(String, mesh_core_module::KeybindAction)> {
        let host = &self.compiled.manifest;
        let contributions = self
            .frontend_catalog
            .contribution_entries_for_host(&host.package.id);
        std::iter::once(host)
            .chain(contributions.iter().map(|compiled| &compiled.manifest))
            .find_map(|manifest| {
                manifest
                    .keybinds
                    .actions
                    .get(keybind_id)
                    .map(|action| (manifest.package.id.clone(), action.clone()))
            })
    }

    /// Keybinds the surface module declares, then those of the contribution
    /// modules it hosts: a navigation item declares its own shortcut. The
    /// host's declaration wins an id both declare.
    fn manifest_surface_shortcut_declarations(&self) -> Vec<SurfaceShortcutDeclaration> {
        let host = &self.compiled.manifest;
        let contributions = self
            .frontend_catalog
            .contribution_entries_for_host(&host.package.id);
        let mut seen = HashSet::new();
        std::iter::once(host)
            .chain(contributions.iter().map(|compiled| &compiled.manifest))
            .flat_map(|manifest| manifest.keybinds.actions.iter())
            .filter(|(keybind_id, _)| seen.insert(keybind_id.as_str()))
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

    pub(in crate::shell::component) fn has_surface_shortcut_declarations(&self) -> bool {
        !self.manifest_surface_shortcut_declarations().is_empty()
            || !self
                .legacy_settings_surface_shortcut_declarations()
                .is_empty()
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
        let focused_key = self.normalized_focused_key(tree);
        let mut candidates = self
            .resolved_surface_shortcuts(keyboard_settings)
            .into_iter()
            .filter(|shortcut| {
                Self::key_matches_binding(key, &shortcut.key)
                    && shortcut_modifiers_match(&shortcut.modifiers, modifiers)
            })
            .collect::<Vec<_>>();
        // The narrowest scope answers first: a binding limited to the focused
        // component beats a surface-wide one on the same key.
        candidates.sort_by_key(|shortcut| !self.keybind_is_focus_scoped(&shortcut.keybind_id));
        if candidates.is_empty() {
            return Ok(None);
        }

        let mut chosen = None;
        for shortcut in candidates {
            let focus_scoped = self.keybind_is_focus_scoped(&shortcut.keybind_id);
            let subscribers = self
                .keybind_subscribers(tree)
                .into_iter()
                .filter(|subscriber| subscriber.keybind_id == shortcut.keybind_id)
                .filter(|subscriber| {
                    mesh_core_interaction::active_modal_scope(tree).is_none_or(|scope| {
                        mesh_core_interaction::scope_contains(scope, &subscriber.node_key)
                    })
                })
                .filter(|subscriber| {
                    !focus_scoped
                        || focused_key
                            .as_deref()
                            .is_some_and(|focused| focus_is_within(focused, &subscriber.node_key))
                })
                .collect::<Vec<_>>();
            if !subscribers.is_empty() {
                chosen = Some((shortcut, subscribers));
                break;
            }
            if !focus_scoped {
                self.record_keybind_diagnostic(
                    &shortcut.keybind_id,
                    "resolved keybind has no runtime subscribers on focused surface",
                );
            }
        }
        let Some((shortcut, subscribers)) = chosen else {
            return Ok(None);
        };

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
                            mesh_core_module::KeybindTriggerKind::Auto => "auto",
                        },
                        "source": match shortcut.source.clone() {
                            KeybindResolutionSource::UserOverride => "user_override".to_string(),
                            KeybindResolutionSource::LocaleDefault { locale } => {
                                format!("locale:{locale}")
                            }
                            KeybindResolutionSource::ModuleDefault => "module_default".to_string(),
                        KeybindResolutionSource::Auto => "auto".to_string(),
                        },
                    }),
                );
            }
            requests.extend(self.call_handler_target(&subscriber.handler, &[event])?);
        }
        Ok(Some(requests))
    }

    #[cfg(test)]
    pub(in crate::shell::component) fn annotate_surface_shortcuts(&self, tree: &mut WidgetNode) {
        if !self.has_surface_shortcut_declarations() {
            return;
        }
        let keyboard_settings = self.current_keyboard_settings();
        // Resolve once to validate the cache key and populate the derived
        // accessibility index on a miss. Subsequent finalize passes borrow
        // the preformatted index instead of rebuilding it.
        self.resolved_surface_shortcuts(&keyboard_settings);
        let cache = self.resolved_surface_shortcuts_cache.borrow();
        if let Some(cached) = cache.as_ref() {
            annotate_nodes_by_keybind(tree, &cached.shortcuts_by_keybind);
        }
    }

    pub(in crate::shell::component) fn debug_surface_keybinds(
        &self,
    ) -> Vec<mesh_core_debug::DebugKeybindEntry> {
        let keyboard_settings = self.current_keyboard_settings();
        self.resolved_surface_shortcuts(&keyboard_settings)
            .into_iter()
            .map(|shortcut| {
                let (declaring_module, action) = self
                    .keybind_declaration(&shortcut.keybind_id)
                    .map_or((None, None), |(module, action)| (Some(module), Some(action)));
                let module_id = declaring_module
                    .unwrap_or_else(|| self.compiled.manifest.package.id.clone());
                let focus_scoped = self.keybind_is_focus_scoped(&shortcut.keybind_id);
                let focus_active = self.last_tree.as_ref().is_some_and(|tree| {
                    self.keybind_subscribers(tree).iter().any(|subscriber| {
                        subscriber.keybind_id == shortcut.keybind_id
                            && self
                                .focused_key
                                .as_deref()
                                .is_some_and(|key| focus_is_within(key, &subscriber.node_key))
                    })
                });
                let action = action.as_ref();
                let label = action.and_then(|action| {
                    action.label.as_ref().map(|text| {
                        self.resolve_manifest_text(
                            &module_id,
                            &format!("mesh.contributes.keybinds.{}.label", shortcut.keybind_id),
                            text,
                        )
                    })
                });
                let description = action.and_then(|action| {
                    action.description.as_ref().map(|text| {
                        self.resolve_manifest_text(
                            &module_id,
                            &format!(
                                "mesh.contributes.keybinds.{}.description",
                                shortcut.keybind_id
                            ),
                            text,
                        )
                    })
                });
                let category = action.and_then(|action| {
                    action.category.as_ref().map(|text| {
                        self.resolve_manifest_text(
                            &module_id,
                            &format!("mesh.contributes.keybinds.{}.category", shortcut.keybind_id),
                            text,
                        )
                    })
                });
                mesh_core_debug::DebugKeybindEntry {
                    surface_id: self.surface_id().to_string(),
                    module_id: module_id.clone(),
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
                        mesh_core_module::KeybindTriggerKind::Auto => "auto".to_string(),
                    },
                    source: match shortcut.source.clone() {
                        KeybindResolutionSource::UserOverride => "user_override".to_string(),
                        KeybindResolutionSource::LocaleDefault { locale } => {
                            format!("locale:{locale}")
                        }
                        KeybindResolutionSource::ModuleDefault => "module_default".to_string(),
                        KeybindResolutionSource::Auto => "auto".to_string(),
                    },
                    accessibility_shortcut: format_shortcut_for_accessibility(&shortcut),
                    scope: if focus_scoped { "focus" } else { "surface" }.to_string(),
                    active: !focus_scoped || focus_active,
                    focused: false,
                }
            })
            .collect()
    }

    fn resolve_surface_shortcut_declaration(
        &self,
        declaration: &SurfaceShortcutDeclaration,
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

    /// Handles the key-hint chord. Returns whether the press was consumed.
    /// `Escape` also ends sticky hints.
    fn key_hint_press(
        &mut self,
        keyboard_settings: &mesh_core_config::KeyboardSettings,
        key: &str,
        modifiers: KeyModifiers,
    ) -> bool {
        use mesh_core_config::keybinds::{normalize_key, shell_action_for_key};
        if self.key_hints.sticky && normalize_key(key) == "escape" {
            self.key_hints.sticky = false;
            self.mark_key_hints_changed();
            return true;
        }
        let is_hint_chord = shell_action_for_key(
            &keyboard_settings.shell_shortcuts,
            key,
            modifiers.ctrl,
            modifiers.shift,
            modifiers.alt,
        )
        .is_some_and(|action| action.id == "hints");
        if !is_hint_chord {
            return false;
        }
        // Key repeat of a held chord arrives as further presses.
        if self.key_hints.held || self.key_hints.down_at.is_some() {
            return true;
        }
        match keyboard_settings.hint_mode.as_str() {
            "toggle" => self.key_hints.sticky = !self.key_hints.sticky,
            "hold" => {
                self.key_hints.held = true;
                self.key_hints.down_at = Some(std::time::Instant::now());
            }
            _ if self.key_hints.sticky => {
                self.key_hints.sticky = false;
                self.key_hints.down_at = Some(std::time::Instant::now());
            }
            _ => {
                self.key_hints.held = true;
                self.key_hints.down_at = Some(std::time::Instant::now());
            }
        }
        self.mark_key_hints_changed();
        true
    }

    /// Ends a held chord when either of its keys is released; a quick tap in
    /// `both` mode leaves the hints on.
    fn key_hint_release(
        &mut self,
        keyboard_settings: &mesh_core_config::KeyboardSettings,
        key: &str,
    ) {
        use mesh_core_config::keybinds::{KeyChord, SHELL_ACTIONS, normalize_key, shell_action_chord};
        let Some(down_at) = self.key_hints.down_at else {
            return;
        };
        let Some(action) = SHELL_ACTIONS.iter().find(|action| action.id == "hints") else {
            return;
        };
        let chord: KeyChord = shell_action_chord(action, &keyboard_settings.shell_shortcuts);
        let key = normalize_key(key);
        let part_of_chord = key == chord.key
            || (chord.ctrl && (key.contains("control") || key == "ctrl"))
            || (chord.shift && key.contains("shift"))
            || (chord.alt && key.contains("alt"));
        if !part_of_chord {
            return;
        }
        self.key_hints.down_at = None;
        let was_held = std::mem::take(&mut self.key_hints.held);
        if was_held
            && keyboard_settings.hint_mode != "hold"
            && down_at.elapsed() < KEY_HINT_TAP_WINDOW
        {
            self.key_hints.sticky = true;
        }
        self.mark_key_hints_changed();
    }

    pub(in crate::shell::component) fn key_hints_visible(&self) -> bool {
        self.key_hints.sticky || self.key_hints.held
    }

    fn mark_key_hints_changed(&mut self) {
        self.key_hints.repaint = true;
        self.invalidate_interaction_restyle();
    }

    /// Resolved shortcut text per action id, for key hints.
    pub(in crate::shell::component) fn current_surface_shortcut_labels(
        &self,
    ) -> HashMap<String, Vec<String>> {
        self.resolved_surface_shortcuts(&self.current_keyboard_settings());
        self.resolved_surface_shortcuts_cache
            .borrow()
            .as_ref()
            .map(|cache| cache.shortcuts_by_keybind.clone())
            .unwrap_or_default()
    }

    /// A focus-scoped binding is only worth hinting while focus is inside the
    /// control that subscribes to it.
    pub(in crate::shell::component) fn key_hint_is_active(
        &self,
        keybind_id: &str,
        node_key: &str,
    ) -> bool {
        !self.keybind_is_focus_scoped(keybind_id)
            || self
                .focused_key
                .as_deref()
                .is_some_and(|focused| focus_is_within(focused, node_key))
    }

    fn keybind_is_focus_scoped(&self, keybind_id: &str) -> bool {
        self.keybind_declaration(keybind_id)
            .is_some_and(|(_, action)| action.scope == mesh_core_module::KeybindScope::Focus)
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

/// A press-and-release shorter than this counts as a tap that keeps key hints
/// on.
const KEY_HINT_TAP_WINDOW: std::time::Duration = std::time::Duration::from_millis(250);

/// Whether key hints are showing and whether the surface owes a full repaint
/// for the last change.
#[derive(Debug, Default)]
pub(in crate::shell::component) struct KeyHintState {
    pub(in crate::shell::component) sticky: bool,
    pub(in crate::shell::component) held: bool,
    pub(in crate::shell::component) down_at: Option<std::time::Instant>,
    pub(in crate::shell::component) repaint: bool,
}

/// Whether `focused` is `ancestor` itself or lies beneath it; node keys are
/// slash-separated tree paths.
fn focus_is_within(focused: &str, ancestor: &str) -> bool {
    focused == ancestor
        || focused
            .strip_prefix(ancestor)
            .is_some_and(|rest| rest.starts_with('/'))
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
    declaration: &SurfaceShortcutDeclaration,
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
                trigger.modifiers.clone(),
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
    declaration: &SurfaceShortcutDeclaration,
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
        keybind_id: declaration.keybind_id.clone(),
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
        node.attributes.get(Prop::Keybind),
        node.mesh_key(),
        node.event_handlers.get("keybind"),
    ) {
        subscribers.push(KeybindSubscriber {
            keybind_id: keybind_id.clone(),
            node_key: node_key.to_owned(),
            handler: handler.clone(),
        });
    }

    for child in &node.children {
        collect_keybind_subscribers(child, subscribers);
    }
}

#[cfg(test)]
fn annotate_nodes_by_keybind(
    node: &mut WidgetNode,
    shortcuts_by_keybind: &HashMap<String, Vec<String>>,
) {
    annotate_node_by_keybind(node, shortcuts_by_keybind);
    for child in &mut node.children {
        annotate_nodes_by_keybind(child, shortcuts_by_keybind);
    }
}

pub(in crate::shell::component) fn annotate_node_by_keybind(
    node: &mut WidgetNode,
    shortcuts_by_keybind: &HashMap<String, Vec<String>>,
) {
    let (mut replace_keybind_label, shortcuts) = match node.attributes.get(Prop::Keybind) {
        Some(keybind_id) => (
            node.accessibility.keyboard_shortcut.as_deref() == Some(keybind_id),
            shortcuts_by_keybind.get(keybind_id),
        ),
        None => (false, None),
    };
    if let Some(shortcuts) = shortcuts {
        for shortcut in shortcuts {
            apply_accessibility_shortcut(node, replace_keybind_label, shortcut);
            replace_keybind_label = false;
        }
    }
}

fn apply_accessibility_shortcut(
    node: &mut WidgetNode,
    replace_keybind_label: bool,
    accessibility_shortcut: &str,
) {
    match node.accessibility.keyboard_shortcut.as_deref() {
        Some(existing) if existing == accessibility_shortcut => {}
        Some(_) if replace_keybind_label => {
            node.accessibility.keyboard_shortcut = Some(accessibility_shortcut.to_string());
        }
        Some(existing) => {
            node.accessibility.keyboard_shortcut =
                Some(format!("{existing}, {accessibility_shortcut}"));
        }
        None => {
            node.accessibility.keyboard_shortcut = Some(accessibility_shortcut.to_string());
        }
    }
}

#[cfg(test)]
mod shortcut_annotation_tests {
    use super::*;

    fn keyed_node(keybind: Option<&str>, children: Vec<WidgetNode>) -> WidgetNode {
        let mut node = WidgetNode::new("button");
        if let Some(keybind) = keybind {
            node.attributes.insert(Prop::Keybind, keybind.into());
        }
        node.children = children.into();
        node
    }

    #[test]
    fn annotate_nodes_by_keybind_applies_shortcuts_in_one_walk() {
        let mut tree = keyed_node(
            None,
            vec![
                keyed_node(Some("mute"), Vec::new()),
                keyed_node(Some("launch"), Vec::new()),
            ],
        );
        let shortcuts = HashMap::from([
            ("mute".to_string(), vec!["Control+m".to_string()]),
            ("launch".to_string(), vec!["Super+space".to_string()]),
        ]);

        annotate_nodes_by_keybind(&mut tree, &shortcuts);

        assert_eq!(
            tree.children[0].accessibility.keyboard_shortcut.as_deref(),
            Some("Control+m")
        );
        assert_eq!(
            tree.children[1].accessibility.keyboard_shortcut.as_deref(),
            Some("Super+space")
        );
    }
}

/// XKB keysym names of keys that only modify other keys.
fn is_modifier_key(key: &str) -> bool {
    const PREFIXES: [&str; 8] = [
        "Shift",
        "Control",
        "Alt",
        "Super",
        "Meta",
        "Hyper",
        "ISO_Level",
        "Mode_switch",
    ];
    PREFIXES.iter().any(|prefix| key.starts_with(prefix))
        || matches!(key, "Caps_Lock" | "Num_Lock" | "Scroll_Lock")
}

/// The tab that an arrow, `Home` or `End` key moves to from the focused tab,
/// wrapping at the ends. Both arrow axes rove, so the group works whatever
/// way it is laid out; left and right swap in a right-to-left group.
fn tab_roving_target(tree: &WidgetNode, focused_key: &str, key: &str) -> Option<String> {
    if !matches!(
        key,
        "ArrowLeft" | "ArrowRight" | "ArrowUp" | "ArrowDown" | "Home" | "End"
    ) {
        return None;
    }
    let focused = find_node_by_key(tree, focused_key)?;
    if !node_is_source(focused, &["tab"]) {
        return None;
    }
    let (tabs, index) = mesh_core_interaction::tab_group_items(tree, focused_key)?;
    let rtl =
        focused.computed_style.text_direction == mesh_core_elements::style::TextDirection::Rtl;
    let backward = match key {
        "ArrowUp" => true,
        "ArrowLeft" => !rtl,
        "ArrowRight" => rtl,
        _ => false,
    };
    let next = match key {
        "Home" => 0,
        "End" => tabs.len() - 1,
        _ if backward => (index + tabs.len() - 1) % tabs.len(),
        _ => (index + 1) % tabs.len(),
    };
    (next != index).then(|| tabs[next].clone())
}
