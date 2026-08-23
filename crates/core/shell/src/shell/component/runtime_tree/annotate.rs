use super::*;
use crate::shell::component::TextPreeditState;

pub(in crate::shell::component) fn input_accepts_char(node: &WidgetNode, ch: char) -> bool {
    if ch.is_control() {
        return false;
    }

    match node.attributes.get("type").map(|value| value.as_str()) {
        Some("number") => ch.is_ascii_digit() || matches!(ch, '.' | '-'),
        _ => true,
    }
}

pub(in crate::shell::component) fn collect_element_metrics(
    node: &WidgetNode,
    offset_x: f32,
    offset_y: f32,
    collect_elements: bool,
    collect_refs: bool,
    elements: &mut serde_json::Map<String, serde_json::Value>,
    refs: &mut serde_json::Map<String, serde_json::Value>,
    ref_keys: &mut HashMap<String, String>,
) {
    let node_key = node.mesh_key();
    let id = collect_refs.then(|| node.attributes.get("id")).flatten();
    let reference = collect_refs.then(|| node.attributes.get("ref")).flatten();
    let binding = collect_refs
        .then(|| node.attributes.get("_mesh_bind_this"))
        .flatten();
    let publishes_element = collect_elements && node_key.is_some();
    let publishes_ref = id.is_some() || reference.is_some() || binding.is_some();

    let mut metrics = (publishes_element || publishes_ref)
        .then(|| element_snapshot_json(node, offset_x, offset_y));
    let mut remaining_publications = usize::from(publishes_element)
        + usize::from(id.is_some())
        + usize::from(reference.is_some())
        + usize::from(binding.is_some());

    if collect_elements && let (Some(key), Some(_)) = (node_key, metrics.as_ref()) {
        elements.insert(
            key.to_owned(),
            clone_or_take_last_metric(&mut metrics, &mut remaining_publications),
        );
    }
    // Map each `refs.<name>` to the node's runtime key so imperative element
    // actions (focus/blur/…) can resolve a name back to the live widget node.
    if collect_refs && metrics.is_some() {
        if let Some(id) = id {
            refs.insert(
                id.clone(),
                clone_or_take_last_metric(&mut metrics, &mut remaining_publications),
            );
            if let Some(key) = node_key {
                ref_keys.insert(id.clone(), key.to_owned());
            }
        }
        if let Some(reference) = reference {
            refs.insert(
                reference.clone(),
                clone_or_take_last_metric(&mut metrics, &mut remaining_publications),
            );
            if let Some(key) = node_key {
                ref_keys.insert(reference.clone(), key.to_owned());
            }
        }
        if let Some(binding) = binding {
            refs.insert(
                binding.clone(),
                clone_or_take_last_metric(&mut metrics, &mut remaining_publications),
            );
            if let Some(key) = node_key {
                ref_keys.insert(binding.clone(), key.to_owned());
            }
        }
    }

    let scroll = node.resolved_scroll_metrics();
    let scroll_x = scroll.x;
    let scroll_y = scroll.y;
    let child_offset_x = offset_x - scroll_x;
    let child_offset_y = offset_y - scroll_y;
    for child in &node.children {
        collect_element_metrics(
            child,
            child_offset_x,
            child_offset_y,
            collect_elements,
            collect_refs,
            elements,
            refs,
            ref_keys,
        );
    }
}

pub(super) fn clone_or_take_last_metric(
    metrics: &mut Option<serde_json::Value>,
    remaining_publications: &mut usize,
) -> serde_json::Value {
    debug_assert!(*remaining_publications > 0);
    *remaining_publications -= 1;
    if *remaining_publications == 0 {
        metrics.take().expect("metric exists for final publication")
    } else {
        metrics
            .as_ref()
            .expect("metric exists for shared publication")
            .clone()
    }
}

