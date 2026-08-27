use super::common::*;
use super::*;
use mesh_core_backend::BackendIdentity;

#[test]
fn profiling_session_reset_discards_previous_samples() {
    let mut shell = Shell::new();

    shell
        .apply_request(CoreRequest::ToggleDebugProfiling)
        .unwrap();
    shell.profiling.record_shell_stage(
        ProfilingStage::RuntimeUpdateHandling,
        std::time::Duration::from_micros(25),
        Some("service_update"),
    );
    shell.record_backend_profiling_stage(
        "mesh.audio",
        "@mesh/pulse",
        ProfilingBackendStage::PollUpdate,
        std::time::Duration::from_micros(9),
        Some("service_update"),
    );
    let snapshot = shell.build_debug_snapshot();
    let profiling = snapshot.profiling.expect("profiling should be enabled");
    assert_eq!(profiling.session_id, 1);
    assert_eq!(profiling.shell.stages.len(), 1);
    assert_eq!(profiling.backends.len(), 1);

    shell
        .apply_request(CoreRequest::ToggleDebugProfiling)
        .unwrap();
    shell
        .apply_request(CoreRequest::ToggleDebugProfiling)
        .unwrap();

    let reset_snapshot = shell.build_debug_snapshot();
    let profiling = reset_snapshot
        .profiling
        .expect("profiling should be enabled after the second toggle");
    assert_eq!(profiling.session_id, 2);
    assert!(
        profiling.shell.stages.is_empty(),
        "enabling a fresh profiling session must clear previous samples"
    );
    assert!(
        profiling.backends.is_empty(),
        "enabling a fresh profiling session must also clear backend samples"
    );
}

#[test]
fn profiling_snapshot_tracks_bounded_surface_samples_and_redraw_counts() {
    let mut shell = Shell::new();
    shell
        .apply_request(CoreRequest::ToggleDebugProfiling)
        .unwrap();

    shell.profiling.record_surface_stage(
        "@mesh/navigation-bar",
        Some("@mesh/navigation-bar"),
        ProfilingStage::TotalSurfaceRender,
        std::time::Duration::from_micros(120),
        Some("rebuild"),
    );
    shell.profiling.record_surface_redraw(
        "@mesh/navigation-bar",
        Some("@mesh/navigation-bar"),
        Some("rebuild"),
    );

    let snapshot = shell.build_debug_snapshot();
    let profiling = snapshot.profiling.expect("profiling should be enabled");
    let surface = profiling
        .surfaces
        .iter()
        .find(|surface| surface.surface_id == "@mesh/navigation-bar")
        .expect("surface snapshot should be recorded when work occurs");

    assert_eq!(surface.module_id.as_deref(), Some("@mesh/navigation-bar"));
    assert_eq!(surface.redraw_count, 1);
    assert_eq!(surface.total_surface_render_time_micros, 120);
    assert!(
        surface
            .stages
            .iter()
            .any(|stage| stage.stage == ProfilingStage::TotalSurfaceRender),
        "surface summaries must expose total surface render timing as a first-class stage"
    );
}

