use super::lowering::*;
use super::retained::*;
use super::*;
use super::*;
use crate::style::{AlignSelf, Color, Display, Edges, FlexDirection, Position};
use std::cell::Cell;

fn make_node(tag: &str, width: Dimension, height: Dimension) -> WidgetNode {
    let mut node = WidgetNode::new(tag);
    node.computed_style.width = width;
    node.computed_style.height = height;
    node
}

fn keyed_node(key: &str, tag: &str, width: Dimension, height: Dimension) -> WidgetNode {
    let mut node = make_node(tag, width, height);
    node.attributes.insert("_mesh_key".into(), key.into());
    node
}

fn retained_fixture() -> WidgetNode {
    let mut root = keyed_node("root", "row", Dimension::Px(200.0), Dimension::Px(100.0));
    root.computed_style.direction = FlexDirection::Row;
    root.children = vec![
        keyed_node("root/0", "a", Dimension::Px(50.0), Dimension::Px(20.0)),
        keyed_node("root/1", "b", Dimension::Px(60.0), Dimension::Px(20.0)),
    ]
    .into();
    root
}

fn broad_retained_fixture(width: usize, depth: usize) -> WidgetNode {
    fn build(key: String, width: usize, depth: usize) -> WidgetNode {
        let mut node = keyed_node(&key, "box", Dimension::Px(20.0), Dimension::Px(20.0));
        if depth > 0 {
            node.children = (0..width)
                .map(|index| build(format!("{key}/{index}"), width, depth - 1))
                .collect();
        }
        node
    }
    build("root".into(), width, depth)
}

fn collect_keyed_layouts(node: &WidgetNode, layouts: &mut HashMap<String, LayoutRect>) {
    if let Some(key) = node.mesh_key() {
        layouts.insert(key.to_owned(), node.layout);
    }
    for child in &node.children {
        collect_keyed_layouts(child, layouts);
    }
}

fn keyed_layouts(node: &WidgetNode) -> HashMap<String, LayoutRect> {
    let mut layouts = HashMap::new();
    collect_keyed_layouts(node, &mut layouts);
    layouts
}

fn assert_layout_maps_eq(
    retained: &HashMap<String, LayoutRect>,
    fresh: &HashMap<String, LayoutRect>,
) {
    assert_eq!(retained.len(), fresh.len());
    for (key, retained_rect) in retained {
        let fresh_rect = fresh.get(key).expect("fresh layout has key");
        assert_eq!(
            (
                retained_rect.x,
                retained_rect.y,
                retained_rect.width,
                retained_rect.height
            ),
            (
                fresh_rect.x,
                fresh_rect.y,
                fresh_rect.width,
                fresh_rect.height
            ),
            "layout mismatch for {key}"
        );
    }
}

fn assert_retained_matches_fresh(mut retained: WidgetNode, mut fresh: WidgetNode) {
    let mut state = PerSurfaceLayoutState::default();
    let mut cache = IntrinsicLayoutCache::default();
    LayoutEngine::compute_incremental(
        &mut retained,
        &mut state,
        200.0,
        100.0,
        false,
        false,
        &mut cache,
        None,
    );
    LayoutEngine::compute_with_intrinsic_cache_and_measurer(
        &mut fresh,
        200.0,
        100.0,
        &mut IntrinsicLayoutCache::default(),
        None,
    );
    assert_layout_maps_eq(&keyed_layouts(&retained), &keyed_layouts(&fresh));
}

#[derive(Default)]
struct CountingMeasurer {
    calls: Cell<usize>,
}

impl TextMeasurer for CountingMeasurer {
    fn measure_text(
        &self,
        text: &str,
        _font_family: &str,
        _font_size: f32,
        _font_weight: u16,
        _line_height: f32,
        _max_width: Option<f32>,
    ) -> (f32, f32) {
        self.calls.set(self.calls.get() + 1);
        (text.len() as f32 * 8.0, 16.0)
    }
}

#[test]
fn simple_row_layout() {
    let mut root = make_node("row", Dimension::Px(300.0), Dimension::Px(50.0));
    root.computed_style.direction = FlexDirection::Row;

    let child1 = make_node("text", Dimension::Px(100.0), Dimension::Auto);
    let child2 = make_node("text", Dimension::Px(100.0), Dimension::Auto);
    root.children = vec![child1, child2].into();

    LayoutEngine::compute(&mut root, 300.0, 50.0);

    assert_eq!(root.layout.width, 300.0);
    assert_eq!(root.children[0].layout.x, 0.0);
    assert_eq!(root.children[0].layout.width, 100.0);
    assert_eq!(root.children[1].layout.x, 100.0);
    assert_eq!(root.children[1].layout.width, 100.0);
}

#[test]
fn column_with_gap() {
    let mut root = make_node("column", Dimension::Px(200.0), Dimension::Px(300.0));
    root.computed_style.direction = FlexDirection::Column;
    root.computed_style.gap = 10.0;

    let child1 = make_node("text", Dimension::Auto, Dimension::Px(50.0));
    let child2 = make_node("text", Dimension::Auto, Dimension::Px(50.0));
    root.children = vec![child1, child2].into();

    LayoutEngine::compute(&mut root, 200.0, 300.0);

    assert_eq!(root.children[0].layout.y, 0.0);
    assert_eq!(root.children[0].layout.height, 50.0);
    assert_eq!(root.children[1].layout.y, 60.0); // 50 + 10 gap
    assert_eq!(root.children[1].layout.height, 50.0);
}

#[test]
fn percentage_max_width_clamps_a_fixed_root_to_its_container() {
    // A window that asks for 920px but must never exceed the surface it
    // was given. Before `min-`/`max-` took full dimensions this could only
    // be written as a px length, so the clamp had to be restated per size.
    let mut root = make_node("column", Dimension::Px(920.0), Dimension::Px(700.0));
    root.computed_style.max_width = Dimension::Percent(100.0);
    root.computed_style.max_height = Dimension::Percent(100.0);

    LayoutEngine::compute(&mut root, 400.0, 300.0);

    assert_eq!(root.layout.width, 400.0);
    assert_eq!(root.layout.height, 300.0);
}

#[test]
fn percentage_min_and_max_constrain_children_against_the_parent() {
    let mut root = make_node("row", Dimension::Px(400.0), Dimension::Px(100.0));
    root.computed_style.direction = FlexDirection::Row;

    let mut clamped = make_node("a", Dimension::Px(300.0), Dimension::Px(20.0));
    clamped.computed_style.max_width = Dimension::Percent(25.0);
    let mut raised = make_node("b", Dimension::Px(10.0), Dimension::Px(20.0));
    raised.computed_style.min_width = Dimension::Percent(50.0);
    root.children = vec![clamped, raised].into();

    LayoutEngine::compute(&mut root, 400.0, 100.0);

    assert_eq!(root.children[0].layout.width, 100.0);
    assert_eq!(root.children[1].layout.width, 200.0);
}