pub(in crate::shell::component) struct RuntimeAnnotationContext<'a> {
    pub(super) focused_id: Option<NodeId>,
    pub(super) focus_visible_id: Option<NodeId>,
    pub(super) hovered_ids: HashSet<NodeId>,
    pub(super) active_id: Option<NodeId>,
    pub(super) active_slider_id: Option<NodeId>,
    pub(super) input_values: &'a HashMap<NodeId, String>,
    pub(super) input_preedits: &'a HashMap<NodeId, TextPreeditState>,
    pub(super) slider_values: &'a mut HashMap<NodeId, f32>,
    pub(super) slider_script_values: &'a mut HashMap<NodeId, f32>,
    pub(super) checked_values: &'a HashMap<NodeId, bool>,
    pub(super) scroll_offsets: &'a mut HashMap<NodeId, ScrollOffsetState>,
    pub(super) window: WindowSurfaceState,
}

impl<'a> RuntimeAnnotationContext<'a> {
    pub(in crate::shell::component) fn new(
        focused_id: Option<NodeId>,
        focus_visible_id: Option<NodeId>,
        hovered_path: &'a [NodeId],
        active_id: Option<NodeId>,
        active_slider_id: Option<NodeId>,
        input_values: &'a HashMap<NodeId, String>,
        input_preedits: &'a HashMap<NodeId, TextPreeditState>,
        slider_values: &'a mut HashMap<NodeId, f32>,
        slider_script_values: &'a mut HashMap<NodeId, f32>,
        checked_values: &'a HashMap<NodeId, bool>,
        scroll_offsets: &'a mut HashMap<NodeId, ScrollOffsetState>,
    ) -> Self {
        Self {
            focused_id,
            focus_visible_id,
            hovered_ids: hovered_path.iter().copied().collect(),
            active_id,
            active_slider_id,
            input_values,
            input_preedits,
            slider_values,
            slider_script_values,
            checked_values,
            scroll_offsets,
            window: WindowSurfaceState::default(),
        }
    }

    /// Ambient toplevel state for this surface, projected onto every annotated
    /// node so `:windowed`, `:fullscreen` and friends are reachable from any
    /// selector. Popups and layer surfaces that were never promoted leave it at
    /// the default (all false).
    pub(in crate::shell::component) fn with_window_state(
        mut self,
        window: WindowSurfaceState,
    ) -> Self {
        self.window = window;
        self
    }
}

#[derive(Debug, Clone, Copy)]
struct InputPreeditProjection {
    start: usize,
    end: usize,
    cursor_begin: usize,
    cursor_end: usize,
}

fn clamp_preedit_cursor(text: &str, cursor: i32) -> usize {
    let cursor = usize::try_from(cursor).unwrap_or(0).min(text.len());
    (0..=cursor)
        .rev()
        .find(|offset| text.is_char_boundary(*offset))
        .unwrap_or(0)
}

fn compose_input_value(
    value: &str,
    preedit: Option<&TextPreeditState>,
) -> (String, Option<InputPreeditProjection>) {
    let Some(preedit) = preedit.filter(|preedit| !preedit.text.is_empty()) else {
        return (value.to_owned(), None);
    };
    let insert_at = preedit.insert_at.min(value.len());
    let insert_at = value
        .is_char_boundary(insert_at)
        .then_some(insert_at)
        .unwrap_or(value.len());
    let preedit_end = insert_at + preedit.text.len();
    let mut composed = String::with_capacity(value.len() + preedit.text.len());
    composed.push_str(&value[..insert_at]);
    composed.push_str(&preedit.text);
    composed.push_str(&value[insert_at..]);
    let cursor_begin = insert_at + clamp_preedit_cursor(&preedit.text, preedit.cursor_begin);
    let cursor_end = insert_at + clamp_preedit_cursor(&preedit.text, preedit.cursor_end);
    (
        composed,
        Some(InputPreeditProjection {
            start: insert_at,
            end: preedit_end,
            cursor_begin,
            cursor_end,
        }),
    )
}

#[cfg(test)]
pub(in crate::shell::component) fn annotate_runtime_tree(
    node: &mut WidgetNode,
    key: String,
    context: &mut RuntimeAnnotationContext<'_>,
) {
    let node_id = stable_runtime_node_id(&key);
    let mut key = key;
    annotate_runtime_tree_inner(node, &mut key, node_id, context, false);
}

