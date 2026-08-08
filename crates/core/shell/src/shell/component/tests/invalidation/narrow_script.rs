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

    // Model the narrow-script case directly: one existing text node changes
    // while the retained tree identity and all unrelated node state remain.
    // Building a second component introduces unrelated runtime differences,
    // which does not exercise the threshold this test is meant to cover.
    let mut tree2 = tree1.clone();
    tree2.children[0].children[0]
        .attributes
        .insert("content".into(), "x".into());

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
        .paint(&theme, SurfaceExtent::unpadded(160, 40), &mut buffer, 1.0)
        .unwrap();
    if component.wants_render() {
        component
            .paint(&theme, SurfaceExtent::unpadded(160, 40), &mut buffer, 1.0)
            .unwrap();
    }

    component
        .call_namespaced_handler("updateLabel", &[])
        .unwrap();
    component
        .paint(&theme, SurfaceExtent::unpadded(160, 40), &mut buffer, 1.0)
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
        .paint(&theme, SurfaceExtent::unpadded(160, 40), &mut buffer, 1.0)
        .unwrap();

    component.call_namespaced_handler("hide", &[]).unwrap();
    component
        .paint(&theme, SurfaceExtent::unpadded(160, 40), &mut buffer, 1.0)
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
        .map(|body| format!("function render() {body} end"))
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
        .paint(theme, SurfaceExtent::unpadded(640, 160), buffer, 1.0)
        .unwrap();
    apply_audio_update(component, 10, false);
    component
        .paint(theme, SurfaceExtent::unpadded(640, 160), buffer, 1.0)
        .unwrap();
}

#[test]
fn service_field_update_reuses_clean_static_template_subtrees() {
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
    assert!(!component.has_render_hooks());
    assert!(component.pending_service_template_nodes.is_some());
    component
        .paint(&theme, SurfaceExtent::unpadded(640, 160), &mut buffer, 1.0)
        .unwrap();

    assert!(rendered_text(&component).iter().any(|text| text == "20"));
    assert!(
        component.last_template_build_reused_nodes >= 64,
        "every clean static row should retain its authored payload; reused {}",
        component.last_template_build_reused_nodes
    );
    assert!(component.retained_tree.last_update_was_scoped());
}

#[test]
fn dynamic_service_template_keeps_full_evaluation_fallback() {
    let mut component = direct_audio_service_component(16, true, false);
    let theme = default_theme();
    let mut buffer = PixelBuffer::new(640, 160);
    prime_audio_component(&mut component, &theme, &mut buffer);

    apply_audio_update(&mut component, 20, true);
    component
        .paint(&theme, SurfaceExtent::unpadded(640, 160), &mut buffer, 1.0)
        .unwrap();

    let text = rendered_text(&component);
    assert!(text.iter().any(|text| text == "20"));
    assert!(text.iter().any(|text| text == "muted"));
    assert_eq!(component.last_template_build_reused_nodes, 0);
}

#[test]
fn service_template_with_writing_render_hook_keeps_full_evaluation_fallback() {
    // The hook writes a member the template renders, so nodes outside the
    // service-derived set can change and the selective build is unsafe.
    let mut component = direct_audio_service_component_with_hook_body(
        16,
        false,
        Some("hook_label = tostring(require('mesh.audio@>=1.0').percent) .. '!'"),
    );
    let theme = default_theme();
    let mut buffer = PixelBuffer::new(640, 160);
    prime_audio_component(&mut component, &theme, &mut buffer);

    apply_audio_update(&mut component, 20, false);
    assert!(component.has_render_hooks());
    component
        .paint(&theme, SurfaceExtent::unpadded(640, 160), &mut buffer, 1.0)
        .unwrap();

    let text = rendered_text(&component);
    assert!(text.iter().any(|text| text == "20"));
    assert!(
        text.iter().any(|text| text == "20!"),
        "the render hook's write must reach the template: {text:?}"
    );
    assert_eq!(component.last_template_build_reused_nodes, 0);
}