#[test]
fn auto_max_width_leaves_the_element_unconstrained() {
    // `max-width: none` and `max-width: auto` both parse to `Auto`, which
    // must mean "no constraint" rather than a zero-length clamp.
    let mut root = make_node("row", Dimension::Px(400.0), Dimension::Px(100.0));
    let mut child = make_node("a", Dimension::Px(300.0), Dimension::Px(20.0));
    child.computed_style.max_width = Dimension::Auto;
    root.children = vec![child].into();

    LayoutEngine::compute(&mut root, 400.0, 100.0);

    assert_eq!(root.children[0].layout.width, 300.0);
}

#[test]
fn flex_grow_distributes_space() {
    let mut root = make_node("row", Dimension::Px(300.0), Dimension::Px(50.0));
    root.computed_style.direction = FlexDirection::Row;

    let mut child1 = make_node("a", Dimension::Auto, Dimension::Auto);
    child1.computed_style.flex_grow = 1.0;
    let mut child2 = make_node("b", Dimension::Auto, Dimension::Auto);
    child2.computed_style.flex_grow = 2.0;
    root.children = vec![child1, child2].into();

    LayoutEngine::compute(&mut root, 300.0, 50.0);

    assert!((root.children[0].layout.width - 100.0).abs() < 0.1);
    assert!((root.children[1].layout.width - 200.0).abs() < 0.1);
}

#[test]
fn padding_insets_children() {
    let mut root = make_node("row", Dimension::Px(200.0), Dimension::Px(100.0));
    root.computed_style.padding = Edges::all(10.0);

    let child = make_node("text", Dimension::Px(50.0), Dimension::Auto);
    root.children = vec![child].into();

    LayoutEngine::compute(&mut root, 200.0, 100.0);

    assert_eq!(root.children[0].layout.x, 10.0);
    assert_eq!(root.children[0].layout.y, 10.0);
}

#[test]
fn fit_sizing_does_not_double_count_trailing_padding() {
    let mut root = make_node("column", Dimension::Px(200.0), Dimension::Px(100.0));
    root.computed_style.direction = FlexDirection::Column;

    let mut panel = make_node("column", Dimension::Fit, Dimension::Fit);
    panel.computed_style.direction = FlexDirection::Column;
    panel.computed_style.align_self = AlignSelf::Start;
    panel.computed_style.padding = Edges::all(12.0);
    panel.children = vec![make_node("text", Dimension::Px(80.0), Dimension::Px(20.0))].into();
    root.children = vec![panel].into();

    LayoutEngine::compute(&mut root, 200.0, 100.0);

    let panel = &root.children[0];
    assert_eq!(panel.layout.width, 104.0);
    assert_eq!(panel.layout.height, 44.0);
    assert_eq!(panel.children[0].layout.x, 12.0);
    assert_eq!(panel.children[0].layout.y, 12.0);
}

#[test]
fn absolute_child_positioned_from_insets() {
    use crate::style::Position;

    let mut root = make_node("row", Dimension::Px(300.0), Dimension::Px(200.0));

    // An absolutely-positioned overlay in the bottom-right corner.
    let mut overlay = make_node("overlay", Dimension::Px(80.0), Dimension::Px(40.0));
    overlay.computed_style.position = Position::Absolute;
    overlay.computed_style.inset_right = Some(10.0);
    overlay.computed_style.inset_bottom = Some(10.0);

    // A normal flow child that should not be displaced by the overlay.
    let flow = make_node("content", Dimension::Px(100.0), Dimension::Auto);

    root.children = vec![flow, overlay].into();
    LayoutEngine::compute(&mut root, 300.0, 200.0);

    // Flow child starts at origin.
    assert_eq!(root.children[0].layout.x, 0.0);
    assert_eq!(root.children[0].layout.y, 0.0);

    // Overlay: right=10 → x = 300 - 80 - 10 = 210; bottom=10 → y = 200 - 40 - 10 = 150.
    assert!(
        (root.children[1].layout.x - 210.0).abs() < 0.5,
        "overlay x = {}",
        root.children[1].layout.x
    );
    assert!(
        (root.children[1].layout.y - 150.0).abs() < 0.5,
        "overlay y = {}",
        root.children[1].layout.y
    );
    assert_eq!(root.children[1].layout.width, 80.0);
    assert_eq!(root.children[1].layout.height, 40.0);
}

#[test]
fn absolute_child_with_top_left_insets() {
    let mut root = make_node("container", Dimension::Px(400.0), Dimension::Px(300.0));

    let mut tooltip = make_node("tooltip", Dimension::Px(120.0), Dimension::Px(30.0));
    tooltip.computed_style.position = Position::Absolute;
    tooltip.computed_style.inset_top = Some(20.0);
    tooltip.computed_style.inset_left = Some(50.0);

    root.children = vec![tooltip].into();
    LayoutEngine::compute(&mut root, 400.0, 300.0);

    assert!((root.children[0].layout.x - 50.0).abs() < 0.5);
    assert!((root.children[0].layout.y - 20.0).abs() < 0.5);
}

#[test]
fn absolute_position_uses_inset_edges() {
    let mut root = make_node("container", Dimension::Px(300.0), Dimension::Px(200.0));
    root.computed_style.padding = Edges::all(10.0);

    let mut panel = make_node("panel", Dimension::Auto, Dimension::Auto);
    panel.computed_style.position = Position::Absolute;
    panel.computed_style.inset_top = Some(15.0);
    panel.computed_style.inset_right = Some(30.0);
    panel.computed_style.inset_bottom = Some(25.0);
    panel.computed_style.inset_left = Some(20.0);

    root.children = vec![panel].into();
    LayoutEngine::compute(&mut root, 300.0, 200.0);

    assert_eq!(root.children[0].layout.x, 30.0);
    assert_eq!(root.children[0].layout.y, 25.0);
    assert_eq!(root.children[0].layout.width, 230.0);
    assert_eq!(root.children[0].layout.height, 140.0);
}

#[test]
fn taffy_layout_flex_basis_participates_in_growth() {
    let mut root = make_node("row", Dimension::Px(200.0), Dimension::Px(40.0));
    root.computed_style.direction = FlexDirection::Row;

    let mut basis_child = make_node("basis", Dimension::Auto, Dimension::Auto);
    basis_child.computed_style.flex_grow = 1.0;
    basis_child.computed_style.flex_shrink = 0.0;
    basis_child.computed_style.flex_basis = Dimension::Px(80.0);
    let fixed_child = make_node("fixed", Dimension::Px(40.0), Dimension::Auto);

    root.children = vec![basis_child, fixed_child].into();
    LayoutEngine::compute(&mut root, 200.0, 40.0);

    assert_eq!(root.children[0].layout.width, 160.0);
    assert_eq!(root.children[1].layout.x, 160.0);
    assert_eq!(root.children[1].layout.width, 40.0);
}

