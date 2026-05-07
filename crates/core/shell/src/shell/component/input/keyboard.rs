use super::super::*;

#[derive(Debug, Clone)]
pub(super) struct ResolvedSurfaceShortcut {
    pub(super) key: String,
    pub(super) handler: String,
    pub(super) target_ref: Option<String>,
}

impl FrontendSurfaceComponent {
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

    fn resolved_surface_shortcuts(
        &self,
        keyboard_settings: &mesh_core_config::KeyboardSettings,
    ) -> Vec<ResolvedSurfaceShortcut> {
        let Some(shortcuts) = self
            .settings_json
            .get("keyboard")
            .and_then(|value| value.get("shortcuts"))
            .and_then(serde_json::Value::as_object)
        else {
            return Vec::new();
        };

        let overrides = keyboard_settings.surface_shortcuts.get(self.surface_id());

        shortcuts
            .iter()
            .filter_map(|(shortcut_id, value)| {
                let handler = value.get("handler")?.as_str()?.to_string();
                let default_key = value.get("key")?.as_str()?.to_string();
                let override_key = overrides
                    .and_then(|surface| surface.get(shortcut_id))
                    .and_then(|shortcut| shortcut.key.clone());
                let effective_key = override_key.unwrap_or(default_key);
                if effective_key.trim().is_empty() {
                    return None;
                }

                Some(ResolvedSurfaceShortcut {
                    key: effective_key,
                    handler,
                    target_ref: value
                        .get("target_ref")
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_string),
                })
            })
            .collect()
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
            .find(|shortcut| Self::key_matches_binding(key, &shortcut.key));
        let Some(shortcut) = matched else {
            return Ok(None);
        };

        let target_key = shortcut
            .target_ref
            .as_deref()
            .and_then(|reference| find_node_key_by_reference(tree, reference))
            .or_else(|| self.normalized_focused_key(tree))
            .unwrap_or_else(|| "root".to_string());
        let event = self.build_keyboard_event(tree, &target_key, "keydown", key, modifiers);
        Ok(Some(
            self.call_namespaced_handler(&shortcut.handler, &[event])?,
        ))
    }

    pub(in crate::shell::component) fn annotate_surface_shortcuts(&self, tree: &mut WidgetNode) {
        let keyboard_settings = self.current_keyboard_settings();
        for shortcut in self.resolved_surface_shortcuts(&keyboard_settings) {
            let Some(target_ref) = shortcut.target_ref.as_deref() else {
                continue;
            };
            let Some(node) = find_node_by_reference_mut(tree, target_ref) else {
                continue;
            };
            match node.accessibility.keyboard_shortcut.as_deref() {
                Some(existing) if existing == shortcut.key => {}
                Some(existing) => {
                    node.accessibility.keyboard_shortcut =
                        Some(format!("{existing}, {}", shortcut.key));
                }
                None => {
                    node.accessibility.keyboard_shortcut = Some(shortcut.key);
                }
            }
        }
    }
}

fn normalize_key_name(value: &str) -> String {
    match value {
        " " => "space".into(),
        other => other.to_ascii_lowercase(),
    }
}

fn find_node_key_by_reference(node: &WidgetNode, reference: &str) -> Option<String> {
    if node
        .attributes
        .get("id")
        .is_some_and(|value| value == reference)
        || node
            .attributes
            .get("ref")
            .is_some_and(|value| value == reference)
    {
        return node.attributes.get("_mesh_key").cloned();
    }

    node.children
        .iter()
        .find_map(|child| find_node_key_by_reference(child, reference))
}

fn find_node_by_reference_mut<'a>(
    node: &'a mut WidgetNode,
    reference: &str,
) -> Option<&'a mut WidgetNode> {
    if node
        .attributes
        .get("id")
        .is_some_and(|value| value == reference)
        || node
            .attributes
            .get("ref")
            .is_some_and(|value| value == reference)
    {
        return Some(node);
    }

    for child in &mut node.children {
        if let Some(found) = find_node_by_reference_mut(child, reference) {
            return Some(found);
        }
    }

    None
}
