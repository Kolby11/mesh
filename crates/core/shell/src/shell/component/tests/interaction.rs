pub(super) use super::common::*;
use super::*;
pub(super) use crate::shell::{CoreRequest, KeyModifiers};
pub(super) use mesh_core_elements::Color;
pub(super) use mesh_core_elements::LayoutRect;
pub(super) use mesh_core_service::InterfaceCatalog;
pub(super) use std::collections::HashMap;
pub(super) use std::path::PathBuf;
pub(super) use std::time::{Duration, Instant};

#[allow(clippy::too_many_arguments)]
pub(super) fn annotate_runtime_tree(
    node: &mut WidgetNode,
    key: String,
    focused_key: &Option<String>,
    focus_visible_key: &Option<String>,
    hovered_path: &[NodeId],
    active_id: Option<NodeId>,
    active_slider_id: Option<NodeId>,
    input_values: &HashMap<NodeId, String>,
    slider_values: &mut HashMap<NodeId, f32>,
    slider_script_values: &mut HashMap<NodeId, f32>,
    checked_values: &HashMap<NodeId, bool>,
    scroll_offsets: &mut HashMap<NodeId, ScrollOffsetState>,
) {
    let input_preedits = HashMap::new();
    let mut context = crate::shell::component::runtime_tree::RuntimeAnnotationContext::new(
        focused_key.as_deref().map(runtime_node_id_for_key),
        focus_visible_key.as_deref().map(runtime_node_id_for_key),
        hovered_path,
        active_id,
        active_slider_id,
        input_values,
        &input_preedits,
        slider_values,
        slider_script_values,
        checked_values,
        scroll_offsets,
    );
    crate::shell::component::runtime_tree::annotate_runtime_tree(node, key, &mut context);
}

mod activation;
mod animation;
mod diagnostics;
mod gestures;
mod keybinds;
mod navigation;
mod policy;
mod pseudo;
mod reflow;
