use super::*;

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
        "PHASE26_BASELINE hover style_restyle={}us paint={}us traversal={}us retained={} full_rebuild={}",
        stage_max_micros(
            &hover_records,
            mesh_core_debug::ProfilingStage::StyleRestyle
        ),
        stage_max_micros(&hover_records, mesh_core_debug::ProfilingStage::Paint),
        stage_max_micros(
            &hover_records,
            mesh_core_debug::ProfilingStage::PaintTraversal
        ),
        hover_invalidation.retained_path,
        hover_invalidation.full_rebuild
    );
    eprintln!(
        "PHASE26_BASELINE surface_open_close paint={}us traversal={}us shaping={}us retained={} full_rebuild={}",
        stage_max_micros(
            &surface_open_close_records,
            mesh_core_debug::ProfilingStage::Paint
        ),
        stage_max_micros(
            &surface_open_close_records,
            mesh_core_debug::ProfilingStage::PaintTraversal
        ),
        surface_open_close_invalidation.text.shaping_micros,
        surface_open_close_invalidation.retained_path,
        surface_open_close_invalidation.full_rebuild
    );
    eprintln!(
        "PHASE26_BASELINE pointer_update layout={}us paint={}us traversal={}us retained={} full_rebuild={}",
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
        pointer_update_invalidation.retained_path,
        pointer_update_invalidation.full_rebuild
    );
    eprintln!(
        "PHASE26_BASELINE keyboard_traversal style_restyle={}us paint={}us traversal={}us retained={} full_rebuild={}",
        stage_max_micros(
            &keyboard_records,
            mesh_core_debug::ProfilingStage::StyleRestyle
        ),
        stage_max_micros(&keyboard_records, mesh_core_debug::ProfilingStage::Paint),
        stage_max_micros(
            &keyboard_records,
            mesh_core_debug::ProfilingStage::PaintTraversal
        ),
        keyboard_invalidation.retained_path,
        keyboard_invalidation.full_rebuild
    );
    eprintln!(
        "PHASE26_BASELINE backend_update paint={}us traversal={}us shaping={}us retained={} full_rebuild={}",
        stage_max_micros(
            &backend_update_records,
            mesh_core_debug::ProfilingStage::Paint
        ),
        stage_max_micros(
            &backend_update_records,
            mesh_core_debug::ProfilingStage::PaintTraversal
        ),
        backend_update_invalidation.text.shaping_micros,
        backend_update_invalidation.retained_path,
        backend_update_invalidation.full_rebuild
    );

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
}
