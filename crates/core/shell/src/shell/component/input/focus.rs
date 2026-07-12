use super::super::*;
use mesh_core_elements::style::{Overflow, TextAlign, TextDirection, TextOverflow};

impl FrontendSurfaceComponent {
    pub(in crate::shell::component) fn set_focus_target(
        &mut self,
        tree: &WidgetNode,
        next_key: Option<String>,
        focus_visible: bool,
    ) -> Result<Vec<CoreRequest>, ComponentError> {
        let previous_key = self.focused_key.clone();
        let mut requests = Vec::new();

        if previous_key != next_key {
            self.keyboard_button_press_activations.clear();
            if let Some(previous_key) = previous_key.as_deref() {
                let event = self.build_focus_event(tree, previous_key, "blur");
                requests.extend(self.call_node_handler(tree, previous_key, "blur", &[event])?);
            }
            self.focused_key = next_key.clone();
            if let Some(next_key) = next_key.as_deref() {
                let event = self.build_focus_event(tree, next_key, "focus");
                requests.extend(self.call_node_handler(tree, next_key, "focus", &[event])?);
            }
        }

        self.focus_visible_key = if focus_visible { next_key } else { None };
        Ok(requests)
    }

    pub(super) fn advance_keyboard_focus(
        &mut self,
        tree: &WidgetNode,
        backward: bool,
    ) -> Result<Vec<CoreRequest>, ComponentError> {
        let next_key = next_focus_target(tree, self.focused_key.as_deref(), backward);
        self.set_focus_target(tree, next_key, true)
    }

    pub(super) fn pointer_focus_visible_for_key(&self, tree: &WidgetNode, key: &str) -> bool {
        // Native focusable widgets that benefit from a focus indicator after
        // a pointer click: input (typing), slider (arrow keys), checkbox /
        // switch (Space toggle). Buttons activate immediately on click so a
        // focus ring is noise rather than help.
        find_node_by_key(tree, key).is_some_and(|node| {
            matches!(node.tag.as_str(), "input" | "slider")
                || node_is_source(
                    node,
                    &[
                        "select",
                        "option",
                        "checkbox",
                        "switch",
                        "radio",
                        "menu",
                        "menu-item",
                        "command-item",
                        "preference-row",
                        "tab",
                        "list-item",
                    ],
                )
        })
    }

    /// Tab handling that knows about cross-surface focus transfer:
    /// - If the focused element declares `popover_target="X"` and X is a
    ///   visible surface, forward Tab transfers focus into X's first
    ///   tabbable and records the trigger as the return target. Shift+Tab
    ///   stays in-surface (you Tab *out* via end-of-chain wrap, not by
    ///   shifting back from the trigger).
    /// - If this surface has a `return_focus` set (it was entered via
    ///   transfer) and the in-surface Tab would wrap around, transfer
    ///   focus back to the return target instead and close this surface.
    /// - Otherwise: ordinary in-surface advance.
    pub(super) fn handle_tab_with_cross_surface(
        &mut self,
        tree: &WidgetNode,
        backward: bool,
    ) -> Result<Vec<CoreRequest>, ComponentError> {
        let traversal = collect_focus_traversal(tree);
        let current = self.focused_key.clone();

        // Forward Tab on a key that triggered an open popover → transfer
        // into that popover. The mapping is registered when the click
        // handler called `mesh.popover.activate(...)`; normally activation
        // focuses immediately, but this still supports focus=false.
        if !backward {
            if let Some(focused) = current.as_deref() {
                if let Some(target) = self.triggered_popovers.get(focused).cloned() {
                    return Ok(vec![CoreRequest::TransferTabFocus {
                        from_surface: self.surface_id().to_string(),
                        to_surface: target,
                        target: TabFocusTarget::First,
                        return_target: Some((self.surface_id().to_string(), focused.to_string())),
                        target_closes_on_leave: true,
                        close_source: None,
                    }]);
                }
            }
        }

        // End-of-chain wrap inside a popover with a return target → transfer back + close.
        if let Some(return_focus) = self.return_focus.clone() {
            if !traversal.is_empty() {
                let at_boundary = match (current.as_deref(), backward) {
                    (Some(key), false) => traversal.last().map(String::as_str) == Some(key),
                    (Some(key), true) => traversal.first().map(String::as_str) == Some(key),
                    (None, _) => false,
                };
                if at_boundary {
                    let (return_surface, return_key) = return_focus;
                    let target = if backward {
                        // Shift+Tab leaves a popover by landing on the
                        // trigger itself, so the user can shift+tab
                        // again to keep going backward through the
                        // navbar. Forward Tab skips past the trigger.
                        TabFocusTarget::AtKey(return_key)
                    } else {
                        TabFocusTarget::AfterKey(return_key)
                    };
                    let close = if self.close_on_focus_leave {
                        Some(self.surface_id().to_string())
                    } else {
                        None
                    };
                    return Ok(vec![CoreRequest::TransferTabFocus {
                        from_surface: self.surface_id().to_string(),
                        to_surface: return_surface,
                        target,
                        return_target: None,
                        target_closes_on_leave: false,
                        close_source: close,
                    }]);
                }
            }
        }

        self.advance_keyboard_focus(tree, backward)
    }

