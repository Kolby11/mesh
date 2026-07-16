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
    component.paint(&theme, 200, 60, &mut buffer, 1.0).unwrap();
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
        component.dirty,
        "bound write must still rebuild the template"
    );
    assert!(
        component
            .dirty_types
            .contains(ComponentDirtyFlags::SCRIPT_NARROW)
    );
    assert!(!component.dirty_types.contains(ComponentDirtyFlags::SCRIPT));
}

#[test]
fn bound_handler_write_uses_sparse_pixel_damage() {
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
    <text>unchanged row three</text>
  </column>
</template>
"#,
    );
    let theme = default_theme();
    let mut buffer = PixelBuffer::new(640, 160);
    component.paint(&theme, 640, 160, &mut buffer, 1.0).unwrap();
    while component.wants_render() {
        component.paint(&theme, 640, 160, &mut buffer, 1.0).unwrap();
    }

    component.call_namespaced_handler("update", &[]).unwrap();
    component.paint(&theme, 640, 160, &mut buffer, 1.0).unwrap();
    let snapshot = component
        .take_invalidation_snapshot()
        .expect("handler paint records invalidation");

    assert_eq!(snapshot.component.script_narrow, 1);
    assert!(!snapshot.full_rebuild);
    assert!(!snapshot.paint.full_surface_damage);
    assert!(snapshot.paint.damage_area < snapshot.paint.surface_area);
    assert!(snapshot.paint.skipped_paint_pixels > 0);
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
        .paint(&theme, 320, 100, &mut narrow_buffer, 1.0)
        .unwrap();
    full.paint(&theme, 320, 100, &mut full_buffer, 1.0).unwrap();

    narrow.call_namespaced_handler("toggle", &[]).unwrap();
    full.call_namespaced_handler("toggle", &[]).unwrap();
    full.invalidate_script_state();
    narrow
        .paint(&theme, 320, 100, &mut narrow_buffer, 1.0)
        .unwrap();
    full.paint(&theme, 320, 100, &mut full_buffer, 1.0).unwrap();

    assert_eq!(narrow_buffer.data, full_buffer.data);
    let snapshot = narrow
        .take_invalidation_snapshot()
        .expect("structural handler paint records invalidation");
    assert_eq!(snapshot.component.script_narrow, 1);
    assert!(snapshot.retained.inserted > 0 || snapshot.retained.removed > 0);
}

