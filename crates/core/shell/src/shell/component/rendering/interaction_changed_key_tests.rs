use super::*;
use mesh_core_component::style::StyleValue;

fn keyed_node(key: &str, children: Vec<WidgetNode>) -> WidgetNode {
    let mut node = WidgetNode::new("box");
    node.id = crate::shell::component::runtime_tree::stable_runtime_node_id(key);
    node.attributes.insert("_mesh_key".into(), key.into());
    node.children = children.into();
    node
}

fn diagnostic_fingerprint(retained_tree_generation: u64) -> RuntimeStyleDiagnosticFingerprint {
    RuntimeStyleDiagnosticFingerprint {
        rules_generation: 7,
        retained_tree_generation,
        props: 11,
        container_width: 800.0f32.to_bits(),
        container_height: 600.0f32.to_bits(),
    }
}

#[test]
fn runtime_style_diagnostic_generation_tracks_every_resolution_input() {
    let mut child = WidgetNode::new("button");
    child
        .attributes
        .insert("class".into(), "primary wide".into());
    child.attributes.insert("id".into(), "save".into());
    child.set_mesh_key("root/save");
    child.set_module_id("@test/controls");
    let mut tree = WidgetNode::new("surface");
    tree.children.push(child);
    let mut retained = RetainedWidgetTree::default();
    retained.update(&tree);
    let baseline_generation = retained.generation();
    let baseline = diagnostic_fingerprint(baseline_generation);

    let assert_tree_change = |changed: WidgetNode| {
        let mut retained = RetainedWidgetTree::default();
        retained.update(&tree);
        retained.update(&changed);
        assert_ne!(retained.generation(), baseline_generation);
    };

    let mut changed = tree.clone();
    changed.children[0].tag = "input".into();
    assert_tree_change(changed);
    let mut changed = tree.clone();
    changed.children[0]
        .attributes
        .insert("class".into(), "secondary".into());
    assert_tree_change(changed);
    let mut changed = tree.clone();
    changed.children[0]
        .attributes
        .insert("id".into(), "apply".into());
    assert_tree_change(changed);
    let mut changed = tree.clone();
    changed.children[0].state.set(NodeState::FOCUSED, true);
    assert_tree_change(changed);
    let mut changed = tree.clone();
    changed.children[0].set_module_id("@test/alternate");
    assert_tree_change(changed);
    let mut changed = tree.clone();
    changed.children.push(WidgetNode::new("text"));
    assert_tree_change(changed);

    let mut changed = baseline;
    changed.rules_generation += 1;
    assert_ne!(changed, baseline);
    let mut changed = baseline;
    changed.container_width = 801.0f32.to_bits();
    assert_ne!(changed, baseline);
    let mut changed = baseline;
    changed.container_height = 601.0f32.to_bits();
    assert_ne!(changed, baseline);
    let mut changed = baseline;
    changed.props += 1;
    assert_ne!(changed, baseline);
}

#[test]
fn runtime_style_diagnostic_gate_reuses_only_identical_inputs() {
    let fingerprint = diagnostic_fingerprint(42);
    let mut previous = None;
    assert!(runtime_style_diagnostic_inputs_changed(
        &mut previous,
        fingerprint
    ));
    assert!(!runtime_style_diagnostic_inputs_changed(
        &mut previous,
        fingerprint
    ));
    let changed = RuntimeStyleDiagnosticFingerprint {
        rules_generation: fingerprint.rules_generation + 1,
        ..fingerprint
    };
    assert!(runtime_style_diagnostic_inputs_changed(
        &mut previous,
        changed
    ));
}

#[test]
fn runtime_style_diagnostic_props_hash_is_order_independent_and_value_sensitive() {
    let mut left = SurfaceCssProps::new();
    left.insert("accent".into(), StyleValue::Literal("#abcdef".into()));
    left.insert("spacing".into(), StyleValue::Var("--space-md".into()));
    let mut right = SurfaceCssProps::new();
    right.insert("spacing".into(), StyleValue::Var("--space-md".into()));
    right.insert("accent".into(), StyleValue::Literal("#abcdef".into()));
    assert_eq!(
        runtime_style_diagnostic_props_fingerprint(&left),
        runtime_style_diagnostic_props_fingerprint(&right)
    );
    right.insert("accent".into(), StyleValue::Literal("#fedcba".into()));
    assert_ne!(
        runtime_style_diagnostic_props_fingerprint(&left),
        runtime_style_diagnostic_props_fingerprint(&right)
    );
}

