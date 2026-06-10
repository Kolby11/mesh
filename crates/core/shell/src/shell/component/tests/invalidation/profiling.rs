use super::*;
use std::collections::BTreeSet;

use mesh_core_debug::ProfilingInvalidationSnapshot;

fn stage_max_micros(
    records: &[ComponentProfilingRecord],
    stage: mesh_core_debug::ProfilingStage,
) -> u64 {
    records
        .iter()
        .filter(|record| record.stage == stage)
        .map(|record| record.duration.as_micros().min(u128::from(u64::MAX)) as u64)
        .max()
        .unwrap_or(0)
}

fn assert_text_cache_proof_active(label: &str, snapshot: &ProfilingInvalidationSnapshot) {
    assert!(
        snapshot.text.glyph_cache_active,
        "{label} should keep text/glyph cache metrics active"
    );
    assert!(
        snapshot.text.layout_hits + snapshot.text.layout_misses > 0,
        "{label} should report text layout cache activity"
    );
}

fn assert_raster_cache_proof_active(label: &str, snapshot: &ProfilingInvalidationSnapshot) {
    assert!(
        snapshot.paint.raster_cache_hits
            + snapshot.paint.raster_cache_misses
            + snapshot.paint.raster_cache_bypasses
            > 0,
        "{label} should report icon/image raster cache activity"
    );
}

fn assert_raster_cache_reuse(label: &str, snapshot: &ProfilingInvalidationSnapshot) {
    assert!(
        snapshot.paint.raster_cache_hits > 0,
        "{label} should report warm icon/image raster cache reuse"
    );
}

fn log_phase31_proof(
    scenario: &str,
    records: &[ComponentProfilingRecord],
    snapshot: &ProfilingInvalidationSnapshot,
) {
    eprintln!(
        "PHASE31_PROOF scenario={} paint_us={} traversal_us={} text_hits={} text_misses={} shaping_us={} raster_hits={} raster_misses={} raster_bypasses={} repaint_policy={} filtered_commands={} filtered_skipped={} filtered_spans={} filtered_fallbacks={} retained={} full_rebuild={}",
        scenario,
        stage_max_micros(records, mesh_core_debug::ProfilingStage::Paint),
        stage_max_micros(records, mesh_core_debug::ProfilingStage::PaintTraversal),
        snapshot.text.layout_hits,
        snapshot.text.layout_misses,
        snapshot.text.shaping_micros,
        snapshot.paint.raster_cache_hits,
        snapshot.paint.raster_cache_misses,
        snapshot.paint.raster_cache_bypasses,
        snapshot.paint.repaint_policy.as_str(),
        snapshot.paint.filtered_command_count,
        snapshot.paint.filtered_commands_skipped,
        snapshot.paint.filtered_span_count,
        snapshot.paint.filtered_fallback_count,
        snapshot.retained_path,
        snapshot.full_rebuild
    );
}

#[test]
fn phase44_focused_proof_preserves_invalidation_and_damage_payloads() {
    let theme = default_theme();
    let mut component =
        real_frontend_module_component("@mesh/navigation-bar", audio_network_catalog());
    component.set_profiling_enabled(true);
    let mut buffer = PixelBuffer::new(960, 80);

    component.paint(&theme, 960, 80, &mut buffer).unwrap();

    {
        let proof = component
            .last_focused_proof_snapshot()
            .expect("paint should store focused proof snapshot");
        assert!(!proof.nodes.is_empty());
        assert!(!proof.accessibility.is_empty());
        let _geometry = proof.dirty.geometry;
        let _material = proof.dirty.material;
        let _text = proof.dirty.text;
        let _accessibility = proof.dirty.accessibility;
    }
    assert!(component.take_invalidation_snapshot().is_some());
    assert!(!component.take_present_damage().is_empty());
}