#[test]
fn profiling_snapshot_exposes_typed_surface_invalidation_counts() {
    let mut shell = Shell::new();
    shell
        .apply_request(CoreRequest::ToggleDebugProfiling)
        .unwrap();

    shell.record_surface_invalidation(
        "@mesh/navigation-bar",
        Some("@mesh/navigation-bar"),
        ProfilingInvalidationSnapshot {
            full_rebuild: false,
            retained_path: true,
            narrow_path: false,
            affected_node_count: 0,
            retained_generation: 7,
            component: ComponentInvalidationCounts {
                state: 1,
                style: 1,
                layout: 1,
                paint: 1,
                accessibility: 1,
                metrics: 1,
                ..Default::default()
            },
            retained: RetainedInvalidationCounts {
                style: 2,
                layout: 1,
                state: 1,
                ..Default::default()
            },
            paint: RetainedPaintSnapshot {
                entries_total: 5,
                entries_reused: 3,
                entries_rebuilt: 2,
                damage_area: 120,
                surface_area: 1_000,
                partial_present_supported: false,
                skipped_paint_pixels: 0,
                omitted_subtrees: 2,
                omitted_nodes: 5,
                omitted_commands: 10,
                preclipped_descendants: 4,
                repaint_policy: RepaintPolicySnapshot::BoundingRect,
                filtered_span_count: 3,
                filtered_command_count: 4,
                filtered_commands_skipped: 1,
                batch_count: 2,
                batched_primitives: 5,
                barrier_count: 3,
                barriers: DisplayBatchBarrierSnapshot {
                    text: 1,
                    material_change: 2,
                    ..Default::default()
                },
                raster_cache_hits: 8,
                raster_cache_misses: 2,
                raster_cache_bypasses: 1,
                raster_cache_opaque_hits: 5,
                raster_cache_translucent_hits: 3,
                glyph_cache_hits: 12,
                glyph_cache_misses: 2,
                glyph_cache_entries: 900,
                glyph_cache_capacity: 1_024,
                font_bytes_cache_hits: 2,
                font_bytes_cache_misses: 1,
                font_bytes_cache_entries: 20,
                font_bytes_cache_capacity: 32,
                skia_glyph_cache_hits: 7,
                skia_glyph_cache_misses: 3,
                skia_glyph_cache_entries: 400,
                skia_glyph_cache_capacity: 512,
                ..Default::default()
            },
            text: TextCacheSnapshot {
                layout_hits: 4,
                layout_misses: 1,
                shaped_entries: 1,
                glyph_cache_active: true,
                ..Default::default()
            },
        },
    );

    let snapshot = shell.build_debug_snapshot();
    let profiling = snapshot.profiling.expect("profiling should be enabled");
    let surface = profiling
        .surfaces
        .iter()
        .find(|surface| surface.surface_id == "@mesh/navigation-bar")
        .expect("surface snapshot should be recorded when invalidation occurs");
    let invalidation = surface
        .invalidation
        .as_ref()
        .expect("surface profiling should carry typed invalidation counts");

    assert!(!invalidation.full_rebuild);
    assert!(invalidation.retained_path);
    assert_eq!(invalidation.retained_generation, 7);
    assert_eq!(invalidation.component.style, 1);
    assert_eq!(invalidation.component.text, 0);
    assert_eq!(invalidation.retained.style, 2);
    assert_eq!(invalidation.retained.layout, 1);
    assert_eq!(invalidation.paint.entries_total, 5);
    assert_eq!(invalidation.paint.damage_area, 120);
    assert!(!invalidation.paint.partial_present_supported);
    assert_eq!(invalidation.paint.skipped_paint_pixels, 0);
    assert_eq!(invalidation.paint.omitted_subtrees, 2);
    assert_eq!(invalidation.paint.omitted_nodes, 5);
    assert_eq!(invalidation.paint.omitted_commands, 10);
    assert_eq!(invalidation.paint.preclipped_descendants, 4);
    assert_eq!(
        invalidation.paint.repaint_policy,
        RepaintPolicySnapshot::BoundingRect
    );
    assert_eq!(invalidation.paint.filtered_span_count, 3);
    assert_eq!(invalidation.paint.filtered_command_count, 4);
    assert_eq!(invalidation.paint.filtered_commands_skipped, 1);
    assert_eq!(invalidation.paint.filtered_fallback_count, 0);
    assert_eq!(invalidation.paint.batch_count, 2);
    assert_eq!(invalidation.paint.batched_primitives, 5);
    assert_eq!(invalidation.paint.barriers.text, 1);
    assert_eq!(invalidation.paint.barriers.material_change, 2);
    assert_eq!(invalidation.paint.raster_cache_hits, 8);
    assert_eq!(invalidation.paint.raster_cache_misses, 2);
    assert_eq!(invalidation.paint.raster_cache_bypasses, 1);
    assert_eq!(invalidation.paint.raster_cache_opaque_hits, 5);
    assert_eq!(invalidation.paint.raster_cache_translucent_hits, 3);
    assert_eq!(invalidation.paint.glyph_cache_hits, 12);
    assert_eq!(invalidation.paint.glyph_cache_entries, 900);
    assert_eq!(invalidation.paint.glyph_cache_capacity, 1_024);
    assert_eq!(invalidation.paint.font_bytes_cache_entries, 20);
    assert_eq!(invalidation.paint.skia_glyph_cache_misses, 3);
    assert_eq!(invalidation.paint.skia_glyph_cache_capacity, 512);
    assert_eq!(invalidation.text.layout_hits, 4);
    assert_eq!(invalidation.text.layout_misses, 1);
    assert!(invalidation.text.glyph_cache_active);
}

#[test]
fn profiling_stage_surface_records_roll_up_into_shell_summary() {
    let mut shell = Shell::new();
    shell
        .apply_request(CoreRequest::ToggleDebugProfiling)
        .unwrap();

    shell.record_surface_profiling_stage(
        "@mesh/navigation-bar",
        Some("@mesh/navigation-bar"),
        ProfilingStage::TreeBuild,
        std::time::Duration::from_micros(30),
        Some("rebuild"),
    );
    shell.record_surface_profiling_stage(
        "@mesh/navigation-bar",
        Some("@mesh/navigation-bar"),
        ProfilingStage::Layout,
        std::time::Duration::from_micros(45),
        Some("rebuild"),
    );
    shell.record_surface_profiling_stage(
        "@mesh/navigation-bar",
        Some("@mesh/navigation-bar"),
        ProfilingStage::PresentCommit,
        std::time::Duration::from_micros(12),
        Some("present"),
    );

    let snapshot = shell.build_debug_snapshot();
    let profiling = snapshot.profiling.expect("profiling should be enabled");

    let shell_stages: std::collections::HashMap<_, _> = profiling
        .shell
        .stages
        .iter()
        .map(|stage| (stage.stage, stage.total_micros))
        .collect();
    assert_eq!(shell_stages.get(&ProfilingStage::TreeBuild), Some(&30));
    assert_eq!(shell_stages.get(&ProfilingStage::Layout), Some(&45));
    assert_eq!(shell_stages.get(&ProfilingStage::PresentCommit), Some(&12));
}

#[test]
fn profiling_surface_snapshot_preserves_surface_and_module_identity_with_comparable_totals() {
    let mut shell = Shell::new();
    shell
        .apply_request(CoreRequest::ToggleDebugProfiling)
        .unwrap();

    shell.record_surface_profiling_stage(
        "@mesh/navigation-bar",
        Some("@mesh/navigation-bar"),
        ProfilingStage::Layout,
        std::time::Duration::from_micros(45),
        Some("rebuild"),
    );
    shell.record_surface_profiling_stage(
        "@mesh/navigation-bar",
        Some("@mesh/navigation-bar"),
        ProfilingStage::Paint,
        std::time::Duration::from_micros(30),
        Some("rebuild"),
    );

    let snapshot = shell.build_debug_snapshot();
    let profiling = snapshot.profiling.expect("profiling should be enabled");
    let surface = profiling
        .surfaces
        .iter()
        .find(|surface| surface.surface_id == "@mesh/navigation-bar")
        .expect("worked surfaces must be keyed by surface_id");
    let shell_stages: std::collections::HashMap<_, _> = profiling
        .shell
        .stages
        .iter()
        .map(|stage| (stage.stage, stage.total_micros))
        .collect();
    let surface_stages: std::collections::HashMap<_, _> = surface
        .stages
        .iter()
        .map(|stage| (stage.stage, stage.total_micros))
        .collect();

    assert_eq!(surface.surface_id, "@mesh/navigation-bar");
    assert_eq!(surface.module_id.as_deref(), Some("@mesh/navigation-bar"));
    assert_eq!(shell_stages.get(&ProfilingStage::Layout), Some(&45));
    assert_eq!(surface_stages.get(&ProfilingStage::Layout), Some(&45));
    assert_eq!(shell_stages.get(&ProfilingStage::Paint), Some(&30));
    assert_eq!(surface_stages.get(&ProfilingStage::Paint), Some(&30));
}