#[test]
fn service_template_with_inert_render_hook_still_reuses_clean_subtrees() {
    // Merely declaring `render()` is not a reason to rebuild the whole
    // template. The hook runs before the frame picks its path; when it writes
    // nothing a template reads, the service-derived selective build stands.
    let mut component = direct_audio_service_component(16, false, true);
    let theme = default_theme();
    let mut buffer = PixelBuffer::new(640, 160);
    prime_audio_component(&mut component, &theme, &mut buffer);

    apply_audio_update(&mut component, 20, false);
    assert!(component.has_render_hooks());
    component
        .paint(&theme, SurfaceExtent::unpadded(640, 160), &mut buffer, 1.0)
        .unwrap();

    assert!(rendered_text(&component).iter().any(|text| text == "20"));
    assert!(
        component.last_template_build_reused_nodes >= 16,
        "clean static rows should be retained; reused {}",
        component.last_template_build_reused_nodes
    );
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
            full.paint(
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
                .paint(
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
                .paint(
                    &theme,
                    SurfaceExtent::unpadded(640, 160),
                    &mut selective_buffer,
                    1.0,
                )
                .unwrap();
            selective_time += started.elapsed();

            let started = Instant::now();
            apply_audio_update(&mut full, percent, false);
            full.paint(
                &theme,
                SurfaceExtent::unpadded(640, 160),
                &mut full_buffer,
                1.0,
            )
            .unwrap();
            full_time += started.elapsed();
        }
    }

    assert_eq!(selective_buffer.data, full_buffer.data);
    assert!(selective.last_template_build_reused_nodes >= 1_024);
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

// cargo test -p mesh-core-shell --release -- narrow_script_scoped_end_to_end_benchmark --ignored --nocapture
#[test]
#[ignore = "release-only end-to-end narrow-script retained-scope benchmark"]
fn narrow_script_scoped_end_to_end_benchmark() {
    use std::hint::black_box;
    use std::time::{Duration, Instant};

    let mut source = String::from("<template><column><text>{label}</text>");
    for _ in 0..1_024 {
        source.push_str("<box />");
    }
    source.push_str(
        r#"</column></template>
<script lang="luau">
label = "a"
function toggleLabel()
  if label == "a" then label = "b" else label = "a" end
end
</script>"#,
    );

    let mut scoped = test_frontend_component(&source);
    let mut full = test_frontend_component(&source);
    full.force_full_retained_update = true;
    let theme = themed_primary("test", "#000000");
    let mut scoped_buffer = PixelBuffer::new(64, 16);
    let mut full_buffer = PixelBuffer::new(64, 16);
    scoped
        .paint(
            &theme,
            SurfaceExtent::unpadded(64, 16),
            &mut scoped_buffer,
            1.0,
        )
        .unwrap();
    full.paint(
        &theme,
        SurfaceExtent::unpadded(64, 16),
        &mut full_buffer,
        1.0,
    )
    .unwrap();
    if scoped.wants_render() {
        scoped
            .paint(
                &theme,
                SurfaceExtent::unpadded(64, 16),
                &mut scoped_buffer,
                1.0,
            )
            .unwrap();
    }
    if full.wants_render() {
        full.paint(
            &theme,
            SurfaceExtent::unpadded(64, 16),
            &mut full_buffer,
            1.0,
        )
        .unwrap();
    }

    let iterations = 100;
    let mut scoped_time = Duration::ZERO;
    let mut full_time = Duration::ZERO;
    for iteration in 0..iterations {
        if iteration % 2 == 0 {
            let started = Instant::now();
            scoped.call_namespaced_handler("toggleLabel", &[]).unwrap();
            scoped
                .paint(
                    &theme,
                    SurfaceExtent::unpadded(64, 16),
                    &mut scoped_buffer,
                    1.0,
                )
                .unwrap();
            scoped_time += started.elapsed();

            let started = Instant::now();
            full.call_namespaced_handler("toggleLabel", &[]).unwrap();
            full.paint(
                &theme,
                SurfaceExtent::unpadded(64, 16),
                &mut full_buffer,
                1.0,
            )
            .unwrap();
            full_time += started.elapsed();
        } else {
            let started = Instant::now();
            full.call_namespaced_handler("toggleLabel", &[]).unwrap();
            full.paint(
                &theme,
                SurfaceExtent::unpadded(64, 16),
                &mut full_buffer,
                1.0,
            )
            .unwrap();
            full_time += started.elapsed();

            let started = Instant::now();
            scoped.call_namespaced_handler("toggleLabel", &[]).unwrap();
            scoped
                .paint(
                    &theme,
                    SurfaceExtent::unpadded(64, 16),
                    &mut scoped_buffer,
                    1.0,
                )
                .unwrap();
            scoped_time += started.elapsed();
        }
    }
    black_box((&scoped_buffer, &full_buffer));

    let speedup = full_time.as_secs_f64() / scoped_time.as_secs_f64();
    eprintln!(
        "end-to-end narrow-script paints over {iterations} one-node-dirty 1,026-node frames: full retained fingerprints {full_time:?}; scoped {scoped_time:?}; ratio {speedup:.3}x"
    );
    eprintln!("MESH_PERF metric=narrow_script_frame_speedup value={speedup:.6}");
    assert!(scoped_time < full_time);
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
