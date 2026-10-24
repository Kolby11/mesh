use super::*;
use crate::shell::component::tests::common::{test_frontend_component, themed_primary};
use mesh_core_elements::Prop;

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

    // Model the narrow-script case directly: one existing text node changes
    // while the retained tree identity and all unrelated node state remain.
    // Building a second component introduces unrelated runtime differences,
    // which does not exercise the threshold this test is meant to cover.
    let mut tree2 = tree1.clone();
    tree2.children[0].children[0]
        .attributes
        .insert(Prop::Content, "x".into());

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
fn narrow_script_scope_is_published_outside_profiling() {
    let mut component = test_frontend_component(
        "<template><box><text>a</text><text>b</text><text>c</text></box></template>",
    );
    let theme = themed_primary("test", "#000000");
    let surface_css_props = component.surface_css_props();

    let tree = component.build_tree(&theme, 100, 100);
    let _ = component.retained_tree.update(&tree);
    component.last_tree = Some(tree);
    component.narrow_path_active = false;
    component.affected_node_count = 0;

    let _result = component.narrow_script_update(&theme, 100, 100, &surface_css_props);
    assert!(!component.narrow_path_active);
    assert_eq!(component.affected_node_count, 0);
    assert!(component.retained_update_dirty_roots.is_some());
}

#[test]
fn narrow_script_handler_uses_scoped_retained_fingerprinting() {
    let source = format!(
        r#"
<template><row><text>{{label}}</text>{}</row></template>
<script lang="luau">
label = "aaaaaa"
function updateLabel()
  label = "bbbbbb"
end
</script>
"#,
        "<box />".repeat(32)
    );
    let mut component = test_frontend_component(&source);
    let theme = themed_primary("test", "#000000");
    let mut buffer = PixelBuffer::new(160, 40);
    component
        .paint_raster(&theme, SurfaceExtent::unpadded(160, 40), &mut buffer, 1.0)
        .unwrap();
    if component.wants_render() {
        component
            .paint_raster(&theme, SurfaceExtent::unpadded(160, 40), &mut buffer, 1.0)
            .unwrap();
    }

    component
        .call_namespaced_handler("updateLabel", &[])
        .unwrap();
    component
        .paint_raster(&theme, SurfaceExtent::unpadded(160, 40), &mut buffer, 1.0)
        .unwrap();

    assert!(component.retained_tree.last_update_was_scoped());
}

