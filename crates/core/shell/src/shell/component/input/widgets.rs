use super::super::*;

impl FrontendSurfaceComponent {
    pub(super) fn dispatch_text_input_value_handlers(
        &mut self,
        tree: &WidgetNode,
        input_key: &str,
        value: &str,
    ) -> Result<Vec<CoreRequest>, ComponentError> {
        let payload = serde_json::json!(value);
        let mut requests = self.call_node_handler(tree, input_key, "input", &[payload.clone()])?;
        requests.extend(self.call_node_handler(tree, input_key, "change", &[payload])?);
        Ok(requests)
    }

    pub(super) fn preserve_slider_value(
        &mut self,
        tree: &WidgetNode,
        slider_key: &str,
        value: f32,
    ) {
        if let Some(script_value) = find_node_by_key(tree, slider_key).and_then(|node| {
            node.attributes
                .get("value")
                .and_then(|value| value.parse::<f32>().ok())
        }) {
            self.slider_script_values
                .insert(slider_key.to_string(), script_value);
        }
        self.slider_values.insert(slider_key.to_string(), value);
    }

    pub(super) fn update_slider_from_position(
        &mut self,
        tree: &WidgetNode,
        slider_key: &str,
        x: f32,
        y: f32,
    ) {
        let Some(node) = find_node_by_key(tree, slider_key) else {
            return;
        };
        let is_vertical = node
            .attributes
            .get("orient")
            .map(|v| v == "vertical")
            .unwrap_or(false);
        let Some((left, top, right, bottom)) = find_node_bounds_by_key(tree, slider_key, 0.0, 0.0)
        else {
            return;
        };

        let min = node
            .attributes
            .get("min")
            .and_then(|value: &String| value.parse::<f32>().ok())
            .unwrap_or(0.0);
        let max = node
            .attributes
            .get("max")
            .and_then(|value: &String| value.parse::<f32>().ok())
            .unwrap_or(100.0);

        if max <= min {
            return;
        }

        let pct = if is_vertical {
            // Vertical: top = 100%, bottom = 0% (inverted Y axis).
            let height = (bottom - top).max(1.0);
            let local_y = (y - top).clamp(0.0, height);
            1.0 - (local_y / height).clamp(0.0, 1.0)
        } else {
            let width = (right - left).max(1.0);
            let local_x = (x - left).clamp(0.0, width);
            (local_x / width).clamp(0.0, 1.0)
        };
        let value = min + (max - min) * pct;
        self.preserve_slider_value(tree, slider_key, value);
    }

    pub(super) fn slider_value(&self, tree: &WidgetNode, slider_key: &str) -> Option<f32> {
        self.slider_values.get(slider_key).copied().or_else(|| {
            find_node_by_key(tree, slider_key).and_then(|node| {
                node.attributes
                    .get("value")
                    .and_then(|value| value.parse::<f32>().ok())
            })
        })
    }

    pub(super) fn current_checked_value(&self, tree: &WidgetNode, key: &str) -> bool {
        self.checked_values.get(key).copied().unwrap_or_else(|| {
            find_node_by_key(tree, key)
                .and_then(|node| node.attributes.get("checked"))
                .is_some_and(|value| matches!(value.as_str(), "true" | "1" | "checked"))
        })
    }

    pub(super) fn toggle_checked_value(&mut self, tree: &WidgetNode, key: &str) -> bool {
        let next = !self.current_checked_value(tree, key);
        self.checked_values.insert(key.to_string(), next);
        next
    }

    pub(super) fn pointer_event_target_key(
        &self,
        tree: &WidgetNode,
        x: f32,
        y: f32,
    ) -> Option<String> {
        find_focusable_at(tree, x, y).or_else(|| {
            find_node_path_at(tree, x, y).and_then(|path| {
                path.into_iter()
                    .rev()
                    .find(|key| find_event_handler(tree, key, "click").is_some())
            })
        })
    }

    pub(super) fn build_click_event(
        &self,
        tree: &WidgetNode,
        node_key: &str,
        x: f32,
        y: f32,
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
            "type": "click",
            "pointer": {
                "x": x,
                "y": y,
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
            "current_target": current_target
        })
    }

    pub(super) fn slider_step_value(
        &self,
        tree: &WidgetNode,
        slider_key: &str,
        delta: f32,
    ) -> Option<f32> {
        let node = find_node_by_key(tree, slider_key)?;
        let min = node
            .attributes
            .get("min")
            .and_then(|value| value.parse::<f32>().ok())
            .unwrap_or(0.0);
        let max = node
            .attributes
            .get("max")
            .and_then(|value| value.parse::<f32>().ok())
            .unwrap_or(100.0);
        if max <= min {
            return None;
        }

        let current = self
            .slider_value(tree, slider_key)
            .unwrap_or(min)
            .clamp(min, max);
        let step = node
            .attributes
            .get("step")
            .and_then(|value| value.parse::<f32>().ok())
            .filter(|value| *value > 0.0)
            .unwrap_or_else(|| ((max - min) / 20.0).max(0.01));
        let raw_next = (current + delta * step).clamp(min, max);
        let stepped = (((raw_next - min) / step).round() * step + min).clamp(min, max);
        Some(stepped)
    }
}