    /// Apply a focus transfer received from the shell (cross-surface Tab).
    /// `target` selects which key in this surface receives focus; the
    /// `return_focus`/`close_on_focus_leave` flags govern what happens on
    /// the next Tab/Shift+Tab leave. Called from the shell's
    /// `TransferTabFocus` request handler. The component must already have
    /// painted at least once (otherwise tree traversal is empty); for
    /// freshly-shown surfaces, the shell pairs this with a paint pass.
    pub(in crate::shell::component) fn apply_focus_transfer(
        &mut self,
        tree: &WidgetNode,
        target: &super::super::TabFocusTarget,
        return_focus: Option<(String, String)>,
        close_on_focus_leave: bool,
    ) {
        let traversal = collect_focus_traversal(tree);
        self.apply_focus_transfer_from_traversal(
            &traversal,
            target,
            return_focus,
            close_on_focus_leave,
        );
    }

    pub(in crate::shell::component) fn apply_focus_transfer_from_traversal(
        &mut self,
        traversal: &[String],
        target: &super::super::TabFocusTarget,
        return_focus: Option<(String, String)>,
        close_on_focus_leave: bool,
    ) {
        let new_key = match target {
            super::super::TabFocusTarget::First => traversal.first().cloned(),
            super::super::TabFocusTarget::Last => traversal.last().cloned(),
            super::super::TabFocusTarget::AfterKey(anchor) => {
                let pos = traversal.iter().position(|k| k == anchor);
                match pos {
                    // Wrap to first if anchor was the last entry.
                    Some(i) if i + 1 < traversal.len() => Some(traversal[i + 1].clone()),
                    Some(_) => traversal.first().cloned(),
                    None => traversal.first().cloned(),
                }
            }
            super::super::TabFocusTarget::AtKey(anchor) => {
                if traversal.iter().any(|k| k == anchor) {
                    Some(anchor.clone())
                } else {
                    traversal.first().cloned()
                }
            }
        };
        if let Some(key) = new_key {
            self.focused_key = Some(key.clone());
            self.focus_visible_key = Some(key);
            self.invalidate_interaction_restyle();
        }
        self.return_focus = return_focus;
        self.close_on_focus_leave = close_on_focus_leave;
    }

    /// Clear focus state on a surface that just lost focus to another
    /// surface. Called from the shell's `TransferTabFocus` handler on the
    /// source side.
    pub(in crate::shell::component) fn clear_focus_for_transfer(&mut self) {
        self.focused_key = None;
        self.focus_visible_key = None;
        self.invalidate_interaction_restyle();
    }

    /// Escape inside a surface with a recorded `return_focus` (i.e. a
    /// popover entered via Tab from another surface) closes the popover
    /// and lands focus back on the trigger element. Returns `Ok(None)` for
    /// surfaces that aren't popovers — the caller falls back to whatever
    /// the script-defined Escape handler does.
    pub(super) fn handle_escape_with_cross_surface(
        &mut self,
    ) -> Result<Option<Vec<CoreRequest>>, ComponentError> {
        let Some((return_surface, return_key)) = self.return_focus.clone() else {
            return Ok(None);
        };
        let close = if self.close_on_focus_leave {
            Some(self.surface_id().to_string())
        } else {
            None
        };
        Ok(Some(vec![CoreRequest::TransferTabFocus {
            from_surface: self.surface_id().to_string(),
            to_surface: return_surface,
            target: TabFocusTarget::AtKey(return_key),
            return_target: None,
            target_closes_on_leave: false,
            close_source: close,
        }]))
    }

    /// If `pending_auto_focus` is set (a popover just became visible), seed
    /// `focused_key` with the first tabbable element so keyboard works
    /// immediately, with no need to click into the surface first. No-op if
    /// nothing is tabbable or if focus was already established (e.g. the
    /// user clicked something in the same paint frame).
    pub(in crate::shell::component) fn apply_pending_auto_focus(&mut self, tree: &WidgetNode) {
        if !self.pending_auto_focus {
            return;
        }
        self.pending_auto_focus = false;
        if self.focused_key.is_some() {
            return;
        }
        if let Some(first) = next_focus_target(tree, None, false) {
            self.focused_key = Some(first.clone());
            self.focus_visible_key = Some(first);
        }
    }

