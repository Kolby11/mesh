use super::super::super::*;
use super::super::common::*;
use mesh_core_elements::WidgetNode;
use mesh_core_elements::layout::LayoutRect;

fn shape_node(attributes: &[(&str, &str)], fill: Color, stroke: Color, stroke_width: f32) -> WidgetNode {
    let mut node = WidgetNode::new("shape");
    for (name, value) in attributes {
        node.attributes.insert(*name, (*value).to_string());
    }
    node.layout = LayoutRect {
        x: 0.0,
        y: 0.0,
        width: 100.0,
        height: 100.0,
    };
    node.computed_style.background_color = fill;
    node.computed_style.border_color = stroke;
    node.computed_style.border_width.top = stroke_width;
    node
}

fn paint(node: &WidgetNode) -> PixelBuffer {
    let shape = mesh_core_elements::ShapeGeometry::from_node(node).expect("shape geometry");
    let bounds = ClipRect {
        x: 0,
        y: 0,
        width: 100,
        height: 100,
    };
    let commands = tree::shape_commands(
        &shape,
        node.computed_style.background_color,
        node.computed_style.border_color,
        node.computed_style.border_width.top,
        1.0,
        bounds,
        bounds,
    );
    let mut buffer = PixelBuffer::new(100, 100);
    let mut diagnostics = Vec::new();
    SkiaPaintBackend.execute_commands(&mut buffer, &commands, &mut diagnostics);
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    buffer
}

#[test]
fn retained_shape_geometry_updates_match_fresh_pixels() {
    use crate::display_list::{DamageRect, DisplayListRepaintPolicy, RetainedDisplayList};
    let red = Color::from_hex("#ff0000").unwrap();
    for (kind, prop, before, after) in [
        ("sector", "end", "90", "180"),
        ("sector", "inner", "0.2", "0.7"),
        ("arc", "end", "90", "180"),
        ("polygon", "points", "0,0 100,0 50,100", "0,0 100,0 0,100"),
        ("polyline", "points", "0,0 100,0", "0,0 0,100"),
    ] {
        let mut shape = shape_node(&[("type", kind), (prop, before)], red, red, 4.0);
        shape.id = 1;
        let mut list = RetainedDisplayList::default();
        list.update(&shape, 100, 100, false, true);
        let original = paint(&shape);
        shape.attributes.insert(prop, after.to_string());
        list.update(&shape, 100, 100, false, false);
        assert!(!list.damage_rects().is_empty(), "{kind} {prop}");
        let selection = list.select_paint_commands(
            Some(DamageRect { x: 0, y: 0, width: 100, height: 100 }),
            DisplayListRepaintPolicy::FullSurface,
        );
        let engine = FrontendRenderEngine::new();
        let mut retained = PixelBuffer::new(100, 100);
        engine.render_selected_display_list_for_module(&selection, &mut retained, 1.0, None, None, None);
        let mut fresh = PixelBuffer::new(100, 100);
        engine.render_tree(&shape, &mut fresh, 1.0);
        assert_ne!(original.data(), fresh.data(), "{kind} {prop}");
        assert_eq!(retained.data(), fresh.data(), "{kind} {prop}");
    }
}

#[test]
fn sector_fills_only_its_wedge_and_leaves_the_hole_clear() {
    let red = Color::from_hex("#ff0000").unwrap();
    let buffer = paint(&shape_node(
        &[("type", "sector"), ("start", "-45"), ("end", "45"), ("inner", "0.5")],
        red,
        Color::TRANSPARENT,
        0.0,
    ));
    assert_eq!(pixel(&buffer, 50, 10), red, "top, between the radii");
    assert_eq!(pixel(&buffer, 50, 45), Color::TRANSPARENT, "inside the hole");
    assert_eq!(pixel(&buffer, 90, 50), Color::TRANSPARENT, "right, outside the wedge");
    assert_eq!(pixel(&buffer, 50, 90), Color::TRANSPARENT, "bottom, outside the wedge");
}

#[test]
fn arc_strokes_with_the_border_colour_and_never_fills() {
    let green = Color::from_hex("#00ff00").unwrap();
    let buffer = paint(&shape_node(
        &[("type", "arc"), ("start", "0"), ("end", "180")],
        Color::from_hex("#ff0000").unwrap(),
        green,
        6.0,
    ));
    assert_eq!(pixel(&buffer, 97, 50), green, "right edge of the ring");
    assert_eq!(pixel(&buffer, 50, 50), Color::TRANSPARENT, "centre stays unfilled");
    assert_eq!(pixel(&buffer, 3, 50), Color::TRANSPARENT, "left half is outside the sweep");
}
