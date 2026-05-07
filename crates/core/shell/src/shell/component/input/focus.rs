use super::super::*;
use mesh_core_elements::style::{Overflow, TextAlign, TextDirection, TextOverflow};

impl FrontendSurfaceComponent {
    pub(super) fn set_focus_target(
        &mut self,
        tree: &WidgetNode,
        next_key: Option<String>,
        focus_visible: bool,
    ) -> Result<Vec<CoreRequest>, ComponentError> {
        let previous_key = self.focused_key.clone();
        let mut requests = Vec::new();

        if previous_key != next_key {
            if let Some(previous_key) = previous_key.as_deref() {
                requests.extend(self.call_node_handler(tree, previous_key, "blur", &[])?);
            }
            self.focused_key = next_key.clone();
            if let Some(next_key) = next_key.as_deref() {
                requests.extend(self.call_node_handler(tree, next_key, "focus", &[])?);
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
        find_node_by_key(tree, key).is_some_and(|node| node.tag == "input")
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

    pub(super) fn selectable_text_target_key(
        &self,
        tree: &WidgetNode,
        x: f32,
        y: f32,
    ) -> Option<String> {
        let path = find_node_path_at(tree, x, y)?;
        if path.iter().any(|key| {
            find_node_by_key(tree, key).is_some_and(|node| {
                matches!(
                    node.tag.as_str(),
                    "button" | "slider" | "switch" | "checkbox" | "input"
                ) || node.event_handlers.contains_key("click")
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