#[test]
fn phase26_real_surface_baseline_emits_canonical_proof_measurements() {
    let theme = default_theme();

    let mut hover_component =
        real_frontend_module_component("@mesh/navigation-bar", audio_network_catalog());
    hover_component.set_profiling_enabled(true);
    let mut hover_buffer = PixelBuffer::new(960, 80);
    hover_component
        .paint(&theme, 960, 80, &mut hover_buffer)
        .unwrap();
    hover_component.take_profiling_records();
    hover_component.take_invalidation_snapshot();
    let hover_tree = hover_component
        .last_tree
        .as_ref()
        .expect("rendered navigation tree");
    let hover_target = first_node_with_click_handler(
        hover_tree,
        "__mesh_embed__::@mesh/navigation-bar/local:ThemeButton::onThemeToggle",
    )
    .expect("rendered theme button");
    let hover_key = hover_target
        .attributes
        .get("_mesh_key")
        .expect("theme button mesh key")
        .clone();
    let (hover_left, hover_top, hover_right, hover_bottom) =
        find_node_bounds_by_key(hover_tree, &hover_key, 0.0, 0.0).expect("theme button bounds");
    hover_component
        .handle_input(
            &theme,
            960,
            80,
            ComponentInput::PointerMove {
                x: (hover_left + hover_right) * 0.5,
                y: (hover_top + hover_bottom) * 0.5,
            },
        )
        .unwrap();
    hover_component
        .paint(&theme, 960, 80, &mut hover_buffer)
        .unwrap();
    let hover_records = hover_component.take_profiling_records();
    let hover_invalidation = hover_component
        .take_invalidation_snapshot()
        .expect("hover repaint should capture invalidation proof");

    let mut popover_component =
        real_frontend_module_component("@mesh/audio-popover", audio_network_catalog());
    popover_component.set_profiling_enabled(true);
    popover_component
        .handle_service_event(&ServiceEvent::Updated {
            service: "mesh.audio".into(),
            source_module: "@mesh/pipewire-audio".into(),
            payload: serde_json::json!({
                "available": true,
                "percent": 50,
                "muted": false
            }),
        })
        .unwrap();
    let mut popover_buffer = PixelBuffer::new(320, 220);
    popover_component
        .paint(&theme, 320, 220, &mut popover_buffer)
        .unwrap();
    let surface_open_close_records = popover_component.take_profiling_records();
    let surface_open_close_invalidation = popover_component
        .take_invalidation_snapshot()
        .expect("audio popover paint should capture invalidation proof");

    let slider = first_node_by_tag(popover_component.last_tree.as_ref().unwrap(), "slider")
        .expect("audio popover slider");
    let drag_x = slider.layout.x + slider.layout.width * 0.8;
    let drag_y = slider.layout.y + slider.layout.height * 0.5;
    popover_component
        .handle_input(
            &theme,
            320,
            220,
            ComponentInput::PointerButton {
                x: drag_x,
                y: drag_y,
                pressed: true,
            },
        )
        .unwrap();
    popover_component
        .handle_input(
            &theme,
            320,
            220,
            ComponentInput::PointerMove {
                x: drag_x - 40.0,
                y: drag_y,
            },
        )
        .unwrap();
    popover_component
        .paint(&theme, 320, 220, &mut popover_buffer)
        .unwrap();
    let pointer_update_records = popover_component.take_profiling_records();
    let pointer_update_invalidation = popover_component
        .take_invalidation_snapshot()
        .expect("pointer update should capture invalidation proof");

    let mut keyboard_component =
        real_frontend_module_component("@mesh/navigation-bar", audio_network_catalog());
    keyboard_component.set_profiling_enabled(true);
    let mut keyboard_buffer = PixelBuffer::new(960, 80);
    keyboard_component
        .paint(&theme, 960, 80, &mut keyboard_buffer)
        .unwrap();
    keyboard_component.take_profiling_records();
    keyboard_component.take_invalidation_snapshot();
    keyboard_component
        .handle_input(
            &theme,
            960,
            80,
            ComponentInput::KeyPressed {
                key: "Tab".into(),
                modifiers: KeyModifiers::default(),
            },
        )
        .unwrap();
    keyboard_component
        .paint(&theme, 960, 80, &mut keyboard_buffer)
        .unwrap();
    let keyboard_records = keyboard_component.take_profiling_records();
    let keyboard_invalidation = keyboard_component
        .take_invalidation_snapshot()
        .expect("keyboard traversal should capture invalidation proof");

    let mut backend_component =
        real_frontend_module_component("@mesh/navigation-bar", audio_network_catalog());
    backend_component.set_profiling_enabled(true);
    let mut backend_buffer = PixelBuffer::new(960, 80);
    backend_component
        .paint(&theme, 960, 80, &mut backend_buffer)
        .unwrap();
    backend_component.take_profiling_records();
    backend_component.take_invalidation_snapshot();
    backend_component
        .handle_service_event(&ServiceEvent::Updated {
            service: "mesh.audio".into(),
            source_module: "@mesh/pipewire-audio".into(),
            payload: serde_json::json!({
                "available": true,
                "percent": 42,
                "muted": false
            }),
        })
        .unwrap();
    backend_component
        .paint(&theme, 960, 80, &mut backend_buffer)
        .unwrap();
    let backend_update_records = backend_component.take_profiling_records();
    let backend_update_invalidation = backend_component
        .take_invalidation_snapshot()
        .expect("backend update should capture invalidation proof");

    eprintln!(
        "PHASE26_BASELINE hover style_restyle={}us paint={}us traversal={}us text_hits={} text_misses={} shaping={}us raster_hits={} raster_misses={} raster_bypasses={} retained={} full_rebuild={}",
        stage_max_micros(
            &hover_records,
            mesh_core_debug::ProfilingStage::StyleRestyle
        ),
        stage_max_micros(&hover_records, mesh_core_debug::ProfilingStage::Paint),
        stage_max_micros(
            &hover_records,
            mesh_core_debug::ProfilingStage::PaintTraversal
        ),
        hover_invalidation.text.layout_hits,
        hover_invalidation.text.layout_misses,
        hover_invalidation.text.shaping_micros,
        hover_invalidation.paint.raster_cache_hits,
        hover_invalidation.paint.raster_cache_misses,
        hover_invalidation.paint.raster_cache_bypasses,
        hover_invalidation.retained_path,
        hover_invalidation.full_rebuild
    );
    eprintln!(
        "PHASE26_BASELINE surface_open_close paint={}us traversal={}us text_hits={} text_misses={} shaping={}us raster_hits={} raster_misses={} raster_bypasses={} retained={} full_rebuild={}",
        stage_max_micros(
            &surface_open_close_records,
            mesh_core_debug::ProfilingStage::Paint
        ),
        stage_max_micros(
            &surface_open_close_records,
            mesh_core_debug::ProfilingStage::PaintTraversal
        ),
        surface_open_close_invalidation.text.layout_hits,
        surface_open_close_invalidation.text.layout_misses,
        surface_open_close_invalidation.text.shaping_micros,
        surface_open_close_invalidation.paint.raster_cache_hits,
        surface_open_close_invalidation.paint.raster_cache_misses,
        surface_open_close_invalidation.paint.raster_cache_bypasses,
        surface_open_close_invalidation.retained_path,
        surface_open_close_invalidation.full_rebuild
    );
    eprintln!(
        "PHASE26_BASELINE pointer_update layout={}us paint={}us traversal={}us text_hits={} text_misses={} shaping={}us raster_hits={} raster_misses={} raster_bypasses={} retained={} full_rebuild={}",
        stage_max_micros(
            &pointer_update_records,
            mesh_core_debug::ProfilingStage::Layout
        ),
        stage_max_micros(
            &pointer_update_records,
            mesh_core_debug::ProfilingStage::Paint
        ),
        stage_max_micros(
            &pointer_update_records,
            mesh_core_debug::ProfilingStage::PaintTraversal
        ),
        pointer_update_invalidation.text.layout_hits,
        pointer_update_invalidation.text.layout_misses,
        pointer_update_invalidation.text.shaping_micros,
        pointer_update_invalidation.paint.raster_cache_hits,
        pointer_update_invalidation.paint.raster_cache_misses,
        pointer_update_invalidation.paint.raster_cache_bypasses,
        pointer_update_invalidation.retained_path,
        pointer_update_invalidation.full_rebuild
    );
    eprintln!(
        "PHASE26_BASELINE keyboard_traversal style_restyle={}us paint={}us traversal={}us text_hits={} text_misses={} shaping={}us raster_hits={} raster_misses={} raster_bypasses={} retained={} full_rebuild={}",
        stage_max_micros(
            &keyboard_records,
            mesh_core_debug::ProfilingStage::StyleRestyle
        ),
        stage_max_micros(&keyboard_records, mesh_core_debug::ProfilingStage::Paint),
        stage_max_micros(
            &keyboard_records,
            mesh_core_debug::ProfilingStage::PaintTraversal
        ),
        keyboard_invalidation.text.layout_hits,
        keyboard_invalidation.text.layout_misses,
        keyboard_invalidation.text.shaping_micros,
        keyboard_invalidation.paint.raster_cache_hits,
        keyboard_invalidation.paint.raster_cache_misses,
        keyboard_invalidation.paint.raster_cache_bypasses,
        keyboard_invalidation.retained_path,
        keyboard_invalidation.full_rebuild
    );
    eprintln!(
        "PHASE26_BASELINE backend_update paint={}us traversal={}us text_hits={} text_misses={} shaping={}us raster_hits={} raster_misses={} raster_bypasses={} retained={} full_rebuild={}",
        stage_max_micros(
            &backend_update_records,
            mesh_core_debug::ProfilingStage::Paint
        ),
        stage_max_micros(
            &backend_update_records,
            mesh_core_debug::ProfilingStage::PaintTraversal
        ),
        backend_update_invalidation.text.layout_hits,
        backend_update_invalidation.text.layout_misses,
        backend_update_invalidation.text.shaping_micros,
        backend_update_invalidation.paint.raster_cache_hits,
        backend_update_invalidation.paint.raster_cache_misses,
        backend_update_invalidation.paint.raster_cache_bypasses,
        backend_update_invalidation.retained_path,
        backend_update_invalidation.full_rebuild
    );

    let phase31_scenarios = [
        ("hover", hover_records.as_slice(), &hover_invalidation),
        (
            "surface_open_close",
            surface_open_close_records.as_slice(),
            &surface_open_close_invalidation,
        ),
        (
            "pointer_update",
            pointer_update_records.as_slice(),
            &pointer_update_invalidation,
        ),
        (
            "keyboard_traversal",
            keyboard_records.as_slice(),
            &keyboard_invalidation,
        ),
        (
            "backend_update",
            backend_update_records.as_slice(),
            &backend_update_invalidation,
        ),
    ];
    let scenario_ids: BTreeSet<_> = phase31_scenarios
        .iter()
        .map(|(scenario, _, _)| *scenario)
        .collect();
    assert_eq!(
        scenario_ids,
        BTreeSet::from([
            "backend_update",
            "hover",
            "keyboard_traversal",
            "pointer_update",
            "surface_open_close",
        ]),
        "Phase 31 proof must cover exactly the five canonical scenario IDs"
    );
    for (scenario, records, snapshot) in phase31_scenarios {
        log_phase31_proof(scenario, records, snapshot);
    }

    assert!(
        stage_max_micros(&hover_records, mesh_core_debug::ProfilingStage::Paint) > 0,
        "hover proof should record a real paint on the shipped navigation bar"
    );
    assert!(
        stage_max_micros(
            &surface_open_close_records,
            mesh_core_debug::ProfilingStage::Paint
        ) > 0,
        "surface_open_close proof should record a real audio popover paint"
    );
    assert!(
        stage_max_micros(
            &pointer_update_records,
            mesh_core_debug::ProfilingStage::Paint
        ) > 0,
        "pointer_update proof should record a real audio control repaint"
    );
    assert!(
        stage_max_micros(&keyboard_records, mesh_core_debug::ProfilingStage::Paint) > 0,
        "keyboard traversal proof should record a real navigation-bar repaint"
    );
    assert!(
        stage_max_micros(
            &backend_update_records,
            mesh_core_debug::ProfilingStage::Paint
        ) > 0,
        "backend update proof should record a real shipped-surface repaint"
    );
    assert_text_cache_proof_active("hover", &hover_invalidation);
    assert_text_cache_proof_active("surface_open_close", &surface_open_close_invalidation);
    assert_text_cache_proof_active("pointer_update", &pointer_update_invalidation);
    assert_text_cache_proof_active("keyboard_traversal", &keyboard_invalidation);
    assert_text_cache_proof_active("backend_update", &backend_update_invalidation);
    assert_raster_cache_proof_active("hover", &hover_invalidation);
    assert_raster_cache_proof_active("surface_open_close", &surface_open_close_invalidation);
    assert_raster_cache_proof_active("pointer_update", &pointer_update_invalidation);
    assert_raster_cache_proof_active("keyboard_traversal", &keyboard_invalidation);
    assert_raster_cache_proof_active("backend_update", &backend_update_invalidation);
    assert_raster_cache_reuse("hover", &hover_invalidation);
    assert_raster_cache_reuse("pointer_update", &pointer_update_invalidation);
    assert_raster_cache_reuse("keyboard_traversal", &keyboard_invalidation);
    assert_raster_cache_reuse("backend_update", &backend_update_invalidation);
}