#[test]
fn display_none_excludes_node_from_layout() {
    let mut root = make_node("row", Dimension::Px(300.0), Dimension::Px(40.0));
    root.computed_style.direction = FlexDirection::Row;

    let mut hidden = make_node("hidden", Dimension::Px(100.0), Dimension::Px(20.0));
    hidden.computed_style.display = Display::None;
    let visible = make_node("visible", Dimension::Px(50.0), Dimension::Px(20.0));

    root.children = vec![hidden, visible].into();
    LayoutEngine::compute(&mut root, 300.0, 40.0);

    assert_eq!(root.children[0].layout.x, 0.0);
    assert_eq!(root.children[0].layout.y, 0.0);
    assert_eq!(root.children[0].layout.width, 0.0);
    assert_eq!(root.children[0].layout.height, 0.0);
    assert_eq!(root.children[1].layout.x, 0.0);
    assert_eq!(root.children[1].layout.width, 50.0);
}

#[test]
fn taffy_layout_text_leaf_uses_measurer() {
    let mut root = make_node("row", Dimension::Px(100.0), Dimension::Px(40.0));
    root.computed_style.direction = FlexDirection::Row;
    root.computed_style.align_items = AlignItems::Start;
    let mut child = make_node("text", Dimension::Content, Dimension::Content);
    child.attributes.insert("content".into(), "hello".into());
    root.children = vec![child].into();

    let measurer = CountingMeasurer::default();
    LayoutEngine::compute_with_measurer(&mut root, 100.0, 40.0, Some(&measurer));

    assert!(measurer.calls.get() > 0);
    assert_eq!(root.children[0].layout.width, 40.0);
    assert_eq!(root.children[0].layout.height, 16.0);
}

#[test]
fn retained_text_context_keeps_clean_content_and_replaces_changed_content() {
    let mut root = keyed_node("root", "row", Dimension::Content, Dimension::Auto);
    let mut text = keyed_node("root/text", "text", Dimension::Auto, Dimension::Auto);
    text.attributes.insert("content".into(), "hello".into());
    let text_id = text.id;
    root.children.push(text);

    let measurer = CountingMeasurer::default();
    let mut state = PerSurfaceLayoutState::default();
    let mut cache = IntrinsicLayoutCache::default();
    LayoutEngine::compute_incremental(
        &mut root,
        &mut state,
        300.0,
        40.0,
        false,
        false,
        &mut cache,
        Some(&measurer),
    );
    let first_content = Arc::clone(&state.text_nodes[&text_id].content);

    LayoutEngine::compute_incremental(
        &mut root,
        &mut state,
        300.0,
        40.0,
        true,
        false,
        &mut cache,
        Some(&measurer),
    );
    assert!(Arc::ptr_eq(
        &first_content,
        &state.text_nodes[&text_id].content
    ));

    root.children[0]
        .attributes
        .insert("content".into(), "hello world".into());
    LayoutEngine::compute_incremental(
        &mut root,
        &mut state,
        300.0,
        40.0,
        true,
        false,
        &mut cache,
        Some(&measurer),
    );

    assert!(!Arc::ptr_eq(
        &first_content,
        &state.text_nodes[&text_id].content
    ));
    assert_eq!(state.text_nodes[&text_id].content.as_ref(), "hello world");
}

#[test]
fn structural_layout_keeps_unkeyed_text_measurement_contexts() {
    let mut root = make_node("row", Dimension::Content, Dimension::Auto);
    let mut text = make_node("text", Dimension::Auto, Dimension::Auto);
    text.attributes
        .insert("content".into(), "unkeyed text".into());
    let text_id = text.id;
    root.children.push(text);

    let mut state = PerSurfaceLayoutState::default();
    let mut cache = IntrinsicLayoutCache::default();
    LayoutEngine::compute_incremental(
        &mut root, &mut state, 300.0, 40.0, false, false, &mut cache, None,
    );
    assert!(state.text_nodes.contains_key(&text_id));

    root.children
        .push(make_node("spacer", Dimension::Px(1.0), Dimension::Px(1.0)));
    LayoutEngine::compute_incremental(
        &mut root, &mut state, 300.0, 40.0, false, true, &mut cache, None,
    );

    assert!(state.text_nodes.contains_key(&text_id));
}

#[test]
fn structural_layout_indexes_unkeyed_nodes_without_reusing_them() {
    let mut root = make_node("row", Dimension::Px(100.0), Dimension::Px(20.0));
    root.computed_style.direction = FlexDirection::Row;
    let child = make_node("a", Dimension::Px(40.0), Dimension::Px(20.0));
    let child_id = child.id;
    root.children.push(child);

    let mut state = PerSurfaceLayoutState::default();
    let mut cache = IntrinsicLayoutCache::default();
    LayoutEngine::compute_incremental(
        &mut root, &mut state, 100.0, 20.0, false, false, &mut cache, None,
    );
    let prior_root = state.node_map[&root.id];
    let prior_child = state.node_map[&child_id];

    let sibling = make_node("b", Dimension::Px(30.0), Dimension::Px(20.0));
    let sibling_id = sibling.id;
    root.children.push(sibling);
    LayoutEngine::compute_incremental(
        &mut root, &mut state, 100.0, 20.0, false, true, &mut cache, None,
    );

    assert_eq!(state.node_map.len(), 3);
    assert_ne!(state.node_map[&root.id], prior_root);
    assert_ne!(state.node_map[&child_id], prior_child);
    assert!(state.node_map.contains_key(&sibling_id));
    assert_eq!(root.children[0].layout.width, 40.0);
    assert_eq!(root.children[1].layout.x, 40.0);
    assert_eq!(root.children[1].layout.width, 30.0);
}

// cargo test -p mesh-core-elements --release -- retained_text_context_reuse_beats_rebuilding_inputs --ignored --nocapture
#[test]
#[ignore = "release-only retained text-context benchmark"]
fn retained_text_context_reuse_beats_rebuilding_inputs() {
    use std::time::Instant;

    let mut root = keyed_node("root", "row", Dimension::Px(1200.0), Dimension::Auto);
    let content = "performance-sensitive text measurement content ".repeat(8);
    root.children = (0..512)
        .map(|index| {
            let mut text = keyed_node(
                &format!("root/text/{index}"),
                "text",
                Dimension::Auto,
                Dimension::Auto,
            );
            text.attributes.insert("content".into(), content.clone());
            text
        })
        .collect();

    let mut state = PerSurfaceLayoutState::default();
    let mut cache = IntrinsicLayoutCache::default();
    LayoutEngine::compute_incremental(
        &mut root, &mut state, 1200.0, 800.0, false, false, &mut cache, None,
    );
    let iterations = 2_000usize;

    let rebuild_started = Instant::now();
    for _ in 0..iterations {
        let mut text_nodes = HashMap::new();
        for text in &root.children {
            text_nodes.insert(text.id, TextMeasureData::from_node(text));
        }
        std::hint::black_box(text_nodes);
    }
    let rebuild_time = rebuild_started.elapsed();

    let retained_started = Instant::now();
    for _ in 0..iterations {
        for text in &root.children {
            let taffy_id = state.node_map[&text.id];
            update_text_context(text, &mut state.tree, taffy_id, &mut state.text_nodes)
                .expect("existing retained text node accepts a context refresh");
        }
    }
    let retained_time = retained_started.elapsed();

    let speedup = rebuild_time.as_secs_f64() / retained_time.as_secs_f64();
    eprintln!(
        "retained text contexts: rebuild {rebuild_time:?}; reuse {retained_time:?}; ratio {speedup:.2}x"
    );
    assert!(
        speedup >= 1.25,
        "retained text-context reuse regressed: {speedup:.2}x"
    );
}

