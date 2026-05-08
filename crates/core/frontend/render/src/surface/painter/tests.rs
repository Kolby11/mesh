use super::*;
use mesh_core_elements::layout::LayoutRect;
use mesh_core_elements::style::{Dimension, Edges};
use mesh_core_frontend::compile_frontend_module;
use mesh_core_theme::default_theme;
use std::path::PathBuf;

fn node(tag: &str, layout: LayoutRect, color: Color) -> WidgetNode {
    let mut node = WidgetNode::new(tag);
    node.layout = layout;
    node.computed_style.width = Dimension::Px(layout.width);
    node.computed_style.height = Dimension::Px(layout.height);
    node.computed_style.background_color = color;
    node
}

fn text_node(text: &str, x: f32, y: f32, width: f32, height: f32, color: Color) -> WidgetNode {
    let mut node = node(
        "text",
        LayoutRect {
            x,
            y,
            width,
            height,
        },
        Color::TRANSPARENT,
    );
    node.attributes.insert("content".into(), text.into());
    node.attributes.insert("selectable".into(), "true".into());
    node.computed_style.color = color;
    node.computed_style.font_size = 14.0;
    node.computed_style.line_height = 1.4;
    node.computed_style.padding = Edges::zero();
    node
}

fn pixel(buffer: &PixelBuffer, x: u32, y: u32) -> Color {
    let offset = (y * buffer.stride + x * 4) as usize;
    Color {
        b: buffer.data[offset],
        g: buffer.data[offset + 1],
        r: buffer.data[offset + 2],
        a: buffer.data[offset + 3],
    }
}

#[test]
fn painter_draws_border_from_computed_edges() {
    let mut root = node(
        "box",
        LayoutRect {
            x: 0.0,
            y: 0.0,
            width: 20.0,
            height: 20.0,
        },
        Color::TRANSPARENT,
    );
    root.computed_style.border_width = Edges::all(2.0);
    root.computed_style.border_color = Color::from_hex("#ff0000").unwrap();

    let mut buffer = PixelBuffer::new(24, 24);
    FrontendRenderEngine::new().render_tree(&root, &mut buffer, 1.0);

    assert_eq!(pixel(&buffer, 1, 1), Color::from_hex("#ff0000").unwrap());
    assert_eq!(pixel(&buffer, 10, 10), Color::TRANSPARENT);
}

#[test]
fn painter_clips_children_when_overflow_hidden() {
    let mut root = node(
        "box",
        LayoutRect {
            x: 0.0,
            y: 0.0,
            width: 10.0,
            height: 10.0,
        },
        Color::TRANSPARENT,
    );
    root.computed_style.overflow_x = Overflow::Hidden;
    root.computed_style.overflow_y = Overflow::Hidden;
    root.children = vec![node(
        "box",
        LayoutRect {
            x: 8.0,
            y: 0.0,
            width: 8.0,
            height: 10.0,
        },
        Color::from_hex("#00ff00").unwrap(),
    )];

    let mut buffer = PixelBuffer::new(20, 12);
    FrontendRenderEngine::new().render_tree(&root, &mut buffer, 1.0);

    assert_eq!(pixel(&buffer, 9, 5), Color::from_hex("#00ff00").unwrap());
    assert_eq!(pixel(&buffer, 11, 5), Color::TRANSPARENT);
}

#[test]
fn painter_orders_children_by_z_index() {
    let mut root = node(
        "box",
        LayoutRect {
            x: 0.0,
            y: 0.0,
            width: 16.0,
            height: 16.0,
        },
        Color::TRANSPARENT,
    );

    let mut bottom = node(
        "box",
        LayoutRect {
            x: 2.0,
            y: 2.0,
            width: 10.0,
            height: 10.0,
        },
        Color::from_hex("#ff0000").unwrap(),
    );
    bottom.computed_style.z_index = 0;
    let mut top = node(
        "box",
        LayoutRect {
            x: 2.0,
            y: 2.0,
            width: 10.0,
            height: 10.0,
        },
        Color::from_hex("#0000ff").unwrap(),
    );
    top.computed_style.z_index = 1;
    root.children = vec![top, bottom];

    let mut buffer = PixelBuffer::new(20, 20);
    FrontendRenderEngine::new().render_tree(&root, &mut buffer, 1.0);

    assert_eq!(pixel(&buffer, 5, 5), Color::from_hex("#0000ff").unwrap());
}