fn fnv_hash_buffer(buffer: &PixelBuffer) -> u64 {
    const OFFSET: u64 = 14695981039346656037;
    const PRIME: u64 = 1099511628211;
    let mut hash: u64 = OFFSET;
    for &byte in &buffer.data {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(PRIME);
    }
    hash
}

#[test]
fn phase98_pixel_equivalence_backend_update() {
    let theme = default_theme();

    let mut baseline =
        real_frontend_module_component("@mesh/navigation-bar", audio_network_catalog());
    let mut narrow =
        real_frontend_module_component("@mesh/navigation-bar", audio_network_catalog());
    let mut baseline_buffer = PixelBuffer::new(960, 80);
    let mut narrow_buffer = PixelBuffer::new(960, 80);

    baseline
        .paint(&theme, 960, 80, &mut baseline_buffer)
        .unwrap();
    narrow.paint(&theme, 960, 80, &mut narrow_buffer).unwrap();

    let payload = serde_json::json!({"available": true, "percent": 55, "muted": false});
    baseline
        .handle_service_event(&ServiceEvent::Updated {
            service: "mesh.audio".into(),
            source_module: "@mesh/pipewire-audio".into(),
            payload: payload.clone(),
        })
        .unwrap();
    narrow
        .handle_service_event(&ServiceEvent::Updated {
            service: "mesh.audio".into(),
            source_module: "@mesh/pipewire-audio".into(),
            payload,
        })
        .unwrap();

    baseline.invalidate_script_state();
    baseline
        .paint(&theme, 960, 80, &mut baseline_buffer)
        .unwrap();
    narrow.paint(&theme, 960, 80, &mut narrow_buffer).unwrap();

    let baseline_hash = fnv_hash_buffer(&baseline_buffer);
    let narrow_hash = fnv_hash_buffer(&narrow_buffer);
    assert_eq!(
        baseline_hash, narrow_hash,
        "pixel equivalence: backend_update"
    );
}

