use super::*;
use mesh_core_elements::Prop;
use mesh_core_interaction::{
    active_modal_scope, menu_for_key, menu_items, menu_label, nested_menu, open_focus_scopes,
    parent_menu_trigger, scope_contains, scope_focus_traversal,
};

#[derive(Debug, Default)]
pub(super) struct PrimitiveState {
    scopes: Vec<(String, Option<String>)>,
    pending_menu_focus: Option<String>,
    typeahead: String,
    typeahead_menu: Option<String>,
    pub(super) typed_at: Option<Instant>,
    blocked_pointer: bool,
    consumed_keys: HashSet<String>,
}

impl FrontendSurfaceComponent {
    pub(super) fn primitives_need_focus_refresh(&self) -> bool {
        self.primitives.pending_menu_focus.is_some()
    }

    pub(super) fn refresh_focus_scopes(&mut self, tree: &WidgetNode) {
        let scopes = open_focus_scopes(tree);
        let keys: HashSet<_> = scopes.iter().filter_map(|node| node.mesh_key()).collect();
        let previous = std::mem::take(&mut self.primitives.scopes);
        let mut restore = None;
        // If several scopes disappear together, restore the outermost opener.
        for (key, return_key) in &previous {
            if !keys.contains(key.as_str()) && restore.is_none() {
                restore = return_key.clone();
            }
        }
        if let Some(key) =
            restore.filter(|key| node_can_receive_interaction(tree, key, InteractionTarget::Focus))
        {
            self.apply_primitive_focus(Some(key));
        }
        for scope in scopes {
            let Some(key) = scope.mesh_key() else {
                continue;
            };
            let saved = previous.iter().find(|(old, _)| old == key);
            let return_key = saved
                .map(|(_, key)| key.clone())
                .unwrap_or_else(|| self.focused_key.clone());
            self.primitives.scopes.push((key.to_owned(), return_key));
            if saved.is_none() {
                let target = scope_focus_traversal(tree, scope)
                    .into_iter()
                    .next()
                    .or_else(|| Some(key.to_owned()));
                self.apply_primitive_focus(target);
            }
        }
        if let Some(key) = self.primitives.pending_menu_focus.take()
            && let Some(menu) = find_node_by_key(tree, &key)
            && let Some(item) = menu_items(tree, menu).first()
        {
            self.apply_primitive_focus(item.mesh_key().map(str::to_owned));
        }
        if let Some(scope) = active_modal_scope(tree)
            && !self
                .focused_key
                .as_deref()
                .is_some_and(|key| scope_contains(scope, key))
        {
            let target = scope_focus_traversal(tree, scope)
                .into_iter()
                .next()
                .or_else(|| scope.mesh_key().map(str::to_owned));
            self.apply_primitive_focus(target);
        }
    }

    fn apply_primitive_focus(&mut self, key: Option<String>) {
        if self.focused_key == key {
            return;
        }
        self.primitives.consumed_keys.extend(
            self.keyboard_button_press_activations
                .iter()
                .map(|(_, key)| key.clone()),
        );
        self.keyboard_button_press_activations.clear();
        self.input_preedits.clear();
        self.focused_id = key.as_deref().map(runtime_node_id_for_key);
        self.focus_visible_id = self.focused_id;
        self.focused_key = key.clone();
        self.focus_visible_key = key;
        let mut transaction = self.interaction_state.begin();
        transaction.focus(self.focused_id, true);
        self.commit_interaction_delta(transaction);
        self.invalidate_interaction_restyle();
    }

