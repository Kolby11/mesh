use super::*;

impl FrontendSurfaceComponent {
    pub(super) fn clear_selection(&mut self) {
        self.selection = None;
    }

    pub(super) fn begin_text_selection(&mut self, node_key: String, x: f32, y: f32) {
        let point = TextSelectionPoint { node_key, x, y };
        self.selection = Some(TextSelectionState {
            anchor: point.clone(),
            focus: point,
            dragging: true,
        });
    }

    pub(super) fn end_text_selection_drag(&mut self) {
        if let Some(selection) = self.selection.as_mut() {
            selection.dragging = false;
        }
    }

    pub(super) fn update_text_selection_focus(&mut self, x: f32, y: f32) {
        let Some(selection) = self.selection.as_mut() else {
            return;
        };
        if !selection.dragging {
            return;
        }
        selection.focus.x = x;
        selection.focus.y = y;
        selection.focus.node_key = selection.anchor.node_key.clone();
    }

    pub(super) fn clear_runtime_dirty_states(&self) {
        for runtime in self.runtimes.lock().unwrap().values_mut() {
            runtime.script_ctx.state_mut().clear_dirty();
        }
    }

    pub(super) fn publish_element_metrics(&self, tree: &WidgetNode) {
        let mut elements = serde_json::Map::new();
        let mut refs = serde_json::Map::new();
        collect_element_metrics(tree, 0.0, 0.0, &mut elements, &mut refs);

        if let Some(root_runtime) = self.runtimes.lock().unwrap().get_mut(self.id()) {
            let state = root_runtime.script_ctx.state_mut();
            state.set_host_value("elements", serde_json::Value::Object(elements));
            state.set_host_value("refs", serde_json::Value::Object(refs));
        }
    }

    /// Remove hover, focus, and active targets that no longer exist in the final
    /// post-restyle tree. Call this after every `build_tree` to ensure that nodes
    /// removed by a conditional render or hidden by `display:none` no longer
    /// receive interaction styling on the next frame.
    ///
    /// Sibling and ancestor state is preserved: only targets whose exact key is
    /// absent from `valid_keys` are cleared. Input, slider, checked, and scroll
    /// maps are never pruned here — their cleanup rules are deliberate and covered
    /// by separate logic when elements are explicitly removed by the user.
    pub(super) fn prune_stale_interaction_targets(&mut self, tree: &WidgetNode) {
        let mut valid_keys = std::collections::HashSet::new();
        collect_all_keys(tree, &mut valid_keys);

        if let Some(key) = &self.focused_key {
            if !valid_keys.contains(key) {
                self.focused_key = None;
                self.focus_visible_key = None;
            }
        }

        if let Some(key) = &self.focus_visible_key
            && !valid_keys.contains(key)
        {
            self.focus_visible_key = None;
        }

        if let Some(key) = &self.hovered_key {
            if !valid_keys.contains(key) {
                self.hovered_key = None;
                self.hovered_path.clear();
                self.hover_start = None;
                self.tooltip_visible = false;
                self.hovered_element_bounds = None;
                self.tooltip_appeared_at = None;
            }
        }

        if let Some(key) = &self.pointer_down_key {
            if !valid_keys.contains(key) {
                self.pointer_down_key = None;
                self.pointer_down_bounds = None;
            }
        }

        if let Some(key) = &self.active_slider_key {
            if !valid_keys.contains(key) {
                self.active_slider_key = None;
            }
        }

        let should_clear_selection = self
            .selection
            .as_ref()
            .is_some_and(|selection| !valid_keys.contains(&selection.anchor.node_key));
        if should_clear_selection {
            self.selection = None;
        }
    }
}