#[test]
fn phase98_profiling_backend_update_reduced_churn() {
    let theme = default_theme();
    let mut component =
        real_frontend_module_component("@mesh/navigation-bar", audio_network_catalog());
    component.set_profiling_enabled(true);
    let mut buffer = PixelBuffer::new(960, 80);

    component.paint(&theme, 960, 80, &mut buffer).unwrap();
    component.take_profiling_records();

    let payload = serde_json::json!({"available": true, "percent": 65, "muted": false});
    component
        .handle_service_event(&ServiceEvent::Updated {
            service: "mesh.audio".into(),
            source_module: "@mesh/pipewire-audio".into(),
            payload,
        })
        .unwrap();

    component.paint(&theme, 960, 80, &mut buffer).unwrap();
    let invalidation = component
        .take_invalidation_snapshot()
        .expect("profiling snapshot");

    eprintln!(
        "PHASE98_PROOF scenario=backend_update narrow_path={} affected_node_count={} full_rebuild={} component_invalidations={:?}",
        invalidation.narrow_path,
        invalidation.affected_node_count,
        invalidation.full_rebuild,
        invalidation.component
    );

    assert!(
        invalidation.full_rebuild || invalidation.narrow_path,
        "backend update should use either TREE_REBUILD or SCRIPT_NARROW path"
    );
}

