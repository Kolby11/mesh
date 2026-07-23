use super::*;

mod focus;
mod keyboard;
mod widgets;

use focus::selectable_text_target_key;
#[cfg(test)]
use widgets::pointer_event_target_with_focus;
pub(in crate::shell::component) use widgets::{LONG_PRESS_DELAY, PressedTargetSnapshot};

#[cfg(test)]
pub(crate) use keyboard::KeybindResolutionSource;
pub(in crate::shell::component) use keyboard::ResolvedSurfaceShortcut;

fn point_in_bounds(x: f32, y: f32, (left, top, right, bottom): (f32, f32, f32, f32)) -> bool {
    x >= left && x <= right && y >= top && y <= bottom
}

impl FrontendSurfaceComponent {
    pub(in crate::shell::component) fn handle_component_input(
        &mut self,
        theme: &Theme,
        width: u32,
        height: u32,
        input: ComponentInput,
    ) -> Result<Vec<CoreRequest>, ComponentError> {
        let _span = tracing::debug_span!("handle_component_input", surface = %self.id()).entered();
        tracing::trace!(
            "[hover] handle_input called: id={} visible={} input={:?}",
            self.id(),
            self.visible,
            std::mem::discriminant(&input)
        );
        if !self.visible {
            return Ok(Vec::new());
        }

        let tree = self
            .last_tree
            .take()
            .unwrap_or_else(|| self.build_tree(theme, width, height));
        let result = self.handle_component_input_with_tree(&tree, input);
        debug_assert!(
            self.last_tree.is_none(),
            "input dispatch must not replace the retained tree"
        );
        self.last_tree = Some(tree);
        result
    }

