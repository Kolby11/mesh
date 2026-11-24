use super::super::super::*;
use super::super::common::*;
use mesh_core_elements::WidgetNode;
use mesh_core_elements::layout::LayoutRect;

fn shape_node(
    attributes: &[(&str, &str)],
    fill: Color,
    stroke: Color,
    stroke_width: f32,
) -> WidgetNode {
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
    let mut buffer = PixelBuffer::new(100, 100);
    FrontendRenderEngine::new().render_tree(node, &mut buffer, 1.0);
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
        (
            "path",
            "d",
            "M5 5 C50 75 60 5 95 5 V30 H5 Z",
            "M5 5 C50 75 60 5 95 5 V90 H5 Z",
        ),
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
            Some(DamageRect {
                x: 0,
                y: 0,
                width: 100,
                height: 100,
            }),
            DisplayListRepaintPolicy::FullSurface,
        );
        let engine = FrontendRenderEngine::new();
        let mut retained = PixelBuffer::new(100, 100);
        engine.render_selected_display_list_for_module(
            &selection,
            &mut retained,
            1.0,
            None,
            None,
            None,
        );
        let mut fresh = PixelBuffer::new(100, 100);
        engine.render_tree(&shape, &mut fresh, 1.0);
        assert_ne!(original.data(), fresh.data(), "{kind} {prop}");
        assert_eq!(retained.data(), fresh.data(), "{kind} {prop}");
    }
}

#[test]
fn svg_path_masks_background_layers_and_paints_outline_on_the_contour() {
    use mesh_core_elements::style::{
        BackgroundPaint, GradientGeometry, GradientStop, Outline, StyleGradient, StyleImageSource,
    };
    let red = Color::from_hex("#ff0000").unwrap();
    let blue = Color::from_hex("#0000ff").unwrap();
    let mut node = shape_node(
        &[
            ("type", "path"),
            ("fill-rule", "evenodd"),
            ("d", "M20 20 Q50 8 80 20 V80 H20 Z M40 40 H60 V60 H40 Z"),
        ],
        Color::TRANSPARENT,
        Color::TRANSPARENT,
        0.0,
    );
    node.computed_style.background_paint = BackgroundPaint::Gradient(StyleGradient {
        geometry: GradientGeometry::Linear {
            angle_degrees: 90.0,
        },
        stops: vec![
            GradientStop {
                color: red,
                position: 0.0,
            },
            GradientStop {
                color: blue,
                position: 1.0,
            },
        ],
    });
    node.computed_style.outline = Outline {
        width: 2.0,
        offset: 2.0,
        color: red,
    };
    let buffer = paint(&node);
    assert!(pixel(&buffer, 30, 45).a > 0);
    assert_ne!(pixel(&buffer, 30, 45), pixel(&buffer, 70, 45));
    assert_eq!(pixel(&buffer, 50, 50), Color::TRANSPARENT);
    assert_eq!(pixel(&buffer, 10, 10), Color::TRANSPARENT);
    assert_eq!(pixel(&buffer, 17, 50), red);

    let (_dir, path) = write_effect_test_image("shape.png");
    node.computed_style.outline = Outline::NONE;
    node.computed_style.background_paint = BackgroundPaint::Image(StyleImageSource { path });
    let buffer = paint(&node);
    assert!(pixel(&buffer, 30, 45).a > 0);
    assert_eq!(pixel(&buffer, 50, 50), Color::TRANSPARENT);
    assert_eq!(pixel(&buffer, 10, 10), Color::TRANSPARENT);
}

#[test]
fn malformed_path_with_background_stays_transparent() {
    let node = shape_node(
        &[("type", "path"), ("d", "M 4 4 L invalid")],
        Color::from_hex("#ff0000").unwrap(),
        Color::TRANSPARENT,
        0.0,
    );
    assert_eq!(pixel(&paint(&node), 50, 50), Color::TRANSPARENT);
}

#[test]
fn svg_path_clips_children_and_refreshes_retained_child_clips() {
    use crate::RenderObjectDirtySummary;
    use crate::display_list::{DisplayListRepaintPolicy, RetainedDisplayList};

    let red = Color::from_hex("#ff0000").unwrap();
    let mut mask = shape_node(
        &[
            ("type", "path"),
            ("fill-rule", "evenodd"),
            ("d", "M5 5 H95 V95 H5 Z M35 35 H65 V65 H35 Z"),
            ("clip-children", "true"),
        ],
        Color::TRANSPARENT,
        Color::TRANSPARENT,
        0.0,
    );
    mask.id = 1;
    let mut child = WidgetNode::new("box");
    child.id = 2;
    child.layout = mask.layout;
    child.computed_style.background_color = red;
    mask.children.push(child);
    let engine = FrontendRenderEngine::new();
    let mut list = RetainedDisplayList::default();
    list.update_at_for_retained_generation(&mask, 1, 0.0, 0.0, 100, 100, false, true);
    let mut pixels = PixelBuffer::new(100, 100);
    engine.render_display_list_for_module(
        list.paint_commands(),
        &mut pixels,
        1.0,
        None,
        None,
        None,
    );
    list.mark_painted();
    assert_eq!(pixel(&pixels, 20, 20), red);
    assert_eq!(pixel(&pixels, 50, 50), Color::TRANSPARENT);

    mask.attributes.insert("d", "M40 5 H95 V95 H40 Z".into());
    list.update_at_for_retained_generation_with_dirty_nodes(
        &mask,
        2,
        RenderObjectDirtySummary {
            material: 1,
            ..Default::default()
        },
        &std::collections::HashSet::from([1]),
        0.0,
        0.0,
        100,
        100,
        false,
        true,
    );
    let damage = list.damage_rects();
    assert!(!damage.is_empty());
    let selected =
        list.select_paint_commands_for_rects(damage, DisplayListRepaintPolicy::MinimalDamage);
    for rect in damage {
        pixels.clear_rect(rect.x, rect.y, rect.width, rect.height, Color::TRANSPARENT);
    }
    engine.render_selected_display_list_for_module(&selected, &mut pixels, 1.0, None, None, None);
    let fresh = paint(&mask);
    assert_eq!(pixels.data(), fresh.data());
    assert_eq!(pixel(&pixels, 20, 20), Color::TRANSPARENT);
    assert_eq!(pixel(&pixels, 50, 50), red);
}