#[test]
fn profiling_disabled_runtime_stage_helpers_remain_inert() {
    let mut shell = Shell::new();

    shell.record_shell_profiling_stage(
        ProfilingStage::InputHandling,
        std::time::Duration::from_micros(10),
        Some("key"),
    );
    shell.record_surface_profiling_stage(
        "@mesh/navigation-bar",
        Some("@mesh/navigation-bar"),
        ProfilingStage::Paint,
        std::time::Duration::from_micros(20),
        Some("rebuild"),
    );
    shell.record_surface_redraw(
        "@mesh/navigation-bar",
        Some("@mesh/navigation-bar"),
        Some("present"),
    );
    shell.record_backend_profiling_stage(
        "mesh.audio",
        "@mesh/pulse",
        ProfilingBackendStage::CommandHandling,
        std::time::Duration::from_micros(8),
        Some("service_command"),
    );

    let snapshot = shell.build_debug_snapshot();
    assert!(
        snapshot.profiling.is_none(),
        "profiling-disabled helpers must not fabricate shell or surface snapshots"
    );
}

#[test]
fn profiling_disabled_backend_paths_do_not_fabricate_snapshots() {
    let runtime = Runtime::new().unwrap();
    let mut shell = Shell::new();
    shell
        .interfaces
        .register_contract(test_contract("mesh.audio"));
    register_test_provider(&shell.interfaces, "mesh.audio", "@mesh/pipewire-audio");
    let (slot, mut rx) = backend_runtime_slot(&runtime, "mesh.audio", "@mesh/pipewire-audio");
    shell.replace_backend_runtime("mesh.audio".to_string(), slot);
    let mut pending = std::collections::VecDeque::new();
    let mut capabilities = mesh_core_capability::CapabilitySet::new();
    capabilities.grant(mesh_core_capability::Capability::new(
        "service.audio.control",
    ));

    shell
        .handle_shell_message(
            &mut pending,
            super::types::ShellMessage::BackendServiceUpdate {
                interface: "mesh.audio".to_string(),
                provider_id: "@mesh/pipewire-audio".to_string(),
                identity: BackendIdentity::default(),
                event: service_update(
                    "mesh.audio",
                    "@mesh/pipewire-audio",
                    serde_json::json!({ "available": true, "percent": 44.0 }),
                ),
            },
        )
        .unwrap();
    shell
        .broadcast_service_event(service_update(
            "mesh.audio",
            "@mesh/pipewire-audio",
            serde_json::json!({ "available": true, "percent": 45.0 }),
        ))
        .unwrap();

    let result = shell.dispatch_service_command(
        "mesh.audio",
        "set_volume",
        &serde_json::json!({ "percent": 40 }),
        "@mesh/panel",
        &capabilities,
    );

    assert_eq!(result["ok"], serde_json::json!(true));
    assert_eq!(rx.try_recv().unwrap().command, "set_volume");
    assert!(
        shell.build_debug_snapshot().profiling.is_none(),
        "profiling-disabled backend attribution paths must stay silent"
    );
}

#[test]
fn profiling_snapshot_tracks_bounded_backend_samples_by_provider() {
    let mut shell = Shell::new();
    shell
        .apply_request(CoreRequest::ToggleDebugProfiling)
        .unwrap();

    for index in 0..20 {
        shell.record_backend_profiling_stage(
            "mesh.audio",
            "@mesh/pulse",
            ProfilingBackendStage::PollUpdate,
            std::time::Duration::from_micros(10 + index),
            Some("service_update"),
        );
    }
    shell.record_backend_profiling_stage(
        "mesh.audio",
        "@mesh/pulse",
        ProfilingBackendStage::CommandHandling,
        std::time::Duration::from_micros(44),
        Some("service_command"),
    );
    shell.record_backend_profiling_stage(
        "mesh.audio",
        "@mesh/pulse",
        ProfilingBackendStage::StatePublishDelivery,
        std::time::Duration::from_micros(55),
        Some("service_publish"),
    );
    shell.record_backend_profiling_stage(
        "mesh.network",
        "@mesh/networkmanager",
        ProfilingBackendStage::PollUpdate,
        std::time::Duration::from_micros(33),
        Some("service_update"),
    );

    let snapshot = shell.build_debug_snapshot();
    let profiling = snapshot.profiling.expect("profiling should be enabled");

    assert_eq!(profiling.backends.len(), 2);

    let audio_backend = profiling
        .backends
        .iter()
        .find(|backend| backend.interface == "mesh.audio" && backend.provider_id == "@mesh/pulse")
        .expect("backend profiling should be keyed by interface and provider");

    let poll_update = audio_backend
        .stages
        .iter()
        .find(|stage| stage.stage == ProfilingBackendStage::PollUpdate)
        .expect("poll/update stage should be captured");
    assert_eq!(poll_update.sample_count, 20);
    assert_eq!(poll_update.max_micros, 29);
    assert_eq!(poll_update.recent_samples.len(), 16);
    assert_eq!(
        poll_update
            .recent_samples
            .first()
            .map(|sample| sample.order),
        Some(4),
        "backend recent samples should retain only the newest bounded window"
    );
    assert!(
        poll_update
            .recent_samples
            .iter()
            .all(|sample| sample.stage == ProfilingBackendStage::PollUpdate)
    );

    assert!(
        audio_backend
            .stages
            .iter()
            .any(|stage| stage.stage == ProfilingBackendStage::CommandHandling)
    );
    assert!(
        audio_backend
            .stages
            .iter()
            .any(|stage| stage.stage == ProfilingBackendStage::StatePublishDelivery)
    );
}