    fn handle_component_input_with_tree(
        &mut self,
        tree: &WidgetNode,
        input: ComponentInput,
    ) -> Result<Vec<CoreRequest>, ComponentError> {
        match input {
            ComponentInput::PointerButton { x, y, pressed } => {
                if pressed {
                    if let Some(selection_key) = selectable_text_target_key(tree, x, y) {
                        let requests = self.set_focus_target(tree, None, false)?;
                        self.pointer_down_key = None;
                        self.pointer_down_bounds = None;
                        self.pointer_down_target = None;
                        self.active_slider_key = None;
                        self.begin_text_selection(selection_key, x, y);
                        self.invalidate_paint();
                        return Ok(requests);
                    }

                    self.clear_selection();
                    let press_hit = pointer_press_hit(tree, x, y);
                    if let Some(target) = press_hit.target {
                        let node_key = target.key.to_owned();
                        let focusable_key = press_hit.focusable.map(|hit| hit.key.to_owned());
                        self.pointer_down_key = Some(node_key.clone());
                        self.pointer_down_bounds = Some(target.bounds);
                        self.pointer_down_target = Some(self.pressed_target_snapshot(
                            &node_key,
                            target.node,
                            target.bounds,
                        ));
                        let mut requests = if let Some(focused_key) = focusable_key {
                            let focus_visible =
                                self.pointer_focus_visible_for_key(tree, &focused_key);
                            self.set_focus_target(tree, Some(focused_key), focus_visible)?
                        } else {
                            self.set_focus_target(tree, None, false)?
                        };

                        if target.node.tag == "slider" {
                            self.active_slider_key = Some(node_key.clone());
                            self.update_slider_from_resolved_press(
                                &node_key,
                                target.node,
                                target.bounds,
                                x,
                                y,
                            );
                            if target.node.event_handlers.contains_key("change")
                                && let Some(value) = self.slider_value(tree, &node_key)
                            {
                                requests.extend(self.call_node_handler(
                                    tree,
                                    &node_key,
                                    "change",
                                    &[serde_json::json!(value)],
                                )?);
                                self.invalidate_script_state();
                            } else {
                                self.invalidate_interaction_restyle();
                            }
                        } else {
                            self.active_slider_key = None;
                            if node_is_source(target.node, &["option"]) {
                                requests.extend(self.activate_option_choice_for_node(
                                    tree,
                                    &node_key,
                                    target.node,
                                )?);
                            } else if node_is_source(target.node, &["radio"]) {
                                requests.extend(self.activate_radio_choice_for_node(
                                    tree,
                                    &node_key,
                                    target.node,
                                )?);
                            } else if node_is_source(target.node, &["switch", "checkbox"])
                                || matches!(target.node.tag.as_str(), "switch" | "checkbox")
                            {
                                let value =
                                    self.toggle_checked_value_for_node(&node_key, target.node);
                                requests.extend(self.call_node_handler(
                                    tree,
                                    &node_key,
                                    "change",
                                    &[serde_json::json!(value)],
                                )?);
                            }
                        }

                        self.invalidate_interaction_restyle();
                        if !requests.is_empty() {
                            return Ok(requests);
                        }
                    } else {
                        let requests = self.set_focus_target(tree, None, false)?;
                        self.pointer_down_key = None;
                        self.pointer_down_bounds = None;
                        self.pointer_down_target = None;
                        self.active_slider_key = None;
                        self.invalidate_interaction_restyle();
                        if !requests.is_empty() {
                            return Ok(requests);
                        }
                    }
                } else {
                    let mut requests = Vec::new();
                    if let Some(slider_key) = self.active_slider_key.clone()
                        && let Some(value) = self.slider_value(tree, &slider_key)
                        && find_event_handler(tree, &slider_key, "release").is_some()
                    {
                        requests.extend(self.call_node_handler(
                            tree,
                            &slider_key,
                            "release",
                            &[serde_json::json!(value)],
                        )?);
                        self.invalidate_script_state();
                    }

                    self.end_text_selection_drag();

                    if self.selection.is_some() && self.pointer_down_key.is_none() {
                        self.invalidate_paint();
                        return Ok(requests);
                    }

                    let captured_click_key = self.captured_release_key(tree, x, y);
                    if let Some(node_key) = captured_click_key {
                        if let Some(pressed_target) = self
                            .pointer_down_target
                            .clone()
                            .filter(|target| target.key == node_key)
                        {
                            let click_event = self.build_click_event_for_pressed_target(
                                tree,
                                &pressed_target,
                                x,
                                y,
                            );
                            if pressed_target.activation_item {
                                requests.extend(self.dispatch_pressed_activation_handlers(
                                    &pressed_target,
                                    click_event,
                                )?);
                            } else if pressed_target.click_handler.is_some() {
                                requests.extend(
                                    self.call_pressed_click_handler(&pressed_target, click_event)?,
                                );
                            }
                        } else if let Some(target) = find_node_by_key(tree, &node_key) {
                            let bounds = self
                                .pointer_down_bounds
                                .or_else(|| find_node_bounds_by_key(tree, &node_key, 0.0, 0.0))
                                .unwrap_or((0.0, 0.0, 0.0, 0.0));
                            let click_event = self.build_click_event_for(
                                tree,
                                &node_key,
                                Some(target),
                                bounds,
                                x,
                                y,
                            );
                            if node_is_source(
                                target,
                                &["menu-item", "command-item", "preference-row"],
                            ) || node_is_source(target, &["tab", "list-item"])
                            {
                                requests.extend(
                                    self.dispatch_resolved_activation_handlers(
                                        target,
                                        click_event,
                                    )?,
                                );
                            } else if target.event_handlers.contains_key("click") {
                                requests.extend(self.call_resolved_node_handler(
                                    target,
                                    "click",
                                    &[click_event],
                                )?);
                            }
                        }
                    }
                    self.pointer_down_key = None;
                    self.pointer_down_bounds = None;
                    self.pointer_down_target = None;
                    self.active_slider_key = None;
                    self.invalidate_interaction_restyle();
                    if !requests.is_empty() {
                        return Ok(requests);
                    }
                }
            }
            ComponentInput::PointerMove { x, y } => {
                if let Some(slider_key) = self.active_slider_key.clone() {
                    self.update_slider_from_position(tree, &slider_key, x, y);
                    let mut requests = Vec::new();
                    if let Some(value) = self.slider_value(tree, &slider_key) {
                        if find_event_handler(tree, &slider_key, "change").is_some() {
                            requests.extend(self.call_node_handler(
                                tree,
                                &slider_key,
                                "change",
                                &[serde_json::json!(value)],
                            )?);
                            // Slider drag with script handlers can mutate reactive globals
                            // such as labels bound to the value, so preserve the rebuild path.
                            self.invalidate_script_state();
                        } else {
                            self.invalidate_interaction_restyle();
                        }
                    }
                    if !requests.is_empty() {
                        return Ok(requests);
                    }
                }

                if self.selection.is_some() {
                    self.update_text_selection_focus(x, y);
                    self.invalidate_paint();
                }

                // Update hover state for CSS :hover and the tooltip system.
                self.hovered_pos = (x, y);
                let mut pointer_hit = mesh_core_interaction::pointer_hit_test(tree, x, y);
                let new_path = pointer_hit
                    .as_mut()
                    .map(|hit| std::mem::take(&mut hit.path))
                    .unwrap_or_default();
                let new_key = new_path.last().cloned();
                tracing::trace!(
                    "[hover] pointer=({x:.1},{y:.1}) path={:?} hit={:?} prev={:?}",
                    new_path,
                    new_key,
                    self.hovered_key
                );
                if new_key != self.hovered_key || new_path != self.hovered_path {
                    let previous_tooltip = self.hovered_tooltip.clone();
                    let next_tooltip = pointer_hit.as_ref().and_then(|hit| hit.tooltip.clone());
                    let tooltip_may_change = previous_tooltip.is_some()
                        || next_tooltip.is_some()
                        || self.tooltip_visible
                        || self.last_tooltip_damage.is_some();
                    let same_tooltip_owner = previous_tooltip
                        .as_ref()
                        .zip(next_tooltip.as_ref())
                        .is_some_and(|((previous_owner, _), (next_owner, _))| {
                            previous_owner == next_owner
                        });
                    self.hovered_key = new_key.clone();
                    let previous_path = std::mem::take(&mut self.hovered_path);
                    self.hovered_tooltip = next_tooltip.clone();
                    self.tooltip_target_cache.clear();
                    // Store the hovered element's bounds for tooltip positioning.
                    // Use the tooltip owner's bounds when available; fall back to
                    // the hovered node itself.
                    self.hovered_element_bounds = pointer_hit.as_ref().map(|hit| hit.bounds);
                    // Preserve an already-running tooltip when moving between a
                    // tooltip owner and descendants that inherit that tooltip.
                    if same_tooltip_owner {
                        if self.hover_start.is_none() {
                            self.hover_start = Some(std::time::Instant::now());
                            self.tooltip_visible = false;
                        }
                    } else {
                        self.hover_start = next_tooltip.map(|_| std::time::Instant::now());
                        self.tooltip_visible = false;
                        self.tooltip_appeared_at = None;
                    }
                    // Hover changes don't mutate script state — flag the surface
                    // for a style-only repaint so paint() can reuse the cached
                    // widget tree instead of re-running Luau scripts.
                    self.invalidate_hover_change(tooltip_may_change);
                    // Dispatch pointerenter/pointerleave to any script handlers on
                    // the entered/left nodes (e.g. hover-to-open popovers).
                    let hover_result = self.dispatch_hover_transition_handlers(
                        tree,
                        &previous_path,
                        &new_path,
                        x,
                        y,
                    );
                    self.hovered_path = new_path;
                    let hover_requests = hover_result?;
                    if !hover_requests.is_empty() {
                        return Ok(hover_requests);
                    }
                }
            }
            ComponentInput::PointerLeave => {
                let had_hover_state = self.hovered_key.is_some()
                    || !self.hovered_path.is_empty()
                    || self.hover_start.is_some();
                let previous_path = std::mem::take(&mut self.hovered_path);
                if had_hover_state {
                    let tooltip_may_change = self.hovered_tooltip.is_some()
                        || self.tooltip_visible
                        || self.last_tooltip_damage.is_some();
                    self.hovered_key = None;
                    self.hovered_tooltip = None;
                    self.hover_start = None;
                    self.tooltip_visible = false;
                    self.hovered_element_bounds = None;
                    self.tooltip_target_cache.clear();
                    self.tooltip_appeared_at = None;
                    self.invalidate_hover_change(tooltip_may_change);
                }
                // The pointer left the whole surface — fire pointerleave/mouseleave
                // on everything that was hovered so popovers can close themselves.
                let leave_requests =
                    self.dispatch_hover_transition_handlers(tree, &previous_path, &[], 0.0, 0.0)?;
                if !leave_requests.is_empty() {
                    return Ok(leave_requests);
                }
            }
            ComponentInput::Scroll { x, y, dx, dy } => {
                if let Some(requests) = self.dispatch_scroll_handler(tree, x, y, dx, dy)? {
                    return Ok(requests);
                }

                if let Some(scroll_hit) = find_scrollable_at_with_limits(tree, x, y) {
                    let current = self.scroll_offsets.entry(scroll_hit.key).or_default();
                    let next_x = (current.x - dx * 28.0).clamp(0.0, scroll_hit.max_x);
                    let next_y = (current.y - dy * 28.0).clamp(0.0, scroll_hit.max_y);
                    if (next_x - current.x).abs() > f32::EPSILON
                        || (next_y - current.y).abs() > f32::EPSILON
                    {
                        current.x = next_x;
                        current.y = next_y;
                        self.invalidate(ComponentDirtyFlags::PAINT | ComponentDirtyFlags::METRICS);
                    }
                }
            }
            ComponentInput::TwoFingerScroll { x, y, dx, dy } => {
                if let Some(requests) =
                    self.dispatch_two_finger_scroll_handler(tree, x, y, dx, dy)?
                {
                    return Ok(requests);
                }

                // A surface that does not opt into `ontwofingerscroll` keeps
                // the existing continuous-scroll behavior.
                if let Some(scroll_hit) = find_scrollable_at_with_limits(tree, x, y) {
                    let current = self.scroll_offsets.entry(scroll_hit.key).or_default();
                    let next_x = (current.x - dx * 28.0).clamp(0.0, scroll_hit.max_x);
                    let next_y = (current.y - dy * 28.0).clamp(0.0, scroll_hit.max_y);
                    if (next_x - current.x).abs() > f32::EPSILON
                        || (next_y - current.y).abs() > f32::EPSILON
                    {
                        current.x = next_x;
                        current.y = next_y;
                        self.invalidate(ComponentDirtyFlags::PAINT | ComponentDirtyFlags::METRICS);
                    }
                }
            }
            ComponentInput::GestureSwipeBegin { fingers } => {
                return self.dispatch_swipe_begin(tree, fingers);
            }
            ComponentInput::GestureSwipeUpdate { dx, dy } => {
                return self.dispatch_swipe_update(tree, dx, dy);
            }
            ComponentInput::GestureSwipeEnd { cancelled } => {
                return self.dispatch_swipe_end(tree, cancelled);
            }
            ComponentInput::GesturePinchBegin { fingers } => {
                return self.dispatch_pinch_begin(tree, fingers);
            }
            ComponentInput::GesturePinchUpdate {
                dx,
                dy,
                scale,
                rotation,
            } => {
                return self.dispatch_pinch_update(tree, dx, dy, scale, rotation);
            }
            ComponentInput::GesturePinchEnd { cancelled } => {
                return self.dispatch_pinch_end(tree, cancelled);
            }
            ComponentInput::GestureHoldBegin { fingers } => {
                return self.dispatch_hold_begin(tree, fingers);
            }
            ComponentInput::GestureHoldEnd { cancelled } => {
                return self.dispatch_hold_end(tree, cancelled);
            }
            ComponentInput::TouchDown { id, x, y } => {
                return self.dispatch_touch_down(tree, id, x, y);
            }
            ComponentInput::TouchMove { id, x, y } => {
                return self.dispatch_touch_move(tree, id, x, y);
            }
            ComponentInput::TouchUp { id } => {
                return self.dispatch_touch_up(tree, id);
            }
            ComponentInput::TouchCancel => {
                return self.dispatch_touch_cancel(tree);
            }
            ComponentInput::Char { ch } => {
                if let Some(focused_key) = self.focused_key.clone() {
                    let accepts_char = find_node_by_key(tree, &focused_key)
                        .is_some_and(|node| input_accepts_char(node, ch));
                    if is_input_key(tree, &focused_key) && accepts_char {
                        self.clear_selection();
                        let value = self.input_values.entry(focused_key.clone()).or_default();
                        value.push(ch);
                        let current = value.clone();
                        self.invalidate_text_state();
                        return self.dispatch_text_input_value_handlers(
                            tree,
                            &focused_key,
                            &current,
                        );
                    }
                }

                let keyboard_settings = self.current_keyboard_settings();
                let key = ch.to_string();
                if let Some(requests) = self.dispatch_surface_shortcut(
                    tree,
                    &key,
                    KeyModifiers::default(),
                    &keyboard_settings,
                )? {
                    return Ok(requests);
                }
            }
            ComponentInput::KeyPressed { key, modifiers } => {
                return self.handle_key_pressed(tree, key, modifiers);
            }
            ComponentInput::KeyReleased { key, modifiers } => {
                return self.handle_key_released(tree, key, modifiers);
            }
        }

        Ok(Vec::new())
    }