    pub(super) fn normalized_focused_key(&mut self, tree: &WidgetNode) -> Option<String> {
        let focused_key = self.focused_key.clone()?;
        if find_node_by_key(tree, &focused_key).is_some() {
            Some(focused_key)
        } else {
            self.focused_key = None;
            self.focus_visible_key = None;
            None
        }
    }

    pub(super) fn selection_copy_payload(&self, tree: &WidgetNode) -> Option<String> {
        let selection = self.selection.as_ref()?;
        let node = find_node_by_key(tree, &selection.anchor.node_key)?;
        if node.tag != "text" {
            return None;
        }

        let text = node
            .attributes
            .get("text")
            .map(String::as_str)
            .or_else(|| node.attributes.get("content").map(String::as_str))
            .unwrap_or("");
        if text.is_empty() {
            return None;
        }

        let style = &node.computed_style;
        if style.text_overflow == TextOverflow::Ellipsis
            || style.overflow_x != Overflow::Visible
            || style.overflow_y != Overflow::Visible
        {
            return None;
        }

        let inner_width = (node.layout.width - style.padding.horizontal()).max(0.0);
        if inner_width <= 0.0 {
            return None;
        }

        let text_align =
            if style.text_direction == TextDirection::Rtl && style.text_align == TextAlign::Left {
                TextAlign::Right
            } else {
                style.text_align
            };
        let text_x = node.layout.x + style.padding.left;
        let text_y = node.layout.y + style.padding.top;
        let geometry = TextRenderer::new().selection_geometry(
            text,
            &style.font_family,
            style.font_size,
            style.font_weight,
            style.line_height,
            text_align,
            Some(inner_width),
            (selection.anchor.x - text_x, selection.anchor.y - text_y),
            (selection.focus.x - text_x, selection.focus.y - text_y),
        )?;

        if geometry.selected_text.is_empty() {
            return None;
        }

        Some(geometry.selected_text)
    }
}

/// Selectable-text press target at a point, or `None` if the point is over
/// an interactive element (buttons, inputs, menu items, anything with a
/// click handler, ...) or not over selectable text at all.
///
/// `find_node_path_at` returns a depth-many key path; resolving each key with
/// a separate `find_node_by_key` call re-walks the whole tree per key
/// (O(depth × tree)). `find_nodes_by_keys` resolves the whole path in one
/// traversal instead.
pub(super) fn selectable_text_target_key(tree: &WidgetNode, x: f32, y: f32) -> Option<String> {
    let path = find_node_path_at(tree, x, y)?;
    let keys: std::collections::HashSet<&str> = path.iter().map(String::as_str).collect();
    let nodes = find_nodes_by_keys(tree, &keys);

    let is_interactive = path.iter().any(|key| {
        nodes.get(key).is_some_and(|(node, _)| {
            matches!(node.tag.as_str(), "button" | "slider" | "input")
                || node_is_source(
                    node,
                    &[
                        "select",
                        "option",
                        "switch",
                        "checkbox",
                        "radio",
                        "menu",
                        "menu-item",
                        "command-item",
                        "preference-row",
                        "tab",
                        "list-item",
                    ],
                )
                || node.event_handlers.contains_key("click")
        })
    });
    if is_interactive {
        return None;
    }

    path.into_iter().rev().find(|key| {
        nodes.get(key.as_str()).is_some_and(|(node, _)| {
            node.tag == "text"
                && node
                    .attributes
                    .get("selectable")
                    .is_some_and(|value| matches!(value.as_str(), "" | "true" | "1"))
        })
    })
}

#[cfg(test)]
mod selectable_text_target_key_tests {
    use super::*;
    use std::hint::black_box;
    use std::time::Instant;

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
    fn finds_selectable_text_with_no_interactive_ancestors() {
        let mut text = positioned_node("root/0", "text", 0.0, 0.0, 40.0, 20.0);
        text.attributes.insert("selectable".into(), "true".into());
        let mut root = positioned_node("root", "row", 0.0, 0.0, 40.0, 20.0);
        root.children.push(text);

        assert_eq!(
            selectable_text_target_key(&root, 10.0, 10.0).as_deref(),
            Some("root/0")
        );
    }