#[test]
fn profiling_snapshot_groups_backend_stage_proof_under_expected_provider_identity() {
    let runtime = Runtime::new().unwrap();
    let mut shell = Shell::new();
    shell
        .apply_request(CoreRequest::ToggleDebugProfiling)
        .unwrap();
    shell
        .interfaces
        .register_contract(test_contract("mesh.audio"));
    register_test_provider(&shell.interfaces, "mesh.audio", "@mesh/pipewire-audio");
    let (slot, mut rx) = backend_runtime_slot(&runtime, "mesh.audio", "@mesh/pipewire-audio");
    shell.replace_backend_runtime("mesh.audio".to_string(), slot);
    let mut pending = std::collections::VecDeque::new();
    let mut capabilities = mesh_core_capability::CapabilitySet::new();
    capabilities.grant(mesh_core_capability::Capability::new(
        "service.audio.control",
    ));

    shell
        .handle_shell_message(
            &mut pending,
            super::types::ShellMessage::BackendServiceUpdate {
                interface: "mesh.audio".to_string(),
                provider_id: "@mesh/pipewire-audio".to_string(),
                identity: BackendIdentity::default(),
                event: service_update(
                    "mesh.audio",
                    "@mesh/pipewire-audio",
                    serde_json::json!({ "available": true, "percent": 44.0 }),
                ),
            },
        )
        .unwrap();
    shell
        .broadcast_service_event(service_update(
            "mesh.audio",
            "@mesh/pipewire-audio",
            serde_json::json!({ "available": true, "percent": 45.0 }),
        ))
        .unwrap();

    let result = shell.dispatch_service_command(
        "mesh.audio",
        "set_volume",
        &serde_json::json!({ "percent": 40 }),
        "@mesh/panel",
        &capabilities,
    );

    assert_eq!(result["ok"], serde_json::json!(true));
    assert_eq!(rx.try_recv().unwrap().command, "set_volume");

    let snapshot = shell.build_debug_snapshot();
    let profiling = snapshot.profiling.expect("profiling should be enabled");
    let backend = profiling
        .backends
        .iter()
        .find(|backend| {
            backend.interface == "mesh.audio" && backend.provider_id == "@mesh/pipewire-audio"
        })
        .expect("backend stages should stay grouped under the accepted provider identity");
    let stages: std::collections::HashSet<_> =
        backend.stages.iter().map(|stage| stage.stage).collect();

    assert_eq!(backend.interface, "mesh.audio");
    assert_eq!(backend.provider_id, "@mesh/pipewire-audio");
    assert!(stages.contains(&ProfilingBackendStage::PollUpdate));
    assert!(stages.contains(&ProfilingBackendStage::CommandHandling));
    assert!(stages.contains(&ProfilingBackendStage::StatePublishDelivery));
}