// cargo test -p mesh-core-shell --release -- narrow_handler_damage_beats_forced_full_repaint --ignored --nocapture
#[test]
#[ignore = "release-only end-to-end handler damage benchmark"]
fn narrow_handler_damage_beats_forced_full_repaint() {
    use std::hint::black_box;
    use std::time::Instant;

    let declarations = (0..49)
        .map(|index| format!("label{index} = 'row {index:02}'\n"))
        .collect::<String>();
    let expression_rows = (0..49)
        .map(|index| format!("<text>{{label{index}}}</text>"))
        .collect::<String>();
    let source = format!(
        r#"
<script lang="luau">
{declarations}
function update() label0 = label0 == "row 00" and "changed" or "row 00" end
</script>
<template><column>{expression_rows}</column></template>
"#
    );
    let theme = default_theme();
    let mut narrow = test_frontend_component(&source);
    let mut uncached_sparse = test_frontend_component(&source);
    let mut full = test_frontend_component(&source);
    let mut narrow_buffer = PixelBuffer::new(800, 1000);
    let mut uncached_sparse_buffer = PixelBuffer::new(800, 1000);
    let mut full_buffer = PixelBuffer::new(800, 1000);
    narrow
        .paint(&theme, 800, 1000, &mut narrow_buffer, 1.0)
        .unwrap();
    uncached_sparse
        .paint(&theme, 800, 1000, &mut uncached_sparse_buffer, 1.0)
        .unwrap();
    full.paint(&theme, 800, 1000, &mut full_buffer, 1.0)
        .unwrap();

    let iterations = 200;
    let started = Instant::now();
    for _ in 0..iterations {
        narrow.call_namespaced_handler("update", &[]).unwrap();
        narrow
            .paint(&theme, 800, 1000, &mut narrow_buffer, 1.0)
            .unwrap();
        black_box(&narrow_buffer);
    }
    let narrow_time = started.elapsed();

    let uncached_sparse_id = uncached_sparse.id().to_string();
    let started = Instant::now();
    for _ in 0..iterations {
        uncached_sparse
            .call_namespaced_handler("update", &[])
            .unwrap();
        uncached_sparse
            .runtimes
            .lock()
            .unwrap()
            .get(uncached_sparse_id.as_str())
            .unwrap()
            .script_ctx
            .clear_template_expression_cache();
        uncached_sparse
            .paint(&theme, 800, 1000, &mut uncached_sparse_buffer, 1.0)
            .unwrap();
        black_box(&uncached_sparse_buffer);
    }
    let uncached_sparse_time = started.elapsed();

    let full_id = full.id().to_string();
    let started = Instant::now();
    for _ in 0..iterations {
        full.call_namespaced_handler("update", &[]).unwrap();
        full.runtimes
            .lock()
            .unwrap()
            .get(full_id.as_str())
            .unwrap()
            .script_ctx
            .clear_template_expression_cache();
        full.invalidate_script_state();
        full.paint(&theme, 800, 1000, &mut full_buffer, 1.0)
            .unwrap();
        black_box(&full_buffer);
    }
    let full_time = started.elapsed();

    eprintln!(
        "{iterations} bound handler paints: cached+sparse {narrow_time:?}; uncached+sparse {uncached_sparse_time:?}; uncached+full repaint {full_time:?}; expression ratio {:.2}x; total ratio {:.2}x",
        uncached_sparse_time.as_secs_f64() / narrow_time.as_secs_f64(),
        full_time.as_secs_f64() / narrow_time.as_secs_f64(),
    );
    eprintln!(
        "MESH_PERF metric=handler_expression_cache_speedup value={:.6}",
        uncached_sparse_time.as_secs_f64() / narrow_time.as_secs_f64()
    );
    eprintln!(
        "MESH_PERF metric=handler_sparse_vs_full_speedup value={:.6}",
        full_time.as_secs_f64() / narrow_time.as_secs_f64()
    );
    assert_eq!(narrow_buffer.data, full_buffer.data);
    assert_eq!(narrow_buffer.data, uncached_sparse_buffer.data);
    assert!(narrow_time < uncached_sparse_time);
    assert!(narrow_time < full_time);
}

