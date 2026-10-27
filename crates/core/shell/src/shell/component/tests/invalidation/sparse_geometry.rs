use super::*;
use mesh_core_elements::NodeId;
use std::collections::BTreeMap;

/// One keyed column whose middle (or first/last) item changes geometry.
/// `case` selects what changes; every other item is a clean sibling.
fn geometry_patch_source(siblings: usize, case: &str) -> String {
    if case == "clean_column" {
        return format!(
            r##"
<script lang="luau">
items = {{}}
for i = 1, 4 do items[i] = {{id = i, size = "1px"}} end
clean = {{}}
for i = 1, {siblings} do clean[i] = i end
target = 2
grown = false
function mutate()
    grown = not grown
    items[target].size = grown and "3px" or "1px"
end
</script>
<template><row style="width: 320px; height: 100px; align-items: flex-start">
<column style="width: 40px; align-items: flex-start">{{#for item in items key={{item.id}}}}<box style="width: 4px; background: #ff0000" style:height={{item.size}} />{{/for}}</column>
<column style="width: 40px">{{#for c in clean key={{c}}}}<box style="width: 1px; height: 1px" />{{/for}}</column>
</row></template>
"##
        );
    }
    let (body, item, target, toggle) = match case {
        "text_width" => (
            "<text>{item.label}</text>",
            "label = \"ab\"",
            siblings / 2,
            "items[target].label = grown and \"abcdefghijkl\" or \"ab\"",
        ),
        "box_width" => (
            "<box style=\"height: 1px; background: #ff0000\" style:width={item.size} />",
            "size = \"10px\"",
            siblings / 2,
            "items[target].size = grown and \"20px\" or \"10px\"",
        ),
        "box_height_last" => (
            "<box style=\"width: 4px; background: #ff0000\" style:height={item.size} />",
            "size = \"1px\"",
            siblings,
            "items[target].size = grown and \"3px\" or \"1px\"",
        ),
        "box_height_first" => (
            "<box style=\"width: 4px; background: #ff0000\" style:height={item.size} />",
            "size = \"1px\"",
            1,
            "items[target].size = grown and \"3px\" or \"1px\"",
        ),
        _ => unreachable!("{case}"),
    };
    format!(
        r##"
<script lang="luau">
items = {{}}
for i = 1, {siblings} do items[i] = {{id = i, {item}}} end
target = {target}
grown = false
function mutate()
    grown = not grown
    {toggle}
end
</script>
<template><column style="width: 320px; height: 100px; align-items: flex-start">
<column style="align-items: flex-start">{{#for item in items key={{item.id}}}}{body}{{/for}}</column>
</column></template>
"##
    )
}

pub(super) const GEOMETRY_CASES: [&str; 5] = [
    "clean_column",
    "text_width",
    "box_width",
    "box_height_last",
    "box_height_first",
];

fn layouts(node: &WidgetNode, out: &mut Vec<(NodeId, mesh_core_elements::LayoutRect)>) {
    out.push((node.id, node.layout));
    node.children.iter().for_each(|child| layouts(child, out));
}

const REPORTED_PASSES: [&str; 12] = [
    "layout_sync",
    "layout_write",
    "layout_extents",
    "layout_fit",
    "retained_sync",
    "annotation",
    "display_entries",
    "display_ancestors",
    "display_build",
    "accessibility_normalize",
    "restyle_scan",
    "style_resolve",
];

/// Steady-state visits for each geometry case at `siblings`, after checking
/// geometry, pixels and retained completeness against a full-build twin.
pub(super) fn geometry_patch_visits(case: &str, siblings: usize) -> Vec<BTreeMap<String, u64>> {
    let theme = default_theme();
    let extent = SurfaceExtent::unpadded(320, 100);
    let source = geometry_patch_source(siblings, case);
    let mut patched = test_frontend_component(&source);
    let mut full = test_frontend_component(&source);
    full.force_full_template_build = true;
    let mut actual = PixelBuffer::new(320, 100);
    let mut expected = PixelBuffer::new(320, 100);
    for _ in 0..3 {
        patched
            .paint_raster(&theme, extent, &mut actual, 1.0)
            .unwrap();
        full.paint_raster(&theme, extent, &mut expected, 1.0)
            .unwrap();
    }
    patched.set_profiling_enabled(true);
    let _ = patched.take_invalidation_snapshot();
    let mut frames = Vec::new();
    for turn in 0..4 {
        let scope = mesh_core_elements::frame_work::Scope::new(true);
        patched.call_namespaced_handler("mutate", &[]).unwrap();
        let handler = scope.snapshot();
        drop(scope);
        full.call_namespaced_handler("mutate", &[]).unwrap();
        patched
            .paint_raster(&theme, extent, &mut actual, 1.0)
            .unwrap();
        full.paint_raster(&theme, extent, &mut expected, 1.0)
            .unwrap();
        let snapshot = patched.take_invalidation_snapshot().unwrap();
        assert!(
            snapshot.patched_node_count >= 1,
            "{case}/{siblings}/{turn}: {snapshot:?}"
        );
        patched
            .retained_tree
            .assert_complete(patched.last_tree.as_ref().unwrap());
        let (mut patched_layouts, mut full_layouts) = (Vec::new(), Vec::new());
        layouts(patched.last_tree.as_ref().unwrap(), &mut patched_layouts);
        layouts(full.last_tree.as_ref().unwrap(), &mut full_layouts);
        let patched_rects: Vec<_> = patched_layouts.iter().map(|(_, rect)| *rect).collect();
        let full_rects: Vec<_> = full_layouts.iter().map(|(_, rect)| *rect).collect();
        if std::env::var_os("MESH_GEOMETRY_PROBE_SKIP_PARITY").is_none() {
            assert_eq!(patched_rects, full_rects, "{case}/{siblings}/{turn}");
            assert_eq!(actual.data(), expected.data(), "{case}/{siblings}/{turn}");
        }
        let mut visits: BTreeMap<String, u64> = snapshot
            .node_visits
            .into_iter()
            .filter(|(pass, _)| REPORTED_PASSES.contains(&pass.as_str()))
            .collect();
        for pass in [
            "keyed_item_compare",
            "keyed_key_validation",
            "binding_patch",
        ] {
            visits.insert(format!("handler_{pass}"), handler[pass]);
        }
        frames.push(visits);
    }
    frames
}

// cargo test -p mesh-core-shell --release --lib -- geometry_patch_visit_profile --ignored --nocapture
#[test]
#[ignore = "prints per-pass visits for geometry-changing patches"]
fn geometry_patch_visit_profile() {
    for case in GEOMETRY_CASES {
        for siblings in [16, 1024] {
            let frames = geometry_patch_visits(case, siblings);
            println!("{case} siblings={siblings} {:?}", frames.last().unwrap());
        }
    }
}

// cargo test -p mesh-core-shell --release --lib -- geometry_patch_frame_timing_profile --ignored --nocapture
// MESH_GEOMETRY_VISIBLE=1 keeps every sibling on screen (see `visible_geometry_source`).
#[test]
#[ignore = "release-only geometry-changing patch timing"]
fn geometry_patch_frame_timing_profile() {
    use std::time::{Duration, Instant};
    let theme = default_theme();
    let only = std::env::var("MESH_GEOMETRY_CASE").ok();
    let visible = std::env::var_os("MESH_GEOMETRY_VISIBLE").is_some();
    let height = if visible { 1100 } else { 100 };
    let extent = SurfaceExtent::unpadded(320, height);
    for (case, siblings) in GEOMETRY_CASES
        .into_iter()
        .filter(|case| only.as_deref().is_none_or(|only| only == *case))
        .filter(|case| !visible || *case != "box_height_first")
        .flat_map(|case| [(case, 16), (case, 1024)])
    {
        let source = if visible {
            visible_geometry_source(siblings, case)
        } else {
            geometry_patch_source(siblings, case)
        };
        let mut component = test_frontend_component(&source);
        let mut pixels = PixelBuffer::new(320, height);
        for _ in 0..3 {
            component
                .paint_raster(&theme, extent, &mut pixels, 1.0)
                .unwrap();
        }
        let iterations = 200;
        let mut handler = Vec::with_capacity(iterations);
        let mut frame = Vec::with_capacity(iterations);
        for _ in 0..iterations {
            let started = Instant::now();
            component.call_namespaced_handler("mutate", &[]).unwrap();
            let handled = Instant::now();
            component
                .paint_raster(&theme, extent, &mut pixels, 1.0)
                .unwrap();
            handler.push(handled - started);
            frame.push(handled.elapsed());
        }
        for (stage, mut samples) in [("handler", handler), ("paint", frame)] {
            samples.sort_unstable();
            let mean = samples.iter().sum::<Duration>() / iterations as u32;
            for (stat, value) in [
                ("mean", mean),
                ("p50", samples[iterations / 2]),
                ("p90", samples[iterations * 9 / 10]),
            ] {
                println!(
                    "MESH_PERF metric=geometry_patch_{case}_{siblings}_{stage}_{stat}_us value={:.1}",
                    value.as_secs_f64() * 1e6
                );
            }
        }
    }
}

/// Like [`geometry_patch_source`], but every item stays on screen, so the
/// display patch, rather than pruning, decides what is rebuilt.
fn visible_geometry_source(siblings: usize, case: &str) -> String {
    let (body, item, target, toggle) = match case {
        "text_width" => (
            "<text style=\"font-size: 1px; line-height: 1px\">{item.label}</text>",
            "label = \"a\"",
            2,
            "items[target].label = grown and \"abcdefgh\" or \"a\"",
        ),
        "box_width" => (
            "<box style=\"height: 1px; background: #ff0000\" style:width={item.size} />",
            "size = \"10px\"",
            2,
            "items[target].size = grown and \"20px\" or \"10px\"",
        ),
        "box_height_last" => (
            "<box style=\"width: 4px; background: #ff0000\" style:height={item.size} />",
            "size = \"1px\"",
            siblings,
            "items[target].size = grown and \"3px\" or \"1px\"",
        ),
        _ => return geometry_patch_source(siblings, case),
    };
    format!(
        r##"
<script lang="luau">
items = {{}}
for i = 1, {siblings} do items[i] = {{id = i, {item}}} end
target = {target}
grown = false
function mutate()
    grown = not grown
    {toggle}
end
</script>
<template><column style="width: 320px; height: 1100px; align-items: flex-start">
<column style="align-items: flex-start; gap: 0px">{{#for item in items key={{item.id}}}}{body}{{/for}}</column>
</column></template>
"##
    )
}

/// Geometry-changing patches visit only the changed display nodes: entry
/// collection, command rebuilding, retained and layout synchronization and the
/// keyed-list compare stay constant as clean siblings grow. A shrink that
/// gives up its parent's coverage edge re-unions that parent's children, and
/// producer deltas select layout write-back, and retained child unions avoid
/// sibling scans in overflow annotation. Pixels, rects and the
/// retained tree match a full rebuild every frame.
#[test]
fn geometry_patch_frames_scale_with_the_change() {
    if crate::shell::component::tests::common::isolate_frame_test(
        "shell::component::tests::invalidation::sparse_geometry::geometry_patch_frames_scale_with_the_change",
    ) {
        return;
    }
    let theme = default_theme();
    let extent = SurfaceExtent::unpadded(320, 1100);
    for case in ["clean_column", "text_width", "box_width", "box_height_last"] {
        let mut per_size = Vec::new();
        for siblings in [16, 1024] {
            let source = visible_geometry_source(siblings, case);
            let mut patched = test_frontend_component(&source);
            let mut full = test_frontend_component(&source);
            full.force_full_template_build = true;
            let mut actual = PixelBuffer::new(320, 1100);
            let mut expected = PixelBuffer::new(320, 1100);
            for _ in 0..3 {
                patched
                    .paint_raster(&theme, extent, &mut actual, 1.0)
                    .unwrap();
                full.paint_raster(&theme, extent, &mut expected, 1.0)
                    .unwrap();
            }
            patched.set_profiling_enabled(true);
            let _ = patched.take_invalidation_snapshot();
            let mut frames = Vec::new();
            for turn in 0..4 {
                let scope = mesh_core_elements::frame_work::Scope::new(true);
                patched.call_namespaced_handler("mutate", &[]).unwrap();
                let handler = scope.snapshot();
                drop(scope);
                full.call_namespaced_handler("mutate", &[]).unwrap();
                patched
                    .paint_raster(&theme, extent, &mut actual, 1.0)
                    .unwrap();
                full.paint_raster(&theme, extent, &mut expected, 1.0)
                    .unwrap();
                let snapshot = patched.take_invalidation_snapshot().unwrap();
                assert!(snapshot.patched_node_count >= 1, "{case}/{siblings}/{turn}");
                patched
                    .retained_tree
                    .assert_complete(patched.last_tree.as_ref().unwrap());
                let (mut patched_layouts, mut full_layouts) = (Vec::new(), Vec::new());
                layouts(patched.last_tree.as_ref().unwrap(), &mut patched_layouts);
                layouts(full.last_tree.as_ref().unwrap(), &mut full_layouts);
                assert!(
                    patched_layouts
                        .iter()
                        .map(|(_, rect)| rect)
                        .eq(full_layouts.iter().map(|(_, rect)| rect)),
                    "{case}/{siblings}/{turn}: layouts diverged"
                );
                assert_eq!(actual.data(), expected.data(), "{case}/{siblings}/{turn}");
                let counts = snapshot.node_visits;
                eprintln!("geometry {case}/{siblings}/{turn}: {counts:?}");
                // Debug builds of the renderer always collect entries in full.
                if !cfg!(debug_assertions) {
                    assert_eq!(
                        counts["display_entries"], 0,
                        "{case}/{siblings}/{turn}: {counts:?}"
                    );
                }
                let mut constant = vec![
                    counts["display_build"],
                    counts["retained_sync"],
                    counts["layout_sync"],
                    counts["layout_write"],
                    counts["layout_extents"],
                    counts["annotation"],
                    handler["keyed_item_compare"],
                    handler["keyed_key_validation"],
                ];
                // Growth keeps the coverage update local.
                if turn % 2 == 0 {
                    constant.push(counts["display_ancestors"]);
                }
                if case == "clean_column" {
                    constant.extend([
                        counts["layout_write"],
                        counts["layout_extents"],
                        counts["annotation"],
                        counts["display_ancestors"],
                    ]);
                }
                frames.push(constant);
            }
            per_size.push(frames);
        }
        assert_eq!(
            per_size[0], per_size[1],
            "{case}: visits grew with clean siblings"
        );
    }
}

/// Item writes to a keyed list are recorded when state syncs: the patch
/// compares and key-checks only the written items. Key changes and resizes
/// still rebuild, and every frame matches a full rebuild.
#[test]
fn keyed_list_item_writes_compare_only_written_items() {
    let theme = default_theme();
    let extent = SurfaceExtent::unpadded(320, 100);
    let mut per_size = Vec::new();
    for siblings in [16, 1024] {
        let source = format!(
            r##"
<script lang="luau">
items = {{}}
for i = 1, {siblings} do items[i] = {{id = i, color = "#ff0000"}} end
writes = 0
local function color()
    writes = writes + 1
    return string.format("#%02x00ff", writes)
end
function one() items[3].color = color() end
function two() items[2].color = color() items[5].color = color() end
function reassign()
    local copy = {{}}
    for i, item in ipairs(items) do copy[i] = {{id = item.id, color = item.color}} end
    copy[4].color = color()
    items = copy
end
function rekey() items[3].id = -1 end
shown = false
function toggle() shown = not shown end
function grow() table.insert(items, {{id = 0, color = "#ffffff"}}) end
</script>
<template><row style="width: 320px; height: 100px">
{{#for item in items key={{item.id}}}}<box style="width: 1px; height: 10px" style:background={{item.color}} />{{/for}}
{{#if shown}}<box style="width: 2px; height: 2px" />{{/if}}
</row></template>
"##
        );
        let mut patched = test_frontend_component(&source);
        let mut full = test_frontend_component(&source);
        full.force_full_template_build = true;
        let mut actual = PixelBuffer::new(320, 100);
        let mut expected = PixelBuffer::new(320, 100);
        for _ in 0..2 {
            patched
                .paint_raster(&theme, extent, &mut actual, 1.0)
                .unwrap();
            full.paint_raster(&theme, extent, &mut expected, 1.0)
                .unwrap();
        }
        patched.set_profiling_enabled(true);
        let _ = patched.take_invalidation_snapshot();
        let mut compares = Vec::new();
        // `toggle` rebuilds after delta patches, which must not read a
        // memoized list value from before them.
        for handler in [
            "one", "two", "reassign", "one", "toggle", "rekey", "one", "grow", "two",
        ] {
            let scope = mesh_core_elements::frame_work::Scope::new(true);
            patched.call_namespaced_handler(handler, &[]).unwrap();
            let counts = scope.snapshot();
            drop(scope);
            full.call_namespaced_handler(handler, &[]).unwrap();
            patched
                .paint_raster(&theme, extent, &mut actual, 1.0)
                .unwrap();
            full.paint_raster(&theme, extent, &mut expected, 1.0)
                .unwrap();
            let snapshot = patched.take_invalidation_snapshot().unwrap();
            assert_eq!(actual.data(), expected.data(), "{siblings}: {handler}");
            let patched_frame = snapshot.patched_node_count > 0;
            assert_eq!(
                patched_frame,
                !matches!(handler, "rekey" | "grow" | "toggle"),
                "{siblings}: {handler}: {snapshot:?}"
            );
            compares.push((handler, counts["keyed_item_compare"]));
        }
        per_size.push(compares);
    }
    assert_eq!(
        per_size[0], per_size[1],
        "keyed compares grew with the list"
    );
    let expected = [("one", 1), ("two", 2), ("reassign", 1), ("one", 1)];
    assert_eq!(&per_size[1][..4], &expected);
}

#[test]
fn producer_geometry_changes_match_full_commands_scroll_metrics_and_pixels() {
    fn metrics(node: &WidgetNode, out: &mut Vec<Option<mesh_core_elements::WidgetScrollMetrics>>) {
        out.push(node.scroll_metrics);
        for child in &node.children {
            metrics(child, out);
        }
    }
    let theme = default_theme();
    let extent = SurfaceExtent::unpadded(320, 1100);
    for siblings in [16, 1024] {
        for target in [1, siblings / 2, siblings] {
            for mode in [
                "wrap",
                "grow",
                "shrink",
                "align",
                "fractional",
                "clip",
                "fixed",
            ] {
                let root_style = match mode {
                    "wrap" => "flex-direction: row; flex-wrap: wrap; align-items: flex-start",
                    "grow" | "shrink" => "flex-direction: row; align-items: flex-start",
                    "align" => "align-items: center",
                    _ => "align-items: flex-start",
                };
                let item_style = match mode {
                    "grow" => "flex-shrink: 0",
                    "shrink" => "flex-shrink: 1",
                    "clip" => "flex-shrink: 0; overflow: hidden",
                    "fixed" => "flex-shrink: 0; position: fixed",
                    _ => "flex-shrink: 0",
                };
                let mutate = match mode {
                    "grow" => "items[target].grow = grown and 3 or 1",
                    "shrink" => "items[target].shrink = grown and 3 or 1",
                    _ => "items[target].size = grown and '20.75px' or '8.25px'",
                };
                let inner = if mode == "clip" {
                    "<box style=\"width: 40px; height: 4px; flex-shrink: 0; background: #0000ff\" />"
                } else {
                    ""
                };
                let source = format!(
                    r##"
<script lang="luau">
items = {{}}
for i = 1, {siblings} do
    items[i] = {{id = i, size = "8.25px", grow = {grow}, shrink = {shrink},
        color = i % 2 == 0 and "#ff0000" or "#00ff00",
        x = tostring((i % 32) * 9) .. "px", y = tostring(math.floor(i / 32) * 2) .. "px"}}
end
target = {target}
grown = false
function mutate()
    grown = not grown
    {mutate}
end
</script>
<template><column style="width: 320px; height: 1100px; {root_style}; padding: 0.35px">
{{#for item in items key={{item.id}}}}
<box style="height: 0.75px; {item_style}" style:width={{item.size}}
    style:flex-grow={{item.grow}} style:flex-shrink={{item.shrink}}
    style:background={{item.color}} style:left={{item.x}} style:top={{item.y}}>{inner}</box>
{{/for}}
</column></template>
"##,
                    grow = if mode == "grow" { 1 } else { 0 },
                    shrink = if mode == "shrink" { 1 } else { 0 }
                );
                let mut patched = test_frontend_component(&source);
                let mut full = test_frontend_component(&source);
                patched.motion_policy.reduced_motion = true;
                full.motion_policy.reduced_motion = true;
                full.force_full_template_build = true;
                let (mut actual, mut expected) =
                    (PixelBuffer::new(320, 1100), PixelBuffer::new(320, 1100));
                for _ in 0..3 {
                    patched
                        .paint_raster(&theme, extent, &mut actual, 1.0)
                        .unwrap();
                    full.paint_raster(&theme, extent, &mut expected, 1.0)
                        .unwrap();
                }
                for turn in 0..2 {
                    patched.call_namespaced_handler("mutate", &[]).unwrap();
                    full.call_namespaced_handler("mutate", &[]).unwrap();
                    patched
                        .paint_raster(&theme, extent, &mut actual, 1.0)
                        .unwrap();
                    full.paint_raster(&theme, extent, &mut expected, 1.0)
                        .unwrap();
                    let (mut a, mut b) = (Vec::new(), Vec::new());
                    layouts(patched.last_tree.as_ref().unwrap(), &mut a);
                    layouts(full.last_tree.as_ref().unwrap(), &mut b);
                    if mode == "fractional" && turn == 0 {
                        assert_eq!(
                            a[target + 1].1.width,
                            21.0,
                            "fractional mutation was not applied"
                        );
                    }
                    let mismatch = a
                        .iter()
                        .zip(&b)
                        .enumerate()
                        .find(|(_, (actual, expected))| actual.1 != expected.1);
                    assert!(
                        a.len() == b.len() && mismatch.is_none(),
                        "{mode}/{siblings}/{target}/{turn}: lengths {}/{}; first mismatch {mismatch:?}",
                        a.len(),
                        b.len()
                    );
                    let (mut a, mut b) = (Vec::new(), Vec::new());
                    metrics(patched.last_tree.as_ref().unwrap(), &mut a);
                    metrics(full.last_tree.as_ref().unwrap(), &mut b);
                    assert_eq!(a, b, "metrics {mode}/{siblings}/{target}/{turn}");
                    let a = patched.retained_display_list.paint_commands();
                    let b = full.retained_display_list.paint_commands();
                    assert_eq!(
                        a.len(),
                        b.len(),
                        "commands {mode}/{siblings}/{target}/{turn}"
                    );
                    for (a, b) in a.iter().zip(b) {
                        assert_eq!(
                            (
                                a.kind,
                                a.node.id,
                                a.node.transform,
                                a.node.layout,
                                a.node.local_layout
                            ),
                            (
                                b.kind,
                                b.node.id,
                                b.node.transform,
                                b.node.layout,
                                b.node.local_layout
                            )
                        );
                        assert_eq!(format!("{:?}", a.clip), format!("{:?}", b.clip));
                        assert_eq!(
                            format!("{:?}", a.node.ancestor_clips),
                            format!("{:?}", b.node.ancestor_clips)
                        );
                    }
                    assert_eq!(
                        actual.data(),
                        expected.data(),
                        "pixels {mode}/{siblings}/{target}/{turn}"
                    );
                }
            }
        }
    }
}