fn benchmark_selection(target: String) -> TextSelectionState {
    TextSelectionState {
        anchor: TextSelectionPoint {
            node_key: target.clone(),
            x: 2.0,
            y: 3.0,
        },
        focus: TextSelectionPoint {
            node_key: target,
            x: 18.0,
            y: 3.0,
        },
        dragging: true,
    }
}

#[test]
fn direct_interaction_scope_keeps_only_changed_targets() {
    let changed = HashSet::from([stable_runtime_node_id("root/0")]);
    let affected = direct_interaction_changed_node_ids(changed);

    assert_eq!(
        affected.affected,
        HashSet::from([crate::shell::component::runtime_tree::stable_runtime_node_id("root/0")])
    );
}

#[test]
fn direct_interaction_scope_keeps_nested_targets() {
    let parent = stable_runtime_node_id("root/0");
    let child = stable_runtime_node_id("root/0/0");
    let changed = HashSet::from([parent, child]);
    let affected = direct_interaction_changed_node_ids(changed);

    assert_eq!(affected.affected, HashSet::from([parent, child]));
}

#[test]
fn hover_changed_ids_only_collects_tails_after_common_ancestor() {
    let previous = ["root", "root/menu", "root/menu/left"].map(stable_runtime_node_id);
    let current = [
        "root",
        "root/menu",
        "root/menu/right",
        "root/menu/right/icon",
    ]
    .map(stable_runtime_node_id);
    let mut changed = HashSet::new();

    collect_hover_changed_ids(&previous, &current, &mut changed);

    assert_eq!(
        changed,
        HashSet::from([
            stable_runtime_node_id("root/menu/left"),
            stable_runtime_node_id("root/menu/right"),
            stable_runtime_node_id("root/menu/right/icon"),
        ])
    );
}

#[test]
fn targeted_attribute_state_only_updates_affected_subtrees() {
    // Element tag defaults are resolved during style resolution, not here;
    // `apply_runtime_attribute_state` owns the attribute-driven layout state,
    // and this proves it reaches the affected node's whole subtree and nothing
    // outside it.
    let mut tree = keyed_node(
        "root",
        vec![
            keyed_node("root/0", vec![keyed_node("root/0/0", vec![])]),
            keyed_node("root/1", vec![keyed_node("root/1/0", vec![])]),
        ],
    );
    for branch in 0..2 {
        for descendant in [None, Some(0)] {
            let node = match descendant {
                None => &mut tree.children[branch],
                Some(child) => &mut tree.children[branch].children[child],
            };
            node.attributes.insert("hidden".into(), "true".into());
        }
    }

    let affected =
        HashSet::from([crate::shell::component::runtime_tree::stable_runtime_node_id("root/0")]);

    apply_runtime_attribute_state_for_ids(&mut tree, &affected);

    use mesh_core_elements::style::Display;
    assert_eq!(tree.children[0].computed_style.display, Display::None);
    assert_eq!(
        tree.children[0].children[0].computed_style.display,
        Display::None,
        "the affected node's descendants are updated with it"
    );
    assert_ne!(
        tree.children[1].computed_style.display,
        Display::None,
        "an unaffected sibling keeps its previous layout state"
    );
    assert_ne!(
        tree.children[1].children[0].computed_style.display,
        Display::None
    );
}

#[test]
fn keyed_selection_annotation_only_marks_selectable_text_target() {
    let target = "root/0".to_string();
    let selection = benchmark_selection(target.clone());
    let mut selectable = WidgetNode::new("text");
    selectable.attributes.insert("_mesh_key".into(), target);
    selectable
        .attributes
        .insert("selectable".into(), "true".into());
    assert!(annotate_selected_text_node(
        &mut selectable,
        &selection,
        "#112233",
        "#ffffff"
    ));
    assert!(
        selectable
            .attributes
            .contains_key("_mesh_selection_background")
    );

    let mut non_selectable = WidgetNode::new("text");
    non_selectable
        .attributes
        .insert("_mesh_key".into(), "root/0".into());
    assert!(!annotate_selected_text_node(
        &mut non_selectable,
        &selection,
        "#112233",
        "#ffffff"
    ));
    assert!(
        !non_selectable
            .attributes
            .contains_key("_mesh_selection_background")
    );
}