    #[test]
    fn returns_none_when_an_ancestor_is_interactive() {
        let mut text = positioned_node("root/0/0", "text", 0.0, 0.0, 40.0, 20.0);
        text.attributes.insert("selectable".into(), "true".into());
        let mut button = positioned_node("root/0", "button", 0.0, 0.0, 40.0, 20.0);
        button.children.push(text);
        let mut root = positioned_node("root", "row", 0.0, 0.0, 40.0, 20.0);
        root.children.push(button);

        assert_eq!(selectable_text_target_key(&root, 10.0, 10.0), None);
    }

    #[test]
    fn returns_none_for_non_selectable_text() {
        let text = positioned_node("root/0", "text", 0.0, 0.0, 40.0, 20.0);
        let mut root = positioned_node("root", "row", 0.0, 0.0, 40.0, 20.0);
        root.children.push(text);

        assert_eq!(selectable_text_target_key(&root, 10.0, 10.0), None);
    }

    fn chain_leaf(depth: usize) -> WidgetNode {
        if depth == 0 {
            let mut text = positioned_node("chain/leaf", "text", 0.0, 0.0, 40.0, 20.0);
            text.attributes.insert("selectable".into(), "true".into());
            text
        } else {
            let mut node = positioned_node(&format!("chain/{depth}"), "box", 0.0, 0.0, 40.0, 20.0);
            node.children.push(chain_leaf(depth - 1));
            node
        }
    }

    // A deep single-child chain (models path length) plus wide unrelated
    // padding subtrees off the root (models total tree size), so a
    // per-path-key `find_node_by_key` walk pays for the whole tree on every
    // one of the `depth` keys.
    fn chain_tree_with_padding(depth: usize, padding: usize) -> WidgetNode {
        let mut root = positioned_node("root", "column", 0.0, 0.0, 40.0, 20.0);
        for index in 0..padding {
            root.children.push(positioned_node(
                &format!("pad/{index}"),
                "box",
                1000.0,
                1000.0,
                10.0,
                10.0,
            ));
        }
        root.children.push(chain_leaf(depth));
        root
    }

    fn old_selectable_text_target_key(tree: &WidgetNode, x: f32, y: f32) -> Option<String> {
        let path = find_node_path_at(tree, x, y)?;
        if path.iter().any(|key| {
            find_node_by_key(tree, key).is_some_and(|node| {
                matches!(node.tag.as_str(), "button" | "slider" | "input")
                    || node_is_source(
                        node,
                        &[
                            "select",
                            "option",
                            "switch",
                            "checkbox",
                            "radio",
                            "menu",
                            "menu-item",
                            "command-item",
                            "preference-row",
                            "tab",
                            "list-item",
                        ],
                    )
                    || node.event_handlers.contains_key("click")
            })
        }) {
            return None;
        }

        path.into_iter().rev().find(|key| {
            find_node_by_key(tree, key).is_some_and(|node| {
                node.tag == "text"
                    && node
                        .attributes
                        .get("selectable")
                        .is_some_and(|value| matches!(value.as_str(), "" | "true" | "1"))
            })
        })
    }

    #[test]
    fn fused_lookup_matches_old_per_key_walk_on_deep_chain() {
        let tree = chain_tree_with_padding(40, 200);
        assert_eq!(
            selectable_text_target_key(&tree, 10.0, 10.0),
            old_selectable_text_target_key(&tree, 10.0, 10.0)
        );
        assert_eq!(
            selectable_text_target_key(&tree, 10.0, 10.0).as_deref(),
            Some("chain/leaf")
        );
    }

    // Run with:
    // cargo test -p mesh-core-shell --release -- fused_selectable_text_lookup_beats_per_key_tree_walk --ignored --nocapture
    #[test]
    #[ignore]
    fn fused_selectable_text_lookup_beats_per_key_tree_walk() {
        let tree = chain_tree_with_padding(40, 200);
        let iterations = 2_000usize;

        let old_start = Instant::now();
        let mut old_total = 0usize;
        for _ in 0..iterations {
            let key = old_selectable_text_target_key(black_box(&tree), 10.0, 10.0);
            old_total = old_total.wrapping_add(key.map_or(0, |k| k.len()));
        }
        let old_time = old_start.elapsed();

        let new_start = Instant::now();
        let mut new_total = 0usize;
        for _ in 0..iterations {
            let key = selectable_text_target_key(black_box(&tree), 10.0, 10.0);
            new_total = new_total.wrapping_add(key.map_or(0, |k| k.len()));
        }
        let new_time = new_start.elapsed();

        eprintln!(
            "selectable text target: per-key walk {old_time:?}; fused {new_time:?}; ratio {:.1}x; totals={old_total}/{new_total}",
            old_time.as_secs_f64() / new_time.as_secs_f64()
        );
        assert_eq!(old_total, new_total);
        assert!(new_time < old_time);
    }
}