#[test]
fn selection_paint_uses_selection_colors() {
    let mut root = node(
        "box",
        LayoutRect {
            x: 0.0,
            y: 0.0,
            width: 160.0,
            height: 60.0,
        },
        Color::TRANSPARENT,
    );
    let mut text = text_node(
        "selection proof text",
        0.0,
        0.0,
        160.0,
        60.0,
        Color::from_hex("#111111").unwrap(),
    );
    text.attributes
        .insert("_mesh_selection_background".into(), "#00ff00".into());
    text.attributes
        .insert("_mesh_selection_foreground".into(), "#ff00ff".into());
    text.attributes
        .insert("_mesh_selection_anchor_x".into(), "0.00".into());
    text.attributes
        .insert("_mesh_selection_anchor_y".into(), "0.00".into());
    text.attributes
        .insert("_mesh_selection_focus_x".into(), "1000.00".into());
    text.attributes
        .insert("_mesh_selection_focus_y".into(), "1000.00".into());
    text.attributes
        .insert("_mesh_selection_text_x".into(), "0.00".into());
    text.attributes
        .insert("_mesh_selection_text_y".into(), "0.00".into());
    root.children = vec![text];

    let mut buffer = PixelBuffer::new(180, 80);
    FrontendRenderEngine::new().render_tree(&root, &mut buffer, 1.0);

    let mut saw_selection_background = false;
    let mut saw_selection_foreground = false;
    for y in 0..buffer.height {
        for x in 0..buffer.width {
            let color = pixel(&buffer, x, y);
            if color == Color::from_hex("#00ff00").unwrap() {
                saw_selection_background = true;
            }
            if color == Color::from_hex("#ff00ff").unwrap() {
                saw_selection_foreground = true;
            }
        }
    }

    assert!(saw_selection_background);
    assert!(saw_selection_foreground);
}

#[test]
fn selection_paint_does_not_bleed_into_neighbors() {
    let mut root = node(
        "box",
        LayoutRect {
            x: 0.0,
            y: 0.0,
            width: 220.0,
            height: 80.0,
        },
        Color::TRANSPARENT,
    );
    let mut selected = text_node(
        "selected",
        0.0,
        0.0,
        100.0,
        40.0,
        Color::from_hex("#111111").unwrap(),
    );
    selected
        .attributes
        .insert("_mesh_selection_background".into(), "#00ff00".into());
    selected
        .attributes
        .insert("_mesh_selection_foreground".into(), "#ff00ff".into());
    selected
        .attributes
        .insert("_mesh_selection_anchor_x".into(), "0.00".into());
    selected
        .attributes
        .insert("_mesh_selection_anchor_y".into(), "0.00".into());
    selected
        .attributes
        .insert("_mesh_selection_focus_x".into(), "1000.00".into());
    selected
        .attributes
        .insert("_mesh_selection_focus_y".into(), "1000.00".into());
    selected
        .attributes
        .insert("_mesh_selection_text_x".into(), "0.00".into());
    selected
        .attributes
        .insert("_mesh_selection_text_y".into(), "0.00".into());

    let neighbor = text_node(
        "neighbor",
        120.0,
        0.0,
        100.0,
        40.0,
        Color::from_hex("#111111").unwrap(),
    );
    root.children = vec![selected, neighbor];

    let mut buffer = PixelBuffer::new(240, 80);
    FrontendRenderEngine::new().render_tree(&root, &mut buffer, 1.0);

    for y in 0..40 {
        for x in 120..220 {
            assert_ne!(
                pixel(&buffer, x, y),
                Color::from_hex("#00ff00").unwrap(),
                "selection background should stay inside the selected text node"
            );
        }
    }
}

#[test]
fn selection_fixture_preview_tree_paints_nonempty_surface() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../..")
        .canonicalize()
        .unwrap();
    let module_dir = root.join("modules/frontend/text-selection-proof");
    let loaded = mesh_core_module::manifest::load_manifest(&module_dir).unwrap();
    let compiled = compile_frontend_module(&loaded.manifest, &module_dir).unwrap();
    let tree = compiled.build_preview_tree(&default_theme(), 360, 176);

    let mut buffer = PixelBuffer::new(380, 196);
    FrontendRenderEngine::new().render_tree(&tree, &mut buffer, 1.0);

    let has_visible_pixels = buffer.data.chunks_exact(4).any(|px| px[3] != 0);
    assert!(
        has_visible_pixels,
        "proof fixture should paint visible output"
    );
}