#[test]
fn rtl_row_reverses_child_order() {
    use crate::style::TextDirection;

    // Container 300px wide, two children 100px each.
    let mut root = make_node("row", Dimension::Px(300.0), Dimension::Px(50.0));
    root.computed_style.direction = FlexDirection::Row;
    root.computed_style.text_direction = TextDirection::Rtl;

    let a = make_node("a", Dimension::Px(100.0), Dimension::Auto);
    let b = make_node("b", Dimension::Px(100.0), Dimension::Auto);
    root.children = vec![a, b].into();
    LayoutEngine::compute(&mut root, 300.0, 50.0);

    // In RTL the first child should be at x=200 (right side) and the second at x=100.
    assert!(
        (root.children[0].layout.x - 200.0).abs() < 0.5,
        "a.x = {}",
        root.children[0].layout.x
    );
    assert!(
        (root.children[1].layout.x - 100.0).abs() < 0.5,
        "b.x = {}",
        root.children[1].layout.x
    );
}

#[test]
fn rtl_column_is_unaffected() {
    use crate::style::TextDirection;

    let mut root = make_node("col", Dimension::Px(200.0), Dimension::Px(200.0));
    root.computed_style.direction = FlexDirection::Column;
    root.computed_style.text_direction = TextDirection::Rtl;

    let a = make_node("a", Dimension::Auto, Dimension::Px(40.0));
    let b = make_node("b", Dimension::Auto, Dimension::Px(40.0));
    root.children = vec![a, b].into();
    LayoutEngine::compute(&mut root, 200.0, 200.0);

    // Column direction is not affected by RTL — children still stack top-to-bottom.
    assert_eq!(root.children[0].layout.y, 0.0);
    assert_eq!(root.children[1].layout.y, 40.0);
}

#[test]
fn align_content_end_pushes_wrapped_lines_to_cross_end() {
    use crate::style::{AlignContent, FlexWrap};

    // Row container 100px wide, two 60px children wrap into two lines.
    // The two 20px lines occupy 40px of the 100px cross axis, leaving 60px
    // free. align-content: end must push both lines to the bottom.
    let mut root = make_node("row", Dimension::Px(100.0), Dimension::Px(100.0));
    root.computed_style.direction = FlexDirection::Row;
    root.computed_style.flex_wrap = FlexWrap::Wrap;
    root.computed_style.align_content = AlignContent::End;
    root.computed_style.align_items = AlignItems::Start;

    let a = make_node("a", Dimension::Px(60.0), Dimension::Px(20.0));
    let b = make_node("b", Dimension::Px(60.0), Dimension::Px(20.0));
    root.children = vec![a, b].into();
    LayoutEngine::compute(&mut root, 100.0, 100.0);

    // First line starts at y=60 (100 - 40 free space consumed at the start),
    // second line at y=80. Without align-content wired they would sit at 0/20.
    assert!(
        (root.children[0].layout.y - 60.0).abs() < 0.5,
        "a.y = {}",
        root.children[0].layout.y
    );
    assert!(
        (root.children[1].layout.y - 80.0).abs() < 0.5,
        "b.y = {}",
        root.children[1].layout.y
    );
}

#[test]
fn taffy_layout_text_content_changes_measurement_without_node_id_churn() {
    let mut root = make_node("row", Dimension::Content, Dimension::Auto);
    root.computed_style.direction = FlexDirection::Row;
    let mut child = make_node("text", Dimension::Auto, Dimension::Auto);
    child.attributes.insert("content".into(), "hello".into());
    let child_id = child.id;
    root.children.push(child);

    let measurer = CountingMeasurer::default();
    let mut cache = IntrinsicLayoutCache::default();

    LayoutEngine::compute_with_intrinsic_cache_and_measurer(
        &mut root,
        300.0,
        40.0,
        &mut cache,
        Some(&measurer),
    );
    let first_width = root.children[0].layout.width;

    root.children[0]
        .attributes
        .insert("content".into(), "hello world".into());
    LayoutEngine::compute_with_intrinsic_cache_and_measurer(
        &mut root,
        300.0,
        40.0,
        &mut cache,
        Some(&measurer),
    );

    assert_eq!(root.children[0].id, child_id);
    assert!(measurer.calls.get() >= 2);
    assert!(root.children[0].layout.width > first_width);
}

#[test]
fn phase47_taffy_required_layout_parity_cases() {
    let mut row = make_node("row", Dimension::Px(300.0), Dimension::Px(50.0));
    row.computed_style.direction = FlexDirection::Row;
    row.children = vec![
        make_node("a", Dimension::Px(100.0), Dimension::Px(20.0)),
        make_node("b", Dimension::Px(100.0), Dimension::Px(20.0)),
    ]
    .into();
    LayoutEngine::compute(&mut row, 300.0, 50.0);
    assert_eq!(row.children[0].layout.x, 0.0);
    assert_eq!(row.children[1].layout.x, 100.0);

    let mut nested = make_node("nested-root", Dimension::Px(300.0), Dimension::Px(80.0));
    nested.computed_style.direction = FlexDirection::Row;
    let mut nested_parent = make_node("nested-parent", Dimension::Px(120.0), Dimension::Px(40.0));
    nested_parent.computed_style.margin.left = 30.0;
    nested_parent.children = vec![make_node(
        "nested-child",
        Dimension::Px(20.0),
        Dimension::Px(20.0),
    )]
    .into();
    nested.children = vec![nested_parent].into();
    LayoutEngine::compute(&mut nested, 300.0, 80.0);
    assert_eq!(nested.children[0].layout.x, 30.0);
    assert_eq!(nested.children[0].children[0].layout.x, 30.0);

    let mut column = make_node("column", Dimension::Px(200.0), Dimension::Px(300.0));
    column.computed_style.direction = FlexDirection::Column;
    column.computed_style.gap = 10.0;
    column.children = vec![
        make_node("first", Dimension::Px(100.0), Dimension::Px(50.0)),
        make_node("second", Dimension::Px(100.0), Dimension::Px(50.0)),
    ]
    .into();
    LayoutEngine::compute(&mut column, 200.0, 300.0);
    assert_eq!(column.children[0].layout.y, 0.0);
    assert_eq!(column.children[1].layout.y, 60.0);

    let mut stack = make_node("stack", Dimension::Px(120.0), Dimension::Px(80.0));
    let mut first = make_node("first", Dimension::Px(40.0), Dimension::Px(30.0));
    first.computed_style.position = Position::Absolute;
    first.computed_style.inset_left = Some(0.0);
    first.computed_style.inset_top = Some(0.0);
    let mut second = make_node("second", Dimension::Px(40.0), Dimension::Px(30.0));
    second.computed_style.position = Position::Absolute;
    second.computed_style.inset_left = Some(0.0);
    second.computed_style.inset_top = Some(0.0);
    stack.children = vec![first, second].into();
    LayoutEngine::compute(&mut stack, 120.0, 80.0);
    assert_eq!(stack.children[0].layout.x, 0.0);
    assert_eq!(stack.children[1].layout.x, 0.0);
    assert_eq!(stack.children[0].layout.y, 0.0);
    assert_eq!(stack.children[1].layout.y, 0.0);

    let mut fixed = make_node("fixed-root", Dimension::Px(200.0), Dimension::Px(100.0));
    fixed.children = vec![make_node(
        "fixed-child",
        Dimension::Px(75.0),
        Dimension::Px(25.0),
    )]
    .into();
    LayoutEngine::compute(&mut fixed, 200.0, 100.0);
    assert_eq!(fixed.children[0].layout.width, 75.0);
    assert_eq!(fixed.children[0].layout.height, 25.0);

    let mut padded = make_node("padded", Dimension::Px(200.0), Dimension::Px(100.0));
    padded.computed_style.padding = Edges::all(10.0);
    padded.children = vec![make_node(
        "padded-child",
        Dimension::Px(50.0),
        Dimension::Px(20.0),
    )]
    .into();
    LayoutEngine::compute(&mut padded, 200.0, 100.0);
    assert_eq!(padded.children[0].layout.x, 10.0);
    assert_eq!(padded.children[0].layout.y, 10.0);

    let mut absolute = make_node("absolute-root", Dimension::Px(300.0), Dimension::Px(200.0));
    let mut overlay = make_node("overlay", Dimension::Px(80.0), Dimension::Px(40.0));
    overlay.computed_style.position = Position::Absolute;
    overlay.computed_style.inset_right = Some(10.0);
    overlay.computed_style.inset_bottom = Some(10.0);
    absolute.children = vec![overlay].into();
    LayoutEngine::compute(&mut absolute, 300.0, 200.0);
    assert!((absolute.children[0].layout.x - 210.0).abs() <= 0.5);
    assert!((absolute.children[0].layout.y - 150.0).abs() <= 0.5);

    let mut percent = make_node("percent-root", Dimension::Px(300.0), Dimension::Px(60.0));
    percent.children = vec![make_node(
        "percent-child",
        Dimension::Percent(50.0),
        Dimension::Px(20.0),
    )]
    .into();
    LayoutEngine::compute(&mut percent, 300.0, 60.0);
    assert_eq!(percent.children[0].layout.width, 150.0);
}