#[test]
fn profiling_snapshot_includes_required_shell_stage_buckets() {
    let mut shell = Shell::new();
    shell
        .apply_request(CoreRequest::ToggleDebugProfiling)
        .unwrap();

    shell.record_shell_profiling_stage(
        ProfilingStage::InputHandling,
        std::time::Duration::from_micros(11),
        Some("key"),
    );
    shell.record_shell_profiling_stage(
        ProfilingStage::RuntimeUpdateHandling,
        std::time::Duration::from_micros(12),
        Some("service_event"),
    );
    shell.record_surface_profiling_stage(
        "@mesh/navigation-bar",
        Some("@mesh/navigation-bar"),
        ProfilingStage::TreeBuild,
        std::time::Duration::from_micros(13),
        Some("rebuild"),
    );
    shell.record_surface_profiling_stage(
        "@mesh/navigation-bar",
        Some("@mesh/navigation-bar"),
        ProfilingStage::StyleRestyle,
        std::time::Duration::from_micros(14),
        Some("rebuild"),
    );
    shell.record_surface_profiling_stage(
        "@mesh/navigation-bar",
        Some("@mesh/navigation-bar"),
        ProfilingStage::Layout,
        std::time::Duration::from_micros(15),
        Some("rebuild"),
    );
    shell.record_surface_profiling_stage(
        "@mesh/navigation-bar",
        Some("@mesh/navigation-bar"),
        ProfilingStage::RenderObjectSync,
        std::time::Duration::from_micros(16),
        Some("rebuild"),
    );
    shell.record_surface_profiling_stage(
        "@mesh/navigation-bar",
        Some("@mesh/navigation-bar"),
        ProfilingStage::RetainedDisplayListUpdate,
        std::time::Duration::from_micros(17),
        Some("rebuild"),
    );
    shell.record_surface_profiling_stage(
        "@mesh/navigation-bar",
        Some("@mesh/navigation-bar"),
        ProfilingStage::PaintTraversal,
        std::time::Duration::from_micros(18),
        Some("rebuild"),
    );
    shell.record_surface_profiling_stage(
        "@mesh/navigation-bar",
        Some("@mesh/navigation-bar"),
        ProfilingStage::TextShaping,
        std::time::Duration::from_micros(19),
        Some("rebuild"),
    );
    shell.record_surface_profiling_stage(
        "@mesh/navigation-bar",
        Some("@mesh/navigation-bar"),
        ProfilingStage::IconImageRaster,
        std::time::Duration::from_micros(20),
        Some("rebuild"),
    );
    shell.record_surface_profiling_stage(
        "@mesh/navigation-bar",
        Some("@mesh/navigation-bar"),
        ProfilingStage::Paint,
        std::time::Duration::from_micros(21),
        Some("rebuild"),
    );
    shell.record_surface_profiling_stage(
        "@mesh/navigation-bar",
        Some("@mesh/navigation-bar"),
        ProfilingStage::PresentCommit,
        std::time::Duration::from_micros(22),
        Some("present"),
    );
    shell.record_surface_redraw(
        "@mesh/navigation-bar",
        Some("@mesh/navigation-bar"),
        Some("present"),
    );
    shell.record_surface_profiling_stage(
        "@mesh/navigation-bar",
        Some("@mesh/navigation-bar"),
        ProfilingStage::TotalSurfaceRender,
        std::time::Duration::from_micros(23),
        Some("rebuild"),
    );

    let snapshot = shell.build_debug_snapshot();
    let profiling = snapshot.profiling.expect("profiling should be enabled");
    let stages: std::collections::HashSet<_> = profiling
        .shell
        .stages
        .iter()
        .map(|stage| stage.stage)
        .collect();

    assert!(stages.contains(&ProfilingStage::InputHandling));
    assert!(stages.contains(&ProfilingStage::RuntimeUpdateHandling));
    assert!(stages.contains(&ProfilingStage::TreeBuild));
    assert!(stages.contains(&ProfilingStage::StyleRestyle));
    assert!(stages.contains(&ProfilingStage::Layout));
    assert!(stages.contains(&ProfilingStage::RenderObjectSync));
    assert!(stages.contains(&ProfilingStage::RetainedDisplayListUpdate));
    assert!(stages.contains(&ProfilingStage::PaintTraversal));
    assert!(stages.contains(&ProfilingStage::TextShaping));
    assert!(stages.contains(&ProfilingStage::IconImageRaster));
    assert!(stages.contains(&ProfilingStage::Paint));
    assert!(stages.contains(&ProfilingStage::PresentCommit));
    assert!(stages.contains(&ProfilingStage::RedrawCount));
    assert!(stages.contains(&ProfilingStage::TotalSurfaceRender));
}