    /// Dispatch `pointerenter`/`pointerleave` (plus the `mouseenter`/`mouseleave`
    /// aliases) script handlers for the delta between the previously hovered
    /// node path and the new one: nodes only in `previous_path` get leave
    /// handlers, nodes only in `new_path` get enter handlers. The event payload
    /// mirrors a click event so handlers can read `event.current_target.position`
    /// to position popovers, exactly as the `onclick` open path does.
    fn dispatch_hover_transition_handlers(
        &mut self,
        tree: &WidgetNode,
        previous_path: &[String],
        new_path: &[String],
        x: f32,
        y: f32,
    ) -> Result<Vec<CoreRequest>, ComponentError> {
        let mut requests = Vec::new();
        let left_keys: Vec<&str> = previous_path
            .iter()
            .filter(|key| !new_path.contains(key))
            .map(String::as_str)
            .collect();
        let entered_keys: Vec<&str> = new_path
            .iter()
            .filter(|key| !previous_path.contains(key))
            .map(String::as_str)
            .collect();
        if left_keys.is_empty() && entered_keys.is_empty() {
            return Ok(requests);
        }
        // One traversal resolves every transitioning node + its bounds,
        // instead of a `find_event_handler`/`build_click_event` walk per key
        // (each of those is itself a full-tree walk, so a depth-d hover
        // transition previously cost O(d) walks per handler check).
        let target_keys: HashSet<&str> = left_keys
            .iter()
            .chain(entered_keys.iter())
            .copied()
            .collect();
        let nodes = mesh_core_interaction::find_nodes_by_keys(tree, &target_keys);

        for key in left_keys {
            let Some((node, bounds)) = nodes.get(key) else {
                continue;
            };
            let has_pointerleave = node.event_handlers.contains_key("pointerleave");
            let has_mouseleave = node.event_handlers.contains_key("mouseleave");
            if !has_pointerleave && !has_mouseleave {
                continue;
            }
            let event = self.build_click_event_for(tree, key, Some(node), *bounds, x, y);
            let args = std::slice::from_ref(&event);
            if has_pointerleave {
                requests.extend(self.call_resolved_node_handler(node, "pointerleave", args)?);
            }
            if has_mouseleave {
                requests.extend(self.call_resolved_node_handler(node, "mouseleave", args)?);
            }
        }
        for key in entered_keys {
            let Some((node, bounds)) = nodes.get(key) else {
                continue;
            };
            let has_pointerenter = node.event_handlers.contains_key("pointerenter");
            let has_mouseenter = node.event_handlers.contains_key("mouseenter");
            if !has_pointerenter && !has_mouseenter {
                continue;
            }
            let event = self.build_click_event_for(tree, key, Some(node), *bounds, x, y);
            let args = std::slice::from_ref(&event);
            if has_pointerenter {
                requests.extend(self.call_resolved_node_handler(node, "pointerenter", args)?);
            }
            if has_mouseenter {
                requests.extend(self.call_resolved_node_handler(node, "mouseenter", args)?);
            }
        }
        Ok(requests)
    }
}