#[test]
fn phase87_layout_runtime_stack_spacer_divider_and_scroll_area_stay_compatible() {
    let mut stack = make_node("stack", Dimension::Px(160.0), Dimension::Px(90.0));
    let mut base = make_node("base", Dimension::Px(160.0), Dimension::Px(90.0));
    base.computed_style.position = Position::Absolute;
    base.computed_style.inset_left = Some(0.0);
    base.computed_style.inset_top = Some(0.0);
    let mut overlay = make_node("overlay", Dimension::Px(40.0), Dimension::Px(20.0));
    overlay.computed_style.position = Position::Absolute;
    overlay.computed_style.inset_left = Some(0.0);
    overlay.computed_style.inset_top = Some(0.0);
    overlay.computed_style.z_index = 1;
    stack.children = vec![base, overlay].into();
    LayoutEngine::compute(&mut stack, 160.0, 90.0);
    assert_eq!(stack.children[0].layout.x, 0.0);
    assert_eq!(stack.children[1].layout.x, 0.0);
    assert_eq!(stack.children[0].layout.y, 0.0);
    assert_eq!(stack.children[1].layout.y, 0.0);
    assert!(stack.children[1].computed_style.z_index > stack.children[0].computed_style.z_index);

    let mut row = make_node("row", Dimension::Px(240.0), Dimension::Px(24.0));
    row.computed_style.direction = FlexDirection::Row;
    let fixed = make_node("fixed", Dimension::Px(40.0), Dimension::Px(24.0));
    let mut spacer = make_node("spacer", Dimension::Auto, Dimension::Px(24.0));
    spacer.computed_style.flex_grow = 1.0;
    let divider = make_node("divider", Dimension::Px(1.0), Dimension::Px(24.0));
    row.children = vec![fixed, spacer, divider].into();
    LayoutEngine::compute(&mut row, 240.0, 24.0);
    assert_eq!(row.children[0].layout.width, 40.0);
    assert!((row.children[1].layout.width - 199.0).abs() < 0.5);
    assert_eq!(row.children[2].layout.width, 1.0);

    let mut scroll_area = make_node("scroll", Dimension::Px(120.0), Dimension::Px(60.0));
    scroll_area
        .attributes
        .insert("data-mesh-element".into(), "scroll-area".into());
    scroll_area
        .attributes
        .insert("_mesh_scroll_y".into(), "12.50".into());
    scroll_area.children = vec![make_node(
        "content",
        Dimension::Px(120.0),
        Dimension::Px(180.0),
    )]
    .into();
    LayoutEngine::compute(&mut scroll_area, 120.0, 60.0);
    assert_eq!(
        scroll_area
            .attributes
            .get("data-mesh-element")
            .map(String::as_str),
        Some("scroll-area")
    );
    assert_eq!(scroll_area.layout.width, 120.0);
    assert_eq!(scroll_area.layout.height, 60.0);
    assert_eq!(scroll_area.children[0].layout.height, 180.0);
}

#[test]
fn taffy_diagnostic_records_node_identity_and_reason() {
    let node = make_node("diagnostic-target", Dimension::Auto, Dimension::Auto);
    let mut report = TaffyLayoutReport::default();

    record_taffy_diagnostic(&mut report, &node, "unsupported layout mapping: test-only");

    assert!(!report.is_clean());
    assert_eq!(report.diagnostics.len(), 1);
    assert_eq!(report.diagnostics[0].node_id, node.id);
    assert_eq!(report.diagnostics[0].tag, "diagnostic-target");
    assert_eq!(
        report.diagnostics[0].reason,
        "unsupported layout mapping: test-only"
    );
}

#[test]
fn content_dimension_taffy_diagnostic_is_expected_measurement_noise() {
    assert!(is_expected_taffy_measurement_diagnostic(
        CONTENT_DIMENSION_TAFFY_DIAGNOSTIC
    ));
    assert!(!is_expected_taffy_measurement_diagnostic(
        "unsupported layout mapping: test-only"
    ));
}

#[test]
fn compute_incremental_fresh_build_matches_baseline() {
    assert_retained_matches_fresh(retained_fixture(), retained_fixture());
}

#[test]
fn compute_incremental_visual_repaint_preserves_layout() {
    let mut root = retained_fixture();
    let mut state = PerSurfaceLayoutState::default();
    let mut cache = IntrinsicLayoutCache::default();
    LayoutEngine::compute_incremental(
        &mut root, &mut state, 200.0, 100.0, false, false, &mut cache, None,
    );
    let before = keyed_layouts(&root);

    root.children[0].computed_style.background_color = Color {
        r: 10,
        g: 20,
        b: 30,
        a: 255,
    };
    LayoutEngine::compute_incremental(
        &mut root, &mut state, 200.0, 100.0, false, false, &mut cache, None,
    );

    assert_layout_maps_eq(&keyed_layouts(&root), &before);
}