#[test]
fn profiling_debug_payload_serializes_phase26_surface_attribution_labels() {
    let mut shell = Shell::new();
    shell
        .apply_request(CoreRequest::ToggleDebugProfiling)
        .unwrap();

    shell.record_surface_profiling_stage(
        "@mesh/navigation-bar",
        Some("@mesh/navigation-bar"),
        ProfilingStage::RenderObjectSync,
        std::time::Duration::from_micros(31),
        Some("hover"),
    );
    shell.record_surface_profiling_stage(
        "@mesh/navigation-bar",
        Some("@mesh/navigation-bar"),
        ProfilingStage::RetainedDisplayListUpdate,
        std::time::Duration::from_micros(32),
        Some("hover"),
    );
    shell.record_surface_profiling_stage(
        "@mesh/navigation-bar",
        Some("@mesh/navigation-bar"),
        ProfilingStage::PaintTraversal,
        std::time::Duration::from_micros(33),
        Some("hover"),
    );
    shell.record_surface_invalidation(
        "@mesh/navigation-bar",
        Some("@mesh/navigation-bar"),
        ProfilingInvalidationSnapshot {
            paint: RetainedPaintSnapshot {
                subtree_segments_reused: 7,
                subtree_segments_rebuilt: 2,
                subtree_commands_rebuilt: 5,
                full_fallback_count: 1,
                broad_dirty_fallback_count: 1,
                repaint_policy: RepaintPolicySnapshot::FullSurface,
                filtered_span_count: 4,
                filtered_command_count: 9,
                filtered_commands_skipped: 0,
                filtered_fallback_count: 1,
                raster_cache_hits: 6,
                raster_cache_misses: 2,
                raster_cache_bypasses: 1,
                raster_cache_opaque_hits: 4,
                raster_cache_translucent_hits: 2,
                glyph_cache_hits: 11,
                glyph_cache_misses: 3,
                glyph_cache_entries: 700,
                glyph_cache_capacity: 1_024,
                font_bytes_cache_entries: 18,
                font_bytes_cache_capacity: 32,
                skia_glyph_cache_entries: 250,
                skia_glyph_cache_capacity: 512,
                ..Default::default()
            },
            text: TextCacheSnapshot {
                shaping_micros: 34,
                ..Default::default()
            },
            narrow_path: true,
            affected_node_count: 5,
            component: ComponentInvalidationCounts {
                script_narrow: 2,
                ..Default::default()
            },
            ..Default::default()
        },
    );

    shell.build_debug_snapshot();

    let latest = shell
        .latest_service_state
        .get(mesh_core_debug::DEBUG_INTERFACE)
        .expect("mesh.debug state should be published");
    let stages = latest.state["profiling"]["surfaces"][0]["stages"]
        .as_array()
        .expect("surface stages should serialize as an array");
    let labels: std::collections::HashSet<_> = stages
        .iter()
        .filter_map(|stage| stage["stage"].as_str())
        .collect();

    assert!(labels.contains("render_object_sync"));
    assert!(labels.contains("retained_display_list_update"));
    assert!(labels.contains("paint_traversal"));
    assert_eq!(
        latest.state["profiling"]["surfaces"][0]["invalidation"]["text"]["shaping_micros"],
        serde_json::json!(34)
    );
    assert_eq!(
        latest.state["schema_version"],
        serde_json::json!(mesh_core_debug::DEBUG_TELEMETRY_SCHEMA_VERSION)
    );
    assert_eq!(
        latest.state["profiling"]["surfaces"][0]["invalidation"]["narrow_path"],
        serde_json::json!(true)
    );
    assert_eq!(
        latest.state["profiling"]["surfaces"][0]["invalidation"]["affected_node_count"],
        serde_json::json!(5)
    );
    assert_eq!(
        latest.state["profiling"]["surfaces"][0]["invalidation"]["component"]["script_narrow"],
        serde_json::json!(2)
    );
    assert_eq!(
        latest.state["profiling"]["surfaces"][0]["invalidation"]["paint"]["omitted_subtrees"],
        serde_json::json!(0)
    );
    assert_eq!(
        latest.state["profiling"]["surfaces"][0]["invalidation"]["paint"]["subtree_segments_reused"],
        serde_json::json!(7)
    );
    assert_eq!(
        latest.state["profiling"]["surfaces"][0]["invalidation"]["paint"]["subtree_segments_rebuilt"],
        serde_json::json!(2)
    );
    assert_eq!(
        latest.state["profiling"]["surfaces"][0]["invalidation"]["paint"]["subtree_commands_rebuilt"],
        serde_json::json!(5)
    );
    assert_eq!(
        latest.state["profiling"]["surfaces"][0]["invalidation"]["paint"]["full_fallback_count"],
        serde_json::json!(1)
    );
    assert_eq!(
        latest.state["profiling"]["surfaces"][0]["invalidation"]["paint"]["broad_dirty_fallback_count"],
        serde_json::json!(1)
    );
    assert_eq!(
        latest.state["profiling"]["surfaces"][0]["invalidation"]["paint"]["repaint_policy"],
        serde_json::json!("full_surface")
    );
    assert_eq!(
        latest.state["profiling"]["surfaces"][0]["invalidation"]["paint"]["filtered_span_count"],
        serde_json::json!(4)
    );
    assert_eq!(
        latest.state["profiling"]["surfaces"][0]["invalidation"]["paint"]["filtered_command_count"],
        serde_json::json!(9)
    );
    assert_eq!(
        latest.state["profiling"]["surfaces"][0]["invalidation"]["paint"]["filtered_commands_skipped"],
        serde_json::json!(0)
    );
    assert_eq!(
        latest.state["profiling"]["surfaces"][0]["invalidation"]["paint"]["filtered_fallback_count"],
        serde_json::json!(1)
    );
    assert_eq!(
        latest.state["profiling"]["surfaces"][0]["invalidation"]["paint"]["raster_cache_hits"],
        serde_json::json!(6)
    );
    assert_eq!(
        latest.state["profiling"]["surfaces"][0]["invalidation"]["paint"]["raster_cache_misses"],
        serde_json::json!(2)
    );
    assert_eq!(
        latest.state["profiling"]["surfaces"][0]["invalidation"]["paint"]["raster_cache_bypasses"],
        serde_json::json!(1)
    );
    assert_eq!(
        latest.state["profiling"]["surfaces"][0]["invalidation"]["paint"]["raster_cache_opaque_hits"],
        serde_json::json!(4)
    );
    assert_eq!(
        latest.state["profiling"]["surfaces"][0]["invalidation"]["paint"]["raster_cache_translucent_hits"],
        serde_json::json!(2)
    );
    assert_eq!(
        latest.state["profiling"]["surfaces"][0]["invalidation"]["paint"]["glyph_cache_hits"],
        serde_json::json!(11)
    );
    assert_eq!(
        latest.state["profiling"]["surfaces"][0]["invalidation"]["paint"]["glyph_cache_capacity"],
        serde_json::json!(1_024)
    );
    assert_eq!(
        latest.state["profiling"]["surfaces"][0]["invalidation"]["paint"]["font_bytes_cache_entries"],
        serde_json::json!(18)
    );
    assert_eq!(
        latest.state["profiling"]["surfaces"][0]["invalidation"]["paint"]["skia_glyph_cache_capacity"],
        serde_json::json!(512)
    );
    assert_eq!(
        latest.state["benchmarks"]["scenarios"]
            .as_array()
            .expect("benchmark scenarios should stay serialized")
            .len(),
        12
    );
}