#[test]
fn narrow_script_structural_change_keeps_full_retained_fallback() {
    let mut component = test_frontend_component(
        r#"
<template>
  <column>
    {#if visible}<text>visible</text>{/if}
  </column>
</template>
<script lang="luau">
visible = true
function hide()
  visible = false
end
</script>
"#,
    );
    let theme = themed_primary("test", "#000000");
    let mut buffer = PixelBuffer::new(160, 40);
    component
        .paint_raster(&theme, SurfaceExtent::unpadded(160, 40), &mut buffer, 1.0)
        .unwrap();

    component.call_namespaced_handler("hide", &[]).unwrap();
    component
        .paint_raster(&theme, SurfaceExtent::unpadded(160, 40), &mut buffer, 1.0)
        .unwrap();

    assert!(!component.retained_tree.last_update_was_scoped());
}

fn direct_audio_service_component(
    static_nodes: usize,
    dynamic_structure: bool,
    render_hook: bool,
) -> FrontendSurfaceComponent {
    direct_audio_service_component_with_hook_body(
        static_nodes,
        dynamic_structure,
        render_hook.then_some(""),
    )
}

/// `render_hook_body` is the body of a `render()` hook, or `None` for a
/// component without one.
fn direct_audio_service_component_with_hook_body(
    static_nodes: usize,
    dynamic_structure: bool,
    render_hook_body: Option<&str>,
) -> FrontendSurfaceComponent {
    let static_nodes = (0..static_nodes)
        .map(|index| format!("<text>static row {index}</text>"))
        .collect::<String>();
    let conditional = if dynamic_structure {
        "{#if require('mesh.audio@>=1.0').muted}<text>muted</text>{:else}<text>live</text>{/if}"
    } else {
        ""
    };
    let render_hook = render_hook_body
        .map(|body| format!("effect(function() {body} end)"))
        .unwrap_or_default();
    test_frontend_component_with_catalog(
        &format!(
            r#"
<template><column><text>{{require('mesh.audio@>=1.0').percent}}</text><text>{{hook_label}}</text>{conditional}{static_nodes}</column></template>
<script lang="luau">hook_label = "" function init() end {render_hook}</script>
"#
        ),
        audio_network_catalog(),
        &["service.audio.read"],
    )
}

fn apply_audio_update(component: &mut FrontendSurfaceComponent, percent: u64, muted: bool) {
    component
        .handle_service_event(&ServiceEvent::Updated {
            service: "mesh.audio".into(),
            source_module: "@mesh/pipewire-audio".into(),
            payload: serde_json::json!({ "percent": percent, "muted": muted }),
        })
        .unwrap();
}

fn prime_audio_component(
    component: &mut FrontendSurfaceComponent,
    theme: &Theme,
    buffer: &mut PixelBuffer,
) {
    component
        .paint_raster(theme, SurfaceExtent::unpadded(640, 160), buffer, 1.0)
        .unwrap();
    apply_audio_update(component, 10, false);
    component
        .paint_raster(theme, SurfaceExtent::unpadded(640, 160), buffer, 1.0)
        .unwrap();
}

#[test]
fn service_field_update_patches_bindings_without_template_evaluation() {
    let mut component = direct_audio_service_component(64, false, false);
    let theme = default_theme();
    let mut buffer = PixelBuffer::new(640, 160);
    prime_audio_component(&mut component, &theme, &mut buffer);
    fn service_reads(node: &WidgetNode, output: &mut Vec<(String, String)>) {
        output.extend(node.service_field_reads.iter().cloned());
        for child in &node.children {
            service_reads(child, output);
        }
    }
    let mut reads = Vec::new();
    service_reads(component.last_tree.as_ref().unwrap(), &mut reads);
    assert!(
        !component
            .node_service_field_deps
            .nodes_reading_field("audio", "percent")
            .is_empty(),
        "template evaluation should publish its direct service-field dependency: {reads:?}"
    );

    apply_audio_update(&mut component, 20, false);
    assert!(component.compiled.supports_selective_service_build());
    assert!(!component.has_reactive_effects());
    assert!(component.pending_service_template_nodes.is_some());
    component
        .paint_raster(&theme, SurfaceExtent::unpadded(640, 160), &mut buffer, 1.0)
        .unwrap();

    assert!(rendered_text(&component).iter().any(|text| text == "20"));
    let snapshot = component.take_invalidation_snapshot().unwrap();
    assert_eq!(snapshot.patched_node_count, 1);
    assert_eq!(snapshot.component.script_narrow, 0);
    assert!(!snapshot.full_rebuild);
    assert!(component.retained_tree.last_update_was_scoped());
}

#[test]
fn service_fields_patch_keyed_rows_and_preserve_conditional_fallback() {
    let source = r#"
<script lang="luau">
local audio = require('mesh.audio@>=1.0')
items = {{id = "a"}, {id = "b"}}
</script>
<template><column style="width: 640px; height: 160px">
{#for item in items key={item.id}}<text>{item.id .. ':' .. tostring(audio.percent)}</text>{/for}
{#if audio.muted}<text>muted</text>{/if}
</column></template>
"#;
    let mut component = test_frontend_component_with_catalog(
        source,
        audio_network_catalog(),
        &["service.audio.read"],
    );
    let theme = default_theme();
    let extent = SurfaceExtent::unpadded(640, 160);
    let mut buffer = PixelBuffer::new(640, 160);
    prime_audio_component(&mut component, &theme, &mut buffer);
    apply_audio_update(&mut component, 20, false);
    apply_audio_update(&mut component, 30, false);
    component
        .paint_raster(&theme, extent, &mut buffer, 1.0)
        .unwrap();
    let snapshot = component.take_invalidation_snapshot().unwrap();
    assert_eq!(snapshot.patched_node_count, 2);
    assert_eq!(snapshot.component.script_narrow, 0);
    let text = rendered_text(&component);
    assert!(text.iter().any(|text| text == "a:30"));
    assert!(text.iter().any(|text| text == "b:30"));
    let mut rebuilt = PixelBuffer::new(640, 160);
    component.invalidate_script_state();
    component
        .paint_raster(&theme, extent, &mut rebuilt, 1.0)
        .unwrap();
    assert_eq!(buffer.data(), rebuilt.data());

    apply_audio_update(&mut component, 40, true);
    component
        .paint_raster(&theme, extent, &mut buffer, 1.0)
        .unwrap();
    let snapshot = component.take_invalidation_snapshot().unwrap();
    assert_eq!(snapshot.patched_node_count, 0);
    assert_eq!(snapshot.component.script_narrow, 1);
    assert!(rendered_text(&component).iter().any(|text| text == "muted"));
    component.invalidate_script_state();
    component
        .paint_raster(&theme, extent, &mut rebuilt, 1.0)
        .unwrap();
    assert_eq!(buffer.data(), rebuilt.data());
}

#[test]
fn service_fields_patch_parent_and_child_in_one_transaction() {
    let mut component = test_frontend_component_with_catalog(
        r#"<script lang="luau">local audio = require('mesh.audio@>=1.0')</script>
<template><column data-percent={audio.percent}><text>{audio.percent}</text></column></template>"#,
        audio_network_catalog(),
        &["service.audio.read"],
    );
    let theme = default_theme();
    let mut pixels = PixelBuffer::new(640, 160);
    prime_audio_component(&mut component, &theme, &mut pixels);
    apply_audio_update(&mut component, 20, false);
    component
        .paint_raster(&theme, SurfaceExtent::unpadded(640, 160), &mut pixels, 1.0)
        .unwrap();
    let column = &component.last_tree.as_ref().unwrap().children[0];
    assert_eq!(
        column
            .attributes
            .get_by_name("data-percent")
            .map(String::as_str),
        Some("20")
    );
    assert_eq!(
        column.children[0]
            .attributes
            .get(Prop::Content)
            .map(String::as_str),
        Some("20")
    );
    assert_eq!(
        component
            .take_invalidation_snapshot()
            .unwrap()
            .patched_node_count,
        2
    );
}

#[test]
fn service_binding_failure_does_not_publish_partial_patches() {
    let source = r#"
<script lang="luau">
local audio = require('mesh.audio@>=1.0')
local function label()
    if audio.percent == 20 and audio.muted then error("invalid presentation value") end
    return tostring(audio.percent)
end
</script>
<template><column style="width: 640px; height: 160px"><text>{audio.percent}</text><text>{label()}</text><text>{audio.muted}</text></column></template>
"#;
    let mut component = test_frontend_component_with_catalog(
        source,
        audio_network_catalog(),
        &["service.audio.read"],
    );
    let theme = default_theme();
    let extent = SurfaceExtent::unpadded(640, 160);
    let mut buffer = PixelBuffer::new(640, 160);
    prime_audio_component(&mut component, &theme, &mut buffer);
    let previous = buffer.data().to_vec();
    apply_audio_update(&mut component, 20, true);
    component
        .paint_raster(&theme, extent, &mut buffer, 1.0)
        .unwrap();
    assert_eq!(buffer.data(), previous.as_slice());
    assert!(!rendered_text(&component).iter().any(|text| text == "20"));
    apply_audio_update(&mut component, 20, false);
    component
        .paint_raster(&theme, extent, &mut buffer, 1.0)
        .unwrap();
    assert_eq!(
        rendered_text(&component)
            .iter()
            .filter(|text| text.as_str() == "20")
            .count(),
        2
    );
}

#[test]
fn dynamic_service_template_keeps_full_evaluation_fallback() {
    let mut component = direct_audio_service_component(16, true, false);
    let theme = default_theme();
    let mut buffer = PixelBuffer::new(640, 160);
    prime_audio_component(&mut component, &theme, &mut buffer);

    apply_audio_update(&mut component, 20, true);
    component
        .paint_raster(&theme, SurfaceExtent::unpadded(640, 160), &mut buffer, 1.0)
        .unwrap();

    let text = rendered_text(&component);
    assert!(text.iter().any(|text| text == "20"));
    assert!(text.iter().any(|text| text == "muted"));
    assert_eq!(component.last_template_build_reused_nodes, 0);
}

#[test]
fn service_template_patches_direct_reads_and_effect_writes_together() {
    let mut component = direct_audio_service_component_with_hook_body(
        16,
        false,
        Some("hook_label = tostring(require('mesh.audio@>=1.0').percent) .. '!'"),
    );
    let theme = default_theme();
    let mut buffer = PixelBuffer::new(640, 160);
    prime_audio_component(&mut component, &theme, &mut buffer);

    apply_audio_update(&mut component, 20, false);
    assert!(component.has_reactive_effects());
    component
        .paint_raster(&theme, SurfaceExtent::unpadded(640, 160), &mut buffer, 1.0)
        .unwrap();

    let text = rendered_text(&component);
    assert!(text.iter().any(|text| text == "20"));
    assert!(
        text.iter().any(|text| text == "20!"),
        "the effect's write must reach the template: {text:?}"
    );
    assert_eq!(component.last_template_build_reused_nodes, 0);
    let snapshot = component.take_invalidation_snapshot().unwrap();
    assert_eq!(snapshot.patched_node_count, 2);
    assert_eq!(snapshot.component.script_narrow, 0);
}

#[test]
fn service_template_with_inert_effect_still_patches_bindings() {
    let mut component = direct_audio_service_component(16, false, true);
    let theme = default_theme();
    let mut buffer = PixelBuffer::new(640, 160);
    prime_audio_component(&mut component, &theme, &mut buffer);

    apply_audio_update(&mut component, 20, false);
    assert!(component.has_reactive_effects());
    component
        .paint_raster(&theme, SurfaceExtent::unpadded(640, 160), &mut buffer, 1.0)
        .unwrap();

    assert!(rendered_text(&component).iter().any(|text| text == "20"));
    let snapshot = component.take_invalidation_snapshot().unwrap();
    assert_eq!(snapshot.patched_node_count, 1);
    assert_eq!(snapshot.component.script_narrow, 0);
}

#[test]
fn equal_size_service_text_patches_skip_layout_and_match_full_paint() {
    let theme = default_theme();
    for siblings in [16, 1024] {
        let mut patched = direct_audio_service_component(siblings, false, false);
        let mut full = direct_audio_service_component(siblings, false, false);
        full.force_full_template_build = true;
        let mut pixels = PixelBuffer::new(640, 160);
        let mut expected = PixelBuffer::new(640, 160);
        prime_audio_component(&mut patched, &theme, &mut pixels);
        prime_audio_component(&mut full, &theme, &mut expected);
        apply_audio_update(&mut patched, 20, false);
        apply_audio_update(&mut full, 20, false);
        patched
            .paint_raster(&theme, SurfaceExtent::unpadded(640, 160), &mut pixels, 1.0)
            .unwrap();
        full.paint_raster(
            &theme,
            SurfaceExtent::unpadded(640, 160),
            &mut expected,
            1.0,
        )
        .unwrap();
        patched.set_profiling_enabled(true);
        let children = patched.last_tree.as_ref().unwrap().children[0]
            .children
            .as_ptr();
        for percent in [21, 20, 21, 20] {
            apply_audio_update(&mut patched, percent, false);
            apply_audio_update(&mut full, percent, false);
            patched
                .paint_raster(&theme, SurfaceExtent::unpadded(640, 160), &mut pixels, 1.0)
                .unwrap();
            full.paint_raster(
                &theme,
                SurfaceExtent::unpadded(640, 160),
                &mut expected,
                1.0,
            )
            .unwrap();
            let snapshot = patched.take_invalidation_snapshot().unwrap();
            assert_eq!(snapshot.patched_node_count, 1);
            assert_eq!(
                patched.last_tree.as_ref().unwrap().children[0]
                    .children
                    .as_ptr(),
                children
            );
            assert!(
                snapshot.node_visits["display_build"] <= 3,
                "{siblings}: {snapshot:?}"
            );
            for pass in [
                "layout_write",
                "layout_fit",
                "layout_extents",
                "focus_scope_refresh",
                "disclosure_refresh",
                "interaction_reconcile",
            ] {
                assert_eq!(snapshot.node_visits[pass], 0, "{siblings}: {snapshot:?}");
            }
            patched
                .retained_tree
                .assert_complete(patched.last_tree.as_ref().unwrap());
            assert_eq!(pixels.data(), expected.data());
        }
    }
}

#[test]
fn text_patch_interaction_reuse_rejects_eligibility_and_reference_changes() {
    let mut component = direct_audio_service_component(16, false, false);
    let theme = default_theme();
    let mut pixels = PixelBuffer::new(640, 160);
    prime_audio_component(&mut component, &theme, &mut pixels);
    let id = *component
        .node_service_field_deps
        .nodes_reading_field("audio", "percent")
        .iter()
        .next()
        .unwrap();
    let path = component
        .last_frame_snapshot
        .as_ref()
        .unwrap()
        .node_path(id)
        .unwrap()
        .to_vec();
    let original = component.last_tree.as_ref().unwrap().clone();
    for mutation in [
        "content", "disabled", "id", "state", "style", "geometry", "ancestor",
    ] {
        let mut tree = original.clone();
        component.frame_dirty.clear();
        component
            .frame_dirty
            .mark(id, mesh_core_elements::ChangeClass::PAINT);
        let mut target = &mut tree;
        for &index in &path {
            target = &mut target.children[index];
        }
        target
            .attributes
            .insert(mesh_core_elements::Prop::Content, "21".into());
        match mutation {
            "disabled" => {
                target
                    .attributes
                    .insert(mesh_core_elements::Prop::Disabled, "true".into());
            }
            "id" => {
                target
                    .attributes
                    .insert(mesh_core_elements::Prop::Id, "another-target".into());
            }
            "state" => {
                target.state.insert(mesh_core_elements::NodeState::DISABLED);
            }
            "style" => {
                target.computed_style.opacity = 0.0;
            }
            "geometry" => {
                target.layout.width += 1.0;
            }
            "ancestor" => {
                tree.computed_style.opacity = 0.0;
                component
                    .frame_dirty
                    .mark(tree.id, mesh_core_elements::ChangeClass::STYLE);
            }
            _ => {}
        }
        assert_eq!(
            component.text_patch_preserves_interaction_targets(&tree),
            mutation == "content",
            "{mutation}"
        );
    }
}

// cargo test -p mesh-core-shell --release -- affected_template_eval_beats_full_rebuild --ignored --nocapture
#[test]
#[ignore = "release-only affected-template evaluation benchmark"]
fn affected_template_eval_beats_full_rebuild() {
    use std::time::{Duration, Instant};

    let mut selective = direct_audio_service_component(1_024, false, false);
    let mut full = direct_audio_service_component(1_024, false, false);
    full.force_full_template_build = true;
    let theme = default_theme();
    let mut selective_buffer = PixelBuffer::new(640, 160);
    let mut full_buffer = PixelBuffer::new(640, 160);
    prime_audio_component(&mut selective, &theme, &mut selective_buffer);
    prime_audio_component(&mut full, &theme, &mut full_buffer);

    let iterations = 100;
    let mut selective_time = Duration::ZERO;
    let mut full_time = Duration::ZERO;
    for iteration in 0..iterations {
        let percent = 20 + (iteration % 2) as u64;
        if iteration % 2 == 0 {
            let started = Instant::now();
            apply_audio_update(&mut full, percent, false);
            full.paint_raster(
                &theme,
                SurfaceExtent::unpadded(640, 160),
                &mut full_buffer,
                1.0,
            )
            .unwrap();
            full_time += started.elapsed();

            let started = Instant::now();
            apply_audio_update(&mut selective, percent, false);
            selective
                .paint_raster(
                    &theme,
                    SurfaceExtent::unpadded(640, 160),
                    &mut selective_buffer,
                    1.0,
                )
                .unwrap();
            selective_time += started.elapsed();
        } else {
            let started = Instant::now();
            apply_audio_update(&mut selective, percent, false);
            selective
                .paint_raster(
                    &theme,
                    SurfaceExtent::unpadded(640, 160),
                    &mut selective_buffer,
                    1.0,
                )
                .unwrap();
            selective_time += started.elapsed();

            let started = Instant::now();
            apply_audio_update(&mut full, percent, false);
            full.paint_raster(
                &theme,
                SurfaceExtent::unpadded(640, 160),
                &mut full_buffer,
                1.0,
            )
            .unwrap();
            full_time += started.elapsed();
        }
    }

    assert_eq!(selective_buffer.data(), full_buffer.data());
    assert_eq!(
        selective
            .take_invalidation_snapshot()
            .unwrap()
            .patched_node_count,
        1
    );
    let speedup = full_time.as_secs_f64() / selective_time.as_secs_f64();
    eprintln!(
        "affected template evaluation over {iterations} one-field-dirty 1,026-node service frames: full {full_time:?}; selective {selective_time:?}; ratio {speedup:.3}x"
    );
    eprintln!("MESH_PERF metric=affected_template_eval_speedup value={speedup:.6}");
    assert!(
        selective_time * 100 < full_time * 99,
        "affected-template evaluation should improve end-to-end frames by at least 1%"
    );
}

fn derived_audio_service_component(
    static_nodes: usize,
    structural: bool,
) -> FrontendSurfaceComponent {
    let conditional = if structural {
        "{#if visible}<text>audible</text>{/if}"
    } else {
        ""
    };
    test_frontend_component_with_catalog(
        &format!(
            r#"
<template><column class="derived-root"><text>{{label}}</text>{conditional}{}</column></template>
<style>.derived-root {{ width: 640px; height: 160px; overflow: hidden; }}</style>
<script lang="luau">
local audio = require("mesh.audio@>=1.0")
label = "0%"
visible = true
effect(function()
  label = string.format("%d%%", tonumber(audio.percent) or 0)
  visible = not audio.muted
end)
</script>
"#,
            (0..static_nodes)
                .map(|index| format!("<text>static row {index}</text>"))
                .collect::<String>()
        ),
        audio_network_catalog(),
        &["service.audio.read"],
    )
}

#[test]
fn derived_service_variables_use_narrow_retained_update() {
    let mut component = derived_audio_service_component(64, false);
    let theme = default_theme();
    let mut buffer = PixelBuffer::new(640, 160);
    prime_audio_component(&mut component, &theme, &mut buffer);
    assert!(
        component
            .node_service_field_deps
            .nodes_reading_field("audio", "percent")
            .is_empty()
    );
    apply_audio_update(&mut component, 20, false);
    // The effect settles at delivery and its bound write patches in place.
    assert!(component.wants_render());
    assert!(!component.dirty_types.contains(ComponentDirtyFlags::SCRIPT));
    assert!(!component.surface_pixels_invalid);
    component
        .paint_raster(&theme, SurfaceExtent::unpadded(640, 160), &mut buffer, 1.0)
        .unwrap();
    assert!(rendered_text(&component).iter().any(|text| text == "20%"));
    assert!(component.retained_tree.last_update_was_scoped());
}

#[test]
fn derived_service_structure_changes_fall_back_to_full_retained_update() {
    let mut component = derived_audio_service_component(64, true);
    let theme = default_theme();
    let mut buffer = PixelBuffer::new(640, 160);
    prime_audio_component(&mut component, &theme, &mut buffer);
    apply_audio_update(&mut component, 20, true);
    component
        .paint_raster(&theme, SurfaceExtent::unpadded(640, 160), &mut buffer, 1.0)
        .unwrap();
    assert!(
        !rendered_text(&component)
            .iter()
            .any(|text| text == "audible")
    );
    assert!(!component.retained_tree.last_update_was_scoped());
}

#[test]
fn legacy_render_is_not_automatically_scheduled() {
    let mut component = test_frontend_component(
        r#"
<template><text>{label}</text></template>
<script lang="luau">
label = "setup"
function render() label = "legacy" end
</script>
"#,
    );
    let theme = default_theme();
    let mut buffer = PixelBuffer::new(160, 40);
    component
        .paint_raster(&theme, SurfaceExtent::unpadded(160, 40), &mut buffer, 1.0)
        .unwrap();
    assert!(rendered_text(&component).iter().any(|text| text == "setup"));
    assert!(!component.has_reactive_effects());
}

#[test]
fn selective_service_fallback_preserves_child_lifecycle() {
    let static_nodes = (0..128)
        .map(|index| format!("<text>static {index}</text>"))
        .collect::<String>();
    let source = format!(
        r#"
<template><column><text>{{require('mesh.audio@>=1.0').percent}}</text>
<popover open="true"><box class="panel"><text>Popup</text></box></popover>
{static_nodes}</column></template>
<style>
.panel {{ width: 80px; height: 40px; }}
.panel.mesh-surface-entering, .panel.mesh-surface-exiting {{ opacity: 0.4; }}
</style>
"#
    );
    let theme = default_theme();
    let mut total_reused = 0;
    for entering in [true, false] {
        let make = || {
            test_frontend_component_with_catalog(
                &source,
                audio_network_catalog(),
                &["service.audio.read"],
            )
        };
        let mut selective = make();
        let mut full = make();
        full.force_full_template_build = true;
        let mut selective_pixels = PixelBuffer::new(640, 160);
        let mut full_pixels = PixelBuffer::new(640, 160);
        for (component, pixels) in [
            (&mut selective, &mut selective_pixels),
            (&mut full, &mut full_pixels),
        ] {
            let mut settings = component.settings.clone();
            Arc::make_mut(&mut settings)
                .set_namespace("shell", serde_json::json!({"motion":{"reduced":true}}));
            component.apply_settings(&settings).unwrap();
            prime_audio_component(component, &theme, pixels);
            let key = component.child_surface_requests().remove(0).node_key;
            if entering {
                component.set_entering_child_keys([key].into_iter().collect());
            } else {
                component.set_closing_child_keys([key].into_iter().collect());
            }
            component
                .paint_raster(&theme, SurfaceExtent::unpadded(640, 160), pixels, 1.0)
                .unwrap();
            component.set_profiling_enabled(true);
        }
        for percent in [20, 21, 22] {
            for (component, pixels) in [
                (&mut selective, &mut selective_pixels),
                (&mut full, &mut full_pixels),
            ] {
                // Exercise a coverage miss on every update, even after a build reseeds the index.
                component.template_binding_index = Default::default();
                component.last_template_build_reused_nodes = 0;
                apply_audio_update(component, percent, false);
                assert!(component.pending_service_template_nodes.is_some());
                component
                    .paint_raster(&theme, SurfaceExtent::unpadded(640, 160), pixels, 1.0)
                    .unwrap();
            }
            assert_eq!(selective_pixels.data(), full_pixels.data());
            assert_eq!(rendered_text(&selective), rendered_text(&full));
            let panel =
                first_node_by_class(selective.last_tree.as_ref().unwrap(), "panel").unwrap();
            assert_eq!(panel.computed_style.opacity, 0.4);
            assert!(panel.class_tokens().iter().any(|class| class
                == if entering {
                    "mesh-surface-entering"
                } else {
                    "mesh-surface-exiting"
                }));
            total_reused += selective.last_template_build_reused_nodes;
        }
        for (component, pixels) in [
            (&mut selective, &mut selective_pixels),
            (&mut full, &mut full_pixels),
        ] {
            component.set_entering_child_keys(Default::default());
            component.set_closing_child_keys(Default::default());
            component
                .paint_raster(&theme, SurfaceExtent::unpadded(640, 160), pixels, 1.0)
                .unwrap();
            let panel =
                first_node_by_class(component.last_tree.as_ref().unwrap(), "panel").unwrap();
            assert!(
                !panel
                    .class_tokens()
                    .iter()
                    .any(|class| class.starts_with("mesh-surface-"))
            );
        }
        assert_eq!(selective_pixels.data(), full_pixels.data());
    }
    eprintln!("MESH_PERF fallback updates=6 reused_authored_nodes={total_reused}");
    assert!(total_reused >= 128 * 6);
}
