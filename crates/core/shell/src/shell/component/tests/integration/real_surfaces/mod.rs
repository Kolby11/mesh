mod appearance_profile;
mod audio;
mod brightness;
mod bubbles;
mod commands;
mod hover;
mod inspector;
mod layout;
mod navigation_profile;
mod raster;
mod settings;
mod transitions;

use super::*;
use mesh_core_interaction::find_tooltip_text_by_key;
use mesh_core_wayland::{Edge, KeyboardMode, Layer, ShellSurface};

#[derive(Default)]
struct LayoutRecordingSurface {
    size: Option<(u32, u32)>,
}

impl ShellSurface for LayoutRecordingSurface {
    fn anchor(&mut self, _edge: Edge) {}

    fn set_size(&mut self, width: u32, height: u32) {
        self.size = Some((width, height));
    }

    fn set_exclusive_zone(&mut self, _zone: i32) {}

    fn set_layer(&mut self, _layer: Layer) {}

    fn set_keyboard_interactivity(&mut self, _mode: KeyboardMode) {}

    fn set_margin(&mut self, _top: i32, _right: i32, _bottom: i32, _left: i32) {}

    fn set_blur(&mut self, _blur: bool) {}

    fn set_role(&mut self, _role: mesh_core_wayland::SurfaceRole) {}

    fn set_window_options(&mut self, _options: mesh_core_wayland::WindowOptions) {}

    fn show(&mut self) {}

    fn hide(&mut self) {}
}

fn assert_phase44_focused_proof_snapshot(component: &FrontendSurfaceComponent, label: &str) {
    let snapshot = component
        .last_focused_proof_snapshot()
        .unwrap_or_else(|| panic!("{label} should store a focused proof snapshot"));
    assert!(
        !snapshot.nodes.is_empty(),
        "{label} should retain node proof evidence"
    );
    assert!(
        snapshot
            .paint
            .iter()
            .any(|paint| matches!(paint.display_slot, "Text" | "Icon")),
        "{label} should include text or icon paint proof evidence"
    );
    assert!(
        !snapshot.accessibility.is_empty(),
        "{label} should retain accessibility proof evidence"
    );
}

fn assert_layout_contains(parent: &WidgetNode, child: &WidgetNode, label: &str) {
    assert!(
        parent.layout.width > 0.0 && parent.layout.height > 0.0,
        "{label} parent should have non-zero layout"
    );
    assert!(
        child.layout.width > 0.0 && child.layout.height > 0.0,
        "{label} child should have non-zero layout"
    );
    assert!(
        child.layout.x >= parent.layout.x - 0.5
            && child.layout.y >= parent.layout.y - 0.5
            && child.layout.x + child.layout.width <= parent.layout.x + parent.layout.width + 0.5
            && child.layout.y + child.layout.height <= parent.layout.y + parent.layout.height + 0.5,
        "{label} child layout {:?} should stay inside parent layout {:?}",
        child.layout,
        parent.layout
    );
}

fn i32_rect(bounds: (f32, f32, f32, f32)) -> (i32, i32, i32, i32) {
    let left = bounds.0.floor() as i32;
    let top = bounds.1.floor() as i32;
    let right = bounds.2.ceil() as i32;
    let bottom = bounds.3.ceil() as i32;
    (left, top, (right - left).max(1), (bottom - top).max(1))
}

fn collect_class_attributes(node: &WidgetNode, classes: &mut Vec<String>) {
    if let Some(class) = node.attributes.get("class") {
        classes.push(class.clone());
    }
    for child in &node.children {
        collect_class_attributes(child, classes);
    }
}

fn parent_of_node_key<'a>(node: &'a WidgetNode, key: &str) -> Option<&'a WidgetNode> {
    if node
        .children
        .iter()
        .any(|child| child.mesh_key().is_some_and(|candidate| candidate == key))
    {
        return Some(node);
    }
    node.children
        .iter()
        .find_map(|child| parent_of_node_key(child, key))
}

fn bubble_option_labels(
    component: &FrontendSurfaceComponent,
    node_key: &str,
    content_offset: (f32, f32),
) -> Vec<String> {
    let tree = component
        .child_surface_debug_tree(node_key, content_offset)
        .expect("language child tree");
    let mut labels = Vec::new();
    collect_text_content(&tree, &mut labels);
    labels
}