pub(in crate::shell::component) fn annotate_runtime_and_overflow_tree(
    node: &mut WidgetNode,
    key: String,
    context: &mut RuntimeAnnotationContext<'_>,
) {
    let node_id = stable_runtime_node_id(&key);
    let mut key = key;
    annotate_runtime_tree_inner(node, &mut key, node_id, context, true);
}

pub(super) fn annotate_runtime_tree_inner(
    node: &mut WidgetNode,
    key: &mut String,
    node_id: NodeId,
    context: &mut RuntimeAnnotationContext<'_>,
    annotate_overflow: bool,
) -> Option<mesh_core_interaction::ContentBounds> {
    node.id = node_id;
    node.set_mesh_key(key.clone());

    let authored = node.authored_payload();
    let authored_state = mesh_core_elements::authored_element_state(&authored.attributes);
    let is_input = authored.tag == "input";
    let is_slider = authored.tag == "slider";
    let is_switch_or_checkbox = matches!(authored.tag.as_str(), "switch" | "checkbox");
    let authored_value = authored.attributes.get("value").cloned();
    let source_tag = authored
        .attributes
        .get("data-mesh-element")
        .map(String::as_str)
        .unwrap_or(authored.tag.as_str());
    let checkable_choice = matches!(source_tag, "switch" | "checkbox" | "radio" | "option");
    let selects_choice = matches!(source_tag, "radio" | "option");
    let selectable_group = matches!(source_tag, "select" | "radio-group");
    let trace_tag = context
        .hovered_ids
        .contains(&node_id)
        .then(|| authored.tag.clone());
    let checked = context
        .checked_values
        .get(&node_id)
        .copied()
        .or(Some(
            mesh_core_elements::PseudoState::Checked.value(authored_state),
        ))
        .unwrap_or(false);

    node.state = ElementState {
        focused: context.focused_id == Some(node_id),
        focus_visible: context.focus_visible_id == Some(node_id)
            || (context.focus_visible_id.is_none()
                && context.focused_id == Some(node_id)
                && is_input),
        hovered: context.hovered_ids.contains(&node_id),
        active: context.active_id == Some(node_id),
        window: context.window,
        ..authored_state
    };
    // Runtime widget state is authoritative over the authored fallback.
    mesh_core_elements::PseudoState::Checked.set_value(&mut node.state, checked);
    if node.state.hovered {
        tracing::trace!(
            "[hover] annotate: key={key} tag={} set hovered=true",
            trace_tag.as_deref().unwrap_or_default()
        );
    }

    if node.state.focused {
        node.attributes
            .insert("_mesh_focused".into(), "true".into());
    }
    node.accessibility.focused = node.state.focused;

    if is_input {
        let value = context
            .input_values
            .get(&node_id)
            .cloned()
            .or(authored_value.clone())
            .unwrap_or_default();
        let preedit = (context.focused_id == Some(node_id))
            .then(|| context.input_preedits.get(&node_id))
            .flatten();
        let (value, preedit_projection) = compose_input_value(&value, preedit);
        node.attributes.insert("value".into(), value);
        for attribute in [
            "_mesh_preedit_start",
            "_mesh_preedit_end",
            "_mesh_preedit_cursor_begin",
            "_mesh_preedit_cursor_end",
        ] {
            node.attributes.remove(attribute);
        }
        if let Some(projection) = preedit_projection {
            node.attributes
                .insert("_mesh_preedit_start".into(), projection.start.to_string());
            node.attributes
                .insert("_mesh_preedit_end".into(), projection.end.to_string());
            node.attributes.insert(
                "_mesh_preedit_cursor_begin".into(),
                projection.cursor_begin.to_string(),
            );
            node.attributes.insert(
                "_mesh_preedit_cursor_end".into(),
                projection.cursor_end.to_string(),
            );
        }
    } else if is_slider {
        annotate_slider_node(node, node_id, context);
    } else if is_switch_or_checkbox {
        node.attributes.insert(
            "checked".into(),
            if checked { "true" } else { "false" }.into(),
        );
    }

    if checkable_choice {
        node.attributes.insert(
            "checked".into(),
            if checked { "true" } else { "false" }.into(),
        );
        if selects_choice {
            node.attributes.insert(
                "selected".into(),
                if checked { "true" } else { "false" }.into(),
            );
        }
        mesh_core_elements::PseudoState::Checked.set_value(&mut node.state, checked);
        mesh_core_elements::PseudoState::Selected.set_value(&mut node.state, checked);
        node.accessibility.state.checked = Some(checked);
        node.accessibility.state.selected = checked;
    }

    if selectable_group
        && let Some(value) = context
            .input_values
            .get(&node_id)
            .cloned()
            .or(authored_value)
    {
        node.attributes.insert("value".into(), value.clone());
        mesh_core_elements::PseudoState::Value.set_value(&mut node.state, true);
        node.accessibility.state.value = Some(value);
    }

    let offset = context
        .scroll_offsets
        .get(&node_id)
        .copied()
        .unwrap_or_default();
    let scroll = node.scroll_metrics.get_or_insert_default();
    scroll.x = offset.x;
    scroll.y = offset.y;

    let mut children_bounds: Option<mesh_core_interaction::ContentBounds> = None;
    for (index, child) in node.children.iter_mut().enumerate() {
        let previous_len = key.len();
        {
            use std::fmt::Write as _;
            if let Some(loop_identity) = child.loop_identity() {
                let _ = write!(key, "/@loop:{loop_identity}");
            } else {
                let _ = write!(key, "/{index}");
            }
        }
        let child_id = if child.loop_identity().is_some() {
            stable_runtime_node_id(key)
        } else {
            child_runtime_node_id(node_id, index)
        };
        let child_bounds =
            annotate_runtime_tree_inner(child, key, child_id, context, annotate_overflow);
        if let Some(next) = child_bounds {
            children_bounds = Some(match children_bounds {
                Some(current) => (
                    current.0.min(next.0),
                    current.1.min(next.1),
                    current.2.max(next.2),
                    current.3.max(next.3),
                ),
                None => next,
            });
        }
        key.truncate(previous_len);
    }

    annotate_overflow.then(|| {
        mesh_core_interaction::annotate_overflow_node(node, context.scroll_offsets, children_bounds)
    })
}

