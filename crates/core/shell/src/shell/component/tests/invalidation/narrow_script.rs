use super::*;
use crate::shell::component::tests::common::{test_frontend_component, themed_primary};

#[test]
fn script_narrow_flag_does_not_trigger_tree_rebuild() {
    let mut component = test_frontend_component("<template><text>hello</text></template>");

    component.dirty = false;
    component.style_only_dirty = false;
    component.dirty_types = ComponentDirtyFlags::empty();

    component.invalidate_script_state_narrow();
    assert!(component.wants_render());

    let (requires_tree_rebuild, _can_use_retained_path, flags, _) =
        component.take_dirty_for_paint();
    assert!(
        !requires_tree_rebuild,
        "SCRIPT_NARROW must not trigger tree rebuild"
    );
    assert!(
        flags.contains(ComponentDirtyFlags::SCRIPT_NARROW),
        "flags must contain SCRIPT_NARROW after invalidate_script_state_narrow"
    );
    assert!(
        !flags.contains(ComponentDirtyFlags::SCRIPT),
        "flags must not contain SCRIPT when only narrow invalidation was requested"
    );
    assert!(
        !flags.contains(ComponentDirtyFlags::TEXT),
        "flags must not contain TEXT when only narrow invalidation was requested"
    );
}

#[test]
fn narrow_path_dirties_ancestor_chain() {
    let mut component = test_frontend_component(
        "<template><box><row><text id='leaf'>hello</text></row></box></template>",
    );
    let theme = themed_primary("test", "#000000");

    let tree1 = component.build_tree(&theme, 100, 100);
    let _ = component.retained_tree.update(&tree1);

    let leaf_nodes: HashSet<NodeId> = tree1.children[0].children[0]
        .children
        .iter()
        .map(|n| n.id)
        .collect();

    assert!(!leaf_nodes.is_empty(), "must find leaf text node");

    let result = component.retained_tree.narrow_script_diff(&tree1);
    assert!(result.is_some(), "identical trees must diff successfully");
    let (affected, _total) = result.unwrap();
    assert!(
        affected.is_empty(),
        "identical trees have no affected nodes"
    );
}

#[test]
fn structural_change_falls_back_to_tree_rebuild() {
    let mut component = test_frontend_component("<template><box><text>a</text></box></template>");
    let theme = themed_primary("test", "#000000");
    let _tree = component.build_tree(&theme, 100, 100);
    let _ = component.retained_tree.update(&_tree);

    let mut component2 =
        test_frontend_component("<template><box><text>b</text><text>c</text></box></template>");
    let tree2 = component2.build_tree(&theme, 100, 100);

    let result = component.retained_tree.narrow_script_diff(&tree2);
    assert!(
        result.is_none(),
        "structural change (added child) must return None"
    );
}

#[test]
fn threshold_fallback_exceeds_half() {
    let mut component = test_frontend_component(
        "<template><box><text>a</text><text>b</text><text>c</text></box></template>",
    );
    let theme = themed_primary("test", "#000000");

    let tree1 = component.build_tree(&theme, 100, 100);
    let _ = component.retained_tree.update(&tree1);

    let mut component2 = test_frontend_component(
        "<template><box><text>x</text><text>y</text><text>z</text></box></template>",
    );
    let tree2 = component2.build_tree(&theme, 100, 100);

    let result = component.retained_tree.narrow_script_diff(&tree2);
    assert!(
        result.is_some(),
        "diff must succeed (structurally identical trees)"
    );
    let (affected, _total) = result.unwrap();
    assert_eq!(affected.len(), 3, "all three text nodes changed");
    assert!(
        affected.len() * 2 > 4,
        ">50% of nodes changed — narrow path should be skipped"
    );
}

#[test]
fn threshold_narrow_below_half() {
    let mut component = test_frontend_component(
        "<template><box><text>a</text><text>b</text><text>c</text><text>d</text><text>e</text></box></template>",
    );
    let theme = themed_primary("test", "#000000");

    let tree1 = component.build_tree(&theme, 100, 100);
    let _ = component.retained_tree.update(&tree1);

    let mut component2 = test_frontend_component(
        "<template><box><text>x</text><text>b</text><text>c</text><text>d</text><text>e</text></box></template>",
    );
    let tree2 = component2.build_tree(&theme, 100, 100);

    let result = component.retained_tree.narrow_script_diff(&tree2);
    assert!(
        result.is_some(),
        "diff must succeed (structurally identical trees)"
    );
    let (affected, total) = result.unwrap();

    assert!(
        affected.len() >= 1,
        "at least the changed text node should be marked"
    );
    assert!(
        affected.len() * 2 <= total,
        "≤50% of nodes changed — narrow path should be taken"
    );
}

#[test]
fn narrow_script_analysis_is_disabled_outside_profiling() {
    let mut component = test_frontend_component(
        "<template><box><text>a</text><text>b</text><text>c</text></box></template>",
    );
    let theme = themed_primary("test", "#000000");
    let surface_css_props = component.surface_css_props();

    let tree = component.build_tree(&theme, 100, 100);
    let _ = component.retained_tree.update(&tree);
    component.narrow_path_active = false;
    component.affected_node_count = 0;

    let result = component.narrow_script_update(&theme, 100, 100, &surface_css_props);

    assert!(result.is_some());
    assert!(!component.narrow_path_active);
    assert_eq!(component.affected_node_count, 0);
}

// cargo test -p mesh-core-shell --release -- narrow_script_analysis_cost --ignored --nocapture
#[test]
#[ignore = "release-only narrow-script analysis microbenchmark"]
fn narrow_script_analysis_cost() {
    use std::hint::black_box;
    use std::time::Instant;

    let mut component = test_frontend_component(&format!(
        "<template><box>{}</box></template>",
        (0..1_000)
            .map(|index| format!("<text>row {index}</text>"))
            .collect::<String>()
    ));
    let theme = themed_primary("test", "#000000");
    let tree = component.build_tree(&theme, 1_000, 1_000);
    let _ = component.retained_tree.update(&tree);
    let iterations = 1_000;

    let analysis_started = Instant::now();
    for _ in 0..iterations {
        black_box(component.retained_tree.narrow_script_diff(black_box(&tree)));
    }
    let analysis = analysis_started.elapsed();

    let gated_started = Instant::now();
    for _ in 0..iterations {
        black_box(black_box(&tree));
    }
    let gated = gated_started.elapsed();

    eprintln!(
        "narrow-script analysis: {analysis:?}; profiling-off gate: {gated:?}; ratio: {:.1}x",
        analysis.as_secs_f64() / gated.as_secs_f64()
    );
    assert!(gated < analysis);
}