// cargo test -p mesh-core-shell --release -- unbound_handler_write_eliminates_render_pipeline_benchmark --ignored --nocapture
#[test]
#[ignore]
fn unbound_handler_write_eliminates_render_pipeline_benchmark() {
    use std::hint::black_box;
    use std::time::Instant;

    fn benchmark_component(handler_body: &str) -> FrontendSurfaceComponent {
        test_frontend_component(&format!(
            r#"
<script lang="luau">
label = "a"
telemetry = 0
function update() {handler_body} end
</script>
<template><button>{{label}}</button></template>
"#
        ))
    }

    let theme = default_theme();
    let mut unbound = benchmark_component("telemetry = telemetry + 1");
    let mut bound = benchmark_component("label = label == 'a' and 'b' or 'a'");
    let mut unbound_buffer = PixelBuffer::new(200, 60);
    let mut bound_buffer = PixelBuffer::new(200, 60);
    unbound
        .paint(&theme, 200, 60, &mut unbound_buffer, 1.0)
        .unwrap();
    bound
        .paint(&theme, 200, 60, &mut bound_buffer, 1.0)
        .unwrap();
    for _ in 0..4 {
        if !unbound.wants_render() && !bound.wants_render() {
            break;
        }
        if unbound.wants_render() {
            unbound
                .paint(&theme, 200, 60, &mut unbound_buffer, 1.0)
                .unwrap();
        }
        if bound.wants_render() {
            bound
                .paint(&theme, 200, 60, &mut bound_buffer, 1.0)
                .unwrap();
        }
    }
    assert!(!unbound.wants_render() && !bound.wants_render());

    let started = Instant::now();
    let mut unbound_paints = 0;
    for _ in 0..200 {
        unbound.call_namespaced_handler("update", &[]).unwrap();
        if unbound.wants_render() {
            unbound
                .paint(&theme, 200, 60, &mut unbound_buffer, 1.0)
                .unwrap();
            unbound_paints += 1;
        }
        black_box(&unbound_buffer);
    }
    let unbound_time = started.elapsed();

    let started = Instant::now();
    let mut bound_paints = 0;
    for _ in 0..200 {
        bound.call_namespaced_handler("update", &[]).unwrap();
        if bound.wants_render() {
            bound
                .paint(&theme, 200, 60, &mut bound_buffer, 1.0)
                .unwrap();
            bound_paints += 1;
        }
        black_box(&bound_buffer);
    }
    let bound_time = started.elapsed();

    eprintln!(
        "200 handler writes: unbound={unbound_time:?}/{unbound_paints} paints, bound={bound_time:?}/{bound_paints} paints, ratio={:.1}x",
        bound_time.as_secs_f64() / unbound_time.as_secs_f64()
    );
    eprintln!(
        "MESH_PERF metric=unbound_handler_speedup value={:.6}",
        bound_time.as_secs_f64() / unbound_time.as_secs_f64()
    );
    assert_eq!(unbound_paints, 0);
    assert_eq!(bound_paints, 200);
    assert!(unbound_time < bound_time);
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

// cargo test -p mesh-core-shell --release -- render_surface_config_gate_skips_paint_only_bookkeeping --ignored --nocapture
#[test]
#[ignore = "release-only render surface-config gate microbenchmark"]
fn render_surface_config_gate_skips_paint_only_bookkeeping() {
    use std::hint::black_box;
    use std::time::Instant;

    let iterations = 500_000;
    let mut surface = CountingSurface::default();

    let mut old_component = test_frontend_component("<template><box /></template>");
    old_component.dirty = true;
    old_component.style_only_dirty = true;
    old_component.dirty_types = ComponentDirtyFlags::SURFACE_CONFIG;
    let old_started = Instant::now();
    for _ in 0..iterations {
        old_component.render(black_box(&mut surface)).unwrap();
    }
    let old_time = old_started.elapsed();
    let old_calls = surface.layout_calls + surface.visibility_calls;

    surface.reset();
    let mut gated_component = test_frontend_component("<template><box /></template>");
    gated_component.dirty = true;
    gated_component.style_only_dirty = true;
    gated_component.dirty_types = ComponentDirtyFlags::PAINT;
    let gated_started = Instant::now();
    for _ in 0..iterations {
        gated_component.render(black_box(&mut surface)).unwrap();
    }
    let gated_time = gated_started.elapsed();
    let gated_calls = surface.layout_calls + surface.visibility_calls;

    eprintln!(
        "render surface config bookkeeping: {old_time:?} ({old_calls} calls); gated paint-only: {gated_time:?} ({gated_calls} calls); ratio: {:.1}x",
        old_time.as_secs_f64() / gated_time.as_secs_f64()
    );
    assert_eq!(gated_calls, 0);
    assert!(gated_time * 2 < old_time);
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
    component.paint(&theme, 200, 80, &mut buffer, 1.0).unwrap();
    if component.wants_render() {
        component.paint(&theme, 200, 80, &mut buffer, 1.0).unwrap();
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
    component.paint(&theme, 120, 48, &mut buffer, 1.0).unwrap();
    if component.wants_render() {
        component.paint(&theme, 120, 48, &mut buffer, 1.0).unwrap();
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
    assert!(flags.contains(ComponentDirtyFlags::LAYOUT));
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
</style>
"#,
    );
    assert!(component.module_styles_have_state_rules());

    assert_eq!(
        component.cached_restyle_state_dependencies,
        StyleStateDependencies {
            any: true,
            hover: true,
            focus: true,
            focus_visible: true,
            active: true,
            disabled: true,
            checked: true,
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
    component.previous_focused_key = Some("root/old".into());
    component.focused_key = Some("root/new".into());
    component.previous_active_key = Some("root/pressed".into());
    component.pointer_down_key = None;
    component
        .previous_checked_values
        .insert("root/check".into(), false);
    component.checked_values.insert("root/check".into(), true);

    assert!(
        component
            .collect_interaction_changed_node_ids()
            .affected
            .is_empty(),
        "focus, active, and checked changes cannot affect hover-only rules"
    );
}

// cargo test -p mesh-core-shell --release -- state_dependency_filter_skips_unrelated_targeted_restyle --ignored --nocapture
#[test]
#[ignore = "release-only selector dependency microbenchmark"]
fn state_dependency_filter_skips_unrelated_targeted_restyle() {
    use std::time::Instant;

    let mut source = String::from(r#"<template><row>"#);
    for _ in 0..250 {
        source.push_str(r#"<box class="plain" />"#);
    }
    source.push_str(
        r#"</row></template>
<style>
surface { width: 1000px; height: 80px; }
.plain { width: 4px; height: 40px; background: #222222; }
.plain:hover { background: #444444; }
</style>
"#,
    );
    let theme = default_theme();
    let iterations = 1_000;

    let run = |force_state_agnostic_focus: bool| {
        let mut component = test_frontend_component(&source);
        let mut buffer = PixelBuffer::new(1000, 80);
        component.paint(&theme, 1000, 80, &mut buffer, 1.0).unwrap();
        if component.wants_render() {
            component.paint(&theme, 1000, 80, &mut buffer, 1.0).unwrap();
        }
        if force_state_agnostic_focus {
            component.cached_restyle_state_dependencies.focus = true;
        }

        let started = Instant::now();
        for iteration in 0..iterations {
            component.focused_key = Some(if iteration % 2 == 0 {
                "root/0/0".into()
            } else {
                "root/0/249".into()
            });
            component.invalidate_interaction_restyle();
            component.paint(&theme, 1000, 80, &mut buffer, 1.0).unwrap();
        }
        started.elapsed()
    };

    let state_agnostic = run(true);
    let dependency_filtered = run(false);
    eprintln!(
        "hover-only rules across {iterations} focus moves: state-agnostic {state_agnostic:?}; dependency-filtered {dependency_filtered:?}; ratio {:.2}x",
        state_agnostic.as_secs_f64() / dependency_filtered.as_secs_f64()
    );
    assert!(dependency_filtered < state_agnostic);
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
                    local_components: HashMap::new(),
                    module_component_imports: HashMap::new(),
                    watched_paths: Vec::new(),
                },
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

// cargo test -p mesh-core-shell --release -- hover_without_state_rules_skips_repaint_benchmark --ignored --nocapture
#[test]
#[ignore = "release-only hover invalidation microbenchmark"]
fn hover_without_state_rules_skips_repaint_benchmark() {
    use std::time::Instant;

    let mut source = String::from(r#"<template><row>"#);
    for _ in 0..250 {
        source.push_str(r#"<box class="plain" />"#);
    }
    source.push_str(
        r#"</row></template>
<style>
surface { width: 1000px; height: 80px; }
.plain { width: 4px; height: 40px; background: #222222; }
</style>
"#,
    );
    let theme = default_theme();
    let iterations = 1_000;

    let run = |force_old_restyle: bool| {
        let mut component = test_frontend_component(&source);
        let mut buffer = PixelBuffer::new(1000, 80);
        component.paint(&theme, 1000, 80, &mut buffer, 1.0).unwrap();
        if component.wants_render() {
            component.paint(&theme, 1000, 80, &mut buffer, 1.0).unwrap();
        }
        let started = Instant::now();
        for iteration in 0..iterations {
            let x = if iteration % 2 == 0 { 2.0 } else { 998.0 };
            component
                .handle_input(&theme, 1000, 80, ComponentInput::PointerMove { x, y: 10.0 })
                .unwrap();
            if force_old_restyle {
                component.invalidate_interaction_restyle();
            }
            if component.wants_render() {
                component.paint(&theme, 1000, 80, &mut buffer, 1.0).unwrap();
            }
        }
        started.elapsed()
    };

    let old_time = run(true);
    let gated_time = run(false);
    eprintln!(
        "plain hover forced restyle: {old_time:?}; gated: {gated_time:?}; ratio: {:.1}x",
        old_time.as_secs_f64() / gated_time.as_secs_f64()
    );
    assert!(gated_time * 2 < old_time);
}

// cargo test -p mesh-core-shell --release -- hover_with_focus_only_rules_skips_repaint_benchmark --ignored --nocapture
#[test]
#[ignore = "release-only state-specific hover invalidation microbenchmark"]
fn hover_with_focus_only_rules_skips_repaint_benchmark() {
    use std::time::Instant;

    let mut source = String::from(r#"<template><row>"#);
    for _ in 0..250 {
        source.push_str(r#"<box class="plain" />"#);
    }
    source.push_str(
        r#"</row></template>
<style>
surface { width: 1000px; height: 80px; }
.plain { width: 4px; height: 40px; background: #222222; }
.plain:focus { background: #444444; }
</style>
"#,
    );
    let theme = default_theme();
    let iterations = 1_000;

    let run = |force_state_agnostic_restyle: bool| {
        let mut component = test_frontend_component(&source);
        let mut buffer = PixelBuffer::new(1000, 80);
        component.paint(&theme, 1000, 80, &mut buffer, 1.0).unwrap();
        if component.wants_render() {
            component.paint(&theme, 1000, 80, &mut buffer, 1.0).unwrap();
        }
        let started = Instant::now();
        for iteration in 0..iterations {
            let x = if iteration % 2 == 0 { 2.0 } else { 998.0 };
            component
                .handle_input(&theme, 1000, 80, ComponentInput::PointerMove { x, y: 10.0 })
                .unwrap();
            if force_state_agnostic_restyle {
                component.invalidate_interaction_restyle();
            }
            if component.wants_render() {
                component.paint(&theme, 1000, 80, &mut buffer, 1.0).unwrap();
            }
        }
        started.elapsed()
    };

    let old_time = run(true);
    let gated_time = run(false);
    eprintln!(
        "focus-only rules across {iterations} hover moves: state-agnostic {old_time:?}; hover-gated {gated_time:?}; ratio: {:.1}x",
        old_time.as_secs_f64() / gated_time.as_secs_f64()
    );
    assert!(gated_time * 2 < old_time);
}

// cargo test -p mesh-core-shell --release -- empty_interaction_diff_skips_full_restyle_benchmark --ignored --nocapture
#[test]
#[ignore = "release-only interaction restyle empty-diff microbenchmark"]
fn empty_interaction_diff_skips_full_restyle_benchmark() {
    use std::time::Instant;

    let mut source = String::from(r#"<template><row>"#);
    for _ in 0..250 {
        source.push_str(r#"<box class="plain" />"#);
    }
    source.push_str(
        r#"</row></template>
<style>
surface { width: 1000px; height: 80px; }
.plain { width: 4px; height: 40px; background: #222222; }
.plain:hover { background: #444444; }
</style>
"#,
    );
    let theme = default_theme();
    let iterations = 500;

    let run = |force_old_fallback: bool| {
        let mut component = test_frontend_component(&source);
        let mut buffer = PixelBuffer::new(1000, 80);
        component.paint(&theme, 1000, 80, &mut buffer, 1.0).unwrap();
        if component.wants_render() {
            component.paint(&theme, 1000, 80, &mut buffer, 1.0).unwrap();
        }

        let started = Instant::now();
        for _ in 0..iterations {
            component.previous_hovered_path.clear();
            component.hovered_path.clear();
            component.previous_focused_key = None;
            component.focused_key = None;
            component.interaction_snapshot_valid = !force_old_fallback;
            component.invalidate_interaction_restyle();
            component.paint(&theme, 1000, 80, &mut buffer, 1.0).unwrap();
        }
        started.elapsed()
    };

    let old_time = run(true);
    let no_op_time = run(false);
    eprintln!(
        "empty interaction diff full restyle fallback: {old_time:?}; no-op: {no_op_time:?}; ratio: {:.1}x",
        old_time.as_secs_f64() / no_op_time.as_secs_f64()
    );
    assert!(no_op_time * 2 < old_time);
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
    component.paint(&theme, 160, 48, &mut buffer, 1.0).unwrap();
    component.take_invalidation_snapshot();
    component.take_profiling_records();

    component.invalidate_interaction_restyle();
    component.paint(&theme, 160, 48, &mut buffer, 1.0).unwrap();
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
    component.paint(&theme, 160, 48, &mut buffer, 1.0).unwrap();
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
    component.paint(&theme, 160, 48, &mut buffer, 1.0).unwrap();
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
        slot_contributions: Default::default(),
    };
    let mut component = FrontendSurfaceComponent::new(
        compiled,
        module_dir.to_path_buf(),
        catalog,
        mesh_core_service::InterfaceCatalog::default(),
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
    component.paint(&theme, 240, 80, &mut buffer, 1.0).unwrap();
    assert_eq!(
        first_node_by_tag(component.last_tree.as_ref().unwrap(), "text")
            .unwrap()
            .attributes
            .get("content")
            .map(String::as_str),
        Some("before")
    );

    write_source(module_dir, "after");
    assert!(component.reload_source().unwrap());
    assert!(
        component.last_tree.is_none(),
        "source reload must drop the stale retained widget tree"
    );

    component.paint(&theme, 240, 80, &mut buffer, 1.0).unwrap();
    assert_eq!(
        first_node_by_tag(component.last_tree.as_ref().unwrap(), "text")
            .unwrap()
            .attributes
            .get("content")
            .map(String::as_str),
        Some("after")
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
    component.paint(&theme, 160, 48, &mut buffer, 1.0).unwrap();

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
    assert!(invalidation.text.shaping_micros > 0);
}