// cargo test -p mesh-core-elements --release -- retained_layout_paint_only_fast_path_beats_tree_sync --ignored --nocapture
#[test]
#[ignore = "release-only retained layout paint-only microbenchmark"]
fn retained_layout_paint_only_fast_path_beats_tree_sync() {
    use std::time::Instant;

    let mut root = broad_retained_fixture(4, 5);
    let mut state = PerSurfaceLayoutState::default();
    let mut cache = IntrinsicLayoutCache::default();
    LayoutEngine::compute_incremental(
        &mut root, &mut state, 1200.0, 800.0, false, false, &mut cache, None,
    );
    let iterations = 2_000;

    let old_started = Instant::now();
    for _ in 0..iterations {
        let mut node_map = HashMap::new();
        let mut report = TaffyLayoutReport::default();
        update_retained_node_styles(
            std::hint::black_box(&root),
            &mut state,
            false,
            None,
            &mut report,
        );
        collect_taffy_node_map(&root, &state, &mut node_map);
        std::hint::black_box((node_map, report));
    }
    let old_time = old_started.elapsed();

    let fast_started = Instant::now();
    for _ in 0..iterations {
        LayoutEngine::compute_incremental(
            std::hint::black_box(&mut root),
            &mut state,
            1200.0,
            800.0,
            false,
            false,
            &mut cache,
            None,
        );
    }
    let fast_time = fast_started.elapsed();

    eprintln!(
        "retained layout paint-only: old tree sync {old_time:?}; fast path {fast_time:?}; ratio {:.1}x",
        old_time.as_secs_f64() / fast_time.as_secs_f64()
    );
    assert!(fast_time * 10 < old_time);
}

#[test]
fn retained_layout_parity_style_only() {
    let mut retained = retained_fixture();
    let mut state = PerSurfaceLayoutState::default();
    let mut cache = IntrinsicLayoutCache::default();
    LayoutEngine::compute_incremental(
        &mut retained,
        &mut state,
        200.0,
        100.0,
        false,
        false,
        &mut cache,
        None,
    );

    retained.children[0].computed_style.opacity = 0.5;
    let mut fresh = retained.clone();
    LayoutEngine::compute_incremental(
        &mut retained,
        &mut state,
        200.0,
        100.0,
        false,
        false,
        &mut cache,
        None,
    );
    LayoutEngine::compute_with_intrinsic_cache_and_measurer(
        &mut fresh,
        200.0,
        100.0,
        &mut IntrinsicLayoutCache::default(),
        None,
    );

    assert_layout_maps_eq(&keyed_layouts(&retained), &keyed_layouts(&fresh));
}

#[test]
fn retained_layout_parity_layout_dirty() {
    let mut retained = retained_fixture();
    let mut state = PerSurfaceLayoutState::default();
    let mut cache = IntrinsicLayoutCache::default();
    LayoutEngine::compute_incremental(
        &mut retained,
        &mut state,
        200.0,
        100.0,
        false,
        false,
        &mut cache,
        None,
    );

    retained.children[0].computed_style.width = Dimension::Px(80.0);
    let mut fresh = retained.clone();
    LayoutEngine::compute_incremental(
        &mut retained,
        &mut state,
        200.0,
        100.0,
        true,
        false,
        &mut cache,
        None,
    );
    LayoutEngine::compute_with_intrinsic_cache_and_measurer(
        &mut fresh,
        200.0,
        100.0,
        &mut IntrinsicLayoutCache::default(),
        None,
    );

    assert_layout_maps_eq(&keyed_layouts(&retained), &keyed_layouts(&fresh));
}

#[test]
fn retained_layout_syncs_only_known_dirty_styles() {
    let mut retained = retained_fixture();
    let mut state = PerSurfaceLayoutState::default();
    let mut cache = IntrinsicLayoutCache::default();
    LayoutEngine::compute_incremental(
        &mut retained,
        &mut state,
        200.0,
        100.0,
        false,
        false,
        &mut cache,
        None,
    );

    retained.children[0].computed_style.width = Dimension::Px(80.0);
    let dirty_ids = HashSet::from([retained.children[0].id]);
    let mut fresh = retained.clone();
    LayoutEngine::compute_incremental_with_dirty_nodes(
        &mut retained,
        &mut state,
        200.0,
        100.0,
        true,
        false,
        Some(&dirty_ids),
        &mut cache,
        None,
    );
    LayoutEngine::compute_with_intrinsic_cache_and_measurer(
        &mut fresh,
        200.0,
        100.0,
        &mut IntrinsicLayoutCache::default(),
        None,
    );

    assert_layout_maps_eq(&keyed_layouts(&retained), &keyed_layouts(&fresh));
}

#[test]
fn paint_only_fast_path_defers_style_sync_until_layout_is_dirty() {
    let mut retained = retained_fixture();
    let mut state = PerSurfaceLayoutState::default();
    let mut cache = IntrinsicLayoutCache::default();
    LayoutEngine::compute_incremental(
        &mut retained,
        &mut state,
        200.0,
        100.0,
        false,
        false,
        &mut cache,
        None,
    );

    retained.children[0].computed_style.width = Dimension::Px(80.0);
    LayoutEngine::compute_incremental(
        &mut retained,
        &mut state,
        200.0,
        100.0,
        false,
        false,
        &mut cache,
        None,
    );
    assert_eq!(retained.children[0].layout.width, 50.0);

    LayoutEngine::compute_incremental(
        &mut retained,
        &mut state,
        200.0,
        100.0,
        true,
        false,
        &mut cache,
        None,
    );
    assert_eq!(retained.children[0].layout.width, 80.0);
}

#[test]
fn retained_layout_parity_add_node() {
    let mut retained = retained_fixture();
    let mut state = PerSurfaceLayoutState::default();
    let mut cache = IntrinsicLayoutCache::default();
    LayoutEngine::compute_incremental(
        &mut retained,
        &mut state,
        200.0,
        100.0,
        false,
        false,
        &mut cache,
        None,
    );

    retained.children.push(keyed_node(
        "root/2",
        "c",
        Dimension::Px(40.0),
        Dimension::Px(20.0),
    ));
    let mut fresh = retained.clone();
    LayoutEngine::compute_incremental(
        &mut retained,
        &mut state,
        200.0,
        100.0,
        false,
        true,
        &mut cache,
        None,
    );
    LayoutEngine::compute_with_intrinsic_cache_and_measurer(
        &mut fresh,
        200.0,
        100.0,
        &mut IntrinsicLayoutCache::default(),
        None,
    );

    assert_layout_maps_eq(&keyed_layouts(&retained), &keyed_layouts(&fresh));
}

