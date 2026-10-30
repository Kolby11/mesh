use super::*;
use mesh_core_wayland::{Edge, KeyboardMode, Layer, ShellSurface};
use std::collections::HashSet;

#[derive(Default)]
struct CountingSurface {
    layout_calls: u64,
    visibility_calls: u64,
}

impl CountingSurface {
    fn reset(&mut self) {
        self.layout_calls = 0;
        self.visibility_calls = 0;
    }
}

impl ShellSurface for CountingSurface {
    fn anchor(&mut self, _edge: Edge) {
        self.layout_calls += 1;
    }

    fn set_size(&mut self, _width: u32, _height: u32) {
        self.layout_calls += 1;
    }

    fn set_exclusive_zone(&mut self, _zone: i32) {
        self.layout_calls += 1;
    }

    fn set_layer(&mut self, _layer: Layer) {
        self.layout_calls += 1;
    }

    fn set_keyboard_interactivity(&mut self, _mode: KeyboardMode) {
        self.layout_calls += 1;
    }

    fn set_margin(&mut self, _top: i32, _right: i32, _bottom: i32, _left: i32) {
        self.layout_calls += 1;
    }

    fn set_blur(&mut self, _blur: bool) {
        self.layout_calls += 1;
    }

    fn set_center(&mut self, _center: bool) {}

    fn set_role(&mut self, _role: mesh_core_wayland::SurfaceRole) {
        self.layout_calls += 1;
    }

    fn set_window_options(&mut self, _options: mesh_core_wayland::WindowOptions) {
        self.layout_calls += 1;
    }

    fn show(&mut self) {
        self.visibility_calls += 1;
    }

    fn hide(&mut self) {
        self.visibility_calls += 1;
    }
}

#[test]
fn service_update_marks_component_dirty_only_when_tracked_fields_change() {
    let previous = serde_json::json!({
        "percent": 65,
        "muted": false,
        "source_module": "@mesh/pipewire-audio"
    });
    let unchanged_tracked = serde_json::json!({
        "percent": 65,
        "muted": false,
        "source_module": "@mesh/alternate-audio"
    });
    let changed_tracked = serde_json::json!({
        "percent": 66,
        "muted": false,
        "source_module": "@mesh/alternate-audio"
    });
    let tracked_fields = HashSet::from(["percent".to_string(), "muted".to_string()]);

    assert!(!tracked_service_fields_changed(
        Some(&previous),
        &unchanged_tracked,
        &tracked_fields
    ));
    assert!(tracked_service_fields_changed(
        Some(&previous),
        &changed_tracked,
        &tracked_fields
    ));
}

#[test]
fn handler_write_to_unbound_public_member_skips_component_rebuild() {
    let mut component = test_frontend_component(
        r#"
<script lang="luau">
label = "ready"
telemetry = 0
function recordTelemetry() telemetry = telemetry + 1 end
function changeLabel() label = "done" end
</script>
<template>
  <button onclick={recordTelemetry}>{label}</button>
</template>
"#,
    );
    let theme = default_theme();
    let mut buffer = PixelBuffer::new(200, 60);
    component
        .paint_raster(&theme, SurfaceExtent::unpadded(200, 60), &mut buffer, 1.0)
        .unwrap();
    let dirty_types_before = component.dirty_types;

    component
        .call_namespaced_handler("recordTelemetry", &[])
        .unwrap();
    assert_eq!(
        component
            .runtimes
            .lock()
            .unwrap()
            .get(component.id())
            .unwrap()
            .script_ctx
            .state()
            .get("telemetry"),
        Some(serde_json::json!(1))
    );
    assert!(
        !component.dirty,
        "unbound write must not schedule render work"
    );
    assert_eq!(component.dirty_types, dirty_types_before);

    component
        .call_namespaced_handler("changeLabel", &[])
        .unwrap();
    assert!(
        component.wants_render(),
        "bound write must schedule a paint"
    );
    assert!(
        !component
            .dirty_types
            .contains(ComponentDirtyFlags::SCRIPT_NARROW)
    );
    assert!(!component.dirty_types.contains(ComponentDirtyFlags::SCRIPT));
}