    pub(super) fn primitive_input(
        &mut self,
        tree: &WidgetNode,
        input: &ComponentInput,
    ) -> Result<Option<Vec<CoreRequest>>, ComponentError> {
        match input {
            ComponentInput::KeyReleased { key, .. }
                if self.primitives.consumed_keys.remove(key) =>
            {
                return Ok(Some(Vec::new()));
            }
            ComponentInput::PointerButton {
                x,
                y,
                button,
                pressed,
            } if *button == mesh_core_presentation::PRIMARY_POINTER_BUTTON => {
                if !pressed && self.primitives.blocked_pointer {
                    self.primitives.blocked_pointer = false;
                    return Ok(Some(Vec::new()));
                }
                if *pressed {
                    if let Some(target) = mesh_core_interaction::find_focusable_at(tree, *x, *y)
                        && let Some(item) = find_node_by_key(tree, &target)
                        && menu_for_key(tree, &target).is_some()
                        && let Some(submenu) = nested_menu(item)
                        && let Some(key) = submenu.mesh_key()
                        && active_modal_scope(tree)
                            .is_none_or(|scope| scope_contains(scope, &target))
                    {
                        self.primitives.pending_menu_focus = Some(key.to_owned());
                        self.primitives.blocked_pointer = true;
                        return self
                            .set_disclosure_open(
                                tree,
                                key,
                                true,
                                disclosure::DisclosureReason::Trigger,
                            )
                            .map(Some);
                    }
                    let path =
                        mesh_core_interaction::find_node_path_at(tree, *x, *y).unwrap_or_default();
                    let scopes = open_focus_scopes(tree);
                    let dismissible = scopes.iter().rev().find(|scope| {
                        node_is_source(scope, &["dialog", "sheet"])
                            && scope.attributes.contains_key(Prop::Open)
                    });
                    let mut requests = Vec::new();
                    let mut dismissed = false;
                    if let Some(scope) = dismissible
                        && !scope
                            .mesh_key()
                            .is_some_and(|key| path.iter().any(|entry| entry == key))
                    {
                        requests.extend(self.set_disclosure_open(
                            tree,
                            scope.mesh_key().unwrap(),
                            false,
                            disclosure::DisclosureReason::Outside,
                        )?);
                        dismissed = true;
                    }
                    let outside_modal = active_modal_scope(tree).is_some_and(|scope| {
                        !scope
                            .mesh_key()
                            .is_some_and(|key| path.iter().any(|entry| entry == key))
                    });
                    if dismissed || outside_modal {
                        self.primitives.blocked_pointer = true;
                        return Ok(Some(requests));
                    }
                }
            }
            ComponentInput::KeyPressed { key, modifiers } if !modifiers.ctrl && !modifiers.alt => {
                if let Some(requests) = self.menu_key(tree, key)? {
                    self.primitives.consumed_keys.insert(key.clone());
                    return Ok(Some(requests));
                }
                if key == "Escape" {
                    if let Some(requests) = self.disclosure_escape(tree)? {
                        return Ok(Some(requests));
                    }
                    if let Some(scope) = open_focus_scopes(tree).into_iter().rev().find(|scope| {
                        node_is_source(scope, &["dialog", "sheet"])
                            && scope.attributes.contains_key(Prop::Open)
                    }) {
                        return self
                            .set_disclosure_open(
                                tree,
                                scope.mesh_key().unwrap(),
                                false,
                                disclosure::DisclosureReason::Escape,
                            )
                            .map(Some);
                    }
                }
            }
            ComponentInput::Char { ch } if !ch.is_control() => {
                return self.menu_typeahead(tree, &ch.to_string());
            }
            ComponentInput::TextInput { text } if !text.is_empty() => {
                return self.menu_typeahead(tree, text);
            }
            _ => {}
        }
        Ok(None)
    }