#[test]
fn retained_layout_parity_remove_node() {
    let mut retained = retained_fixture();
    let mut state = PerSurfaceLayoutState::default();
    let mut cache = IntrinsicLayoutCache::default();
    LayoutEngine::compute_incremental(
        &mut retained,
        &mut state,
        200.0,
        100.0,
        false,
        false,
        &mut cache,
        None,
    );

    retained.children.remove(0);
    let mut fresh = retained.clone();
    LayoutEngine::compute_incremental(
        &mut retained,
        &mut state,
        200.0,
        100.0,
        false,
        true,
        &mut cache,
        None,
    );
    LayoutEngine::compute_with_intrinsic_cache_and_measurer(
        &mut fresh,
        200.0,
        100.0,
        &mut IntrinsicLayoutCache::default(),
        None,
    );

    assert_layout_maps_eq(&keyed_layouts(&retained), &keyed_layouts(&fresh));
}

#[test]
fn retained_layout_parity_reorder() {
    let mut retained = retained_fixture();
    let mut state = PerSurfaceLayoutState::default();
    let mut cache = IntrinsicLayoutCache::default();
    LayoutEngine::compute_incremental(
        &mut retained,
        &mut state,
        200.0,
        100.0,
        false,
        false,
        &mut cache,
        None,
    );

    retained.children.swap(0, 1);
    let mut fresh = retained.clone();
    LayoutEngine::compute_incremental(
        &mut retained,
        &mut state,
        200.0,
        100.0,
        false,
        true,
        &mut cache,
        None,
    );
    LayoutEngine::compute_with_intrinsic_cache_and_measurer(
        &mut fresh,
        200.0,
        100.0,
        &mut IntrinsicLayoutCache::default(),
        None,
    );

    assert_layout_maps_eq(&keyed_layouts(&retained), &keyed_layouts(&fresh));
}

#[test]
fn retained_structural_layout_preserves_taffy_node_identity() {
    let mut retained = retained_fixture();
    let mut state = PerSurfaceLayoutState::default();
    let mut cache = IntrinsicLayoutCache::default();
    LayoutEngine::compute_incremental(
        &mut retained,
        &mut state,
        200.0,
        100.0,
        false,
        false,
        &mut cache,
        None,
    );
    let original_ids = state.node_map.clone();

    retained.children.swap(0, 1);
    retained.children.push(keyed_node(
        "root/2",
        "c",
        Dimension::Px(40.0),
        Dimension::Px(20.0),
    ));
    LayoutEngine::compute_incremental(
        &mut retained,
        &mut state,
        200.0,
        100.0,
        false,
        true,
        &mut cache,
        None,
    );

    for (node_id, taffy_id) in original_ids {
        assert_eq!(state.node_map.get(&node_id), Some(&taffy_id));
    }
    assert_eq!(state.node_map.len(), 4);
}

// cargo test -p mesh-core-elements --release -- retained_structural_layout_beats_fresh_tree_rebuild --ignored --nocapture
#[test]
#[ignore = "release-only retained structural layout benchmark"]
fn retained_structural_layout_beats_fresh_tree_rebuild() {
    use std::time::Instant;

    let iterations = 200;
    let mut retained = broad_retained_fixture(4, 5);
    let mut retained_state = PerSurfaceLayoutState::default();
    let mut retained_cache = IntrinsicLayoutCache::default();
    LayoutEngine::compute_incremental(
        &mut retained,
        &mut retained_state,
        1200.0,
        800.0,
        false,
        false,
        &mut retained_cache,
        None,
    );
    let original_ids = retained_state.node_map.clone();

    let retained_started = Instant::now();
    for _ in 0..iterations {
        retained.children.swap(0, 1);
        LayoutEngine::compute_incremental(
            std::hint::black_box(&mut retained),
            &mut retained_state,
            1200.0,
            800.0,
            false,
            true,
            &mut retained_cache,
            None,
        );
    }
    let retained_time = retained_started.elapsed();

    let mut fresh = broad_retained_fixture(4, 5);
    let mut fresh_cache = IntrinsicLayoutCache::default();
    let fresh_started = Instant::now();
    for _ in 0..iterations {
        fresh.children.swap(0, 1);
        LayoutEngine::compute_with_intrinsic_cache_and_measurer(
            std::hint::black_box(&mut fresh),
            1200.0,
            800.0,
            &mut fresh_cache,
            None,
        );
    }
    let fresh_time = fresh_started.elapsed();

    assert_layout_maps_eq(&keyed_layouts(&retained), &keyed_layouts(&fresh));
    for (node_id, taffy_id) in original_ids {
        assert_eq!(retained_state.node_map.get(&node_id), Some(&taffy_id));
    }
    eprintln!(
        "structural layout: fresh tree {fresh_time:?}; retained reconcile {retained_time:?}; ratio {:.1}x",
        fresh_time.as_secs_f64() / retained_time.as_secs_f64()
    );
    assert!(retained_time * 5 < fresh_time * 4);
}

#[test]
fn remove_taffy_subtree_removes_all_descendants() {
    // Build a 4-node TaffyTree: grandparent → parent → leaf + sibling leaf.
    let mut tree = TaffyTree::<NodeId>::new();
    let leaf1 = tree.new_leaf(taffy_style::Style::default()).unwrap();
    let leaf2 = tree.new_leaf(taffy_style::Style::default()).unwrap();
    let parent = tree
        .new_with_children(taffy_style::Style::default(), &[leaf1, leaf2])
        .unwrap();
    let leaf3 = tree.new_leaf(taffy_style::Style::default()).unwrap();
    let grandparent = tree
        .new_with_children(taffy_style::Style::default(), &[parent, leaf3])
        .unwrap();

    assert!(
        tree.total_node_count() >= 4,
        "sanity: tree should have at least 4 nodes"
    );

    // Remove the root → post-order walks children first.
    remove_taffy_subtree(&mut tree, grandparent).unwrap();

    assert_eq!(
        tree.total_node_count(),
        0,
        "all nodes should be removed after post-order subtree removal"
    );
}

#[test]
fn per_surface_layout_state_default_is_invalid() {
    let state = PerSurfaceLayoutState::new();
    assert!(!state.valid);
    assert!(state.node_map.is_empty());
    assert_eq!(state.last_available, (0.0, 0.0));
    assert_eq!(state.tree.total_node_count(), 0);
}

#[test]
fn fixed_child_positioned_from_viewport() {
    let mut root = make_node("root", Dimension::Px(960.0), Dimension::Px(540.0));
    let mut inner = make_node("inner", Dimension::Px(200.0), Dimension::Px(100.0));
    inner.layout.x = 0.0;
    inner.layout.y = 0.0;
    let mut overlay = make_node("overlay", Dimension::Px(100.0), Dimension::Px(40.0));
    overlay.computed_style.position = Position::Fixed;
    overlay.computed_style.inset_right = Some(10.0);
    overlay.computed_style.inset_bottom = Some(8.0);
    inner.children = vec![overlay].into();
    root.children = vec![inner].into();
    LayoutEngine::compute(&mut root, 960.0, 540.0);
    // Fixed: bottom-right corner of the 960x540 viewport
    assert!(
        (root.children[0].children[0].layout.x - 850.0).abs() < 0.5,
        "expected x≈850, got {}",
        root.children[0].children[0].layout.x
    );
    assert!(
        (root.children[0].children[0].layout.y - 492.0).abs() < 0.5,
        "expected y≈492, got {}",
        root.children[0].children[0].layout.y
    );
}

