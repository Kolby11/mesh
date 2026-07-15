pub(super) use super::common::*;
use super::*;
pub(super) use crate::shell::{CoreRequest, KeyModifiers};
pub(super) use mesh_core_elements::Color;
pub(super) use mesh_core_elements::LayoutRect;
pub(super) use mesh_core_elements::style::Display;
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
    hovered_path: &[String],
    active_key: &Option<String>,
    active_slider_key: &Option<String>,
    input_values: &HashMap<String, String>,
    slider_values: &mut HashMap<String, f32>,
    slider_script_values: &mut HashMap<String, f32>,
    checked_values: &HashMap<String, bool>,
    scroll_offsets: &mut HashMap<String, ScrollOffsetState>,
) {
    let mut context = crate::shell::component::runtime_tree::RuntimeAnnotationContext::new(
        focused_key,
        focus_visible_key,
        hovered_path,
        active_key,
        active_slider_key,
        input_values,
        slider_values,
        slider_script_values,
        checked_values,
        scroll_offsets,
    );
    crate::shell::component::runtime_tree::annotate_runtime_tree(node, key, &mut context);
}

mod animation;
mod diagnostics;
mod navigation;
mod policy;
mod pseudo;
mod reflow;