pub(super) fn is_bare_printable_key(key: &str, modifiers: KeyModifiers) -> bool {
    !modifiers.ctrl
        && !modifiers.alt
        && key.chars().count() == 1
        && key.chars().all(|ch| !ch.is_control())
}

#[cfg(test)]
mod press_target_tests {
    use super::pointer_event_target_with_focus;
    use super::widgets::captured_release_key;
    use mesh_core_elements::WidgetNode;
    use mesh_core_interaction::find_focusable_at;

    fn positioned_node(key: &str, tag: &str, x: f32, y: f32, w: f32, h: f32) -> WidgetNode {
        let mut node = WidgetNode::new(tag);
        node.attributes.insert("_mesh_key".into(), key.into());
        node.layout.x = x;
        node.layout.y = y;
        node.layout.width = w;
        node.layout.height = h;
        node
    }

    #[test]
    fn fused_lookup_matches_focusable_target_with_click_handler() {
        let mut button = positioned_node("root/0", "button", 0.0, 0.0, 40.0, 20.0);
        button
            .event_handlers
            .insert("click".into(), "onClick".into());
        let mut root = positioned_node("root", "row", 0.0, 0.0, 40.0, 20.0);
        root.children.push(button);

        let (target, focusable) = pointer_event_target_with_focus(&root, 10.0, 10.0);
        assert_eq!(target.as_deref(), Some("root/0"));
        assert_eq!(focusable.as_deref(), Some("root/0"));
        assert_eq!(focusable, find_focusable_at(&root, 10.0, 10.0));
    }