#[test]
fn nil_write_removes_public_member_and_rebuilds_conditional_tree() {
    let mut component = test_frontend_component(
        r#"
<script lang="luau">
visible = true
function hide() visible = nil end
</script>
<template>
  <column>
    {#if visible}<text>visible</text>{/if}
    <text>always</text>
  </column>
</template>
"#,
    );
    let theme = default_theme();
    let mut buffer = PixelBuffer::new(240, 80);
    component
        .paint_raster(&theme, SurfaceExtent::unpadded(240, 80), &mut buffer, 1.0)
        .unwrap();
    while component.wants_render() {
        component
            .paint_raster(&theme, SurfaceExtent::unpadded(240, 80), &mut buffer, 1.0)
            .unwrap();
    }
    let visible_pixels = buffer.data().to_vec();

    component.call_namespaced_handler("hide", &[]).unwrap();
    let runtime = component
        .runtimes
        .lock()
        .unwrap()
        .get(component.id())
        .expect("root runtime")
        .script_ctx
        .state()
        .get("visible");
    assert_eq!(runtime, None);

    component
        .paint_raster(&theme, SurfaceExtent::unpadded(240, 80), &mut buffer, 1.0)
        .unwrap();
    while component.wants_render() {
        component
            .paint_raster(&theme, SurfaceExtent::unpadded(240, 80), &mut buffer, 1.0)
            .unwrap();
    }

    assert_ne!(buffer.data(), visible_pixels.as_slice());
}

#[test]
fn template_function_call_patches_with_sparse_pixel_damage() {
    let mut component = test_frontend_component(
        r#"
<script lang="luau">
label = "before"
function shown(value) return value end
function update() label = "after" end
</script>
<template>
  <column>
    <text>{shown(label)}</text>
    <text>unchanged row one</text>
    <text>unchanged row two</text>
    <text>unchanged row three</text>
  </column>
</template>
"#,
    );
    let theme = default_theme();
    let mut buffer = PixelBuffer::new(640, 160);
    component
        .paint_raster(&theme, SurfaceExtent::unpadded(640, 160), &mut buffer, 1.0)
        .unwrap();
    while component.wants_render() {
        component
            .paint_raster(&theme, SurfaceExtent::unpadded(640, 160), &mut buffer, 1.0)
            .unwrap();
    }

    component.call_namespaced_handler("update", &[]).unwrap();
    component
        .paint_raster(&theme, SurfaceExtent::unpadded(640, 160), &mut buffer, 1.0)
        .unwrap();
    let snapshot = component
        .take_invalidation_snapshot()
        .expect("handler paint records invalidation");

    assert_eq!(snapshot.component.script_narrow, 0);
    assert_eq!(snapshot.patched_node_count, 1);
    assert!(!snapshot.full_rebuild);
    assert!(!snapshot.paint.full_surface_damage);
    assert!(snapshot.paint.damage_area < snapshot.paint.surface_area);
    assert!(snapshot.paint.skipped_paint_pixels > 0);
    let mut rebuilt = PixelBuffer::new(640, 160);
    component.invalidate_script_state();
    component
        .paint_raster(&theme, SurfaceExtent::unpadded(640, 160), &mut rebuilt, 1.0)
        .unwrap();
    assert_eq!(buffer.data(), rebuilt.data());
}

#[test]
fn keyed_loop_bindings_capture_locals_and_reconcile_collection_writes() {
    let source = r#"
<script lang="luau">
items = {{id = "a", label = "one"}, {id = "b", label = "two"}}
suffix = "before"
local function shown(item) return item.label .. suffix end
function update() suffix = suffix == "before" and "after" or "before" end
function reorder() items = {{id = "b", label = "second"}, {id = "a", label = "first"}} end
</script>
<template><column style="width: 640px; height: 160px">
{#for item in items key={item.id}}<text>{shown(item)}</text>{/for}
</column></template>
"#;
    let theme = default_theme();
    let extent = SurfaceExtent::unpadded(640, 160);
    let mut patched = test_frontend_component(source);
    let mut rebuilt = test_frontend_component(source);
    rebuilt.force_full_template_build = true;
    let mut actual = PixelBuffer::new(640, 160);
    let mut expected = PixelBuffer::new(640, 160);
    patched
        .paint_raster(&theme, extent, &mut actual, 1.0)
        .unwrap();
    rebuilt
        .paint_raster(&theme, extent, &mut expected, 1.0)
        .unwrap();
    for handler in ["update", "reorder", "update"] {
        patched.call_namespaced_handler(handler, &[]).unwrap();
        rebuilt.call_namespaced_handler(handler, &[]).unwrap();
        patched
            .paint_raster(&theme, extent, &mut actual, 1.0)
            .unwrap();
        rebuilt.invalidate_script_state();
        rebuilt
            .paint_raster(&theme, extent, &mut expected, 1.0)
            .unwrap();
        assert_eq!(actual.data(), expected.data(), "{handler}");
        let snapshot = patched.take_invalidation_snapshot().unwrap();
        if handler == "reorder" {
            assert_eq!(snapshot.patched_node_count, 0);
            assert_eq!(snapshot.component.script_narrow, 1);
        } else {
            assert_eq!(snapshot.patched_node_count, 2, "{handler}: {snapshot:?}");
            assert_eq!(snapshot.component.script_narrow, 0);
        }
    }
}

#[test]
fn shared_binding_and_structural_expression_cannot_patch_away_structure() {
    let mut component = test_frontend_component(
        r#"
<script lang="luau">
visible = true
function hide() visible = false end
</script>
<template><column style="width: 320px; height: 100px">{#if visible}<text>{visible}</text>{/if}<text>always</text></column></template>
"#,
    );
    let theme = default_theme();
    let extent = SurfaceExtent::unpadded(320, 100);
    let mut actual = PixelBuffer::new(320, 100);
    component
        .paint_raster(&theme, extent, &mut actual, 1.0)
        .unwrap();
    component.call_namespaced_handler("hide", &[]).unwrap();
    component
        .paint_raster(&theme, extent, &mut actual, 1.0)
        .unwrap();
    let snapshot = component.take_invalidation_snapshot().unwrap();
    assert_eq!(snapshot.patched_node_count, 0);
    assert!(snapshot.retained.removed > 0);
    let mut expected = PixelBuffer::new(320, 100);
    component.invalidate_script_state();
    component
        .paint_raster(&theme, extent, &mut expected, 1.0)
        .unwrap();
    assert_eq!(actual.data(), expected.data());
}

#[test]
fn structural_handler_write_on_narrow_path_matches_full_rebuild_pixels() {
    let source = r#"
<script lang="luau">
expanded = false
function toggle() expanded = not expanded end
</script>
<template>
  <column>
    <text>always</text>
    {#if expanded}<text>conditional</text>{/if}
  </column>
</template>
"#;
    let theme = default_theme();
    let mut narrow = test_frontend_component(source);
    let mut full = test_frontend_component(source);
    let mut narrow_buffer = PixelBuffer::new(320, 100);
    let mut full_buffer = PixelBuffer::new(320, 100);
    narrow
        .paint_raster(
            &theme,
            SurfaceExtent::unpadded(320, 100),
            &mut narrow_buffer,
            1.0,
        )
        .unwrap();
    full.paint_raster(
        &theme,
        SurfaceExtent::unpadded(320, 100),
        &mut full_buffer,
        1.0,
    )
    .unwrap();

    narrow.call_namespaced_handler("toggle", &[]).unwrap();
    full.call_namespaced_handler("toggle", &[]).unwrap();
    full.invalidate_script_state();
    narrow
        .paint_raster(
            &theme,
            SurfaceExtent::unpadded(320, 100),
            &mut narrow_buffer,
            1.0,
        )
        .unwrap();
    full.paint_raster(
        &theme,
        SurfaceExtent::unpadded(320, 100),
        &mut full_buffer,
        1.0,
    )
    .unwrap();

    assert_eq!(narrow_buffer.data(), full_buffer.data());
    let snapshot = narrow
        .take_invalidation_snapshot()
        .expect("structural handler paint records invalidation");
    assert_eq!(snapshot.component.script_narrow, 1);
    assert!(snapshot.retained.inserted > 0 || snapshot.retained.removed > 0);
}

#[test]
fn typed_invalidations_distinguish_restyle_from_script_rebuild() {
    let mut component = test_frontend_component("<template><button /></template>");

    component.dirty = false;
    component.style_only_dirty = false;
    component.dirty_types = ComponentDirtyFlags::empty();

    component.invalidate_interaction_restyle();
    assert!(component.wants_render());

    let (requires_tree_rebuild, can_use_retained_path, flags, _) = component.take_dirty_for_paint();
    assert!(!requires_tree_rebuild);
    assert!(can_use_retained_path);
    assert!(flags.contains(ComponentDirtyFlags::STYLE));
    assert!(flags.contains(ComponentDirtyFlags::LAYOUT));
    assert!(flags.contains(ComponentDirtyFlags::PAINT));
    assert!(flags.contains(ComponentDirtyFlags::ACCESSIBILITY));
    assert!(flags.contains(ComponentDirtyFlags::METRICS));

    component.invalidate_script_state();
    let (requires_tree_rebuild, can_use_retained_path, flags, _) = component.take_dirty_for_paint();
    assert!(requires_tree_rebuild);
    assert!(!can_use_retained_path);
    assert!(flags.contains(ComponentDirtyFlags::SCRIPT));
    assert!(flags.contains(ComponentDirtyFlags::STATE));
}

#[test]
fn targeted_interaction_restyle_uses_scoped_retained_fingerprinting() {
    let mut component = test_frontend_component(
        r#"
<template><row><box class="target" /><box /></row></template>
<style>
.target { width: 20px; height: 20px; background: #202020; }
.target:hover { background: #f0f0f0; }
</style>
"#,
    );
    let theme = default_theme();
    let mut buffer = PixelBuffer::new(120, 40);
    component
        .paint_raster(&theme, SurfaceExtent::unpadded(120, 40), &mut buffer, 1.0)
        .unwrap();

    component.hovered_path = ["root", "root/0", "root/0/0"]
        .map(runtime_node_id_for_key)
        .to_vec();
    component.hovered_key = Some("root/0/0".into());
    component.invalidate_interaction_restyle();
    component
        .paint_raster(&theme, SurfaceExtent::unpadded(120, 40), &mut buffer, 1.0)
        .unwrap();

    assert!(component.retained_tree.last_update_was_scoped());
}

#[test]
fn finalized_dirty_frames_match_full_capture_without_state_selectors() {
    let mut component = test_frontend_component(
        r#"<template><column><button>First</button><checkbox /><text selectable="true">Select me</text><box /></column></template>"#,
    );
    let theme = default_theme();
    let mut buffer = PixelBuffer::new(180, 120);
    for step in 0..7 {
        let previous = component.last_frame_snapshot.clone();
        match step {
            1 => {
                component.hovered_path = ["root", "root/0", "root/0/0"]
                    .map(runtime_node_id_for_key)
                    .to_vec();
            }
            2 => {
                component.focused_id = Some(runtime_node_id_for_key("root/0/0"));
            }
            3 => {
                component
                    .checked_values
                    .insert(runtime_node_id_for_key("root/0/1"), true);
            }
            4 => {
                component.selection = Some(TextSelectionState {
                    anchor: TextSelectionPoint {
                        node_key: "root/0/2".into(),
                        x: 1.0,
                        y: 1.0,
                    },
                    focus: TextSelectionPoint {
                        node_key: "root/0/2".into(),
                        x: 30.0,
                        y: 1.0,
                    },
                    dragging: false,
                });
            }
            5 => {
                component.selection = None;
            }
            6 => {
                component.hovered_path.clear();
                component.focused_id = None;
            }
            _ => {}
        }
        if step > 0 {
            component.invalidate_interaction_restyle();
        }
        component
            .paint_raster(&theme, SurfaceExtent::unpadded(180, 120), &mut buffer, 1.0)
            .unwrap();
        let tree = component.last_tree.as_ref().unwrap();
        let actual = component.last_frame_snapshot.as_ref().unwrap();
        let expected = FrameSnapshot::complete(tree, actual.revision(), previous.as_ref()).unwrap();
        for (actual, expected) in actual.nodes().iter().zip(expected.nodes()) {
            assert_eq!(
                format!("{actual:?}"),
                format!("{expected:?}"),
                "step {step}"
            );
        }
        assert_eq!(
            actual.semantic_diff().changes(),
            expected.semantic_diff().changes(),
            "step {step}"
        );
        if step == 5 {
            let selected = find_node_by_key(tree, "root/0/2").unwrap();
            assert!(selected.text_selection.is_none());
        }
    }
}

#[test]
fn targeted_interaction_animation_merges_scoped_retained_roots() {
    let mut component = test_frontend_component(
        r#"
<template><row><box class="target" /><box /></row></template>
<style>
.target {
  width: 20px;
  height: 20px;
  background: #202020;
  transition: background-color 1000ms linear;
}
.target:hover { background: #f0f0f0; }
</style>
"#,
    );
    let theme = default_theme();
    let mut buffer = PixelBuffer::new(120, 40);
    component
        .paint_raster(&theme, SurfaceExtent::unpadded(120, 40), &mut buffer, 1.0)
        .unwrap();

    component.hovered_path = ["root", "root/0", "root/0/0"]
        .map(runtime_node_id_for_key)
        .to_vec();
    component.hovered_key = Some("root/0/0".into());
    component.invalidate_interaction_restyle();
    component
        .paint_raster(&theme, SurfaceExtent::unpadded(120, 40), &mut buffer, 1.0)
        .unwrap();

    let actual = component.last_frame_snapshot.as_ref().unwrap();
    let expected = FrameSnapshot::complete(
        component.last_tree.as_ref().unwrap(),
        actual.revision(),
        None,
    )
    .unwrap();
    for (actual, expected) in actual.nodes().iter().zip(expected.nodes()) {
        assert_eq!(
            actual.style(),
            expected.style(),
            "snapshot must contain sampled animation styles"
        );
        assert_eq!(
            format!("{:?}", actual.layout()),
            format!("{:?}", expected.layout())
        );
    }
    assert!(component.transitions.has_active(Instant::now()));
    assert!(component.retained_tree.last_update_was_scoped());
}

#[test]
fn paint_only_scroll_uses_scoped_retained_fingerprinting() {
    let source = r#"
<style>
scroll { width: 80px; height: 40px; overflow-y: auto; }
.content { height: 160px; background: #303030; }
</style>
<template><scroll><box class="content" /></scroll></template>
"#;
    let mut scoped = test_frontend_component(source);
    let mut full = test_frontend_component(source);
    full.force_full_retained_update = true;
    let theme = default_theme();
    let mut scoped_buffer = PixelBuffer::new(80, 40);
    let mut full_buffer = PixelBuffer::new(80, 40);
    scoped
        .paint_raster(
            &theme,
            SurfaceExtent::unpadded(80, 40),
            &mut scoped_buffer,
            1.0,
        )
        .unwrap();
    full.paint_raster(
        &theme,
        SurfaceExtent::unpadded(80, 40),
        &mut full_buffer,
        1.0,
    )
    .unwrap();

    scoped.scroll_offsets.insert(
        runtime_node_id_for_key("root/0"),
        ScrollOffsetState { x: 0.0, y: 24.0 },
    );
    full.scroll_offsets.insert(
        runtime_node_id_for_key("root/0"),
        ScrollOffsetState { x: 0.0, y: 24.0 },
    );
    scoped.invalidate(ComponentDirtyFlags::PAINT | ComponentDirtyFlags::METRICS);
    full.invalidate(ComponentDirtyFlags::PAINT | ComponentDirtyFlags::METRICS);
    scoped
        .paint_raster(
            &theme,
            SurfaceExtent::unpadded(80, 40),
            &mut scoped_buffer,
            1.0,
        )
        .unwrap();
    full.paint_raster(
        &theme,
        SurfaceExtent::unpadded(80, 40),
        &mut full_buffer,
        1.0,
    )
    .unwrap();

    assert!(scoped.retained_tree.last_update_was_scoped());
    assert_eq!(
        scoped.retained_tree.last_dirty(),
        full.retained_tree.last_dirty()
    );
    assert_eq!(scoped_buffer.data(), full_buffer.data());
}

#[test]
fn surface_transition_annotation_synchronizes_its_reported_subtree() {
    let mut component = test_frontend_component(
        r#"
<template><box class="target" /></template>
<style>.target:hover { opacity: 0.5; }</style>
"#,
    );
    let theme = default_theme();
    let mut buffer = PixelBuffer::new(120, 40);
    component
        .paint_raster(&theme, SurfaceExtent::unpadded(120, 40), &mut buffer, 1.0)
        .unwrap();

    let expected_nodes = component
        .last_frame_snapshot
        .as_ref()
        .unwrap()
        .nodes()
        .len();
    component.set_profiling_enabled(true);
    component.surface_entering = true;
    component.hovered_path = ["root", "root/0"].map(runtime_node_id_for_key).to_vec();
    component.hovered_key = Some("root/0".into());
    component.invalidate_interaction_restyle();
    component
        .paint_raster(&theme, SurfaceExtent::unpadded(120, 40), &mut buffer, 1.0)
        .unwrap();

    assert_eq!(
        component.retained_tree.last_selected_node_count,
        expected_nodes
    );
    let counts = component.take_invalidation_snapshot().unwrap().node_visits;
    assert_eq!(counts["retained_sync"], expected_nodes as u64);
    assert_eq!(counts["retained_discovery"], 0);
}

#[test]
fn render_skips_surface_bookkeeping_for_paint_only_dirty_frames() {
    let mut component = test_frontend_component("<template><box /></template>");
    let mut surface = CountingSurface::default();

    component.render(&mut surface).unwrap();
    assert!(surface.layout_calls > 0);
    assert!(surface.visibility_calls > 0);

    surface.reset();
    component.dirty = false;
    component.style_only_dirty = false;
    component.dirty_types = ComponentDirtyFlags::empty();
    component.invalidate_paint();
    component.render(&mut surface).unwrap();
    assert_eq!(surface.layout_calls, 0);
    assert_eq!(surface.visibility_calls, 0);

    component.invalidate_surface_config();
    component.render(&mut surface).unwrap();
    assert!(surface.layout_calls > 0);
    assert!(surface.visibility_calls > 0);
}

#[test]
fn plain_hover_change_without_state_rules_does_not_dirty_component() {
    let mut component = test_frontend_component(
        r#"
<template><row><box class="plain" /><box class="plain" /></row></template>
<style>
surface { width: 200px; height: 80px; }
.plain { width: 80px; height: 40px; background: #222222; }
</style>
"#,
    );
    let theme = default_theme();
    let mut buffer = PixelBuffer::new(200, 80);
    component
        .paint_raster(&theme, SurfaceExtent::unpadded(200, 80), &mut buffer, 1.0)
        .unwrap();
    if component.wants_render() {
        component
            .paint_raster(&theme, SurfaceExtent::unpadded(200, 80), &mut buffer, 1.0)
            .unwrap();
    }
    assert!(!component.wants_render());

    component
        .handle_input(
            &theme,
            200,
            80,
            ComponentInput::PointerMove { x: 10.0, y: 10.0 },
        )
        .unwrap();

    assert!(component.hovered_key.is_some());
    assert!(
        !component.wants_render(),
        "hovering plain nodes without state selectors should not schedule restyle/layout/paint"
    );
}

#[test]
fn hover_change_with_state_rules_still_invalidates_interaction_restyle() {
    let mut component = test_frontend_component(
        r#"
<template><button class="target">Hover</button></template>
<style>
surface { width: 120px; height: 48px; }
button { width: 80px; height: 32px; background: #222222; }
button:hover { background: #444444; }
</style>
"#,
    );
    let theme = default_theme();
    let mut buffer = PixelBuffer::new(120, 48);
    component
        .paint_raster(&theme, SurfaceExtent::unpadded(120, 48), &mut buffer, 1.0)
        .unwrap();
    if component.wants_render() {
        component
            .paint_raster(&theme, SurfaceExtent::unpadded(120, 48), &mut buffer, 1.0)
            .unwrap();
    }
    assert!(!component.wants_render());

    component
        .handle_input(
            &theme,
            120,
            48,
            ComponentInput::PointerMove { x: 10.0, y: 10.0 },
        )
        .unwrap();

    let (requires_tree_rebuild, can_use_retained_path, flags, _) = component.take_dirty_for_paint();
    assert!(!requires_tree_rebuild);
    assert!(can_use_retained_path);
    assert!(flags.contains(ComponentDirtyFlags::STYLE));
    assert!(!flags.contains(ComponentDirtyFlags::LAYOUT));
    assert!(flags.contains(ComponentDirtyFlags::PAINT));
}

#[test]
fn hover_change_with_focus_only_rules_skips_interaction_restyle() {
    let mut component = test_frontend_component(
        r#"
<template><box class="target" /></template>
<style>
.target { width: 80px; height: 32px; background: #222222; }
.target:focus { background: #444444; }
</style>
"#,
    );

    assert!(component.module_styles_have_state_rules());
    assert!(!component.module_styles_have_hover_rules());
    component.dirty = false;
    component.style_only_dirty = false;
    component.dirty_types = ComponentDirtyFlags::empty();

    component.invalidate_hover_change(false);

    assert!(
        !component.wants_render(),
        "focus-only selectors cannot change on a hover transition"
    );
}

#[test]
fn selector_dependency_cache_covers_every_supported_pseudo_state() {
    let mut component = test_frontend_component(
        r#"
<template><box /></template>
<style>
box:hover { opacity: 0.9; }
box:focus { opacity: 0.8; }
box:focus-visible { opacity: 0.7; }
box:active { opacity: 0.6; }
box:disabled { opacity: 0.5; }
box:checked { opacity: 0.4; }
box:readonly { opacity: 0.3; }
box:required { opacity: 0.29; }
box:selected { opacity: 0.28; }
box:expanded { opacity: 0.27; }
box:pressed { opacity: 0.26; }
box:invalid { opacity: 0.25; }
box:value { opacity: 0.24; }
box:windowed { opacity: 0.23; }
box:fullscreen { opacity: 0.22; }
box:maximized { opacity: 0.21; }
box:activated { opacity: 0.20; }
box:tiled { opacity: 0.19; }
</style>
"#,
    );
    assert!(component.module_styles_have_state_rules());

    assert_eq!(
        component.cached_restyle_state_dependencies,
        StyleStateDependencies {
            mask: mesh_core_elements::pseudo_state_specs()
                .iter()
                .fold(0, |mask, spec| mask | spec.bit),
        }
    );
}

#[test]
fn selector_dependencies_filter_unrelated_interaction_changes() {
    let mut component = test_frontend_component(
        r#"
<template><box /></template>
<style>box:hover { opacity: 0.5; }</style>
"#,
    );
    assert!(component.module_styles_have_state_rules());
    component.interaction_snapshot_valid = true;
    component.previous_active_key = Some(runtime_node_id_for_key("root/pressed"));
    component.pointer_down_id = None;
    component
        .previous_checked_values
        .insert(runtime_node_id_for_key("root/check"), false);
    component
        .checked_values
        .insert(runtime_node_id_for_key("root/check"), true);

    assert!(
        component
            .collect_interaction_changed_node_ids(None)
            .affected
            .is_empty(),
        "active and checked changes cannot affect hover-only rules"
    );

    // The platform focus ring is a `:focus-visible` rule beneath every
    // theme, so a focus change restyles even where modules only use hover.
    component.previous_focused_key = Some(runtime_node_id_for_key("root/old"));
    component.focused_key = Some("root/new".into());
    assert!(
        !component
            .collect_interaction_changed_node_ids(None)
            .affected
            .is_empty()
    );
}

#[test]
fn hover_gate_sees_state_rules_from_imported_component_modules() {
    use crate::shell::component::catalog::FrontendCatalogEntry;
    use mesh_core_component::parse_component;
    use mesh_core_frontend::CompiledFrontendModule;
    use std::collections::HashMap;
    use std::path::PathBuf;

    // Host surface has no state selectors of its own...
    let mut component = test_frontend_component(
        r#"
<template><row><box class="plain" /></row></template>
<style>
surface { width: 200px; height: 80px; }
.plain { width: 80px; height: 40px; background: #222222; }
</style>
"#,
    );
    // ...but imports a component module whose styles do use `:hover`. The
    // restyle rule set aggregates imported-module rules, so the hover gate
    // must see them too.
    let imported_id = "@test/imported-popover".to_string();
    component
        .compiled
        .module_component_imports
        .insert("ImportedPopover".into(), imported_id.clone());
    Arc::make_mut(&mut component.frontend_catalog)
        .modules
        .insert(
            imported_id.clone(),
            FrontendCatalogEntry {
                module_dir: PathBuf::from("."),
                compiled: CompiledFrontendModule {
                    manifest: minimal_test_manifest(&imported_id),
                    source_path: PathBuf::from("src/main.mesh"),
                    component: parse_component(
                        r#"
<template><button class="opt">Pick</button></template>
<style>
.opt { background: #222222; }
.opt:hover { background: #444444; }
</style>
"#,
                    )
                    .unwrap(),
                    public_props: Default::default(),
                    local_components: HashMap::new(),
                    module_component_imports: HashMap::new(),
                    watched_paths: Vec::new(),
                }
                .into(),
            },
        );
    // The rule cache may have been built before the import was wired (as it
    // is on hot source reload); reset it the same way reloads do.
    component.cached_restyle_rules = None;

    assert!(component.module_styles_have_state_rules());

    component.dirty = false;
    component.style_only_dirty = false;
    component.dirty_types = ComponentDirtyFlags::empty();
    component.invalidate_hover_change(false);
    assert!(
        component.wants_render(),
        "hover changes must schedule an interaction restyle when an imported component module declares state selectors"
    );
}

#[test]
fn hover_gate_sees_state_rules_from_extension_point_contributions() {
    use crate::shell::component::catalog::{
        ResolvedExtensionPointContribution, extension_point_key,
    };
    use mesh_core_component::parse_component;
    use mesh_core_frontend::CompiledFrontendModule;
    use std::collections::HashMap;
    use std::path::PathBuf;

    let mut component = test_frontend_component(
        r#"
<template><row><box class="plain" /></row></template>
<style>
surface { width: 200px; height: 80px; }
.plain { width: 80px; height: 40px; background: #222222; }
</style>
"#,
    );
    let contribution = CompiledFrontendModule {
        manifest: minimal_test_manifest("@test/navigation-item"),
        source_path: PathBuf::from("src/contribution.mesh"),
        component: parse_component(
            r#"
<template><button class="contribution">Pick</button></template>
<style>
.contribution { width: 40px; height: 40px; }
.contribution:hover { transform: scale(1.04); }
</style>
"#,
        )
        .unwrap(),
        public_props: Default::default(),
        local_components: HashMap::new(),
        module_component_imports: HashMap::new(),
        watched_paths: Vec::new(),
    };
    let host_id = component.id().to_owned();
    Arc::make_mut(&mut component.frontend_catalog)
        .extension_point_entries
        .insert(
            format!("{host_id}\u{1}navigation-item"),
            contribution.into(),
        );
    Arc::make_mut(&mut component.frontend_catalog)
        .extension_point_contributions
        .insert(
            extension_point_key(&host_id, "mesh.navigation.item"),
            vec![ResolvedExtensionPointContribution {
                source_module_id: host_id,
                contribution_id: "navigation-item".into(),
                order: 0,
                props_fingerprint: 0,
                props: serde_json::Map::new(),
            }],
        );
    component.cached_restyle_rules = None;

    assert!(component.module_styles_have_state_rules());
    assert!(component.module_styles_have_hover_rules());
}

#[test]
fn typed_invalidations_cover_text_metrics_and_surface_configuration() {
    let mut component = test_frontend_component("<template><input value=\"\" /></template>");

    component.dirty = false;
    component.style_only_dirty = false;
    component.dirty_types = ComponentDirtyFlags::empty();

    component.invalidate_text_state();
    let (requires_tree_rebuild, _, flags, _) = component.take_dirty_for_paint();
    assert!(requires_tree_rebuild);
    assert!(flags.contains(ComponentDirtyFlags::TEXT));
    assert!(flags.contains(ComponentDirtyFlags::METRICS));
    assert!(flags.contains(ComponentDirtyFlags::ACCESSIBILITY));

    component.invalidate_surface_config();
    let (requires_tree_rebuild, can_use_retained_path, flags, _) = component.take_dirty_for_paint();
    assert!(!requires_tree_rebuild);
    assert!(can_use_retained_path);
    assert!(flags.contains(ComponentDirtyFlags::SURFACE_CONFIG));
    assert!(!flags.contains(ComponentDirtyFlags::LAYOUT));
    assert!(!flags.contains(ComponentDirtyFlags::PAINT));
    assert!(!flags.contains(ComponentDirtyFlags::METRICS));
}

#[test]
fn surface_config_only_invalidations_do_not_request_immediate_rerender() {
    let mut component = test_frontend_component("<template><button /></template>");

    component.dirty = false;
    component.style_only_dirty = false;
    component.dirty_types = ComponentDirtyFlags::empty();

    component.invalidate_surface_config();

    assert!(component.wants_render());
    assert!(!component.wants_immediate_rerender());
}

#[test]
fn phase18_script_and_text_invalidations_take_full_rebuild_paint_path() {
    let mut component = test_frontend_component("<template><button>Push</button></template>");
    let theme = default_theme();
    let mut buffer = PixelBuffer::new(160, 48);

    component.set_profiling_enabled(true);
    component
        .paint_raster(&theme, SurfaceExtent::unpadded(160, 48), &mut buffer, 1.0)
        .unwrap();
    component.take_invalidation_snapshot();
    component.take_profiling_records();

    component.invalidate_interaction_restyle();
    component
        .paint_raster(&theme, SurfaceExtent::unpadded(160, 48), &mut buffer, 1.0)
        .unwrap();
    let restyle_snapshot = component
        .take_invalidation_snapshot()
        .expect("restyle paint should record invalidation");
    assert!(!restyle_snapshot.full_rebuild);
    assert!(restyle_snapshot.retained_path);
    assert!(
        !component
            .take_profiling_records()
            .iter()
            .any(|record| record.stage == mesh_core_debug::ProfilingStage::TreeBuild),
        "retained restyle must not rebuild the widget tree"
    );

    component.invalidate_script_state();
    component
        .paint_raster(&theme, SurfaceExtent::unpadded(160, 48), &mut buffer, 1.0)
        .unwrap();
    let script_snapshot = component
        .take_invalidation_snapshot()
        .expect("script paint should record invalidation");
    assert!(script_snapshot.full_rebuild);
    assert!(!script_snapshot.retained_path);
    assert_eq!(script_snapshot.component.script, 1);
    assert!(
        component
            .take_profiling_records()
            .iter()
            .any(|record| record.stage == mesh_core_debug::ProfilingStage::TreeBuild),
        "SCRIPT invalidation must call build_tree_with_state through the full rebuild path"
    );

    component.invalidate_text_state();
    component
        .paint_raster(&theme, SurfaceExtent::unpadded(160, 48), &mut buffer, 1.0)
        .unwrap();
    let text_snapshot = component
        .take_invalidation_snapshot()
        .expect("text paint should record invalidation");
    assert!(text_snapshot.full_rebuild);
    assert!(!text_snapshot.retained_path);
    assert_eq!(text_snapshot.component.text, 1);
    assert!(
        component
            .take_profiling_records()
            .iter()
            .any(|record| record.stage == mesh_core_debug::ProfilingStage::TreeBuild),
        "TEXT invalidation must call build_tree_with_state through the full rebuild path"
    );
}

#[test]
fn source_reload_drops_stale_retained_tree_before_next_paint() {
    use crate::shell::component::catalog::FrontendCatalog;
    use mesh_core_frontend::compile_frontend_module;

    fn write_source(module_dir: &std::path::Path, body: &str) {
        std::fs::create_dir_all(module_dir.join("src")).unwrap();
        std::fs::write(
            module_dir.join("src/main.mesh"),
            format!("<template><text>{body}</text></template>"),
        )
        .unwrap();
    }

    let temp = tempfile::tempdir().unwrap();
    let module_dir = temp.path();
    let manifest = minimal_test_manifest("@test/hot-reload");
    write_source(module_dir, "before");

    let compiled = compile_frontend_module(&manifest, module_dir).unwrap();
    let catalog = FrontendCatalog {
        modules: Default::default(),
        diagnostics: Default::default(),
        extension_point_contributions: Default::default(),
        extension_point_entries: Default::default(),
        node_slot_placements: Default::default(),
        dependency_forks: Default::default(),
    };
    let mut component = FrontendSurfaceComponent::new_for_test(
        compiled,
        module_dir.to_path_buf(),
        catalog,
        mesh_core_service::InterfaceCatalog::default(),
        test_settings_store(),
    );
    component
        .mount(ComponentContext {
            component_id: "@test/hot-reload".into(),
            surface_id: "@test/hot-reload".into(),
            diagnostics: Diagnostics::new("@test/hot-reload"),
        })
        .unwrap();
    component.visible = true;

    let theme = default_theme();
    let mut buffer = PixelBuffer::new(240, 80);
    component
        .paint_raster(&theme, SurfaceExtent::unpadded(240, 80), &mut buffer, 1.0)
        .unwrap();
    assert_eq!(
        first_node_by_tag(component.last_tree.as_ref().unwrap(), "text")
            .unwrap()
            .attributes
            .get(Prop::Content)
            .map(String::as_str),
        Some("before")
    );

    write_source(module_dir, "after");
    assert!(component.reload_source().unwrap());
    assert!(
        component.last_tree.is_none(),
        "source reload must drop the stale retained widget tree"
    );

    component
        .paint_raster(&theme, SurfaceExtent::unpadded(240, 80), &mut buffer, 1.0)
        .unwrap();
    assert_eq!(
        first_node_by_tag(component.last_tree.as_ref().unwrap(), "text")
            .unwrap()
            .attributes
            .get(Prop::Content)
            .map(String::as_str),
        Some("after")
    );
}

#[test]
fn source_reload_swaps_fresh_script_environment_and_preserves_storage() {
    use crate::shell::component::catalog::FrontendCatalog;
    use mesh_core_frontend::compile_frontend_module;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn write_source(module_dir: &std::path::Path, body: &str) {
        std::fs::create_dir_all(module_dir.join("src")).unwrap();
        std::fs::write(module_dir.join("src/main.mesh"), body).unwrap();
    }

    let temp = tempfile::tempdir().unwrap();
    let module_dir = temp.path();
    let module_id = format!(
        "@test/fresh-reload-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let manifest = minimal_test_manifest(&module_id);
    write_source(
        module_dir,
        r#"<template><text content="before" /></template>
<script lang="luau">
legacy_global = "before"

function legacy_handler()
end

function init(self)
    self.storage.reload_count = (self.storage.reload_count or 0) + 1
    storage_count = self.storage.reload_count
end
</script>
"#,
    );

    let compiled = compile_frontend_module(&manifest, module_dir).unwrap();
    let catalog = FrontendCatalog {
        modules: Default::default(),
        diagnostics: Default::default(),
        extension_point_contributions: Default::default(),
        extension_point_entries: Default::default(),
        node_slot_placements: Default::default(),
        dependency_forks: Default::default(),
    };
    let mut component = FrontendSurfaceComponent::new_for_test(
        compiled,
        module_dir.to_path_buf(),
        catalog,
        mesh_core_service::InterfaceCatalog::default(),
        test_settings_store(),
    );
    component
        .mount(ComponentContext {
            component_id: module_id.clone(),
            surface_id: module_id.clone(),
            diagnostics: Diagnostics::new(module_id),
        })
        .unwrap();
    let initial_generation = component
        .runtimes
        .lock()
        .unwrap()
        .get(component.root_instance_key())
        .unwrap()
        .script_ctx
        .generation;

    write_source(
        module_dir,
        r#"<template><text content="after" /></template>
<script lang="luau">
fresh_global = "after"

function fresh_handler()
end

function init(self)
    storage_count = self.storage.reload_count
end
</script>
"#,
    );

    assert!(component.reload_source().unwrap());
    let mut runtimes = component.runtimes.lock().unwrap();
    let runtime = runtimes.get_mut(component.root_instance_key()).unwrap();
    assert_eq!(
        runtime.script_ctx.state().get("fresh_global"),
        Some(serde_json::json!("after"))
    );
    assert_ne!(runtime.script_ctx.generation, initial_generation);
    assert_eq!(runtime.script_ctx.state().get("legacy_global"), None);
    assert_eq!(
        runtime.script_ctx.state().get("storage_count"),
        Some(serde_json::json!(1))
    );
    assert!(!runtime.script_ctx.has_handler("legacy_handler"));
    assert!(runtime.script_ctx.has_handler("fresh_handler"));
}

#[test]
fn failed_source_reload_restores_the_last_good_runtime() {
    use crate::shell::component::catalog::FrontendCatalog;
    use mesh_core_frontend::compile_frontend_module;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn write_source(module_dir: &std::path::Path, body: &str) {
        std::fs::create_dir_all(module_dir.join("src")).unwrap();
        std::fs::write(module_dir.join("src/main.mesh"), body).unwrap();
    }

    let temp = tempfile::tempdir().unwrap();
    let module_dir = temp.path();
    let module_id = format!(
        "@test/rollback-reload-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let manifest = minimal_test_manifest(&module_id);
    write_source(
        module_dir,
        r#"<template><text content="stable" /></template>
<script lang="luau">
stable_global = "before"

function stable_handler()
    stable_global = "still-active"
end

function mount(self)
    self.storage.mounts = (self.storage.mounts or 0) + 1
    mount_count = self.storage.mounts
end

function unmount(self)
    self.storage.unmounts = (self.storage.unmounts or 0) + 1
end
</script>
"#,
    );

    let compiled = compile_frontend_module(&manifest, module_dir).unwrap();
    let catalog = FrontendCatalog {
        modules: Default::default(),
        diagnostics: Default::default(),
        extension_point_contributions: Default::default(),
        extension_point_entries: Default::default(),
        node_slot_placements: Default::default(),
        dependency_forks: Default::default(),
    };
    let mut component = FrontendSurfaceComponent::new_for_test(
        compiled,
        module_dir.to_path_buf(),
        catalog,
        mesh_core_service::InterfaceCatalog::default(),
        test_settings_store(),
    );
    component
        .mount(ComponentContext {
            component_id: module_id.clone(),
            surface_id: module_id.clone(),
            diagnostics: Diagnostics::new(module_id),
        })
        .unwrap();
    let initial_generation = component
        .runtimes
        .lock()
        .unwrap()
        .get(component.root_instance_key())
        .unwrap()
        .script_ctx
        .generation;
    assert_eq!(
        component
            .runtimes
            .lock()
            .unwrap()
            .get(component.root_instance_key())
            .unwrap()
            .script_ctx
            .state()
            .get("mount_count"),
        Some(serde_json::json!(1))
    );

    write_source(
        module_dir,
        r#"<template><text content="broken" /></template>
<script lang="luau">
function init()
    error("candidate initialization failed")
end
</script>
"#,
    );

    assert!(component.reload_source().is_err());
    let mut runtimes = component.runtimes.lock().unwrap();
    let runtime = runtimes.get_mut(component.root_instance_key()).unwrap();
    assert_eq!(
        runtime.script_ctx.state().get("stable_global"),
        Some(serde_json::json!("before"))
    );
    assert_eq!(runtime.script_ctx.generation, initial_generation);
    assert!(runtime.script_ctx.has_handler("stable_handler"));
    assert_eq!(
        runtime.script_ctx.state().get("mount_count"),
        Some(serde_json::json!(2))
    );
}

#[test]
fn frontend_lifecycle_hooks_run_around_source_reload() {
    use crate::shell::component::catalog::FrontendCatalog;
    use mesh_core_frontend::compile_frontend_module;
    use std::time::{SystemTime, UNIX_EPOCH};

    let temp = tempfile::tempdir().unwrap();
    let module_dir = temp.path();
    let module_id = format!(
        "@test/lifecycle-hooks-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let manifest = minimal_test_manifest(&module_id);
    let source = r#"<template><text content="lifecycle" /></template>
<script lang="luau">
function init(self)
    loaded_after_unmount = self.storage.unmounted == true
end

function mount(self)
    self.storage.mounts = (self.storage.mounts or 0) + 1
    mount_count = self.storage.mounts
end

function unmount(self)
    self.storage.unmounted = true
end
</script>
"#;
    std::fs::create_dir_all(module_dir.join("src")).unwrap();
    std::fs::write(module_dir.join("src/main.mesh"), source).unwrap();

    let compiled = compile_frontend_module(&manifest, module_dir).unwrap();
    let catalog = FrontendCatalog {
        modules: Default::default(),
        diagnostics: Default::default(),
        extension_point_contributions: Default::default(),
        extension_point_entries: Default::default(),
        node_slot_placements: Default::default(),
        dependency_forks: Default::default(),
    };
    let mut component = FrontendSurfaceComponent::new_for_test(
        compiled,
        module_dir.to_path_buf(),
        catalog,
        mesh_core_service::InterfaceCatalog::default(),
        test_settings_store(),
    );
    component
        .mount(ComponentContext {
            component_id: module_id.clone(),
            surface_id: module_id.clone(),
            diagnostics: Diagnostics::new(module_id.clone()),
        })
        .unwrap();

    let root = component.runtimes.lock().unwrap();
    let runtime = root.get(component.root_instance_key()).unwrap();
    assert_eq!(
        runtime.script_ctx.state().get("mount_count"),
        Some(serde_json::json!(1))
    );
    assert_eq!(
        runtime.script_ctx.state().get("loaded_after_unmount"),
        Some(serde_json::json!(false))
    );
    drop(root);

    assert!(component.reload_source().unwrap());
    let root = component.runtimes.lock().unwrap();
    let runtime = root.get(component.root_instance_key()).unwrap();
    assert_eq!(
        runtime.script_ctx.state().get("mount_count"),
        Some(serde_json::json!(2))
    );
    assert_eq!(
        runtime.script_ctx.state().get("loaded_after_unmount"),
        Some(serde_json::json!(true))
    );
}

#[test]
fn source_reload_recompiles_primary_and_contribution_roots_atomically() {
    use crate::shell::component::catalog::{
        FrontendCatalog, FrontendCatalogEntry, ResolvedExtensionPointContribution,
        contribution_entry_key, extension_point_key,
    };
    use mesh_core_component::template::{AttributeValue, TemplateNode};
    use mesh_core_frontend::{compile_frontend_entrypoint, compile_frontend_module};

    fn static_content(compiled: &CompiledFrontendModule) -> Option<String> {
        let root = compiled.component.template.as_ref()?.root.first()?;
        let TemplateNode::Element(root) = root else {
            return None;
        };
        let attribute = root
            .attributes
            .iter()
            .find(|attribute| attribute.name == "content")?;
        match &attribute.value {
            AttributeValue::Static(value) => Some(value.clone()),
            _ => None,
        }
    }

    let temp = tempfile::tempdir().unwrap();
    let module_dir = temp.path();
    std::fs::create_dir_all(module_dir.join("src")).unwrap();
    std::fs::write(
        module_dir.join("src/main.mesh"),
        "<template><slot extension-point=\"test.reload.slot\" /></template>",
    )
    .unwrap();
    let contribution_path = module_dir.join("src/contribution.mesh");
    std::fs::write(
        &contribution_path,
        "<template><text content=\"before\" /></template>",
    )
    .unwrap();

    let module_id = "@test/atomic-reload";
    let mut manifest = minimal_test_manifest(module_id);
    manifest.extension_point_contributions.insert(
        "test.reload.slot".into(),
        vec![mesh_core_module::manifest::ExtensionPointContribution {
            id: "contribution".into(),
            entry: "src/contribution.mesh".into(),
            order: Some(0),
            props: Default::default(),
        }],
    );
    let compiled = compile_frontend_module(&manifest, module_dir).unwrap();
    let contribution =
        compile_frontend_entrypoint(&manifest, module_dir, "src/contribution.mesh").unwrap();
    let catalog = FrontendCatalog {
        modules: HashMap::from([(
            module_id.into(),
            FrontendCatalogEntry {
                module_dir: module_dir.to_path_buf(),
                compiled: compiled.clone().into(),
            },
        )]),
        diagnostics: Default::default(),
        extension_point_contributions: HashMap::from([(
            extension_point_key(module_id, "test.reload.slot"),
            vec![ResolvedExtensionPointContribution {
                source_module_id: module_id.into(),
                contribution_id: "contribution".into(),
                order: 0,
                props_fingerprint: 0,
                props: Default::default(),
            }],
        )]),
        extension_point_entries: HashMap::from([(
            contribution_entry_key(module_id, "contribution"),
            contribution.clone().into(),
        )]),
        node_slot_placements: Default::default(),
        dependency_forks: Default::default(),
    };
    let mut component = FrontendSurfaceComponent::new_for_test(
        compiled,
        module_dir.to_path_buf(),
        catalog,
        mesh_core_service::InterfaceCatalog::default(),
        test_settings_store(),
    );
    component
        .mount(ComponentContext {
            component_id: module_id.into(),
            surface_id: module_id.into(),
            diagnostics: Diagnostics::new(module_id),
        })
        .unwrap();

    let before_version = component.frontend_catalog_handle.snapshot().version;
    assert_eq!(static_content(&contribution), Some("before".into()));

    std::fs::write(
        &contribution_path,
        "<template><text content=\"after\" /></template>",
    )
    .unwrap();
    assert!(component.reload_source().unwrap());

    let state = component.frontend_catalog_handle.snapshot();
    assert_eq!(state.version, before_version + 1);
    assert!(state.affected_modules.contains(module_id));
    assert!(
        state
            .catalog
            .module(module_id)
            .unwrap()
            .compiled
            .watched_paths
            .contains(&contribution_path)
    );
    let reloaded_contribution = state
        .catalog
        .contribution_entry(module_id, "contribution")
        .unwrap();
    assert_eq!(static_content(reloaded_contribution), Some("after".into()));

    std::fs::write(&contribution_path, "<template><text").unwrap();
    assert!(component.reload_source().is_err());
    let failed_state = component.frontend_catalog_handle.snapshot();
    assert_eq!(failed_state.version, state.version);
    assert_eq!(
        static_content(
            failed_state
                .catalog
                .contribution_entry(module_id, "contribution")
                .unwrap()
        ),
        Some("after".into())
    );
}

#[test]
fn source_reload_recompiles_contributions_consumed_from_other_modules() {
    use crate::shell::component::catalog::{
        FrontendCatalog, FrontendCatalogEntry, ResolvedExtensionPointContribution,
        contribution_entry_key, extension_point_key,
    };
    use mesh_core_component::template::{AttributeValue, TemplateNode};
    use mesh_core_frontend::{compile_frontend_entrypoint, compile_frontend_module};
    use std::path::PathBuf;

    fn static_content(compiled: &CompiledFrontendModule) -> Option<String> {
        let root = compiled.component.template.as_ref()?.root.first()?;
        let TemplateNode::Element(root) = root else {
            return None;
        };
        let attribute = root
            .attributes
            .iter()
            .find(|attribute| attribute.name == "content")?;
        match &attribute.value {
            AttributeValue::Static(value) => Some(value.clone()),
            _ => None,
        }
    }

    let temp = tempfile::tempdir().unwrap();
    let host_dir = temp.path().join("host");
    let contributor_dir = temp.path().join("contributor");
    std::fs::create_dir_all(host_dir.join("src")).unwrap();
    std::fs::create_dir_all(contributor_dir.join("src")).unwrap();
    std::fs::write(
        host_dir.join("src/main.mesh"),
        "<template><slot extension-point=\"test.cross-module.slot\" /></template>",
    )
    .unwrap();
    std::fs::write(
        contributor_dir.join("src/main.mesh"),
        "<template><text content=\"contributor\" /></template>",
    )
    .unwrap();
    let contribution_path = contributor_dir.join("src/contribution.mesh");
    std::fs::write(
        &contribution_path,
        "<template><text content=\"before\" /></template>",
    )
    .unwrap();

    let host_id = "@test/cross-module-host";
    let contributor_id = "@test/cross-module-contributor";
    let mut host_manifest = minimal_test_manifest(host_id);
    let mut contributor_manifest = minimal_test_manifest(contributor_id);
    contributor_manifest.extension_point_contributions.insert(
        "test.cross-module.slot".into(),
        vec![mesh_core_module::manifest::ExtensionPointContribution {
            id: "contribution".into(),
            entry: "src/contribution.mesh".into(),
            order: Some(0),
            props: Default::default(),
        }],
    );
    host_manifest.extension_point_contributions.clear();

    let host_compiled = compile_frontend_module(&host_manifest, &host_dir).unwrap();
    let contributor_compiled =
        compile_frontend_module(&contributor_manifest, &contributor_dir).unwrap();
    let contribution = compile_frontend_entrypoint(
        &contributor_manifest,
        &contributor_dir,
        "src/contribution.mesh",
    )
    .unwrap();
    let catalog = FrontendCatalog {
        modules: HashMap::from([
            (
                host_id.into(),
                FrontendCatalogEntry {
                    module_dir: host_dir.clone(),
                    compiled: host_compiled.clone().into(),
                },
            ),
            (
                contributor_id.into(),
                FrontendCatalogEntry {
                    module_dir: contributor_dir.clone(),
                    compiled: contributor_compiled.into(),
                },
            ),
        ]),
        diagnostics: Default::default(),
        extension_point_contributions: HashMap::from([(
            extension_point_key(host_id, "test.cross-module.slot"),
            vec![ResolvedExtensionPointContribution {
                source_module_id: contributor_id.into(),
                contribution_id: "contribution".into(),
                order: 0,
                props_fingerprint: 0,
                props: Default::default(),
            }],
        )]),
        extension_point_entries: HashMap::from([(
            contribution_entry_key(contributor_id, "contribution"),
            contribution.clone().into(),
        )]),
        node_slot_placements: Default::default(),
        dependency_forks: Default::default(),
    };
    let mut component = FrontendSurfaceComponent::new_for_test(
        host_compiled,
        PathBuf::from(&host_dir),
        catalog,
        mesh_core_service::InterfaceCatalog::default(),
        test_settings_store(),
    );
    component
        .mount(ComponentContext {
            component_id: host_id.into(),
            surface_id: host_id.into(),
            diagnostics: Diagnostics::new(host_id),
        })
        .unwrap();

    assert!(
        component
            .watched_source_paths()
            .contains(&contribution_path)
    );
    let before_version = component.frontend_catalog_handle.snapshot().version;
    assert_eq!(static_content(&contribution), Some("before".into()));

    std::fs::write(
        &contribution_path,
        "<template><text content=\"after\" /></template>",
    )
    .unwrap();
    assert!(component.reload_source().unwrap());

    let state = component.frontend_catalog_handle.snapshot();
    assert_eq!(state.version, before_version + 1);
    assert_eq!(
        static_content(
            state
                .catalog
                .contribution_entry(contributor_id, "contribution")
                .unwrap()
        ),
        Some("after".into())
    );
}

#[test]
fn contribution_only_import_dependency_invalidates_its_host() {
    use crate::shell::component::catalog::{
        FrontendCatalog, FrontendCatalogEntry, FrontendCatalogHandle,
        ResolvedExtensionPointContribution, contribution_entry_key, extension_point_key,
    };
    use mesh_core_component::parse_component;
    use mesh_core_frontend::CompiledFrontendModule;
    use std::collections::HashMap;
    use std::path::PathBuf;

    fn compiled(
        module_id: &str,
        module_component_imports: HashMap<String, String>,
    ) -> crate::shell::component::catalog::SharedCompiledFrontendModule {
        CompiledFrontendModule {
            manifest: minimal_test_manifest(module_id),
            source_path: format!("{module_id}/src/main.mesh").into(),
            component: parse_component("<template><text /></template>").unwrap(),
            public_props: Default::default(),
            local_components: HashMap::new(),
            module_component_imports,
            watched_paths: Vec::new(),
        }
        .into()
    }

    let host_id = "@test/host";
    let contribution_module_id = "@test/contributor";
    let dependency_id = "@test/contribution-only-dependency";
    let unrelated_id = "@test/unrelated";
    let contribution_id = "contribution";

    let catalog = FrontendCatalog {
        modules: HashMap::from([
            (
                host_id.into(),
                FrontendCatalogEntry {
                    module_dir: PathBuf::from("host"),
                    compiled: compiled(host_id, HashMap::new()),
                },
            ),
            (
                contribution_module_id.into(),
                FrontendCatalogEntry {
                    module_dir: PathBuf::from("contributor"),
                    compiled: compiled(contribution_module_id, HashMap::new()),
                },
            ),
            (
                dependency_id.into(),
                FrontendCatalogEntry {
                    module_dir: PathBuf::from("dependency"),
                    compiled: compiled(dependency_id, HashMap::new()),
                },
            ),
            (
                unrelated_id.into(),
                FrontendCatalogEntry {
                    module_dir: PathBuf::from("unrelated"),
                    compiled: compiled(unrelated_id, HashMap::new()),
                },
            ),
        ]),
        diagnostics: Default::default(),
        extension_point_contributions: HashMap::from([(
            extension_point_key(host_id, "test.slot"),
            vec![ResolvedExtensionPointContribution {
                source_module_id: contribution_module_id.into(),
                contribution_id: contribution_id.into(),
                order: 0,
                props_fingerprint: 0,
                props: Default::default(),
            }],
        )]),
        extension_point_entries: HashMap::from([(
            contribution_entry_key(contribution_module_id, contribution_id),
            compiled(
                contribution_module_id,
                HashMap::from([("Imported".into(), dependency_id.into())]),
            ),
        )]),
        node_slot_placements: Default::default(),
        dependency_forks: Default::default(),
    };

    let handle = FrontendCatalogHandle::from(catalog.clone());
    handle.replace(catalog, Some(dependency_id));
    let state = handle.snapshot();

    assert!(state.affected_modules.contains(host_id));
    assert!(state.affected_modules.contains(contribution_module_id));
    assert!(state.affected_modules.contains(dependency_id));
    assert!(!state.affected_modules.contains(unrelated_id));
}

#[test]
fn profile_fork_mapping_changes_component_lookup_and_invalidates_consumers() {
    use crate::shell::component::catalog::{
        FrontendCatalog, FrontendCatalogEntry, FrontendCatalogHandle,
    };
    use mesh_core_component::parse_component;
    use mesh_core_frontend::CompiledFrontendModule;

    let mut host = minimal_test_manifest("@test/host");
    host.dependencies.modules.insert(
        "@test/upstream".into(),
        mesh_core_module::manifest::DependencySpec::Simple("*".into()),
    );
    let entry = |manifest, imports| FrontendCatalogEntry {
        module_dir: ".".into(),
        compiled: CompiledFrontendModule {
            manifest,
            source_path: "src/main.mesh".into(),
            component: parse_component("<template><text /></template>").unwrap(),
            public_props: Default::default(),
            local_components: Default::default(),
            module_component_imports: imports,
            watched_paths: Vec::new(),
        }
        .into(),
    };
    let catalog = FrontendCatalog {
        modules: HashMap::from([
            (
                "@test/host".into(),
                entry(
                    host.clone(),
                    HashMap::from([("Child".into(), "@test/upstream".into())]),
                ),
            ),
            (
                "@test/fork".into(),
                entry(minimal_test_manifest("@test/fork"), HashMap::new()),
            ),
        ]),
        dependency_forks: [("@test/upstream".into(), "@test/fork".into())].into(),
        ..Default::default()
    };
    assert_eq!(
        catalog
            .imported_component_module_id(&host, None, "Child")
            .unwrap(),
        "@test/fork"
    );
    let handle = FrontendCatalogHandle::from(catalog.clone());
    let mut next = catalog;
    next.dependency_forks.clear();
    handle.replace(next, None);
    assert!(handle.snapshot().affected_modules.contains("@test/host"));
}

#[test]
fn source_reload_rebuilds_imported_component_style_cache() {
    use crate::shell::component::catalog::FrontendCatalog;
    use mesh_core_frontend::compile_frontend_module;

    let temp = tempfile::tempdir().unwrap();
    let module_dir = temp.path();
    let components_dir = module_dir.join("src/components");
    std::fs::create_dir_all(&components_dir).unwrap();
    std::fs::write(
        module_dir.join("src/main.mesh"),
        r#"
<template><Child /></template>
<script lang="luau">
local Child = require("./components/child.mesh")
</script>
"#,
    )
    .unwrap();
    let child_path = components_dir.join("child.mesh");
    let write_child = |width| {
        std::fs::write(
            &child_path,
            format!(
                r#"
<template><box class="reload-style-target" /></template>
<style>
    .reload-style-target {{
        width: {width}px;
        height: 10px;
    }}
</style>
"#
            ),
        )
        .unwrap();
    };
    write_child(10);

    let manifest = minimal_test_manifest("@test/imported-style-hot-reload");
    let compiled = compile_frontend_module(&manifest, module_dir).unwrap();
    let catalog = FrontendCatalog {
        modules: HashMap::from([(
            manifest.package.id.clone(),
            crate::shell::component::catalog::FrontendCatalogEntry {
                module_dir: module_dir.to_path_buf(),
                compiled: compiled.clone().into(),
            },
        )]),
        diagnostics: Default::default(),
        extension_point_contributions: Default::default(),
        extension_point_entries: Default::default(),
        node_slot_placements: Default::default(),
        dependency_forks: Default::default(),
    };
    let mut component = FrontendSurfaceComponent::new_for_test(
        compiled,
        module_dir.to_path_buf(),
        catalog,
        mesh_core_service::InterfaceCatalog::default(),
        test_settings_store(),
    );
    component
        .mount(ComponentContext {
            component_id: "@test/imported-style-hot-reload".into(),
            surface_id: "@test/imported-style-hot-reload".into(),
            diagnostics: Diagnostics::new("@test/imported-style-hot-reload"),
        })
        .unwrap();
    component.visible = true;

    let theme = default_theme();
    let mut buffer = PixelBuffer::new(100, 40);
    component
        .paint_raster(&theme, SurfaceExtent::unpadded(100, 40), &mut buffer, 1.0)
        .unwrap();
    assert_eq!(
        first_node_by_class(component.last_tree.as_ref().unwrap(), "reload-style-target")
            .unwrap()
            .computed_style
            .width,
        mesh_core_elements::Dimension::Px(10.0)
    );
    assert!(
        !component.prepared_component_styles.borrow().is_empty(),
        "first render should populate the imported component style cache"
    );

    write_child(24);
    assert!(component.reload_source().unwrap());
    assert!(
        component.prepared_component_styles.borrow().is_empty(),
        "source reload must discard style indexes built from the old component source"
    );

    component
        .paint_raster(&theme, SurfaceExtent::unpadded(100, 40), &mut buffer, 1.0)
        .unwrap();
    assert_eq!(
        first_node_by_class(component.last_tree.as_ref().unwrap(), "reload-style-target")
            .unwrap()
            .computed_style
            .width,
        mesh_core_elements::Dimension::Px(24.0),
        "the next render must resolve styles from the reloaded child component"
    );
}

#[test]
fn retained_paint_path_records_phase26_cpu_attribution_stages() {
    let mut component = test_frontend_component(
        "<template><row><text>Proof</text><icon name=\"phase26-missing-icon\" /></row></template>",
    );
    let theme = default_theme();
    let mut buffer = PixelBuffer::new(160, 48);

    component.set_profiling_enabled(true);
    component
        .paint_raster(&theme, SurfaceExtent::unpadded(160, 48), &mut buffer, 1.0)
        .unwrap();

    let stages: std::collections::HashSet<_> = component
        .take_profiling_records()
        .into_iter()
        .map(|record| record.stage)
        .collect();

    assert!(stages.contains(&mesh_core_debug::ProfilingStage::RenderObjectSync));
    assert!(stages.contains(&mesh_core_debug::ProfilingStage::RetainedDisplayListUpdate));
    assert!(stages.contains(&mesh_core_debug::ProfilingStage::PaintTraversal));
    assert!(stages.contains(&mesh_core_debug::ProfilingStage::TextShaping));
    assert!(stages.contains(&mesh_core_debug::ProfilingStage::IconImageRaster));
    assert!(stages.contains(&mesh_core_debug::ProfilingStage::Paint));

    let invalidation = component
        .take_invalidation_snapshot()
        .expect("profiling-enabled paint should capture invalidation proof");
    // Shaping time accrues only on layout-cache misses; a paint that reuses
    // the layout pass's shaping legitimately records none. Attribution is
    // proven by the text cache traffic the snapshot captured.
    assert!(invalidation.text.layout_hits + invalidation.text.layout_misses > 0);
}

fn content_of(node: &WidgetNode, text: &str) -> bool {
    node.attributes.get(Prop::Content).map(String::as_str) == Some(text)
        || node.children.iter().any(|child| content_of(child, text))
}

#[test]
fn template_patching_applies_a_bound_write_without_a_rebuild() {
    let mut component = test_frontend_component(
        r#"
<script lang="luau">
label = "before"
function update() label = "after" end
</script>
<template>
  <column>
    <text>{label}</text>
    <text>unchanged row one</text>
    <text>unchanged row two</text>
  </column>
</template>
"#,
    );
    let theme = default_theme();
    let extent = SurfaceExtent::unpadded(640, 160);
    let mut buffer = PixelBuffer::new(640, 160);
    component
        .paint_raster(&theme, extent, &mut buffer, 1.0)
        .unwrap();
    while component.wants_render() {
        component
            .paint_raster(&theme, extent, &mut buffer, 1.0)
            .unwrap();
    }

    component.call_namespaced_handler("update", &[]).unwrap();
    component
        .paint_raster(&theme, extent, &mut buffer, 1.0)
        .unwrap();
    let snapshot = component
        .take_invalidation_snapshot()
        .expect("handler paint records invalidation");

    assert_eq!(snapshot.component.script_narrow, 0);
    assert!(snapshot.patched_node_count > 0);
    assert!(!snapshot.full_rebuild);
    assert!(!snapshot.paint.full_surface_damage);
    assert!(snapshot.paint.damage_area < snapshot.paint.surface_area);
    let tree = component.last_tree.as_ref().expect("tree");
    assert!(content_of(tree, "after"));
    assert!(!content_of(tree, "before"));

    // The patched frame matches what a full rebuild paints.
    let mut rebuilt = PixelBuffer::new(640, 160);
    component.invalidate_script_state();
    component
        .paint_raster(&theme, extent, &mut rebuilt, 1.0)
        .unwrap();
    assert_eq!(buffer.data(), rebuilt.data());
}

#[test]
fn template_patching_updates_style_and_accessibility_like_a_rebuild() {
    let mut component = test_frontend_component(
        r#"
<script lang="luau">
label = "before"
tone = "before"
function update()
    label = "after"
    tone = "after"
end
</script>
<template>
  <column style="width: 640px; height: 160px">
    <button class={tone} aria-label={label}><text>{label}</text></button>
    <text>unchanged</text>
  </column>
</template>
<style>
button.before { background: #ff0000; width: 100px; }
button.after { background: #0000ff; width: 200px; }
</style>
"#,
    );
    let theme = default_theme();
    let extent = SurfaceExtent::unpadded(640, 160);
    let mut buffer = PixelBuffer::new(640, 160);
    component
        .paint_raster(&theme, extent, &mut buffer, 1.0)
        .unwrap();
    while component.wants_render() {
        component
            .paint_raster(&theme, extent, &mut buffer, 1.0)
            .unwrap();
    }

    component.call_namespaced_handler("update", &[]).unwrap();
    component
        .paint_raster(&theme, extent, &mut buffer, 1.0)
        .unwrap();
    let snapshot = component.take_invalidation_snapshot().unwrap();
    assert!(snapshot.patched_node_count >= 2);
    assert_eq!(snapshot.component.script_narrow, 0);
    assert!(!snapshot.full_rebuild);
    let patched_button = first_node_by_tag(component.last_tree.as_ref().unwrap(), "button")
        .unwrap()
        .clone();
    assert_eq!(patched_button.attributes.get(Prop::Class).unwrap(), "after");
    assert_eq!(
        patched_button.attributes.get(Prop::AriaLabel).unwrap(),
        "after"
    );

    let mut rebuilt = PixelBuffer::new(640, 160);
    component.invalidate_script_state();
    component
        .paint_raster(&theme, extent, &mut rebuilt, 1.0)
        .unwrap();
    let rebuilt_button =
        first_node_by_tag(component.last_tree.as_ref().unwrap(), "button").unwrap();
    assert_eq!(patched_button.accessibility.label.as_deref(), Some("after"));
    assert_eq!(
        patched_button.accessibility.label,
        rebuilt_button.accessibility.label
    );
    assert_eq!(patched_button.layout.width, rebuilt_button.layout.width);
    assert_eq!(patched_button.layout.height, rebuilt_button.layout.height);
    assert!(
        buffer.data() == rebuilt.data(),
        "patched pixels differ from a rebuild"
    );
}

#[test]
fn template_patching_leaves_conditional_reads_to_the_template() {
    let mut component = test_frontend_component(
        r#"
<script lang="luau">
shown = false
label = "hidden"
function show()
    shown = true
    label = "visible"
end
</script>
<template>
  <column>
    {#if shown}
      <text>{label}</text>
    {/if}
  </column>
</template>
"#,
    );
    let theme = default_theme();
    let extent = SurfaceExtent::unpadded(320, 80);
    let mut buffer = PixelBuffer::new(320, 80);
    component
        .paint_raster(&theme, extent, &mut buffer, 1.0)
        .unwrap();

    component.call_namespaced_handler("show", &[]).unwrap();
    component
        .paint_raster(&theme, extent, &mut buffer, 1.0)
        .unwrap();
    let tree = component.last_tree.as_ref().expect("tree");
    assert!(content_of(tree, "visible"), "the {{#if}} body must appear");
}

#[test]
fn template_patching_unhides_nodes_without_selector_changes() {
    let mut component = test_frontend_component(
        r#"
<script lang="luau">
hidden = false
function toggle() hidden = not hidden end
</script>
<template><column><button hidden={hidden}>Visible again</button></column></template>
<style>button { transition: none; width: 100px; height: 30px; }</style>
"#,
    );
    let theme = default_theme();
    let extent = SurfaceExtent::unpadded(240, 80);
    let mut buffer = PixelBuffer::new(240, 80);
    component
        .paint_raster(&theme, extent, &mut buffer, 1.0)
        .unwrap();
    for hidden in [true, false, true, false] {
        component.call_namespaced_handler("toggle", &[]).unwrap();
        component
            .paint_raster(&theme, extent, &mut buffer, 1.0)
            .unwrap();
        let node = first_node_by_tag(component.last_tree.as_ref().unwrap(), "button").unwrap();
        assert_eq!(
            node.computed_style.display == mesh_core_elements::Display::None,
            hidden
        );
        assert_eq!(node.accessibility.hidden, hidden);
        if !hidden {
            assert!(node.layout.width > 0.0);
        }
    }
}

#[test]
fn animated_ancestor_scoped_update_leaves_nothing_for_a_full_update() {
    // Only the animated node is fingerprinted, not its subtree. A full
    // update of the same tree afterwards must find nothing the scoped one
    // missed, for paint-only, layer, inherited and layout-affecting values.
    for (rest, hovered) in [
        ("opacity: 1;", "opacity: 0.2;"),
        (
            "transform: translateY(0px);",
            "transform: translateY(12px);",
        ),
        ("color: #202020;", "color: #f0f0f0;"),
        ("padding: 2px;", "padding: 9px;"),
        ("width: 60px;", "width: 100px;"),
    ] {
        let property = rest.split(':').next().unwrap();
        let mut component = test_frontend_component(&format!(
            r#"
<template><row><column class="target"><box class="leaf" /><text>Label</text><row><box class="leaf" /></row></column><box /></row></template>
<style>
.target {{ {rest} transition: {property} 1000ms linear; }}
.target:hover {{ {hovered} }}
.leaf {{ width: 10px; height: 10px; background: #808080; }}
</style>
"#
        ));
        let theme = default_theme();
        let mut buffer = PixelBuffer::new(160, 80);
        component
            .paint_raster(&theme, SurfaceExtent::unpadded(160, 80), &mut buffer, 1.0)
            .unwrap();
        component.hovered_path = ["root", "root/0", "root/0/0"]
            .map(runtime_node_id_for_key)
            .to_vec();
        component.hovered_key = Some("root/0/0".into());
        component.invalidate_interaction_restyle();
        for frame in 0..4 {
            std::thread::sleep(Duration::from_millis(15));
            component
                .paint_raster(&theme, SurfaceExtent::unpadded(160, 80), &mut buffer, 1.0)
                .unwrap();
            if frame == 0 {
                continue;
            }
            assert!(
                component.transitions.has_active(Instant::now()),
                "{property}: transition must be running"
            );
            let layout_affecting = matches!(property, "padding" | "width");
            assert!(
                layout_affecting || component.retained_tree.last_update_was_scoped(),
                "{property}: frame {frame} must take the scoped update"
            );
            let tree = component.last_tree.clone().unwrap();
            let missed = component.retained_tree.update(&tree);
            assert!(
                !missed.any() && component.retained_tree.render_dirty_node_ids().is_empty(),
                "{property}: frame {frame} scoped update missed {missed:?}"
            );
        }
    }
}

#[test]
fn keyed_list_item_patches_preserve_pixels_and_structural_fallbacks() {
    let source = r#"
<script lang="luau">
items = {{id = "a", height = "10px"}, {id = "b", height = "20px"}}
function mutate() items[1].height = "30px" end
function replace() items = {{id = "a", height = "40px"}, {id = "b", height = "20px"}} end
function reorder() items = {items[2], items[1]} end
function append() table.insert(items, {id = "c", height = "15px"}) end
function remove() table.remove(items, 1) end
function duplicate() items[2].id = items[1].id end
function empty() items = {} end
</script>
<template><row style="width: 320px; height: 100px">
{#for item in items key={item.id}}<box style="width: 20px; background: #ff0000" style:height={item.height} />{/for}
</row></template>
"#;
    let theme = default_theme();
    let extent = SurfaceExtent::unpadded(320, 100);
    let mut patched = test_frontend_component(source);
    let mut rebuilt = test_frontend_component(source);
    rebuilt.force_full_template_build = true;
    let mut actual = PixelBuffer::new(320, 100);
    let mut expected = PixelBuffer::new(320, 100);
    patched
        .paint_raster(&theme, extent, &mut actual, 1.0)
        .unwrap();
    rebuilt
        .paint_raster(&theme, extent, &mut expected, 1.0)
        .unwrap();
    for handler in [
        "mutate",
        "replace",
        "reorder",
        "append",
        "remove",
        "duplicate",
        "empty",
    ] {
        patched.call_namespaced_handler(handler, &[]).unwrap();
        rebuilt.call_namespaced_handler(handler, &[]).unwrap();
        patched
            .paint_raster(&theme, extent, &mut actual, 1.0)
            .unwrap();
        rebuilt.invalidate_script_state();
        rebuilt
            .paint_raster(&theme, extent, &mut expected, 1.0)
            .unwrap();
        assert_eq!(actual.data(), expected.data(), "{handler}");
        let snapshot = patched.take_invalidation_snapshot().unwrap();
        if matches!(handler, "mutate" | "replace") {
            assert_eq!(snapshot.patched_node_count, 1, "{handler}: {snapshot:?}");
            assert_eq!(
                snapshot.component.script_narrow, 0,
                "{handler}: {snapshot:?}"
            );
        } else {
            assert_eq!(snapshot.patched_node_count, 0, "{handler}: {snapshot:?}");
            assert_eq!(
                snapshot.component.script_narrow, 1,
                "{handler}: {snapshot:?}"
            );
        }
        let tree = patched.last_tree.clone().unwrap();
        assert!(
            !patched.retained_tree.update(&tree).any(),
            "{handler}: missed retained changes"
        );
    }
}

#[test]
fn keyed_list_one_item_write_does_not_patch_clean_siblings() {
    for count in [8, 256] {
        let source = format!(
            r##"
<script lang="luau">
items = {{}}
for i = 1, {count} do items[i] = {{id = i, color = "#ff0000"}} end
function mutate() items[3].color = "#00ff00" end
</script>
<template><row style="width: 320px; height: 100px">
{{#for item in items key={{item.id}}}}<box style="width: 1px; height: 10px" style:background={{item.color}} />{{/for}}
</row></template>
"##
        );
        let mut component = test_frontend_component(&source);
        let theme = default_theme();
        let extent = SurfaceExtent::unpadded(320, 100);
        let mut pixels = PixelBuffer::new(320, 100);
        component
            .paint_raster(&theme, extent, &mut pixels, 1.0)
            .unwrap();
        component.set_profiling_enabled(true);
        let _ = component.take_profiling_records();
        component.call_namespaced_handler("mutate", &[]).unwrap();
        component
            .paint_raster(&theme, extent, &mut pixels, 1.0)
            .unwrap();
        let snapshot = component.take_invalidation_snapshot().unwrap();
        assert_eq!(snapshot.patched_node_count, 1, "{count}: {snapshot:?}");
        assert_eq!(snapshot.component.script_narrow, 0);
        assert!(!snapshot.full_rebuild);
        assert!(
            component
                .take_profiling_records()
                .iter()
                .all(|record| record.stage != mesh_core_debug::ProfilingStage::TreeBuild)
        );
    }
}

/// A pointer-enter handler runs while input dispatch holds the retained tree.
/// Its write to a keyed list still patches the items once the tree is back,
/// and the patched items' handlers capture the refreshed item.
#[test]
fn keyed_list_handler_write_during_input_patches_and_rebinds_item_handlers() {
    let source = r#"
<script lang="luau">
hovered = 0
picked = ""
items = {}
effect(function()
    local list = {}
    for index, id in {"a", "b", "c"} do
        list[#list + 1] = {
            id = id,
            index = index,
            hot = hovered == index,
            label = if hovered == index then id .. "*" else id,
        }
    end
    items = list
end)
function onHover(index) hovered = index end
function pick(label) picked = label end
</script>
<template><row style="width: 300px; height: 100px">
{#for item in items key={item.id}}<button style="width: 100px; height: 100px" class:hot={item.hot} onpointerenter={onHover(item.index)} onclick={pick(item.label)} />{/for}
</row></template>
"#;
    let theme = default_theme();
    let extent = SurfaceExtent::unpadded(300, 100);
    let mut pixels = PixelBuffer::new(300, 100);
    let mut component = test_frontend_component(source);
    component
        .paint_raster(&theme, extent, &mut pixels, 1.0)
        .unwrap();
    let _ = component.take_invalidation_snapshot();
    let (x, y) = (150.0, 50.0);
    component
        .handle_input(&theme, 300, 100, ComponentInput::PointerMove { x, y })
        .unwrap();
    assert_eq!(runtime_number(&component, "hovered"), 2.0);
    component
        .paint_raster(&theme, extent, &mut pixels, 1.0)
        .unwrap();
    let snapshot = component.take_invalidation_snapshot().unwrap();
    assert!(snapshot.patched_node_count >= 1, "{snapshot:?}");
    assert_eq!(snapshot.component.script_narrow, 0, "{snapshot:?}");
    assert!(!snapshot.full_rebuild, "{snapshot:?}");
    let tree = component.last_tree.clone().unwrap();
    assert!(!component.retained_tree.update(&tree).any());
    for pressed in [true, false] {
        component
            .handle_input(
                &theme,
                300,
                100,
                ComponentInput::PointerButton {
                    x,
                    y,
                    button: mesh_core_presentation::PRIMARY_POINTER_BUTTON,
                    pressed,
                },
            )
            .unwrap();
    }
    assert_eq!(
        runtime_value(&component, "picked"),
        Some(serde_json::json!("b*"))
    );
}

#[test]
fn keyed_list_hover_restyle_visits_scale_with_affected_items() {
    if crate::shell::component::tests::common::isolate_frame_test(
        "shell::component::tests::invalidation::basic::keyed_list_hover_restyle_visits_scale_with_affected_items",
    ) {
        return;
    }
    let theme = default_theme();
    let extent = SurfaceExtent::unpadded(320, 100);
    let mut per_size = Vec::new();
    for item_count in [16, 1024] {
        let source = format!(
            r##"
<script lang="luau">
hovered_index = 0
items = {{}}
effect(function()
    local list = {{}}
    for index = 1, {item_count} do
        list[index] = {{id = index, hot = hovered_index == index,
            neighbor = hovered_index > 0 and math.abs(hovered_index - index) == 1}}
    end
    items = list
end)
function onHover(index) hovered_index = index end
</script>
<template><row style="width: 320px; height: 100px">
{{#for item in items key={{item.id}}}}<button style="width: 40px; height: 40px; flex-shrink: 0" class:hot={{item.hot}} class:neighbor={{item.neighbor}} onpointerenter={{onHover(item.id)}}><text>x</text><box><text>y</text></box></button>{{/for}}
</row></template>
<style>
button {{ color: #101010; background: #222222; }}
button:hover {{ opacity: 0.8; }}
.hot {{ color: #ff0000; background: #0000ff; }}
.neighbor {{ color: #00ff00; }}
</style>
"##
        );
        let mut scoped = test_frontend_component(&source);
        let mut full = test_frontend_component(&source);
        full.force_full_interaction_frames = true;
        let mut pixels = PixelBuffer::new(320, 100);
        let mut full_pixels = PixelBuffer::new(320, 100);
        for _ in 0..3 {
            scoped
                .paint_raster(&theme, extent, &mut pixels, 1.0)
                .unwrap();
            full.paint_raster(&theme, extent, &mut full_pixels, 1.0)
                .unwrap();
        }
        scoped.set_profiling_enabled(true);
        let mut visits = Vec::new();
        for index in [2, 3, 5, 2] {
            for component in [&mut scoped, &mut full] {
                let key = first_node_by_tag(component.last_tree.as_ref().unwrap(), "row")
                    .unwrap()
                    .children[index - 1]
                    .mesh_key()
                    .unwrap()
                    .to_owned();
                let (left, top, right, bottom) =
                    find_node_bounds_by_key(component.last_tree.as_ref().unwrap(), &key, 0.0, 0.0)
                        .unwrap();
                component
                    .handle_input(
                        &theme,
                        320,
                        100,
                        ComponentInput::PointerMove {
                            x: (left + right) / 2.0,
                            y: (top + bottom) / 2.0,
                        },
                    )
                    .unwrap();
                assert_eq!(runtime_number(component, "hovered_index"), index as f64);
            }
            scoped
                .paint_raster(&theme, extent, &mut pixels, 1.0)
                .unwrap();
            full.paint_raster(&theme, extent, &mut full_pixels, 1.0)
                .unwrap();
            let snapshot = scoped.take_invalidation_snapshot().unwrap();
            assert!(!snapshot.full_rebuild, "{snapshot:?}");
            assert!(snapshot.patched_node_count > 0, "{snapshot:?}");
            let counts = snapshot.node_visits;
            assert!(
                counts["style_resolve"] > 3,
                "inherited descendants must resolve: {counts:?}"
            );
            assert_eq!(counts["retained_discovery"], 0);
            visits.push([counts["restyle_scan"], counts["style_resolve"]]);
            scoped
                .retained_tree
                .assert_complete(scoped.last_tree.as_ref().unwrap());
            assert_eq!(pixels.data(), full_pixels.data(), "hover {index}");
            let (mut styles, mut full_styles) = (Vec::new(), Vec::new());
            computed_styles(scoped.last_tree.as_ref().unwrap(), &mut styles);
            computed_styles(full.last_tree.as_ref().unwrap(), &mut full_styles);
            assert!(styles == full_styles, "hover {index}: styles diverged");
            // An unpatched interaction frame advances the retained generation
            // between diagnostic updates, as animation frames do in the dock.
            for component in [&mut scoped, &mut full] {
                component
                    .handle_input(&theme, 320, 100, ComponentInput::PointerLeave)
                    .unwrap();
            }
            scoped
                .paint_raster(&theme, extent, &mut pixels, 1.0)
                .unwrap();
            full.paint_raster(&theme, extent, &mut full_pixels, 1.0)
                .unwrap();
            let _ = scoped.take_invalidation_snapshot();
        }
        eprintln!("keyed hover items={item_count} restyle/resolve={visits:?}");
        per_size.push(visits);
    }
    assert_eq!(
        per_size[0], per_size[1],
        "unrelated items increased restyle work"
    );
}

fn patched_scope_source(clean_siblings: usize) -> String {
    format!(
        r##"
<script lang="luau">
items = {{}}
for i = 1, {clean_siblings} do items[i] = {{id = i, color = "#ff0000"}} end
tint = "#101010"
flips = 0
function mutate()
    flips = flips + 1
    items[3].color = flips % 2 == 0 and "#00ff00" or "#0000ff"
end
function retint()
    flips = flips + 1
    tint = flips % 2 == 0 and "#00ff00" or "#ff00ff"
end
</script>
<template><row style="width: 320px; height: 100px">
<box class="group" style="width: 40px; height: 20px" style:color={{tint}}><text>a</text><box><text>b</text></box></box>
{{#for item in items key={{item.id}}}}<box style="width: 1px; height: 10px" style:background={{item.color}} />{{/for}}
</row></template>
"##
    )
}

/// Patched-binding frames visit the patched nodes, plus the descendants whose
/// inherited values changed, not the tree. A full diff finds nothing missed
/// and a twin on the full paths paints and styles identically.
#[test]
fn patched_frame_restyle_visits_scale_with_the_patch_not_the_tree() {
    let theme = default_theme();
    let extent = SurfaceExtent::unpadded(320, 100);
    let mut per_size = Vec::new();
    for clean_siblings in [16, 1024] {
        let source = patched_scope_source(clean_siblings);
        let mut scoped = test_frontend_component(&source);
        let mut full = test_frontend_component(&source);
        full.force_full_interaction_frames = true;
        let mut scoped_pixels = PixelBuffer::new(320, 100);
        let mut full_pixels = PixelBuffer::new(320, 100);
        for _ in 0..3 {
            scoped
                .paint_raster(&theme, extent, &mut scoped_pixels, 1.0)
                .unwrap();
            full.paint_raster(&theme, extent, &mut full_pixels, 1.0)
                .unwrap();
        }
        scoped.set_profiling_enabled(true);
        let _ = scoped.take_invalidation_snapshot();
        let mut visits = Vec::new();
        for handler in ["mutate", "mutate", "retint", "retint", "mutate"] {
            scoped.call_namespaced_handler(handler, &[]).unwrap();
            full.call_namespaced_handler(handler, &[]).unwrap();
            scoped
                .paint_raster(&theme, extent, &mut scoped_pixels, 1.0)
                .unwrap();
            full.paint_raster(&theme, extent, &mut full_pixels, 1.0)
                .unwrap();
            let snapshot = scoped.take_invalidation_snapshot().unwrap();
            assert!(!snapshot.full_rebuild, "{handler}: {snapshot:?}");
            let counts = snapshot.node_visits;
            assert_eq!(counts["retained_discovery"], 0, "{handler}: {counts:?}");
            visits.push([
                counts["restyle_scan"],
                counts["style_resolve"],
                counts["retained_discovery"],
            ]);

            let tree = scoped.last_tree.clone().unwrap();
            let missed = scoped.retained_tree.update(&tree);
            assert!(!missed.any(), "{handler}: patched frame missed {missed:?}");
            assert_eq!(scoped_pixels.data(), full_pixels.data(), "{handler}");
            let (mut scoped_styles, mut full_styles) = (Vec::new(), Vec::new());
            computed_styles(&tree, &mut scoped_styles);
            computed_styles(full.last_tree.as_ref().unwrap(), &mut full_styles);
            assert!(scoped_styles == full_styles, "{handler}: styles diverged");
        }
        // The container's color reaches its text descendants.
        let group = first_node_by_tag(scoped.last_tree.as_ref().unwrap(), "row")
            .unwrap()
            .children[0]
            .clone();
        let text = first_node_by_tag(&group, "text").unwrap();
        assert_eq!(text.computed_style.color, group.computed_style.color);
        assert_ne!(
            text.computed_style.color,
            mesh_core_elements::Color::from_hex("#101010").unwrap()
        );
        per_size.push(visits);
    }
    assert_eq!(
        per_size[0], per_size[1],
        "patched-frame visits grew with clean siblings"
    );
}

/// Layout-class patches lay out the patched branch and the siblings it moves,
/// not the tree: visits stay constant as an unrelated column grows, and every
/// rect matches a twin on the full path.
#[test]
fn patched_frame_layout_visits_scale_with_the_patch() {
    fn layouts(node: &WidgetNode, out: &mut Vec<(String, mesh_core_elements::LayoutRect)>) {
        out.push((node.mesh_key().unwrap_or_default().to_string(), node.layout));
        node.children.iter().for_each(|child| layouts(child, out));
    }
    let theme = default_theme();
    let extent = SurfaceExtent::unpadded(320, 100);
    let mut per_size = Vec::new();
    for clean_siblings in [16, 1024] {
        let source = format!(
            r##"
<script lang="luau">
items = {{}}
for i = 1, 4 do items[i] = {{id = i, height = "10px"}} end
clean = {{}}
for i = 1, {clean_siblings} do clean[i] = i end
tall = false
function mutate()
    tall = not tall
    items[2].height = tall and "20px" or "10px"
end
</script>
<template><row style="width: 320px; height: 100px">
<column style="width: 40px">{{#for item in items key={{item.id}}}}<box style="width: 10px; background: #ff0000" style:height={{item.height}} />{{/for}}<box style="width: 10px; height: 5px; background: #00ff00" /></column>
<column style="width: 40px">{{#for c in clean key={{c}}}}<box style="width: 1px; height: 1px" />{{/for}}</column>
</row></template>
"##
        );
        let mut scoped = test_frontend_component(&source);
        let mut full = test_frontend_component(&source);
        full.force_full_interaction_frames = true;
        let mut scoped_pixels = PixelBuffer::new(320, 100);
        let mut full_pixels = PixelBuffer::new(320, 100);
        for _ in 0..3 {
            scoped
                .paint_raster(&theme, extent, &mut scoped_pixels, 1.0)
                .unwrap();
            full.paint_raster(&theme, extent, &mut full_pixels, 1.0)
                .unwrap();
        }
        scoped.set_profiling_enabled(true);
        let _ = scoped.take_invalidation_snapshot();
        let mut visits = Vec::new();
        for turn in 0..4 {
            scoped.call_namespaced_handler("mutate", &[]).unwrap();
            full.call_namespaced_handler("mutate", &[]).unwrap();
            scoped
                .paint_raster(&theme, extent, &mut scoped_pixels, 1.0)
                .unwrap();
            full.paint_raster(&theme, extent, &mut full_pixels, 1.0)
                .unwrap();
            let snapshot = scoped.take_invalidation_snapshot().unwrap();
            assert!(snapshot.patched_node_count >= 1, "{turn}: {snapshot:?}");
            let counts = snapshot.node_visits;
            assert_eq!(counts["layout_discovery"], 0, "{turn}: {counts:?}");
            assert_eq!(counts["validation"], 0, "{turn}: {counts:?}");
            assert_eq!(counts["retained_discovery"], 0, "{turn}: {counts:?}");
            scoped
                .retained_tree
                .assert_complete(scoped.last_tree.as_ref().unwrap());
            visits.push([
                counts["retained_sync"],
                counts["layout_sync"],
                counts["layout_write"],
                counts["layout_extents"],
                counts["layout_fit"],
            ]);
            let (mut scoped_layouts, mut full_layouts) = (Vec::new(), Vec::new());
            layouts(scoped.last_tree.as_ref().unwrap(), &mut scoped_layouts);
            layouts(full.last_tree.as_ref().unwrap(), &mut full_layouts);
            assert_eq!(scoped_layouts, full_layouts, "{turn}: layouts diverged");
            assert_eq!(scoped_pixels.data(), full_pixels.data(), "{turn}");
        }
        per_size.push(visits);
    }
    assert_eq!(
        per_size[0], per_size[1],
        "patched-frame layout visits grew with clean siblings"
    );
}

#[test]
fn keyed_list_structural_body_and_duplicate_keys_keep_rebuild() {
    for body in [
        "{#for item in items key={item.id}}{#if item.show}<text>{item.label}</text>{/if}{/for}",
        "{#for item in items key={item.group}}<text>{item.label}</text>{/for}",
    ] {
        let source = format!(
            r#"
<script lang="luau">
items = {{{{id = "a", group = "same", show = true, label = "before"}}, {{id = "b", group = "same", show = true, label = "two"}}}}
function mutate() items[1].label = "after" items[1].show = false end
</script>
<template><column style="width: 320px; height: 100px">
<column>{{#for item in items key={{item.id}}}}<text>{{item.label}}</text>{{/for}}</column>
<column>{body}</column>
</column></template>
"#
        );
        let mut component = test_frontend_component(&source);
        let theme = default_theme();
        let extent = SurfaceExtent::unpadded(320, 100);
        let mut pixels = PixelBuffer::new(320, 100);
        component
            .paint_raster(&theme, extent, &mut pixels, 1.0)
            .unwrap();
        component.call_namespaced_handler("mutate", &[]).unwrap();
        component
            .paint_raster(&theme, extent, &mut pixels, 1.0)
            .unwrap();
        let snapshot = component.take_invalidation_snapshot().unwrap();
        assert_eq!(snapshot.patched_node_count, 0, "{body}: {snapshot:?}");
        assert_eq!(snapshot.component.script_narrow, 1);
        let mut expected = PixelBuffer::new(320, 100);
        component.invalidate_script_state();
        component
            .paint_raster(&theme, extent, &mut expected, 1.0)
            .unwrap();
        assert_eq!(pixels.data(), expected.data());
    }
}

#[test]
fn binding_patch_visits_no_target_discovery() {
    let theme = default_theme();
    let extent = SurfaceExtent::unpadded(320, 100);
    let mut by_size = Vec::new();
    for siblings in [16, 1024] {
        let source = patched_scope_source(siblings);
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
        let mut visits = Vec::new();
        for handler in ["mutate", "mutate", "retint", "retint", "mutate"] {
            let scope = mesh_core_elements::frame_work::Scope::new(true);
            patched.call_namespaced_handler(handler, &[]).unwrap();
            let counts = scope.snapshot();
            assert_eq!(counts["binding_target_discovery"], 0, "{counts:?}");
            assert_eq!(counts["keyed_key_validation"], 0, "{counts:?}");
            assert_eq!(counts["binding_patch"], 1, "{counts:?}");
            visits.push(counts["binding_patch"]);
            drop(scope);
            full.call_namespaced_handler(handler, &[]).unwrap();
            patched
                .paint_raster(&theme, extent, &mut actual, 1.0)
                .unwrap();
            full.paint_raster(&theme, extent, &mut expected, 1.0)
                .unwrap();
            assert_eq!(actual.data(), expected.data(), "{handler}");
            let tree = patched.last_tree.clone().unwrap();
            assert!(
                !patched.retained_tree.update(&tree).any(),
                "{handler}: missed changes"
            );
            let (mut actual_styles, mut expected_styles) = (Vec::new(), Vec::new());
            computed_styles(&tree, &mut actual_styles);
            computed_styles(full.last_tree.as_ref().unwrap(), &mut expected_styles);
            assert!(
                actual_styles == expected_styles,
                "{handler}: styles diverged"
            );
        }
        by_size.push(visits);
    }
    assert_eq!(by_size[0], by_size[1]);
}

#[test]
fn keyed_list_dynamic_keys_and_nested_key_changes_keep_rebuild_parity() {
    for key in ["item.identity.id", "item.identity[field]", "key_for(item)"] {
        let source = format!(
            r##"
<script lang="luau">
field = "id"
items = {{{{identity = {{id = "a"}}, color = "#ff0000"}}, {{identity = {{id = "b"}}, color = "#00ff00"}}}}
function key_for(item) return item.identity.id end
function recolor() items[1].color = "#0000ff" end
function rekey() items[1].identity.id = "c" end
function duplicate() items[1].identity.id = items[2].identity.id end
</script>
<template><row style="width: 100px; height: 30px">
{{#for item in items key={{{key}}}}}<box style="width: 20px; height: 20px" style:background={{item.color}} />{{/for}}
</row></template>
"##
        );
        let theme = default_theme();
        let extent = SurfaceExtent::unpadded(100, 30);
        let mut patched = test_frontend_component(&source);
        let mut full = test_frontend_component(&source);
        full.force_full_template_build = true;
        let mut actual = PixelBuffer::new(100, 30);
        let mut expected = PixelBuffer::new(100, 30);
        patched
            .paint_raster(&theme, extent, &mut actual, 1.0)
            .unwrap();
        full.paint_raster(&theme, extent, &mut expected, 1.0)
            .unwrap();
        for handler in ["recolor", "rekey", "recolor", "duplicate"] {
            patched.call_namespaced_handler(handler, &[]).unwrap();
            full.call_namespaced_handler(handler, &[]).unwrap();
            patched
                .paint_raster(&theme, extent, &mut actual, 1.0)
                .unwrap();
            full.paint_raster(&theme, extent, &mut expected, 1.0)
                .unwrap();
            assert_eq!(actual.data(), expected.data(), "{key}: {handler}");
            if matches!(handler, "rekey" | "duplicate") {
                assert_eq!(
                    patched
                        .take_invalidation_snapshot()
                        .unwrap()
                        .patched_node_count,
                    0
                );
            }
        }
    }
}

#[test]
fn typed_binding_setters_preserve_values_and_change_classes() {
    use mesh_core_elements::{AttrKey, ChangeClass, Prop};
    for (name, prop, initial, next) in [
        ("min", Some(Prop::Min), "1", "2"),
        ("disabled", Some(Prop::Disabled), "false", "true"),
        ("aria-label", Some(Prop::AriaLabel), "\"old\"", "\"new\""),
        ("data-extra", None, "{1}", "{2}"),
    ] {
        let source = format!(
            r#"
<script lang="luau">
value = {initial}
function mutate() value = {next} end
</script>
<template><slider {name}={{value}} /></template>
"#
        );
        let mut component = test_frontend_component(&source);
        let theme = default_theme();
        let extent = SurfaceExtent::unpadded(100, 30);
        let mut pixels = PixelBuffer::new(100, 30);
        component
            .paint_raster(&theme, extent, &mut pixels, 1.0)
            .unwrap();
        fn bound(node: &WidgetNode) -> Option<&WidgetNode> {
            if node.template_bindings().is_some() {
                return Some(node);
            }
            node.children.iter().find_map(bound)
        }
        let node = bound(component.last_tree.as_ref().unwrap()).unwrap();
        let id = node.id;
        let target = &node.template_bindings().unwrap().attributes[0].0;
        match (prop, target) {
            (Some(expected), AttrKey::Known(actual)) => assert_eq!(expected, *actual),
            (None, AttrKey::Shared(_)) => {}
            _ => panic!("unresolved target: {target:?}"),
        }
        component.call_namespaced_handler("mutate", &[]).unwrap();
        let change = component
            .frame_dirty
            .iter()
            .find(|(node, _)| *node == id)
            .unwrap()
            .1;
        assert_eq!(
            change,
            prop.map(Prop::change_class)
                .unwrap_or(ChangeClass::FREE_FORM)
        );
        let node = bound(component.last_tree.as_ref().unwrap()).unwrap();
        let value = node
            .attributes
            .get_value_by_name(name)
            .unwrap()
            .to_json_value();
        match name {
            "min" => assert_eq!(value.as_f64(), Some(2.0)),
            "disabled" => assert_eq!(value.as_bool(), Some(true)),
            "aria-label" => assert_eq!(value.as_str(), Some("new")),
            _ => assert!(value.is_array()),
        }
    }
}
