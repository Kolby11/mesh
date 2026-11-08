use super::*;
use mesh_core_interaction::find_scrollable_at_with_limits;
use mesh_core_render::{FractionalScale, PixelBuffer};
use mesh_core_theme::Theme;

#[test]
fn scroll_contract_handles_repeated_input_without_script_or_theme_rules() {
    let theme = Theme::new("empty", "Empty");
    for tag in ["scroll-area", "scroll", "scroll-view"] {
        for horizontal in [false, true] {
            for scale in [1.0, 1.25, 2.0] {
                let axis_style = if horizontal {
                    "overflow-x: auto; overflow-y: hidden; flex-direction: row;"
                } else {
                    ""
                };
                let source = format!(
                    r#"
<template><box class="host"><{tag} class="viewport"><box class="content" /></{tag}></box></template>
<style>
.host {{ width: 200px; height: 140px; }}
.viewport {{ width: 120px; height: 80px; {axis_style} }}
.content {{ width: 600px; height: 480px; flex-shrink: 0; }}
</style>
"#
                );
                let mut component = test_frontend_component(&source);
                Arc::make_mut(&mut component.settings)
                    .set_namespace("shell", serde_json::json!({"motion":{"reduced":true}}));
                let scaling = FractionalScale::new(scale);
                let mut pixels =
                    PixelBuffer::new(scaling.physical_extent(200), scaling.physical_extent(140));
                let extent = SurfaceExtent::unpadded(200, 140);
                component
                    .paint_raster(&theme, extent, &mut pixels, scale)
                    .unwrap();
                let tree = component.last_tree.as_ref().unwrap();
                let viewport = first_node_by_tag(tree, "scroll").unwrap();
                let id = viewport.id;
                let layout = viewport.layout;
                let content_layout = viewport.children[0].layout;
                let metrics = viewport.resolved_scroll_metrics();
                let max = if horizontal {
                    metrics.max_x
                } else {
                    metrics.max_y
                };
                assert!(max > 0.0, "<{tag}> must scroll with an empty theme");
                let (x, y) = (layout.x + 10.0, layout.y + 10.0);
                let mut expected = 0.0_f32;
                for step in 0..160 {
                    let delta = if step < 80 { 9.25 } else { -9.25 };
                    let hit =
                        find_scrollable_at_with_limits(component.last_tree.as_ref().unwrap(), x, y)
                            .expect("scroll ownership survives boundary reversal");
                    assert_eq!(hit.node_id, id);
                    let wheel = step % 3 == 0;
                    let distance = if wheel { delta / 28.0 } else { delta };
                    let (dx, dy) = if horizontal {
                        (-distance, 0.0)
                    } else {
                        (0.0, -distance)
                    };
                    let input = if wheel {
                        ComponentInput::Scroll { x, y, dx, dy }
                    } else {
                        ComponentInput::TwoFingerScroll { x, y, dx, dy }
                    };
                    component.handle_input(&theme, 200, 140, input).unwrap();
                    component
                        .paint_raster(&theme, extent, &mut pixels, scale)
                        .unwrap();
                    expected = (expected + delta).clamp(0.0, max);
                    let viewport =
                        first_node_by_tag(component.last_tree.as_ref().unwrap(), "scroll").unwrap();
                    let metrics = viewport.resolved_scroll_metrics();
                    let actual = if horizontal { metrics.x } else { metrics.y };
                    assert!(
                        (actual - expected).abs() < 0.001,
                        "<{tag}>, horizontal={horizontal}, scale={scale}, step={step}: {actual} != {expected}"
                    );
                    assert_eq!(viewport.layout, layout);
                    assert_eq!(viewport.children[0].layout, content_layout);
                    assert_eq!(
                        if horizontal {
                            metrics.max_x
                        } else {
                            metrics.max_y
                        },
                        max
                    );
                }
                assert!(expected.abs() < 0.001, "reverse input reaches the start");
            }
        }
    }
}