    #[test]
    fn fused_lookup_falls_back_to_click_handler_when_not_focusable() {
        let mut clickable = positioned_node("root/0", "box", 0.0, 0.0, 40.0, 20.0);
        clickable
            .event_handlers
            .insert("click".into(), "onClick".into());
        let mut root = positioned_node("root", "row", 0.0, 0.0, 40.0, 20.0);
        root.children.push(clickable);

        let (target, focusable) = pointer_event_target_with_focus(&root, 10.0, 10.0);
        assert_eq!(target.as_deref(), Some("root/0"));
        assert_eq!(focusable, None);
        assert_eq!(find_focusable_at(&root, 10.0, 10.0), None);
    }

    #[test]
    fn fused_lookup_returns_none_outside_any_target() {
        let mut clickable = positioned_node("root/0", "box", 0.0, 0.0, 40.0, 20.0);
        clickable
            .event_handlers
            .insert("click".into(), "onClick".into());
        let mut root = positioned_node("root", "row", 0.0, 0.0, 40.0, 20.0);
        root.children.push(clickable);

        let (target, focusable) = pointer_event_target_with_focus(&root, 500.0, 500.0);
        assert_eq!(target, None);
        assert_eq!(focusable, None);
    }

    #[test]
    fn captured_release_key_skips_hit_test_when_release_stays_inside_press_bounds() {
        let mut root = positioned_node("root", "row", 0.0, 0.0, 40.0, 20.0);
        let mut button = positioned_node("root/0", "button", 0.0, 0.0, 40.0, 20.0);
        button
            .event_handlers
            .insert("click".into(), "onClick".into());
        root.children.push(button);

        let captured = captured_release_key(
            &root,
            Some("root/0"),
            Some((0.0, 0.0, 40.0, 20.0)),
            39.0,
            19.0,
        );

        assert_eq!(captured.as_deref(), Some("root/0"));
    }

    #[test]
    fn captured_release_key_falls_back_to_hit_test_outside_press_bounds() {
        let mut root = positioned_node("root", "row", 0.0, 0.0, 80.0, 20.0);
        let mut button = positioned_node("root/0", "button", 0.0, 0.0, 40.0, 20.0);
        button
            .event_handlers
            .insert("click".into(), "onClick".into());
        root.children.push(button);

        let captured = captured_release_key(
            &root,
            Some("root/0"),
            Some((0.0, 0.0, 10.0, 10.0)),
            20.0,
            10.0,
        );

        assert_eq!(captured.as_deref(), Some("root/0"));
    }

    #[test]
    fn captured_release_key_rejects_different_release_target() {
        let mut root = positioned_node("root", "row", 0.0, 0.0, 80.0, 20.0);
        let mut left = positioned_node("root/0", "button", 0.0, 0.0, 40.0, 20.0);
        left.event_handlers.insert("click".into(), "onLeft".into());
        let mut right = positioned_node("root/1", "button", 40.0, 0.0, 40.0, 20.0);
        right
            .event_handlers
            .insert("click".into(), "onRight".into());
        root.children.push(left);
        root.children.push(right);

        let captured = captured_release_key(
            &root,
            Some("root/0"),
            Some((0.0, 0.0, 10.0, 10.0)),
            60.0,
            10.0,
        );

        assert_eq!(captured, None);
    }
}

#[cfg(test)]
mod performance_tests {
    use mesh_core_elements::WidgetNode;
    use std::hint::black_box;
    use std::time::Instant;

    fn large_tree(rows: usize, columns: usize) -> WidgetNode {
        let mut root = WidgetNode::new("column");
        for row_index in 0..rows {
            let mut row = WidgetNode::new("row");
            row.attributes
                .insert("_mesh_key".into(), format!("root/{row_index}"));
            for column_index in 0..columns {
                let mut node = WidgetNode::new("button");
                node.attributes.insert(
                    "_mesh_key".into(),
                    format!("root/{row_index}/{column_index}"),
                );
                node.attributes
                    .insert("class".into(), "toolbar-button compact interactive".into());
                node.attributes
                    .insert("content".into(), format!("Item {row_index}:{column_index}"));
                node.event_handlers
                    .insert("click".into(), "handleItemClick".into());
                row.children.push(node);
            }
            root.children.push(row);
        }
        root
    }

    // Run with:
    // cargo test -p mesh-core-shell --release -- input_tree_take_restore_beats_deep_clone --ignored --nocapture
    #[test]
    #[ignore]
    fn input_tree_take_restore_beats_deep_clone() {
        let tree = large_tree(100, 10);
        let iterations = 10_000usize;

        let clone_start = Instant::now();
        for _ in 0..iterations {
            black_box(black_box(&tree).clone());
        }
        let clone_ns = clone_start.elapsed().as_nanos().max(1);

        let mut retained = Some(tree);
        let take_start = Instant::now();
        for _ in 0..iterations {
            let current = black_box(&mut retained).take().expect("retained tree");
            black_box(&current);
            retained = Some(current);
        }
        let take_ns = take_start.elapsed().as_nanos().max(1);

        eprintln!("deep_clone={clone_ns}ns take_restore={take_ns}ns");
        assert!(
            take_ns.saturating_mul(10) <= clone_ns,
            "moving the retained tree should be at least 10x faster than recursively cloning it"
        );
    }

