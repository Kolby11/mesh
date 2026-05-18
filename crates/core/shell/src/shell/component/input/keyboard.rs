use super::super::*;

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
        if declarations.is_empty() {
            return Vec::new();
        }

        let overrides = keyboard_settings.surface_shortcuts.get(self.surface_id());
        let active_locale = self.locale.current();

        declarations
            .into_iter()
            .filter_map(|declaration| {
                let override_key = overrides
                    .and_then(|surface| surface.get(&declaration.keybind_id))
                    .and_then(|shortcut| shortcut.key.clone());
                resolve_surface_shortcut_declaration(declaration, override_key, active_locale)
            })
            .collect()
    }

    fn surface_shortcut_declarations(&self) -> Vec<SurfaceShortcutDeclaration> {
        let mut declarations = self.manifest_surface_shortcut_declarations();
        for legacy in self.legacy_settings_surface_shortcut_declarations() {
            if declarations
                .iter()
                .any(|declaration| declaration.keybind_id == legacy.keybind_id)
            {
                continue;
            }
            declarations.push(legacy);
        }
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

fn resolve_surface_shortcut_declaration(
    declaration: SurfaceShortcutDeclaration,
    override_key: Option<String>,
    active_locale: &str,
) -> Option<ResolvedSurfaceShortcut> {
    if let Some(key) = override_key {
        let kind = declaration.generic_trigger.kind;
        let modifiers = declaration.generic_trigger.modifiers.clone();
        return resolved_surface_shortcut(
            declaration,
            key,
            modifiers,
            kind,
            KeybindResolutionSource::UserOverride,
        );
    }

    for locale in keybind_locale_candidates(active_locale) {
        let Some((key, trigger_kind, modifiers)) = declaration
            .localized_triggers
            .get(&locale)
            .and_then(|trigger| {
                let key = trigger.key.as_ref()?;
                if key.trim().is_empty() {
                    return None;
                }
                Some((key.clone(), trigger.kind, trigger.modifiers.clone()))
            })
        else {
            continue;
        };
        return resolved_surface_shortcut(
            declaration,
            key,
            modifiers,
            trigger_kind,
            KeybindResolutionSource::LocaleDefault { locale },
        );
    }

    let kind = declaration.generic_trigger.kind;
    let key = declaration.generic_trigger.key.clone()?;
    let modifiers = declaration.generic_trigger.modifiers.clone();
    resolved_surface_shortcut(
        declaration,
        key,
        modifiers,
        kind,
        KeybindResolutionSource::ModuleDefault,
    )
}

fn resolved_surface_shortcut(
    declaration: SurfaceShortcutDeclaration,
    key: String,
    modifiers: Vec<String>,
    trigger_kind: mesh_core_module::KeybindTriggerKind,
    source: KeybindResolutionSource,
) -> Option<ResolvedSurfaceShortcut> {
    if key.trim().is_empty() {
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
