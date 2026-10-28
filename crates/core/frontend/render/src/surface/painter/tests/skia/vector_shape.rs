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