/// Count pixels with non-zero alpha inside `[left,right) x [top,bottom)`.
/// Bounds are surface-local logical coordinates; the buffer is the painted
/// surface at scale 1.0, so they map 1:1.
fn opaque_pixels_in_bounds(
    buffer: &PixelBuffer,
    left: f32,
    top: f32,
    right: f32,
    bottom: f32,
) -> u32 {
    let x0 = left.floor().max(0.0) as u32;
    let y0 = top.floor().max(0.0) as u32;
    let x1 = (right.ceil() as u32).min(buffer.width());
    let y1 = (bottom.ceil() as u32).min(buffer.height());
    let mut count = 0;
    for y in y0..y1 {
        for x in x0..x1 {
            let offset = (y * buffer.stride() + x * 4) as usize;
            if buffer.data()[offset + 3] != 0 {
                count += 1;
            }
        }
    }
    count
}

/// Which bubble carries the active mark, as its index in the ring.
fn active_bubble_index(
    component: &FrontendSurfaceComponent,
    node_key: &str,
    content_offset: (f32, f32),
) -> Option<usize> {
    let tree = component
        .child_surface_debug_tree(node_key, content_offset)
        .expect("bubble popover child tree");
    let mut cores = Vec::new();
    collect_class_token_nodes(&tree, "bubble-option-core", &mut cores);
    cores.iter().position(|node| {
        node.attributes.get("class").is_some_and(|class| {
            class
                .split_whitespace()
                .any(|token| token == "bubble-option-core-active")
        })
    })
}

fn collect_class_token_nodes<'a>(node: &'a WidgetNode, token: &str, out: &mut Vec<&'a WidgetNode>) {
    if node
        .attributes
        .get("class")
        .is_some_and(|class| class.split_whitespace().any(|part| part == token))
    {
        out.push(node);
    }
    for child in &node.children {
        collect_class_token_nodes(child, token, out);
    }
}

/// Open a bubble popover from its navigation-bar trigger and click the bubble
/// at `option_index`, returning the promoted child key, its content offset, and
/// which bubble was marked active before and after the click.
fn pick_bubble_option(enter_handler: &str, option_index: usize) -> (Option<usize>, Option<usize>) {
    let theme = default_theme();
    let width = 1280;
    let height = 80;
    let mut component =
        real_frontend_module_component("@mesh/navigation-bar", navigation_bar_catalog());
    component.visible = true;
    component
        .handle_service_event(&ServiceEvent::Updated {
            service: "mesh.locale".into(),
            source_module: "@mesh/shell".into(),
            payload: serde_json::json!({ "locale": "en", "current": "en" }),
        })
        .unwrap();
    let mut buffer = PixelBuffer::new(width, height);
    component
        .paint(
            &theme,
            SurfaceExtent::unpadded(width, height),
            &mut buffer,
            1.0,
        )
        .unwrap();
    component
        .call_namespaced_handler(enter_handler, &[])
        .unwrap();
    component
        .paint(
            &theme,
            SurfaceExtent::unpadded(width, height),
            &mut buffer,
            1.0,
        )
        .unwrap();

    let requests = component.child_surface_requests();
    assert_eq!(requests.len(), 1, "popover should open: {requests:?}");
    let node_key = requests[0].node_key.clone();
    let (cw, ch) = requests[0].content_size;
    let (pad_left, pad_top, ..) = requests[0].content_padding;
    let content_offset = (pad_left as f32, pad_top as f32);

    let before = active_bubble_index(&component, &node_key, content_offset);

    let option_layout = {
        let tree = component
            .child_surface_debug_tree(&node_key, content_offset)
            .expect("bubble popover child tree");
        let mut options = Vec::new();
        collect_class_token_nodes(&tree, "bubble-option", &mut options);
        options[option_index].layout
    };
    let x = option_layout.x + option_layout.width / 2.0;
    let y = option_layout.y + option_layout.height / 2.0;
    for pressed in [true, false] {
        component
            .handle_child_surface_input(
                &node_key,
                &theme,
                cw,
                ch,
                content_offset,
                ComponentInput::PointerButton { x, y, pressed },
            )
            .unwrap();
    }
    component
        .paint(
            &theme,
            SurfaceExtent::unpadded(width, height),
            &mut buffer,
            1.0,
        )
        .unwrap();

    (
        before,
        active_bubble_index(&component, &node_key, content_offset),
    )
}

fn trigger_flag(component: &FrontendSurfaceComponent) -> Option<String> {
    let tree = component.last_tree.as_ref()?;
    let mut labels = Vec::new();
    collect_class_token_nodes(tree, "language-label", &mut labels);
    let mut text = Vec::new();
    collect_text_content(labels.first()?, &mut text);
    text.first().cloned()
}