pub(super) fn annotate_slider_node(
    node: &mut WidgetNode,
    node_id: NodeId,
    context: &mut RuntimeAnnotationContext<'_>,
) {
    let script_value = node
        .attributes
        .get("value")
        .and_then(|value: &String| value.parse::<f32>().ok());
    let value = resolved_slider_value(node_id, script_value, context);
    {
        use std::fmt::Write as _;
        let entry = node
            .attributes
            .entry("value".into())
            .or_insert_with(String::new);
        entry.clear();
        let _ = write!(entry, "{:.2}", value);
    }
}

pub(super) fn resolved_slider_value(
    node_id: NodeId,
    script_value: Option<f32>,
    context: &mut RuntimeAnnotationContext<'_>,
) -> f32 {
    let preserved_value = context.slider_values.get(&node_id).copied();
    if context.active_slider_id == Some(node_id) {
        return preserved_value.or(script_value).unwrap_or(0.0);
    }

    if let Some(script_value) = script_value {
        match (
            preserved_value,
            context.slider_script_values.get(&node_id).copied(),
        ) {
            (Some(preserved), Some(previous_script)) if float_eq(script_value, previous_script) => {
                preserved
            }
            (Some(preserved), None) => preserved,
            (Some(_), Some(_)) => {
                context.slider_values.remove(&node_id);
                context.slider_script_values.remove(&node_id);
                script_value
            }
            (None, _) => script_value,
        }
    } else {
        preserved_value.unwrap_or(0.0)
    }
}

pub(super) fn float_eq(left: f32, right: f32) -> bool {
    (left - right).abs() <= f32::EPSILON
}