    fn menu_key(
        &mut self,
        tree: &WidgetNode,
        key: &str,
    ) -> Result<Option<Vec<CoreRequest>>, ComponentError> {
        let Some(focused) = self.focused_key.clone() else {
            return Ok(None);
        };
        let Some(menu) = menu_for_key(tree, &focused) else {
            return Ok(None);
        };
        let items = menu_items(tree, menu);
        let Some(index) = items
            .iter()
            .position(|item| item.mesh_key() == Some(focused.as_str()))
        else {
            return Ok(None);
        };
        if matches!(key, "ArrowDown" | "ArrowUp" | "Home" | "End") {
            self.primitives.typeahead.clear();
            let next = match key {
                "Home" => 0,
                "End" => items.len() - 1,
                "ArrowUp" => (index + items.len() - 1) % items.len(),
                _ => (index + 1) % items.len(),
            };
            let requests =
                self.set_focus_target(tree, items[next].mesh_key().map(str::to_owned), true)?;
            self.invalidate_interaction_restyle();
            return Ok(Some(requests));
        }
        let rtl =
            menu.computed_style.text_direction == mesh_core_elements::style::TextDirection::Rtl;
        let enter = if rtl { "ArrowLeft" } else { "ArrowRight" };
        let leave = if rtl { "ArrowRight" } else { "ArrowLeft" };
        if key == enter || key == "Enter" || key == "Space" {
            if let Some(submenu) = nested_menu(items[index]) {
                let Some(subkey) = submenu.mesh_key() else {
                    return Ok(None);
                };
                self.primitives.pending_menu_focus = Some(subkey.to_owned());
                let requests = self.set_disclosure_open(
                    tree,
                    subkey,
                    true,
                    disclosure::DisclosureReason::Keyboard,
                )?;
                self.invalidate_interaction_restyle();
                return Ok(Some(requests));
            }
        }
        if key == leave || key == "Escape" {
            if let Some((parent, trigger)) = parent_menu_trigger(tree, menu) {
                let mut requests = self.set_focus_target(tree, Some(trigger), true)?;
                if menu.attributes.contains_key(Prop::Open) {
                    requests.extend(self.set_disclosure_open(
                        tree,
                        menu.mesh_key().unwrap(),
                        false,
                        if key == "Escape" {
                            disclosure::DisclosureReason::Escape
                        } else {
                            disclosure::DisclosureReason::Keyboard
                        },
                    )?);
                }
                self.primitives.typeahead_menu = parent.mesh_key().map(str::to_owned);
                self.primitives.typeahead.clear();
                self.invalidate_interaction_restyle();
                return Ok(Some(requests));
            }
        }
        // Printable keys are handled by committed text events, as for inputs.
        if key.chars().count() == 1 && !key.chars().any(char::is_control) {
            return Ok(Some(Vec::new()));
        }
        Ok(None)
    }

    fn menu_typeahead(
        &mut self,
        tree: &WidgetNode,
        text: &str,
    ) -> Result<Option<Vec<CoreRequest>>, ComponentError> {
        let Some(focused) = self.focused_key.as_deref() else {
            return Ok(None);
        };
        let Some(menu) = menu_for_key(tree, focused) else {
            return Ok(None);
        };
        let items = menu_items(tree, menu);
        let Some(index) = items
            .iter()
            .position(|item| item.mesh_key() == Some(focused))
        else {
            return Ok(None);
        };
        let now = Instant::now();
        let menu_key = menu.mesh_key().map(str::to_owned);
        if self.primitives.typeahead_menu != menu_key
            || self
                .primitives
                .typed_at
                .is_none_or(|time| now.duration_since(time) > Duration::from_millis(700))
        {
            self.primitives.typeahead.clear();
        }
        self.primitives.typeahead_menu = menu_key;
        self.primitives.typed_at = Some(now);
        // Bound the search buffer even for an unbroken stream of input.
        if self.primitives.typeahead.len() + text.len() > 256 {
            self.primitives.typeahead.clear();
        }
        self.primitives.typeahead.extend(
            text.chars()
                .filter(|ch| !ch.is_control())
                .flat_map(char::to_lowercase)
                .take(64),
        );
        let buffer = &self.primitives.typeahead;
        let repeated = buffer
            .chars()
            .next()
            .is_some_and(|first| buffer.chars().all(|ch| ch == first));
        let query = if repeated {
            buffer.chars().next().unwrap().to_string()
        } else {
            buffer.clone()
        };
        let start = if query.chars().count() == 1 {
            index + 1
        } else {
            index
        };
        let target = (0..items.len())
            .map(|offset| items[(start + offset) % items.len()])
            .find(|item| menu_label(item).starts_with(&query))
            .and_then(|item| item.mesh_key())
            .map(str::to_owned);
        let requests = if target.is_some() {
            self.set_focus_target(tree, target, true)?
        } else {
            Vec::new()
        };
        self.invalidate_interaction_restyle();
        Ok(Some(requests))
    }
}