#[test]
fn phase26_baseline_proof_records_canonical_scenario_values_and_retained_hotspots() {
    let mut shell = Shell::new();
    shell
        .apply_request(CoreRequest::ToggleDebugProfiling)
        .unwrap();

    shell.record_surface_profiling_stage(
        "@mesh/navigation-bar",
        Some("@mesh/navigation-bar"),
        ProfilingStage::InputHandling,
        std::time::Duration::from_micros(24),
        Some("phase26_prechange"),
    );
    shell.record_surface_profiling_stage(
        "@mesh/navigation-bar",
        Some("@mesh/navigation-bar"),
        ProfilingStage::StyleRestyle,
        std::time::Duration::from_micros(61),
        Some("phase26_prechange"),
    );
    shell.record_surface_profiling_stage(
        "@mesh/navigation-bar",
        Some("@mesh/navigation-bar"),
        ProfilingStage::RuntimeUpdateHandling,
        std::time::Duration::from_micros(42),
        Some("phase26_prechange"),
    );
    shell.record_surface_profiling_stage(
        "@mesh/navigation-bar",
        Some("@mesh/navigation-bar"),
        ProfilingStage::Layout,
        std::time::Duration::from_micros(94),
        Some("phase26_prechange"),
    );
    shell.record_surface_profiling_stage(
        "@mesh/navigation-bar",
        Some("@mesh/navigation-bar"),
        ProfilingStage::Paint,
        std::time::Duration::from_micros(149),
        Some("phase26_prechange"),
    );
    shell.record_surface_profiling_stage(
        "@mesh/navigation-bar",
        Some("@mesh/navigation-bar"),
        ProfilingStage::TotalSurfaceRender,
        std::time::Duration::from_micros(214),
        Some("phase26_prechange"),
    );

    shell.record_surface_profiling_stage(
        "@mesh/navigation-bar",
        Some("@mesh/navigation-bar"),
        ProfilingStage::RenderObjectSync,
        std::time::Duration::from_micros(34),
        Some("phase26_post"),
    );
    shell.record_surface_profiling_stage(
        "@mesh/navigation-bar",
        Some("@mesh/navigation-bar"),
        ProfilingStage::RetainedDisplayListUpdate,
        std::time::Duration::from_micros(57),
        Some("phase26_post"),
    );
    shell.record_surface_profiling_stage(
        "@mesh/navigation-bar",
        Some("@mesh/navigation-bar"),
        ProfilingStage::PaintTraversal,
        std::time::Duration::from_micros(91),
        Some("phase26_post"),
    );
    shell.record_surface_profiling_stage(
        "@mesh/navigation-bar",
        Some("@mesh/navigation-bar"),
        ProfilingStage::TextShaping,
        std::time::Duration::from_micros(12),
        Some("phase26_post"),
    );
    shell.record_surface_profiling_stage(
        "@mesh/navigation-bar",
        Some("@mesh/navigation-bar"),
        ProfilingStage::IconImageRaster,
        std::time::Duration::from_micros(6),
        Some("phase26_post"),
    );
    shell.record_surface_invalidation(
        "@mesh/navigation-bar",
        Some("@mesh/navigation-bar"),
        ProfilingInvalidationSnapshot {
            text: TextCacheSnapshot {
                shaping_micros: 12,
                ..Default::default()
            },
            ..Default::default()
        },
    );

    shell.record_surface_redraw(
        "@mesh/audio-popover",
        Some("@mesh/audio-popover"),
        Some("phase26_prechange"),
    );
    shell.record_surface_redraw(
        "@mesh/audio-popover",
        Some("@mesh/audio-popover"),
        Some("phase26_prechange"),
    );
    shell.record_surface_redraw(
        "@mesh/audio-popover",
        Some("@mesh/audio-popover"),
        Some("phase26_prechange"),
    );
    shell.record_surface_profiling_stage(
        "@mesh/audio-popover",
        Some("@mesh/audio-popover"),
        ProfilingStage::TotalSurfaceRender,
        std::time::Duration::from_micros(188),
        Some("phase26_prechange"),
    );

    shell.record_backend_runtime_status(
        "mesh.audio".to_string(),
        "@mesh/pipewire-audio".to_string(),
        BackendRuntimeStatus::Running,
        "phase26 benchmark runtime".to_string(),
    );
    shell.record_backend_profiling_stage(
        "mesh.audio",
        "@mesh/pipewire-audio",
        ProfilingBackendStage::StatePublishDelivery,
        std::time::Duration::from_micros(73),
        Some("phase26_prechange"),
    );

    let snapshot = shell.build_debug_snapshot();
    let scenario_by_id = |id: &str| -> &mesh_core_debug::BenchmarkScenarioSnapshot {
        snapshot
            .benchmarks
            .scenarios
            .iter()
            .find(|scenario| scenario.id.id() == id)
            .expect("benchmark scenario should exist")
    };

    let hover = scenario_by_id("hover");
    assert_eq!(hover.primary_metric, "input_handling: 1 samples, max 24us");
    assert_eq!(hover.secondary_metric, "style_restyle: 1 samples, max 61us");

    let surface_open_close = scenario_by_id("surface_open_close");
    assert_eq!(
        surface_open_close.primary_metric,
        "total_surface_render: 188us"
    );
    assert_eq!(surface_open_close.secondary_metric, "redraw_count: 3");

    let pointer_update = scenario_by_id("pointer_update");
    assert_eq!(
        pointer_update.primary_metric,
        "input_handling: 1 samples, max 24us"
    );
    assert_eq!(
        pointer_update.secondary_metric,
        "layout: 1 samples, max 94us"
    );

    let keyboard_traversal = scenario_by_id("keyboard_traversal");
    assert_eq!(
        keyboard_traversal.primary_metric,
        "input_handling: 1 samples, max 24us"
    );
    assert_eq!(
        keyboard_traversal.secondary_metric,
        "total_surface_render: 1 samples, max 214us"
    );

    let backend_update = scenario_by_id("backend_update");
    assert_eq!(
        backend_update.primary_metric,
        "mesh.audio -> @mesh/pipewire-audio state_publish_delivery: 1 samples, max 73us"
    );
    assert_eq!(
        backend_update.secondary_metric,
        "frontend total_surface_render: 214us"
    );

    let profiling = snapshot
        .profiling
        .as_ref()
        .expect("profiling should be enabled for phase 26 baseline proof");
    let navigation_bar = profiling
        .surfaces
        .iter()
        .find(|surface| surface.surface_id == "@mesh/navigation-bar")
        .expect("navigation bar surface sample should be recorded");
    let retained_hotspots: Vec<_> = navigation_bar
        .stages
        .iter()
        .filter_map(|stage| match stage.stage {
            ProfilingStage::PaintTraversal
            | ProfilingStage::RetainedDisplayListUpdate
            | ProfilingStage::RenderObjectSync
            | ProfilingStage::TextShaping
            | ProfilingStage::IconImageRaster => Some((stage.stage, stage.max_micros)),
            _ => None,
        })
        .collect();

    assert_eq!(
        retained_hotspots,
        vec![
            (ProfilingStage::RenderObjectSync, 34),
            (ProfilingStage::RetainedDisplayListUpdate, 57),
            (ProfilingStage::PaintTraversal, 91),
            (ProfilingStage::TextShaping, 12),
            (ProfilingStage::IconImageRaster, 6),
        ]
    );
}