#[test]
fn svg_path_contains_filtered_children_and_uses_local_backdrop() {
    use crate::display_list::{
        DamageRect, RetainedDisplayList, backdrop_blur_regions, backdrop_blur_regions_from_tree,
    };
    use mesh_core_elements::VisualFilter;

    let red = Color::from_hex("#ff0000").unwrap();
    let mut mask = shape_node(
        &[
            ("type", "path"),
            ("fill-rule", "evenodd"),
            ("d", "M0 0 H100 V100 H0 Z M40 40 H60 V60 H40 Z"),
            ("clip-children", "true"),
        ],
        Color::TRANSPARENT,
        Color::TRANSPARENT,
        0.0,
    );
    mask.id = 1;
    let mut child = WidgetNode::new("box");
    child.id = 2;
    child.layout = mask.layout;
    child.computed_style.background_color = red;
    child.computed_style.filter = VisualFilter {
        blur_radius: 8.0,
        ..VisualFilter::NONE
    };
    child.computed_style.backdrop_filter = VisualFilter {
        blur_radius: 8.0,
        ..VisualFilter::NONE
    };
    mask.children.push(child);

    let buffer = paint(&mask);
    assert!(pixel(&buffer, 30, 45).a > 0);
    assert_eq!(pixel(&buffer, 43, 45), Color::TRANSPARENT);

    let mut list = RetainedDisplayList::default();
    list.update(&mask, 100, 100, false, true);
    assert!(list.paint_commands().iter().any(|command| {
        command.kind == crate::display_list::DisplayPaintCommandKind::ApplyBackdropFilterInSurface
    }));
    assert!(backdrop_blur_regions(list.paint_commands()).is_empty());
    assert!(
        backdrop_blur_regions_from_tree(
            &mask,
            0.0,
            0.0,
            DamageRect {
                x: 0,
                y: 0,
                width: 100,
                height: 100
            },
            false,
        )
        .is_empty()
    );
}

#[test]
fn shaped_text_can_follow_a_path_in_its_view_box() {
    let mut text = shape_node(&[], Color::TRANSPARENT, Color::TRANSPARENT, 0.0);
    text.tag = "text".into();
    text.computed_style.color = Color::from_hex("#ff0000").unwrap();
    text.computed_style.font_size = 15.0;
    text.attributes.insert("text", "ABCD".into());
    text.attributes.insert("text-path", "M 8 80 H 92".into());
    let buffer = paint(&text);
    let ink = |buffer: &PixelBuffer, start: u32, end: u32| {
        (start..end).any(|y| (0..100).any(|x| pixel(buffer, x, y).a > 0))
    };
    assert!(ink(&buffer, 65, 80));
    assert!(!ink(&buffer, 0, 60));

    text.attributes.insert("text-path", "M 8 30 H 92".into());
    let moved = paint(&text);
    assert!(ink(&moved, 15, 30));
    assert!(!ink(&moved, 65, 100));
}

#[test]
fn sector_fills_only_its_wedge_and_leaves_the_hole_clear() {
    let red = Color::from_hex("#ff0000").unwrap();
    let buffer = paint(&shape_node(
        &[
            ("type", "sector"),
            ("start", "-45"),
            ("end", "45"),
            ("inner", "0.5"),
        ],
        red,
        Color::TRANSPARENT,
        0.0,
    ));
    assert_eq!(pixel(&buffer, 50, 10), red, "top, between the radii");
    assert_eq!(
        pixel(&buffer, 50, 45),
        Color::TRANSPARENT,
        "inside the hole"
    );
    assert_eq!(
        pixel(&buffer, 90, 50),
        Color::TRANSPARENT,
        "right, outside the wedge"
    );
    assert_eq!(
        pixel(&buffer, 50, 90),
        Color::TRANSPARENT,
        "bottom, outside the wedge"
    );
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
    assert_eq!(
        pixel(&buffer, 50, 50),
        Color::TRANSPARENT,
        "centre stays unfilled"
    );
    assert_eq!(
        pixel(&buffer, 3, 50),
        Color::TRANSPARENT,
        "left half is outside the sweep"
    );
}