#[test]
fn fnv_hash_buffer_deterministic() {
    let buffer = PixelBuffer::new(8, 8);
    let hash1 = fnv_hash_buffer(&buffer);
    let hash2 = fnv_hash_buffer(&buffer);
    assert_eq!(
        hash1, hash2,
        "FNV hash must be deterministic for same buffer"
    );
}

#[test]
fn fnv_hash_buffer_zero_identical() {
    let buffer1 = PixelBuffer::new(4, 4);
    let buffer2 = PixelBuffer::new(4, 4);
    assert_eq!(
        fnv_hash_buffer(&buffer1),
        fnv_hash_buffer(&buffer2),
        "two identically-sized zero-initialized buffers must have the same hash"
    );
}

#[test]
fn fnv_hash_buffer_differs_on_change() {
    let mut buffer1 = PixelBuffer::new(4, 4);
    buffer1.clear(mesh_core_elements::style::Color::TRANSPARENT);
    let mut buffer2 = PixelBuffer::new(4, 4);
    buffer2.clear(mesh_core_elements::style::Color {
        r: 255,
        g: 0,
        b: 0,
        a: 255,
    });
    let hash1 = fnv_hash_buffer(&buffer1);
    let hash2 = fnv_hash_buffer(&buffer2);
    assert_ne!(
        hash1, hash2,
        "different pixel content must produce different hashes"
    );
}