#[test]
fn profiling_snapshot_uses_surface_id_as_canonical_key_and_skips_unworked_surfaces() {
    let mut shell = Shell::new();
    shell
        .apply_request(CoreRequest::ToggleDebugProfiling)
        .unwrap();
    shell.record_shell_profiling_stage(
        ProfilingStage::InputHandling,
        std::time::Duration::from_micros(9),
        Some("key"),
    );

    let snapshot = shell.build_debug_snapshot();
    let profiling = snapshot.profiling.expect("profiling should be enabled");
    assert!(
        profiling.surfaces.is_empty(),
        "shell-only work must not fabricate per-surface summaries"
    );

    shell.record_surface_profiling_stage(
        "@mesh/audio-popover",
        Some("@mesh/audio-popover"),
        ProfilingStage::Paint,
        std::time::Duration::from_micros(22),
        Some("rebuild"),
    );
    shell.record_surface_redraw(
        "@mesh/audio-popover",
        Some("@mesh/audio-popover"),
        Some("present"),
    );

    let snapshot = shell.build_debug_snapshot();
    let profiling = snapshot.profiling.expect("profiling should be enabled");
    let surface = profiling
        .surfaces
        .iter()
        .find(|surface| surface.surface_id == "@mesh/audio-popover")
        .expect("worked surfaces must use surface_id as the canonical key");
    assert_eq!(surface.module_id.as_deref(), Some("@mesh/audio-popover"));
    assert_eq!(surface.redraw_count, 1);
}

#[test]
fn profiling_snapshot_backfills_surface_module_id_after_empty_stage_metadata() {
    let mut shell = Shell::new();
    shell
        .apply_request(CoreRequest::ToggleDebugProfiling)
        .unwrap();

    shell.record_surface_profiling_stage(
        "@mesh/audio-popover",
        Some(""),
        ProfilingStage::Paint,
        std::time::Duration::from_micros(22),
        Some("rebuild"),
    );
    shell.record_surface_profiling_stage(
        "@mesh/audio-popover",
        Some("@mesh/audio-popover"),
        ProfilingStage::PresentCommit,
        std::time::Duration::from_micros(9),
        Some("present"),
    );

    let snapshot = shell.build_debug_snapshot();
    let profiling = snapshot.profiling.expect("profiling should be enabled");
    let surface = profiling
        .surfaces
        .iter()
        .find(|surface| surface.surface_id == "@mesh/audio-popover")
        .expect("worked surfaces must retain their canonical surface key");
    assert_eq!(surface.module_id.as_deref(), Some("@mesh/audio-popover"));
    assert!(
        surface.stages.iter().any(|stage| stage
            .recent_samples
            .iter()
            .all(|sample| { sample.surface_id.as_deref() == Some("@mesh/audio-popover") })),
        "surface samples must retain explicit surface keys while module ids recover"
    );
}

#[test]
fn debug_snapshot_orders_backend_and_surface_profiling_deterministically() {
    let mut shell = Shell::new();
    shell
        .apply_request(CoreRequest::ToggleDebugProfiling)
        .unwrap();

    shell.record_surface_profiling_stage(
        "@mesh/z-popover",
        Some("@mesh/z-popover"),
        ProfilingStage::Paint,
        std::time::Duration::from_micros(30),
        Some("rebuild"),
    );
    shell.record_surface_profiling_stage(
        "@mesh/a-panel",
        Some("@mesh/a-panel"),
        ProfilingStage::Layout,
        std::time::Duration::from_micros(12),
        Some("rebuild"),
    );
    shell.record_backend_profiling_stage(
        "mesh.network",
        "@mesh/networkmanager",
        ProfilingBackendStage::PollUpdate,
        std::time::Duration::from_micros(25),
        Some("service_update"),
    );
    shell.record_backend_state_publish_delivery(
        "mesh.audio",
        "@mesh/pipewire-audio",
        std::time::Duration::from_micros(18),
        Some("broadcast_service_event"),
    );

    let snapshot = shell.build_debug_snapshot();
    let profiling = snapshot.profiling.expect("profiling should be enabled");

    assert_eq!(
        profiling
            .surfaces
            .iter()
            .map(|surface| surface.surface_id.as_str())
            .collect::<Vec<_>>(),
        vec!["@mesh/a-panel", "@mesh/z-popover"]
    );
    assert_eq!(
        profiling
            .backends
            .iter()
            .map(|backend| (backend.interface.as_str(), backend.provider_id.as_str()))
            .collect::<Vec<_>>(),
        vec![
            ("mesh.audio", "@mesh/pipewire-audio"),
            ("mesh.network", "@mesh/networkmanager"),
        ]
    );
    assert_eq!(
        profiling
            .shell
            .stages
            .iter()
            .find(|stage| stage.stage == ProfilingStage::Paint)
            .map(|stage| stage.total_micros),
        Some(30)
    );
    assert_eq!(
        profiling
            .surfaces
            .iter()
            .find(|surface| surface.surface_id == "@mesh/a-panel")
            .and_then(|surface| surface.module_id.as_deref()),
        Some("@mesh/a-panel")
    );
    assert!(
        profiling.backends.iter().any(|backend| {
            backend.interface == "mesh.audio"
                && backend
                    .stages
                    .iter()
                    .any(|stage| stage.stage == ProfilingBackendStage::StatePublishDelivery)
        }),
        "backend summaries must coexist beside shell and per-surface totals in one snapshot"
    );
}