    // Run with:
    // cargo test -p mesh-core-shell --release -- hover_path_move_replace_beats_clone_shuffle --ignored --nocapture
    #[test]
    #[ignore]
    fn hover_path_move_replace_beats_clone_shuffle() {
        let path: Vec<String> = (0..48).map(|index| format!("root/{index}")).collect();
        let previous: Vec<String> = (0..48).map(|index| format!("prev/{index}")).collect();
        let iterations = 500_000usize;

        let old_start = Instant::now();
        let mut old_hovered_path = previous.clone();
        let mut old_total = 0usize;
        for _ in 0..iterations {
            let pointer_path = black_box(&path).clone();
            let previous_path = old_hovered_path.clone();
            old_hovered_path = pointer_path.clone();
            old_total = old_total.wrapping_add(black_box(
                previous_path.len() + old_hovered_path.len() + pointer_path.len(),
            ));
        }
        let old_time = old_start.elapsed();

        let new_start = Instant::now();
        let mut new_hovered_path = previous;
        let mut new_total = 0usize;
        for _ in 0..iterations {
            let mut pointer_path = black_box(&path).clone();
            let previous_path =
                std::mem::replace(&mut new_hovered_path, std::mem::take(&mut pointer_path));
            new_total = new_total.wrapping_add(black_box(
                previous_path.len() + new_hovered_path.len() + new_hovered_path.len(),
            ));
        }
        let new_time = new_start.elapsed();

        eprintln!(
            "hover path update: clone-shuffle {old_time:?}; move/replace {new_time:?}; ratio {:.1}x; totals={old_total}/{new_total}",
            old_time.as_secs_f64() / new_time.as_secs_f64()
        );
        assert_eq!(old_total, new_total);
        assert!(new_time < old_time);
    }

    fn large_positioned_tree(rows: usize, columns: usize) -> WidgetNode {
        let row_height = 20.0;
        let column_width = 40.0;
        let mut root = WidgetNode::new("column");
        root.layout.width = column_width * columns as f32;
        root.layout.height = row_height * rows as f32;
        for row_index in 0..rows {
            let mut row = WidgetNode::new("row");
            row.attributes
                .insert("_mesh_key".into(), format!("root/{row_index}"));
            row.layout.x = 0.0;
            row.layout.y = row_index as f32 * row_height;
            row.layout.width = column_width * columns as f32;
            row.layout.height = row_height;
            for column_index in 0..columns {
                let mut node = WidgetNode::new("button");
                node.attributes.insert(
                    "_mesh_key".into(),
                    format!("root/{row_index}/{column_index}"),
                );
                node.event_handlers
                    .insert("click".into(), "handleItemClick".into());
                // `WidgetNode::layout` is absolute (root-relative), not
                // parent-relative — hit-testing accumulates transform/scroll
                // offset only, so children must carry their own absolute y.
                node.layout.x = column_index as f32 * column_width;
                node.layout.y = row_index as f32 * row_height;
                node.layout.width = column_width;
                node.layout.height = row_height;
                row.children.push(node);
            }
            root.children.push(row);
        }
        root
    }

    // Run with:
    // cargo test -p mesh-core-shell --release -- fused_press_target_beats_duplicate_focusable_walk --ignored --nocapture
    #[test]
    #[ignore]
    fn fused_press_target_beats_duplicate_focusable_walk() {
        use super::pointer_event_target_with_focus;
        use mesh_core_interaction::{find_event_handler, find_focusable_at, find_node_path_at};

        let tree = large_positioned_tree(200, 12);
        // Last row, last column: worst-case walk depth for both the
        // focusable search and the click-handler path fallback.
        let (x, y) = (tree.layout.width - 5.0, tree.layout.height - 5.0);
        let iterations = 20_000usize;

        let old_start = Instant::now();
        let mut old_total = 0usize;
        for _ in 0..iterations {
            // Old behaviour: `pointer_event_target_key` walks once internally
            // to find the focusable target, then `handle_component_input`
            // walked again with a second `find_focusable_at` call to decide
            // the focus target.
            let first = find_focusable_at(black_box(&tree), x, y).or_else(|| {
                find_node_path_at(&tree, x, y).and_then(|path| {
                    path.into_iter()
                        .rev()
                        .find(|key| find_event_handler(&tree, key, "click").is_some())
                })
            });
            let second = find_focusable_at(black_box(&tree), x, y);
            old_total = old_total.wrapping_add(first.map_or(0, |k| k.len()));
            old_total = old_total.wrapping_add(second.map_or(0, |k| k.len()));
        }
        let old_time = old_start.elapsed();

        let new_start = Instant::now();
        let mut new_total = 0usize;
        for _ in 0..iterations {
            let (target, focusable) = pointer_event_target_with_focus(black_box(&tree), x, y);
            new_total = new_total.wrapping_add(target.map_or(0, |k| k.len()));
            new_total = new_total.wrapping_add(focusable.map_or(0, |k| k.len()));
        }
        let new_time = new_start.elapsed();

        eprintln!(
            "press target lookup: duplicate walk {old_time:?}; fused {new_time:?}; ratio {:.1}x; totals={old_total}/{new_total}",
            old_time.as_secs_f64() / new_time.as_secs_f64()
        );
        assert_eq!(old_total, new_total);
        assert!(new_time < old_time);
    }

