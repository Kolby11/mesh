use super::super::paint_node::*;
use super::super::*;
use super::common::*;
use mesh_core_elements::style::Corners;
use std::sync::Arc;

#[test]
fn display_text_payload_clone_shares_the_text_allocation() {
    let mut text_node = node(1, "text", 0.0, 0.0, 100.0, 20.0);
    text_node
        .attributes
        .insert("content".into(), "shared display text".into());

    let DisplayPaintContent::Text(first) = build_paint_content(&text_node) else {
        panic!("text node must produce text paint content");
    };
    let cloned = first.clone();

    assert!(Arc::ptr_eq(&first.text, &cloned.text));
    assert_eq!(first, cloned);
    assert_eq!(first.text.as_ref(), "shared display text");
}

#[test]
fn rebuilt_display_node_retains_unchanged_text_allocation() {
    let mut text_node = node(1, "text", 0.0, 0.0, 100.0, 20.0);
    text_node
        .attributes
        .insert("content".into(), "retained display text".into());
    let first = build_paint_node(&text_node, 0.0, 0.0);

    text_node.computed_style.font_size += 1.0;
    let rebuilt = build_paint_node_with_previous(&text_node, 0.0, 0.0, Some(&first));
    let DisplayPaintContent::Text(first_text) = &first.content else {
        panic!("text node must produce text paint content");
    };
    let DisplayPaintContent::Text(rebuilt_text) = &rebuilt.content else {
        panic!("text node must produce text paint content");
    };

    assert!(Arc::ptr_eq(&first_text.text, &rebuilt_text.text));
}

#[test]
fn input_display_payload_preserves_preedit_decoration_range() {
    let mut input = node(1, "input", 0.0, 0.0, 160.0, 32.0);
    input.attributes.insert("value".into(), "A🙂候B".into());
    input
        .attributes
        .insert("_mesh_preedit_start".into(), "5".into());
    input
        .attributes
        .insert("_mesh_preedit_end".into(), "8".into());
    input
        .attributes
        .insert("_mesh_preedit_cursor_begin".into(), "5".into());
    input
        .attributes
        .insert("_mesh_preedit_cursor_end".into(), "8".into());

    let DisplayPaintContent::Input(payload) = build_paint_content(&input) else {
        panic!("input node must produce input paint content");
    };
    assert_eq!(
        payload.preedit,
        Some(DisplayInputPreedit {
            start: 5,
            end: 8,
            cursor_begin: 5,
            cursor_end: 8,
        })
    );
}

#[test]
fn display_payload_equality_falls_back_to_text_content() {
    let first = DisplayIconPaint {
        src: Some(Arc::from("icons/search.svg")),
        name: Some(Arc::from("search")),
        size: Some(16),
    };
    let second = DisplayIconPaint {
        src: Some(Arc::from("icons/search.svg")),
        name: Some(Arc::from("search")),
        size: Some(16),
    };

    assert!(!Arc::ptr_eq(
        first.src.as_ref().expect("first src"),
        second.src.as_ref().expect("second src")
    ));
    assert_eq!(first, second);
}

#[test]
fn display_paint_payload_retains_all_corner_radii() {
    let mut root = node(1, "box", 0.0, 0.0, 80.0, 40.0);
    root.computed_style.border_radius = Corners {
        top_left: 2.0,
        top_right: 4.0,
        bottom_right: 6.0,
        bottom_left: 8.0,
    };

    let paint = build_paint_node(&root, 0.0, 0.0);

    assert_eq!(paint.style.border_radius, root.computed_style.border_radius);
}
