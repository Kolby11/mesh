use std::collections::{HashMap, HashSet};

use mesh_core_elements::{ElementState, WidgetNode, element_snapshot_json};

#[derive(Debug, Clone, Copy, Default)]
pub(in crate::shell) struct ScrollOffsetState {
    pub(in crate::shell) x: f32,
    pub(in crate::shell) y: f32,
}

/// Collect every `_mesh_key` present in the fully built and restyled widget tree.
/// Used by `FrontendSurfaceComponent::prune_stale_interaction_targets` to determine
/// which interaction targets are still valid after a restyle.
pub(super) fn collect_all_keys(node: &WidgetNode, keys: &mut HashSet<String>) {
    if let Some(key) = node.attributes.get("_mesh_key") {
        keys.insert(key.clone());
    }
    for child in &node.children {
        collect_all_keys(child, keys);
    }
}

pub(super) fn input_accepts_char(node: &WidgetNode, ch: char) -> bool {
    if ch.is_control() {
        return false;
    }

    match node.attributes.get("type").map(|value| value.as_str()) {
        Some("number") => ch.is_ascii_digit() || matches!(ch, '.' | '-'),
        _ => true,
    }
}

pub(super) fn collect_element_metrics(
    node: &WidgetNode,
    offset_x: f32,
    offset_y: f32,
    elements: &mut serde_json::Map<String, serde_json::Value>,
    refs: &mut serde_json::Map<String, serde_json::Value>,
) {
    let metrics = element_snapshot_json(node, offset_x, offset_y);
    let scroll_x = metrics
        .get("scroll_x")
        .and_then(serde_json::Value::as_f64)
        .unwrap_or(0.0) as f32;
    let scroll_y = metrics
        .get("scroll_y")
        .and_then(serde_json::Value::as_f64)
        .unwrap_or(0.0) as f32;

    if let Some(key) = node.attributes.get("_mesh_key") {
        elements.insert(key.clone(), metrics.clone());
    }
    if let Some(id) = node.attributes.get("id") {
        refs.insert(id.clone(), metrics.clone());
    }
    if let Some(reference) = node.attributes.get("ref") {
        refs.insert(reference.clone(), metrics);
    }

    let child_offset_x = offset_x - scroll_x;
    let child_offset_y = offset_y - scroll_y;
    for child in &node.children {
        collect_element_metrics(child, child_offset_x, child_offset_y, elements, refs);
    }
}

pub(super) fn annotate_runtime_tree(
    node: &mut WidgetNode,
    key: String,
    focused_key: &Option<String>,
    hovered_path: &[String],
    active_key: &Option<String>,
    input_values: &HashMap<String, String>,
    slider_values: &HashMap<String, f32>,
    checked_values: &HashMap<String, bool>,
    scroll_offsets: &HashMap<String, ScrollOffsetState>,
) {
    node.attributes.insert("_mesh_key".into(), key.clone());

    let key_str = key.as_str();
    let disabled = node
        .attributes
        .get("disabled")
        .is_some_and(|value| truthy_attribute(value))
        || node
            .attributes
            .get("aria-disabled")
            .is_some_and(|value| truthy_attribute(value));
    let checked = checked_values
        .get(&key)
        .copied()
        .or_else(|| {
            node.attributes
                .get("checked")
                .map(|value| matches!(value.as_str(), "true" | "1" | "checked"))
        })
        .unwrap_or(false);

    node.state = ElementState {
        focused: focused_key.as_deref() == Some(key_str),
        hovered: hovered_path
            .iter()
            .any(|hovered_key| hovered_key == key_str),
        active: active_key.as_deref() == Some(key_str),
        disabled,
        checked,
    };
    if node.state.hovered {
        tracing::trace!(
            "[hover] annotate: key={key} tag={} set hovered=true",
            node.tag
        );
    }

    if node.state.focused {
        node.attributes
            .insert("_mesh_focused".into(), "true".into());
    }

    match node.tag.as_str() {
        "input" => {
            let value = input_values
                .get(&key)
                .cloned()
                .or_else(|| node.attributes.get("value").cloned())
                .unwrap_or_default();
            node.attributes.insert("value".into(), value);
        }
        "slider" => {
            let value = slider_values
                .get(&key)
                .copied()
                .or_else(|| {
                    node.attributes
                        .get("value")
                        .and_then(|value: &String| value.parse::<f32>().ok())
                })
                .unwrap_or(0.0);
            node.attributes
                .insert("value".into(), format!("{value:.2}"));
        }
        "switch" | "checkbox" => {
            node.attributes.insert(
                "checked".into(),
                if checked { "true" } else { "false" }.into(),
            );
        }
        _ => {}
    }

    let offset = scroll_offsets.get(&key).copied().unwrap_or_default();
    node.attributes
        .insert("_mesh_scroll_x".into(), format!("{:.2}", offset.x));
    node.attributes
        .insert("_mesh_scroll_y".into(), format!("{:.2}", offset.y));

    for (index, child) in node.children.iter_mut().enumerate() {
        annotate_runtime_tree(
            child,
            format!("{key}/{index}"),
            focused_key,
            hovered_path,
            active_key,
            input_values,
            slider_values,
            checked_values,
            scroll_offsets,
        );
    }
}

fn truthy_attribute(value: &str) -> bool {
    matches!(value, "" | "true" | "1" | "disabled" | "checked")
}