#[test]
fn windowed_list_rebuilds_its_window_around_the_scroll_position() {
    let theme = Theme::new("empty", "Empty");
    let source = r#"
<template><box class="host"><scroll class="viewport"><box class="list">{#for item in items window=20 overscan=2}<box class="row"><text>{item}</text></box>{/for}</box></scroll></box></template>
<script lang="luau">
items = {}
for i = 1, 200 do
  items[i] = i
end
</script>
<style>
.host { width: 200px; height: 140px; }
.viewport { width: 120px; height: 100px; }
.list { flex-direction: column; }
.row { height: 20px; flex-shrink: 0; }
</style>
"#;
    let mut component = test_frontend_component(source);
    Arc::make_mut(&mut component.settings)
        .set_namespace("shell", serde_json::json!({"motion":{"reduced":true}}));
    let mut pixels = PixelBuffer::new(200, 140);
    let extent = SurfaceExtent::unpadded(200, 140);
    component
        .paint_raster(&theme, extent, &mut pixels, 1.0)
        .unwrap();

    let list_children = |component: &FrontendSurfaceComponent| {
        let tree = component.last_tree.as_ref().unwrap();
        first_node_by_tag(tree, "scroll").unwrap().children[0]
            .children
            .len()
    };
    let content_max = |component: &FrontendSurfaceComponent| {
        let tree = component.last_tree.as_ref().unwrap();
        first_node_by_tag(tree, "scroll")
            .unwrap()
            .resolved_scroll_metrics()
            .max_y
    };
    // 24 initial items plus overscan, then one trailing spacer.
    assert_eq!(list_children(&component), 26 + 1);
    let max = content_max(&component);
    assert!(max > 3_000.0, "spacers keep the full scroll extent: {max}");
    assert!(!rendered_text(&component).contains(&"100".to_string()));

    let scroll_tree = component.last_tree.as_ref().unwrap();
    let viewport = first_node_by_tag(scroll_tree, "scroll").unwrap();
    let (x, y) = (viewport.layout.x + 10.0, viewport.layout.y + 10.0);
    component
        .handle_input(
            &theme,
            200,
            140,
            ComponentInput::TwoFingerScroll {
                x,
                y,
                dx: 0.0,
                dy: -2_000.0,
            },
        )
        .unwrap();
    for _ in 0..4 {
        component
            .paint_raster(&theme, extent, &mut pixels, 1.0)
            .unwrap();
    }

    let text = rendered_text(&component);
    assert!(text.contains(&"100".to_string()), "{text:?}");
    assert!(!text.contains(&"1".to_string()), "{text:?}");
    assert!(
        list_children(&component) < 40,
        "only the window plus spacers is built"
    );
    let tree = component.last_tree.as_ref().unwrap();
    let viewport = first_node_by_tag(tree, "scroll").unwrap();
    let metrics = viewport.resolved_scroll_metrics();
    assert!(
        (metrics.y - 2_000.0).abs() < 1.0,
        "scroll offset survives the rebuild: {}",
        metrics.y
    );
    assert!((metrics.max_y - max).abs() < 1.0, "extent is unchanged");
}

/// A scroller holding `rows` items; `row_style` varies the height of row 5.
fn list_source(rows: usize, header: &str, row_class: &str, extra_style: &str) -> String {
    format!(
        r#"
<template><box class="host"><scroll class="viewport"><box class="list">{{#for item in items {header}}}<box class="{row_class}"><text>{{item}}</text></box>{{/for}}</box></scroll></box></template>
<script lang="luau">
items = {{}}
for i = 1, {rows} do
  items[i] = i
end
</script>
<style>
.host {{ width: 200px; height: 140px; }}
.viewport {{ width: 120px; height: 100px; }}
.list {{ flex-direction: column; }}
.row {{ height: 20px; flex-shrink: 0; }}
{extra_style}
</style>
"#
    )
}

fn paint_frames(
    component: &mut FrontendSurfaceComponent,
    theme: &Theme,
    pixels: &mut PixelBuffer,
    frames: usize,
) {
    let extent = SurfaceExtent::unpadded(200, 140);
    for _ in 0..frames {
        component.paint_raster(theme, extent, pixels, 1.0).unwrap();
    }
}

fn scroll_list_children(component: &FrontendSurfaceComponent) -> usize {
    first_node_by_tag(component.last_tree.as_ref().unwrap(), "scroll")
        .unwrap()
        .children[0]
        .children
        .len()
}

#[test]
fn large_plain_list_is_windowed_automatically_and_scrolls_like_a_full_one() {
    let theme = Theme::new("empty", "Empty");
    let mut component = test_frontend_component(&list_source(2_000, "", "row", ""));
    Arc::make_mut(&mut component.settings)
        .set_namespace("shell", serde_json::json!({"motion":{"reduced":true}}));
    let mut pixels = PixelBuffer::new(200, 140);
    paint_frames(&mut component, &theme, &mut pixels, 4);

    let tree = component.last_tree.as_ref().unwrap();
    let viewport = first_node_by_tag(tree, "scroll").unwrap();
    let max = viewport.resolved_scroll_metrics().max_y;
    assert!(
        scroll_list_children(&component) < 40,
        "only a window of the 2,000 rows is built"
    );
    assert!(
        (max - (2_000.0 * 20.0 - 100.0)).abs() < 1.0,
        "scroll extent equals the full list's: {max}"
    );

    let (x, y) = (viewport.layout.x + 10.0, viewport.layout.y + 10.0);
    component
        .handle_input(
            &theme,
            200,
            140,
            ComponentInput::TwoFingerScroll {
                x,
                y,
                dx: 0.0,
                dy: -50_000.0,
            },
        )
        .unwrap();
    paint_frames(&mut component, &theme, &mut pixels, 4);
    let text = rendered_text(&component);
    assert!(text.contains(&"2000".to_string()), "{text:?}");
    assert!(!text.contains(&"1".to_string()), "{text:?}");
    assert!(scroll_list_children(&component) < 40);
    let viewport = first_node_by_tag(component.last_tree.as_ref().unwrap(), "scroll").unwrap();
    assert!((viewport.resolved_scroll_metrics().y - max).abs() < 1.0);
}

#[test]
fn large_plain_list_with_uneven_rows_is_windowed_and_scrolls_to_the_end() {
    let theme = Theme::new("empty", "Empty");
    let source = list_source(2_000, "", "row", "").replace(
        r#"<box class="row">"#,
        r#"<box class="row" style:height={item % 7 == 0 and "55px" or "20px"}>"#,
    );
    let mut component = test_frontend_component(&source);
    Arc::make_mut(&mut component.settings)
        .set_namespace("shell", serde_json::json!({"motion":{"reduced":true}}));
    let mut pixels = PixelBuffer::new(200, 140);
    paint_frames(&mut component, &theme, &mut pixels, 6);
    assert!(
        scroll_list_children(&component) < 100,
        "uneven rows are windowed, {} built",
        scroll_list_children(&component)
    );

    let viewport = first_node_by_tag(component.last_tree.as_ref().unwrap(), "scroll").unwrap();
    let (x, y) = (viewport.layout.x + 10.0, viewport.layout.y + 10.0);
    for _ in 0..4 {
        component
            .handle_input(
                &theme,
                200,
                140,
                ComponentInput::TwoFingerScroll {
                    x,
                    y,
                    dx: 0.0,
                    dy: -100_000.0,
                },
            )
            .unwrap();
        paint_frames(&mut component, &theme, &mut pixels, 6);
    }
    let text = rendered_text(&component);
    assert!(text.contains(&"2000".to_string()), "{text:?}");
    assert!(!text.contains(&"1".to_string()), "{text:?}");
    assert!(scroll_list_children(&component) < 100);
}

#[test]
fn a_windowed_list_is_settled_by_its_first_paint() {
    let theme = Theme::new("empty", "Empty");
    let mut component = test_frontend_component(&list_source(2_000, "", "row", ""));
    Arc::make_mut(&mut component.settings)
        .set_namespace("shell", serde_json::json!({"motion":{"reduced":true}}));
    let mut pixels = PixelBuffer::new(200, 140);
    paint_frames(&mut component, &theme, &mut pixels, 1);
    let viewport = first_node_by_tag(component.last_tree.as_ref().unwrap(), "scroll").unwrap();
    let max = viewport.resolved_scroll_metrics().max_y;
    assert!(
        (max - (2_000.0 * 20.0 - 100.0)).abs() < 1.0,
        "the first presented frame already has the full scroll extent: {max}"
    );
    assert!(scroll_list_children(&component) < 40);
    assert!(
        !component.wants_render(),
        "nothing is left to settle after the first paint"
    );
}

/// Counts the nodes a pruned traversal descends into.
struct CountingExtents<'a> {
    inner: &'a dyn mesh_core_interaction::PaintedExtents,
    visited: std::cell::Cell<usize>,
}

impl mesh_core_interaction::PaintedExtents for CountingExtents<'_> {
    fn subtree_bounds(
        &self,
        id: mesh_core_elements::NodeId,
    ) -> Option<mesh_core_interaction::ContentBounds> {
        self.inner.subtree_bounds(id)
    }

    fn is_retained_scroll_owner(&self, id: mesh_core_elements::NodeId) -> bool {
        self.visited.set(self.visited.get() + 1);
        self.inner.is_retained_scroll_owner(id)
    }
}

/// Pruned hit tests must agree with the full traversal at every point, at the
/// top, in the middle and at the end of a fully built uneven list.
#[test]
fn painted_extent_pruning_matches_the_full_hit_test_on_a_scrolled_uneven_list() {
    use mesh_core_interaction::{
        PaintedExtents, pointer_event_handler_hit, pointer_event_handler_hit_pruned,
        pointer_hit_test, pointer_hit_test_pruned, pointer_press_hit, pointer_press_hit_pruned,
    };
    let theme = Theme::new("empty", "Empty");
    let source = list_source(90, "", "row", "")
        .replace(
            r#"<box class="row">"#,
            r#"<button class="row" onclick={function() end} style:height={item == 5 and "55px" or "20px"}>"#,
        )
        .replace("</text></box>{/for}", "</text></button>{/for}");
    let mut component = test_frontend_component(&source);
    Arc::make_mut(&mut component.settings)
        .set_namespace("shell", serde_json::json!({"motion":{"reduced":true}}));
    let mut pixels = PixelBuffer::new(200, 140);
    paint_frames(&mut component, &theme, &mut pixels, 4);
    assert_eq!(scroll_list_children(&component), 90);

    let viewport = first_node_by_tag(component.last_tree.as_ref().unwrap(), "scroll").unwrap();
    let (vx, vy) = (viewport.layout.x, viewport.layout.y);
    for scroll in [0.0_f32, -1_234.0, -50_000.0] {
        if scroll != 0.0 {
            component
                .handle_input(
                    &theme,
                    200,
                    140,
                    ComponentInput::TwoFingerScroll {
                        x: vx + 10.0,
                        y: vy + 10.0,
                        dx: 0.0,
                        dy: scroll,
                    },
                )
                .unwrap();
        }
        paint_frames(&mut component, &theme, &mut pixels, 4);
        let extents = component
            .painted_extents()
            .expect("a settled component exposes its painted extents");
        let tree = component.last_tree.as_ref().unwrap();
        for yi in -2..150 {
            for xi in (-2..210).step_by(7) {
                let (x, y) = (xi as f32 + 0.5, yi as f32 + 0.5);
                let dyn_extents = Some(&extents as &dyn PaintedExtents);
                assert_eq!(
                    pointer_hit_test(tree, x, y),
                    pointer_hit_test_pruned(tree, x, y, dyn_extents),
                    "hit at ({x}, {y}) scrolled {scroll}"
                );
                let full = pointer_press_hit(tree, x, y);
                let pruned = pointer_press_hit_pruned(tree, x, y, dyn_extents);
                assert_eq!(
                    full.target.map(|hit| (hit.key, hit.bounds)),
                    pruned.target.map(|hit| (hit.key, hit.bounds)),
                    "press at ({x}, {y}) scrolled {scroll}"
                );
                assert_eq!(
                    pointer_event_handler_hit(tree, x, y, "click").map(|hit| (hit.key, hit.bounds)),
                    pointer_event_handler_hit_pruned(tree, x, y, "click", dyn_extents)
                        .map(|hit| (hit.key, hit.bounds)),
                    "click handler at ({x}, {y}) scrolled {scroll}"
                );
            }
        }
        let counting = CountingExtents {
            inner: &extents,
            visited: std::cell::Cell::new(0),
        };
        let (x, y) = (vx + 10.0, vy + 10.0);
        pointer_hit_test_pruned(tree, x, y, Some(&counting as &dyn PaintedExtents));
        eprintln!("scroll {scroll}: visited {}", counting.visited.get());
        assert!(
            counting.visited.get() < 40,
            "a hit visits {} of 90 rows' nodes",
            counting.visited.get()
        );
    }
}

#[test]
fn short_plain_list_is_untouched() {
    let theme = Theme::new("empty", "Empty");
    let mut component = test_frontend_component(&list_source(60, "", "row", ""));
    let mut pixels = PixelBuffer::new(200, 140);
    paint_frames(&mut component, &theme, &mut pixels, 3);
    assert_eq!(scroll_list_children(&component), 60);
}

#[test]
fn focused_row_stays_built_when_a_windowed_list_scrolls_away() {
    let theme = Theme::new("empty", "Empty");
    let source = list_source(2_000, "", "row", "").replace(
        r#"<box class="row"><text>{item}</text></box>"#,
        r#"<button class="row"><text>{item}</text></button>"#,
    );
    let mut component = test_frontend_component(&source);
    Arc::make_mut(&mut component.settings)
        .set_namespace("shell", serde_json::json!({"motion":{"reduced":true}}));
    let mut pixels = PixelBuffer::new(200, 140);
    paint_frames(&mut component, &theme, &mut pixels, 4);

    let tree = component.last_tree.as_ref().unwrap();
    let list = &first_node_by_tag(tree, "scroll").unwrap().children[0];
    let focused = list.children[3]
        .mesh_key()
        .expect("row has a key")
        .to_owned();
    component.focused_key = Some(focused.clone());
    let viewport = first_node_by_tag(tree, "scroll").unwrap();
    let (x, y) = (viewport.layout.x + 10.0, viewport.layout.y + 10.0);
    component
        .handle_input(
            &theme,
            200,
            140,
            ComponentInput::TwoFingerScroll {
                x,
                y,
                dx: 0.0,
                dy: -1_000.0,
            },
        )
        .unwrap();
    paint_frames(&mut component, &theme, &mut pixels, 4);

    let text = rendered_text(&component);
    assert!(
        text.contains(&"51".to_string()),
        "window followed the scroll: {text:?}"
    );
    let list = &first_node_by_tag(component.last_tree.as_ref().unwrap(), "scroll")
        .unwrap()
        .children[0];
    assert!(
        list.children
            .iter()
            .any(|child| child.mesh_key() == Some(focused.as_str())),
        "the focused row is still built"
    );
    assert!(
        list.children.len() < 120,
        "only the window and the focused span is built"
    );
}

#[test]
fn first_build_of_a_long_list_covers_a_tall_surface() {
    let theme = Theme::new("empty", "Empty");
    let mut component = test_frontend_component(&list_source(2_000, "", "row", ""));
    Arc::make_mut(&mut component.settings)
        .set_namespace("shell", serde_json::json!({"motion":{"reduced":true}}));
    let mut pixels = PixelBuffer::new(200, 1_600);
    let extent = SurfaceExtent::unpadded(200, 1_600);
    component
        .paint_raster(&theme, extent, &mut pixels, 1.0)
        .unwrap();
    // 1,600 px at the smallest assumed 16 px item is 100 items, plus overscan.
    let first_frame = scroll_list_children(&component);
    assert!(
        first_frame >= 100,
        "the first frame builds enough rows to fill the surface: {first_frame}"
    );
    paint_frames(&mut component, &theme, &mut pixels, 3);
    assert!(scroll_list_children(&component) < 240);
}

#[test]
fn a_row_keeps_its_node_identity_after_leaving_and_re_entering_the_window() {
    let theme = Theme::new("empty", "Empty");
    let mut component = test_frontend_component(&list_source(2_000, "", "row", ""));
    Arc::make_mut(&mut component.settings)
        .set_namespace("shell", serde_json::json!({"motion":{"reduced":true}}));
    let mut pixels = PixelBuffer::new(200, 140);
    paint_frames(&mut component, &theme, &mut pixels, 4);

    let identity = |component: &FrontendSurfaceComponent| {
        let list = &first_node_by_tag(component.last_tree.as_ref().unwrap(), "scroll")
            .unwrap()
            .children[0];
        let row = &list.children[3];
        (row.id, row.mesh_key().map(str::to_owned))
    };
    let before = identity(&component);
    assert!(before.1.is_some());

    let tree = component.last_tree.as_ref().unwrap();
    let viewport = first_node_by_tag(tree, "scroll").unwrap();
    let (x, y) = (viewport.layout.x + 10.0, viewport.layout.y + 10.0);
    for dy in [-50_000.0, 50_000.0] {
        component
            .handle_input(
                &theme,
                200,
                140,
                ComponentInput::TwoFingerScroll { x, y, dx: 0.0, dy },
            )
            .unwrap();
        paint_frames(&mut component, &theme, &mut pixels, 4);
        if dy < 0.0 {
            assert!(
                !rendered_text(&component).contains(&"4".to_string()),
                "row 4 was unbuilt while the end of the list was shown"
            );
        }
    }
    assert!(rendered_text(&component).contains(&"4".to_string()));
    assert_eq!(identity(&component), before);
}

/// Debug-profile comparison; run with
/// `cargo test -p mesh-core-frontend-runtime --lib windowed_list_profile -- --ignored --nocapture`.
#[test]
#[ignore = "measurement, not a gate"]
fn windowed_list_profile() {
    let theme = Theme::new("empty", "Empty");
    let rows: usize = std::env::var("MESH_PROFILE_ROWS")
        .ok()
        .and_then(|rows| rows.parse().ok())
        .unwrap_or(2_000);
    let uneven = std::env::var_os("MESH_PROFILE_UNEVEN").is_some();
    let source = |header: &str| {
        let markup = format!(
            r#"
<template><box class="host"><scroll class="viewport"><box class="list">{{#for item in items {header}}}<box class="row"><text>{{item}}</text></box>{{/for}}</box></scroll></box></template>
<script lang="luau">
items = {{}}
for i = 1, {rows} do
  items[i] = i
end
</script>
<style>
.host {{ width: 200px; height: 140px; }}
.viewport {{ width: 120px; height: 100px; }}
.list {{ flex-direction: column; }}
.row {{ height: 20px; flex-shrink: 0; }}
</style>
"#
        );
        if uneven {
            markup.replace(
                r#"<box class="row">"#,
                r#"<box class="row" style:height={item == 5 and "55px" or "20px"}>"#,
            )
        } else {
            markup
        }
    };
    let extent = SurfaceExtent::unpadded(200, 140);
    let variants: &[(&str, &str)] = if std::env::var_os("MESH_PROFILE_UNWINDOWED_ONLY").is_some() {
        &[("plain", "")]
    } else {
        &[("plain", ""), ("window=20", "window=20 overscan=2")]
    };
    for &(label, header) in variants {
        let mut first_paint = Vec::new();
        let mut settle = Vec::new();
        let mut scroll_frame = Vec::new();
        let iterations: usize = std::env::var("MESH_PROFILE_ITERATIONS")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(3);
        let frames: usize = std::env::var("MESH_PROFILE_FRAMES")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(20);
        for _ in 0..iterations {
            let mut component = test_frontend_component(&source(header));
            Arc::make_mut(&mut component.settings)
                .set_namespace("shell", serde_json::json!({"motion":{"reduced":true}}));
            let mut pixels = PixelBuffer::new(200, 140);
            let started = std::time::Instant::now();
            component
                .paint_raster(&theme, extent, &mut pixels, 1.0)
                .unwrap();
            first_paint.push(started.elapsed().as_secs_f64() * 1e3);
            // Let a list that is rebuilt after measurement (windowed or full)
            // settle, timing that frame separately from steady scrolling.
            let started = std::time::Instant::now();
            for _ in 0..3 {
                component
                    .paint_raster(&theme, extent, &mut pixels, 1.0)
                    .unwrap();
            }
            settle.push(started.elapsed().as_secs_f64() * 1e3);
            let viewport = first_node_by_tag(component.last_tree.as_ref().unwrap(), "scroll")
                .unwrap()
                .layout;
            let (x, y) = (viewport.x + 10.0, viewport.y + 10.0);
            let started = std::time::Instant::now();
            for _ in 0..frames {
                component
                    .handle_input(
                        &theme,
                        200,
                        140,
                        ComponentInput::TwoFingerScroll {
                            x,
                            y,
                            dx: 0.0,
                            dy: -30.0,
                        },
                    )
                    .unwrap();
                component
                    .paint_raster(&theme, extent, &mut pixels, 1.0)
                    .unwrap();
            }
            scroll_frame.push(started.elapsed().as_secs_f64() * 1e3 / frames as f64);
            if std::env::var_os("MESH_PROFILE_COUNTS").is_some() {
                let scope = mesh_core_elements::frame_work::Scope::new(true);
                component
                    .handle_input(
                        &theme,
                        200,
                        140,
                        ComponentInput::TwoFingerScroll {
                            x,
                            y,
                            dx: 0.0,
                            dy: -30.0,
                        },
                    )
                    .unwrap();
                component
                    .paint_raster(&theme, extent, &mut pixels, 1.0)
                    .unwrap();
                let counts: Vec<_> = scope
                    .snapshot()
                    .into_iter()
                    .filter(|(_, count)| *count > 0)
                    .collect();
                println!("{label} rows={rows} scroll-frame visits: {counts:?}");
            }
        }
        println!(
            "{label} rows={rows}: first paint ms {first_paint:.1?}; settle (3 frames) ms {settle:.1?}; scroll frame ms {scroll_frame:.2?}"
        );
    }
}