#[test]
fn fixed_child_top_left_positioned_from_viewport() {
    let mut root = make_node("root", Dimension::Px(800.0), Dimension::Px(600.0));
    let mut panel = make_node("panel", Dimension::Px(400.0), Dimension::Px(300.0));
    panel.computed_style.padding = Edges::all(20.0);
    let mut tooltip = make_node("tooltip", Dimension::Px(120.0), Dimension::Px(30.0));
    tooltip.computed_style.position = Position::Fixed;
    tooltip.computed_style.inset_top = Some(50.0);
    tooltip.computed_style.inset_left = Some(100.0);
    panel.children = vec![tooltip].into();
    root.children = vec![panel].into();
    LayoutEngine::compute(&mut root, 800.0, 600.0);
    let tooltip_layout = &root.children[0].children[0].layout;
    assert!(
        (tooltip_layout.x - 100.0).abs() < 0.5,
        "expected x≈100, got {}",
        tooltip_layout.x
    );
    assert!(
        (tooltip_layout.y - 50.0).abs() < 0.5,
        "expected y≈50, got {}",
        tooltip_layout.y
    );
}

#[test]
fn fixed_child_full_width_stretch() {
    let mut root = make_node("root", Dimension::Px(1920.0), Dimension::Px(1080.0));
    let mut inner = make_node("inner", Dimension::Px(400.0), Dimension::Px(200.0));
    let mut bar = make_node("bar", Dimension::Auto, Dimension::Px(40.0));
    bar.computed_style.position = Position::Fixed;
    bar.computed_style.inset_top = Some(0.0);
    bar.computed_style.inset_left = Some(0.0);
    bar.computed_style.inset_right = Some(0.0);
    inner.children = vec![bar].into();
    root.children = vec![inner].into();
    LayoutEngine::compute(&mut root, 1920.0, 1080.0);
    let bar_layout = &root.children[0].children[0].layout;
    assert!(
        (bar_layout.x - 0.0).abs() < 0.5,
        "expected x=0, got {}",
        bar_layout.x
    );
    assert!(
        (bar_layout.width - 1920.0).abs() < 0.5,
        "expected width=1920, got {}",
        bar_layout.width
    );
}

// cargo test -p mesh-core-elements --release -- shared_text_measure_content_beats_two_string_clones --ignored --nocapture
#[test]
#[ignore = "release-only text measurement content microbenchmark"]
fn shared_text_measure_content_beats_two_string_clones() {
    use std::time::Instant;

    let content = "performance-sensitive text measurement content ".repeat(8);
    let iterations = 1_000_000usize;

    let string_started = Instant::now();
    let mut string_total = 0usize;
    for _ in 0..iterations {
        let measure_data = std::hint::black_box(content.clone());
        let cache_key = std::hint::black_box(measure_data.clone());
        string_total = string_total.wrapping_add(cache_key.len());
    }
    let string_time = string_started.elapsed();

    let shared_started = Instant::now();
    let mut shared_total = 0usize;
    for _ in 0..iterations {
        let measure_data: Arc<str> = std::hint::black_box(Arc::from(content.as_str()));
        let cache_key = std::hint::black_box(Arc::clone(&measure_data));
        shared_total = shared_total.wrapping_add(cache_key.len());
    }
    let shared_time = shared_started.elapsed();

    eprintln!(
        "text measurement content: two String clones {string_time:?}; Arc build+clone {shared_time:?}; ratio {:.1}x; totals={string_total}/{shared_total}",
        string_time.as_secs_f64() / shared_time.as_secs_f64()
    );
    assert_eq!(string_total, shared_total);
    assert!(shared_time < string_time);
}

// cargo test -p mesh-core-elements --release -- borrowed_live_layout_keys_beat_cloned_key_set --ignored --nocapture
#[test]
#[ignore = "release-only retained layout key collection microbenchmark"]
fn borrowed_live_layout_keys_beat_cloned_key_set() {
    use std::time::Instant;

    let keys = (0..1_024)
        .map(|index| format!("root/content/list/row/{index}/label"))
        .collect::<Vec<_>>();
    let iterations = 5_000usize;

    let cloned_started = Instant::now();
    let mut cloned_total = 0usize;
    for _ in 0..iterations {
        let set = keys
            .iter()
            .map(|key| std::hint::black_box(key.clone()))
            .collect::<HashSet<String>>();
        cloned_total = cloned_total.wrapping_add(set.len());
    }
    let cloned_time = cloned_started.elapsed();

    let borrowed_started = Instant::now();
    let mut borrowed_total = 0usize;
    for _ in 0..iterations {
        let set = keys
            .iter()
            .map(|key| std::hint::black_box(key.as_str()))
            .collect::<HashSet<&str>>();
        borrowed_total = borrowed_total.wrapping_add(set.len());
    }
    let borrowed_time = borrowed_started.elapsed();

    eprintln!(
        "retained layout live keys: cloned {cloned_time:?}; borrowed {borrowed_time:?}; ratio {:.1}x; totals={cloned_total}/{borrowed_total}",
        cloned_time.as_secs_f64() / borrowed_time.as_secs_f64()
    );
    assert_eq!(cloned_total, borrowed_total);
    assert!(borrowed_time < cloned_time);
}

// cargo test -p mesh-core-elements --release -- node_id_layout_lookup_beats_string_key_hashing --ignored --nocapture
#[test]
#[ignore = "release-only retained layout identity microbenchmark"]
fn node_id_layout_lookup_beats_string_key_hashing() {
    use std::time::Instant;

    let keys = (0..1_024)
        .map(|index| format!("root/content/list/row/{index}/label"))
        .collect::<Vec<_>>();
    let string_map = keys
        .iter()
        .enumerate()
        .map(|(index, key)| (key.clone(), index))
        .collect::<HashMap<_, _>>();
    let id_map = (0..1_024u64)
        .enumerate()
        .map(|(index, id)| (id + 1, index))
        .collect::<HashMap<_, _>>();
    let iterations = 5_000_000usize;

    let string_started = Instant::now();
    let mut string_total = 0usize;
    for index in 0..iterations {
        string_total = string_total.wrapping_add(
            *string_map
                .get(std::hint::black_box(&keys[index & 1_023]))
                .unwrap(),
        );
    }
    let string_time = string_started.elapsed();

    let id_started = Instant::now();
    let mut id_total = 0usize;
    for index in 0..iterations {
        let id = (index & 1_023) as u64 + 1;
        id_total = id_total.wrapping_add(*id_map.get(std::hint::black_box(&id)).unwrap());
    }
    let id_time = id_started.elapsed();

    eprintln!(
        "retained layout identity lookup: String {string_time:?}; NodeId {id_time:?}; ratio {:.1}x; totals={string_total}/{id_total}",
        string_time.as_secs_f64() / id_time.as_secs_f64()
    );
    assert_eq!(string_total, id_total);
    assert!(id_time < string_time);
}