    // Run with:
    // cargo test -p mesh-core-shell --release -- captured_release_key_beats_unneeded_release_hit_test --ignored --nocapture
    #[test]
    #[ignore]
    fn captured_release_key_beats_unneeded_release_hit_test() {
        use super::widgets::{captured_release_key, pointer_event_target_with_focus};

        let tree = large_positioned_tree(200, 12);
        let down_key = "root/199/11";
        let bounds = Some((440.0, 3980.0, 480.0, 4000.0));
        let (x, y) = (tree.layout.width - 5.0, tree.layout.height - 5.0);
        let iterations = 20_000usize;

        let old_start = Instant::now();
        let mut old_total = 0usize;
        for _ in 0..iterations {
            let release_key = pointer_event_target_with_focus(black_box(&tree), x, y).0;
            let released_on_same_key = release_key.as_deref() == Some(down_key);
            let released_inside_press_bounds =
                bounds.is_some_and(|bounds| super::point_in_bounds(x, y, bounds));
            let captured = (released_on_same_key || released_inside_press_bounds)
                .then_some(down_key.to_owned());
            old_total = old_total.wrapping_add(captured.map_or(0, |key| key.len()));
        }
        let old_time = old_start.elapsed();

        let new_start = Instant::now();
        let mut new_total = 0usize;
        for _ in 0..iterations {
            let captured = captured_release_key(black_box(&tree), Some(down_key), bounds, x, y);
            new_total = new_total.wrapping_add(captured.map_or(0, |key| key.len()));
        }
        let new_time = new_start.elapsed();

        eprintln!(
            "release capture: unconditional hit-test {old_time:?}; bounds short-circuit {new_time:?}; ratio {:.1}x; totals={old_total}/{new_total}",
            old_time.as_secs_f64() / new_time.as_secs_f64()
        );
        assert_eq!(old_total, new_total);
        assert!(new_time < old_time);
    }

    // Run with:
    // cargo test -p mesh-core-shell --release -- captured_release_fallback_uses_fused_press_hit --ignored --nocapture
    #[test]
    #[ignore]
    fn captured_release_fallback_uses_fused_press_hit() {
        use super::widgets::{captured_release_key, pointer_event_target_with_focus};

        let mut tree = large_positioned_tree(200, 12);
        for row in &mut tree.children {
            row.tag = "box".into();
            for cell in &mut row.children {
                cell.tag = "box".into();
            }
        }
        let down_key = "root/199/11";
        let stale_bounds = Some((0.0, 0.0, 1.0, 1.0));
        let (x, y) = (tree.layout.width - 5.0, tree.layout.height - 5.0);
        let iterations = 20_000usize;

        let old_start = Instant::now();
        let mut old_total = 0usize;
        for _ in 0..iterations {
            let release_key = pointer_event_target_with_focus(black_box(&tree), x, y).0;
            let captured =
                (release_key.as_deref() == Some(down_key)).then_some(down_key.to_owned());
            old_total = old_total.wrapping_add(captured.map_or(0, |key| key.len()));
        }
        let old_time = old_start.elapsed();

        let new_start = Instant::now();
        let mut new_total = 0usize;
        for _ in 0..iterations {
            let captured =
                captured_release_key(black_box(&tree), Some(down_key), stale_bounds, x, y);
            new_total = new_total.wrapping_add(captured.map_or(0, |key| key.len()));
        }
        let new_time = new_start.elapsed();

        eprintln!(
            "release fallback: legacy multi-walk {old_time:?}; fused press hit {new_time:?}; ratio {:.1}x; totals={old_total}/{new_total}",
            old_time.as_secs_f64() / new_time.as_secs_f64()
        );
        assert_eq!(old_total, new_total);
        assert!(new_time < old_time);
    }

    // Run with:
    // cargo test -p mesh-core-shell --release -- pressed_target_snapshot_skips_release_dispatch_lookups --ignored --nocapture
    #[test]
    #[ignore]
    fn pressed_target_snapshot_skips_release_dispatch_lookups() {
        use mesh_core_interaction::{find_node_bounds_by_key, find_node_by_key, node_is_source};

        let tree = large_positioned_tree(200, 12);
        let down_key = "root/199/11";
        let bounds = (440.0, 3980.0, 480.0, 4000.0);
        let target = find_node_by_key(&tree, down_key).expect("target node");
        let snapshot = super::widgets::PressedTargetSnapshot::from_node(down_key, target, bounds);
        let iterations = 20_000usize;

        let old_start = Instant::now();
        let mut old_total = 0usize;
        for _ in 0..iterations {
            let node = find_node_by_key(black_box(&tree), down_key).expect("target node");
            let bounds = find_node_bounds_by_key(&tree, down_key, 0.0, 0.0).expect("target bounds");
            let activation = node_is_source(
                node,
                &[
                    "menu-item",
                    "command-item",
                    "preference-row",
                    "tab",
                    "list-item",
                ],
            );
            let has_click = node.event_handlers.contains_key("click")
                || node.event_handler_calls.contains_key("click");
            old_total = old_total.wrapping_add(node.tag.len());
            old_total = old_total.wrapping_add(usize::from(bounds.2 > bounds.0));
            old_total = old_total.wrapping_add(usize::from(activation));
            old_total = old_total.wrapping_add(usize::from(has_click));
        }
        let old_time = old_start.elapsed();

        let new_start = Instant::now();
        let mut new_total = 0usize;
        for _ in 0..iterations {
            let target = black_box(&snapshot);
            new_total = new_total.wrapping_add(target.tag.len());
            new_total = new_total.wrapping_add(usize::from(target.bounds.2 > target.bounds.0));
            new_total = new_total.wrapping_add(usize::from(target.activation_item));
            new_total = new_total.wrapping_add(usize::from(target.click_handler.is_some()));
        }
        let new_time = new_start.elapsed();

        eprintln!(
            "release dispatch metadata: tree lookups {old_time:?}; press snapshot {new_time:?}; ratio {:.1}x; totals={old_total}/{new_total}",
            old_time.as_secs_f64() / new_time.as_secs_f64()
        );
        assert_eq!(old_total, new_total);
        assert!(new_time < old_time);
    }

