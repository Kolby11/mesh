use super::*;
use std::collections::HashSet;

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