    // Run with:
    // cargo test -p mesh-core-shell --release -- resolved_checkable_toggle_skips_key_lookup --ignored --nocapture
    #[test]
    #[ignore]
    fn resolved_checkable_toggle_skips_key_lookup() {
        use mesh_core_interaction::find_node_by_key;

        let mut tree = large_positioned_tree(200, 12);
        for row in &mut tree.children {
            for cell in &mut row.children {
                cell.tag = "checkbox".into();
                cell.attributes.insert("checked".into(), "true".into());
            }
        }
        let key = "root/199/11";
        let node = find_node_by_key(&tree, key).expect("checkbox node");
        let iterations = 200_000usize;

        let old_start = Instant::now();
        let mut old_total = 0usize;
        for _ in 0..iterations {
            let checked = find_node_by_key(black_box(&tree), key)
                .and_then(|node| node.attributes.get("checked"))
                .is_some_and(|value| matches!(value.as_str(), "true" | "1" | "checked"));
            old_total = old_total.wrapping_add(usize::from(!checked));
        }
        let old_time = old_start.elapsed();

        let new_start = Instant::now();
        let mut new_total = 0usize;
        for _ in 0..iterations {
            let checked = black_box(node)
                .attributes
                .get("checked")
                .is_some_and(|value| matches!(value.as_str(), "true" | "1" | "checked"));
            new_total = new_total.wrapping_add(usize::from(!checked));
        }
        let new_time = new_start.elapsed();

        eprintln!(
            "checkable toggle state: key lookup {old_time:?}; resolved node {new_time:?}; ratio {:.1}x; totals={old_total}/{new_total}",
            old_time.as_secs_f64() / new_time.as_secs_f64()
        );
        assert_eq!(old_total, new_total);
        assert!(new_time < old_time);
    }

    // Run with:
    // cargo test -p mesh-core-shell --release -- resolved_option_radio_activation_skips_initial_key_lookup --ignored --nocapture
    #[test]
    #[ignore]
    fn resolved_option_radio_activation_skips_initial_key_lookup() {
        use mesh_core_interaction::{find_node_by_key, node_is_source};

        let mut tree = large_positioned_tree(200, 12);
        for row in &mut tree.children {
            for cell in &mut row.children {
                cell.tag = "option".into();
                cell.attributes
                    .insert("data-mesh-element".into(), "option".into());
                cell.attributes.insert("value".into(), "choice".into());
            }
        }
        let key = "root/199/11";
        let node = find_node_by_key(&tree, key).expect("option node");
        let iterations = 200_000usize;

        let old_start = Instant::now();
        let mut old_total = 0usize;
        for _ in 0..iterations {
            let option = find_node_by_key(black_box(&tree), key).expect("option node");
            let disabled = option
                .attributes
                .get("disabled")
                .is_some_and(|value| matches!(value.as_str(), "" | "true" | "1" | "disabled"));
            let value_len = option
                .attributes
                .get("value")
                .or_else(|| option.attributes.get("label"))
                .map_or(0, String::len);
            old_total = old_total.wrapping_add(usize::from(node_is_source(option, &["option"])));
            old_total = old_total.wrapping_add(usize::from(disabled));
            old_total = old_total.wrapping_add(value_len);
        }
        let old_time = old_start.elapsed();

        let new_start = Instant::now();
        let mut new_total = 0usize;
        for _ in 0..iterations {
            let option = black_box(node);
            let disabled = option
                .attributes
                .get("disabled")
                .is_some_and(|value| matches!(value.as_str(), "" | "true" | "1" | "disabled"));
            let value_len = option
                .attributes
                .get("value")
                .or_else(|| option.attributes.get("label"))
                .map_or(0, String::len);
            new_total = new_total.wrapping_add(usize::from(node_is_source(option, &["option"])));
            new_total = new_total.wrapping_add(usize::from(disabled));
            new_total = new_total.wrapping_add(value_len);
        }
        let new_time = new_start.elapsed();

        eprintln!(
            "option/radio activation target state: key lookup {old_time:?}; resolved node {new_time:?}; ratio {:.1}x; totals={old_total}/{new_total}",
            old_time.as_secs_f64() / new_time.as_secs_f64()
        );
        assert_eq!(old_total, new_total);
        assert!(new_time < old_time);
    }
}
