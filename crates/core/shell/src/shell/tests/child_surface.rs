use super::common::*;
use super::*;

#[test]
fn component_runtime_resolves_parent_and_child_surface_targets() {
    // Proves the one-VM → parent + N child-surface plumbing: a single
    // ComponentRuntime owns a parent surface plus a synthetically injected
    // child popup surface, and the shell resolves *both* surface ids back to
    // the same component while distinguishing which target each names.
    let mut shell = Shell::new();
    let seen_events = Arc::new(Mutex::new(Vec::new()));
    shell
        .components
        .push(super::types::ComponentRuntime::new(Box::new(
            RecordingComponent::new(seen_events),
        )));

    let parent_id = "@test/recording".to_string();
    let child_id = "@test/recording#popover:0".to_string();

    // The parent target alone resolves before any child exists.
    assert_eq!(
        shell.component_target_for_surface(&parent_id),
        Some((0, super::types::TargetRef::Parent))
    );
    assert_eq!(shell.component_target_for_surface(&child_id), None);

    // Inject an auto-derived child surface (what the popover reconcile builds).
    shell.components[0]
        .children
        .push(super::types::ChildSurface {
            target: super::types::SurfaceTarget::new(
                child_id.clone(),
                mesh_core_presentation::LayerSurfaceSizePolicy::Flexible,
            ),
            node_key: "root/0/popover".to_string(),
            anchor_rect: (12, 0, 40, 56),
            content_padding: (0, 0, 0, 0),
            closing_until: None,
            last_paint_generation: None,
            last_paint_exiting: None,
            last_paint_scale_bits: None,
            last_paint_content_offset: None,
            pending_present_damage: Vec::new(),
        });

    // Both surface ids now map to the same component, each tagged with its
    // target; targets() enumerates parent first, then the child.
    assert_eq!(
        shell.component_target_for_surface(&parent_id),
        Some((0, super::types::TargetRef::Parent))
    );
    assert_eq!(
        shell.component_target_for_surface(&child_id),
        Some((0, super::types::TargetRef::Child(0)))
    );
    let target_ids: Vec<&str> = shell.components[0]
        .targets()
        .map(|target| target.surface_id.as_str())
        .collect();
    assert_eq!(target_ids, vec![parent_id.as_str(), child_id.as_str()]);

    // The child carries the originating node key + anchor rect used by the
    // popup reconcile/positioner.
    assert_eq!(shell.components[0].children[0].node_key, "root/0/popover");
    assert_eq!(shell.components[0].children[0].anchor_rect, (12, 0, 40, 56));

    // The child target is independently addressable for per-surface state.
    shell.components[0]
        .target_mut(super::types::TargetRef::Child(0))
        .force_full_present = true;
    assert!(
        shell.components[0].children[0].target.force_full_present,
        "target_mut(Child) must address the child's own render state"
    );
}

fn render_components_until_child_popup(shell: &mut Shell) {
    // Child popups stage one parent repaint with `mesh-surface-entering`
    // before the xdg_popup is created and painted.
    shell.render_components().unwrap();
    shell.render_components().unwrap();
}

#[cfg(feature = "allocation-profiling")]
#[test]
fn allocation_profiler_records_a_completed_surface_render_pass() {
    let mut shell = Shell::new();
    let state = Arc::new(Mutex::new(PopoverHarnessState::default()));
    shell.register_component(Box::new(PopoverHarnessComponent::new(state)));
    shell
        .apply_request(CoreRequest::ToggleDebugProfiling)
        .unwrap();

    shell.render_components().unwrap();

    let snapshot = shell.build_debug_snapshot();
    let profiling = snapshot.profiling.expect("profiling enabled");
    assert!(profiling.allocation_profiling_available);
    let surface = profiling
        .surfaces
        .iter()
        .find(|surface| surface.surface_id == "@test/popover-host")
        .expect("rendered parent surface");
    let allocations = surface
        .allocations
        .as_ref()
        .expect("allocation sample for completed render pass");
    assert_eq!(allocations.sample_count, 1);
    assert!(allocations.allocation_count > 0);
    assert!(allocations.allocated_bytes > 0);
    assert_eq!(allocations.recent_samples.len(), 1);
}

#[test]
fn child_surface_reconcile_creates_popup_and_paints_subtree() {
    let mut shell = Shell::new();
    shell.presentation_engine =
        mesh_core_presentation::PresentationEngine::testing_with_popup_support(true);
    let state = Arc::new(Mutex::new(PopoverHarnessState::default()));
    shell.register_component(Box::new(PopoverHarnessComponent::new(state.clone())));

    render_components_until_child_popup(&mut shell);

    assert_eq!(shell.components[0].children.len(), 1);
    let child_id = shell.components[0].children[0].target.surface_id.clone();
    let config = shell
        .presentation_engine
        .testing_popup_config(&child_id)
        .expect("child popover should configure an xdg_popup");
    assert_eq!(config.parent_surface_id, "@test/popover-host");
    assert_eq!(config.placement.anchor_rect, (8, 10, 40, 16));
    assert_eq!(config.placement.size, (72, 32));
    assert!(
        shell
            .presentation_engine
            .testing_presented_surfaces()
            .iter()
            .any(|surface| surface == &child_id),
        "child popup subtree should be presented separately"
    );
    assert_eq!(
        state.lock().unwrap().painted_nodes.as_slice(),
        ["root/popover"]
    );
}

#[test]
fn child_surface_presents_full_damage_every_frame() {
    // `paint_child_surface` clears and fully repaints the child buffer each
    // frame, so every child present must report full-surface damage —
    // anything narrower leaves stale pixels in the compositor and freezes
    // popover enter/exit transitions.
    let mut shell = Shell::new();
    shell.presentation_engine =
        mesh_core_presentation::PresentationEngine::testing_with_popup_support(true);
    let state = Arc::new(Mutex::new(PopoverHarnessState::default()));
    shell.register_component(Box::new(PopoverHarnessComponent::new(state)));

    render_components_until_child_popup(&mut shell);
    let child_id = shell.components[0].children[0].target.surface_id.clone();

    shell.render_components().unwrap();

    let child_damage = shell
        .presentation_engine
        .testing_presented_damage()
        .iter()
        .filter(|(surface, _)| surface == &child_id)
        .map(|(_, damage)| damage.as_slice())
        .collect::<Vec<_>>();
    assert!(!child_damage.is_empty(), "child popup should be presented");
    assert!(
        child_damage
            .iter()
            .all(|damage| damage.len() == 1 && damage[0].x == 0 && damage[0].y == 0),
        "every child popup present should carry full-surface damage, got {child_damage:?}"
    );
}

#[test]
fn child_surface_forwards_retained_local_damage_to_presentation() {
    let sparse_damage = mesh_core_render::DamageRect {
        x: 11,
        y: 7,
        width: 9,
        height: 5,
    };
    let mut shell = Shell::new();
    shell.presentation_engine =
        mesh_core_presentation::PresentationEngine::testing_with_popup_support(true);
    let state = Arc::new(Mutex::new(PopoverHarnessState {
        paint_generation: Some(1),
        present_damage: Some(vec![sparse_damage]),
        ..Default::default()
    }));
    shell.register_component(Box::new(PopoverHarnessComponent::new(Arc::clone(&state))));

    render_components_until_child_popup(&mut shell);
    let child_id = shell.components[0].children[0].target.surface_id.clone();
    state.lock().unwrap().paint_generation = Some(2);
    shell.render_components().unwrap();

    let (_, damage) = shell
        .presentation_engine
        .testing_presented_damage()
        .iter()
        .rev()
        .find(|(surface, _)| surface == &child_id)
        .expect("changed child popup should be presented");
    assert_eq!(damage, &[sparse_damage]);
}

#[test]
fn child_surface_reuses_buffer_for_unchanged_authoritative_generation() {
    let mut shell = Shell::new();
    shell.presentation_engine =
        mesh_core_presentation::PresentationEngine::testing_with_popup_support(true);
    let state = Arc::new(Mutex::new(PopoverHarnessState {
        paint_generation: Some(7),
        ..Default::default()
    }));
    shell.register_component(Box::new(PopoverHarnessComponent::new(Arc::clone(&state))));

    render_components_until_child_popup(&mut shell);
    let child_id = shell.components[0].children[0].target.surface_id.clone();
    let paints_before = state.lock().unwrap().painted_nodes.len();
    let presents_before = shell
        .presentation_engine
        .testing_presented_surfaces()
        .iter()
        .filter(|surface| *surface == &child_id)
        .count();

    shell.render_components().unwrap();
    assert_eq!(state.lock().unwrap().painted_nodes.len(), paints_before);
    assert_eq!(
        shell
            .presentation_engine
            .testing_presented_surfaces()
            .iter()
            .filter(|surface| *surface == &child_id)
            .count(),
        presents_before
    );

    state.lock().unwrap().paint_generation = Some(8);
    shell.render_components().unwrap();
    assert_eq!(state.lock().unwrap().painted_nodes.len(), paints_before + 1);
    assert_eq!(
        shell
            .presentation_engine
            .testing_presented_surfaces()
            .iter()
            .filter(|surface| *surface == &child_id)
            .count(),
        presents_before + 1
    );
}

#[test]
fn child_surface_reconcile_removes_closed_popover() {
    let mut shell = Shell::new();
    shell.presentation_engine =
        mesh_core_presentation::PresentationEngine::testing_with_popup_support(true);
    let state = Arc::new(Mutex::new(PopoverHarnessState::default()));
    shell.register_component(Box::new(PopoverHarnessComponent::new(state.clone())));

    render_components_until_child_popup(&mut shell);
    let child_id = shell.components[0].children[0].target.surface_id.clone();
    state.lock().unwrap().open = false;
    shell.render_components().unwrap();

    assert!(shell.components[0].children.is_empty());
    assert!(
        shell
            .presentation_engine
            .testing_destroyed_popups()
            .contains(&child_id)
    );
    assert!(!shell.core.surfaces.contains_key(&child_id));
    assert!(shell.component_target_for_surface(&child_id).is_none());
}

#[test]
fn hiding_parent_surface_destroys_child_popups_and_clears_child_keyboard_focus() {
    let mut shell = Shell::new();
    shell.presentation_engine =
        mesh_core_presentation::PresentationEngine::testing_with_popup_support(true);
    let state = Arc::new(Mutex::new(PopoverHarnessState::default()));
    shell.register_component(Box::new(PopoverHarnessComponent::new(state)));

    render_components_until_child_popup(&mut shell);
    let child_id = shell.components[0].children[0].target.surface_id.clone();
    shell.keyboard_focus_surface = Some(child_id.clone());

    shell
        .set_surface_visibility_now("@test/popover-host".to_string(), false)
        .unwrap();

    assert!(shell.components[0].children.is_empty());
    assert!(
        shell
            .presentation_engine
            .testing_destroyed_popups()
            .contains(&child_id)
    );
    assert!(!shell.core.surfaces.contains_key(&child_id));
    assert!(!shell.surfaces.contains_key(&child_id));
    assert!(shell.component_target_for_surface(&child_id).is_none());
    assert_eq!(shell.keyboard_focus_surface, None);
}

#[test]
fn child_surface_reconcile_plays_exit_transition_before_teardown() {
    let mut shell = Shell::new();
    shell.presentation_engine =
        mesh_core_presentation::PresentationEngine::testing_with_popup_support(true);
    let state = Arc::new(Mutex::new(PopoverHarnessState {
        hide_transition_ms: 120,
        ..PopoverHarnessState::default()
    }));
    shell.register_component(Box::new(PopoverHarnessComponent::new(state.clone())));

    render_components_until_child_popup(&mut shell);
    let child_id = shell.components[0].children[0].target.surface_id.clone();

    state.lock().unwrap().open = false;
    shell.render_components().unwrap();

    // The child popup should still be alive and repainted with the exiting
    // class so its own CSS exit transition (opacity/transform) can animate,
    // instead of being torn down the instant `open` flips false.
    assert_eq!(
        shell.components[0].children.len(),
        1,
        "closing popover should stay mounted for its exit transition"
    );
    assert!(shell.components[0].children[0].closing_until.is_some());
    assert!(
        !shell
            .presentation_engine
            .testing_destroyed_popups()
            .contains(&child_id)
    );
    assert_eq!(
        state.lock().unwrap().exiting_paints.last(),
        Some(&true),
        "the closing repaint pass should mark the popover subtree as exiting"
    );

    // Simulate the exit-transition deadline having elapsed.
    shell.components[0].children[0].closing_until = Some(Instant::now() - Duration::from_millis(1));
    shell.render_components().unwrap();

    assert!(shell.components[0].children.is_empty());
    assert!(
        shell
            .presentation_engine
            .testing_destroyed_popups()
            .contains(&child_id)
    );
}

#[test]
fn child_surface_reopen_cancels_pending_exit_transition() {
    let mut shell = Shell::new();
    shell.presentation_engine =
        mesh_core_presentation::PresentationEngine::testing_with_popup_support(true);
    let state = Arc::new(Mutex::new(PopoverHarnessState {
        hide_transition_ms: 120,
        ..PopoverHarnessState::default()
    }));
    shell.register_component(Box::new(PopoverHarnessComponent::new(state.clone())));

    render_components_until_child_popup(&mut shell);
    state.lock().unwrap().open = false;
    shell.render_components().unwrap();
    assert!(shell.components[0].children[0].closing_until.is_some());

    // Reopening before the grace period elapses should cancel the exit and
    // resume normal (non-exiting) repaints.
    state.lock().unwrap().open = true;
    shell.render_components().unwrap();

    assert_eq!(shell.components[0].children.len(), 1);
    assert!(shell.components[0].children[0].closing_until.is_none());
    assert_eq!(state.lock().unwrap().exiting_paints.last(), Some(&false));
}

#[test]
fn parent_pointer_leave_defers_child_popover_close() {
    let mut shell = Shell::new();
    shell.presentation_engine =
        mesh_core_presentation::PresentationEngine::testing_with_popup_support(true);
    let state = Arc::new(Mutex::new(PopoverHarnessState::default()));
    shell.register_component(Box::new(PopoverHarnessComponent::new(state.clone())));

    render_components_until_child_popup(&mut shell);
    let child_id = shell.components[0].children[0].target.surface_id.clone();

    shell.presentation_engine.testing_push_event(
        mesh_core_presentation::WindowEvent::PointerLeave {
            surface_id: "@test/popover-host".into(),
        },
    );
    shell.dispatch_wayland().unwrap();

    assert!(
        shell.pending_popover_hides.contains_key(&child_id),
        "leaving the parent trigger surface should arm a bridge hide for its child popup"
    );

    shell.presentation_engine.testing_push_event(
        mesh_core_presentation::WindowEvent::PointerMove {
            surface_id: child_id.clone().into(),
            x: 4.0,
            y: 4.0,
        },
    );
    shell.dispatch_wayland().unwrap();

    assert!(
        !shell.pending_popover_hides.contains_key(&child_id),
        "entering the promoted child popup should cancel the bridge hide"
    );
}

#[test]
fn parent_to_child_pointer_crossing_keeps_parent_surface_size() {
    let mut shell = Shell::new();
    shell.presentation_engine =
        mesh_core_presentation::PresentationEngine::testing_with_popup_support(true);
    let state = Arc::new(Mutex::new(PopoverHarnessState::default()));
    shell.register_component(Box::new(PopoverHarnessComponent::new(state.clone())));

    render_components_until_child_popup(&mut shell);
    let child_id = shell.components[0].children[0].target.surface_id.clone();
    state.lock().unwrap().surface_sizes.clear();
    shell.presentation_engine.testing_push_event(
        mesh_core_presentation::WindowEvent::PointerLeave {
            surface_id: "@test/popover-host".into(),
        },
    );
    shell.presentation_engine.testing_push_event(
        mesh_core_presentation::WindowEvent::PointerMove {
            surface_id: child_id.clone().into(),
            x: 4.0,
            y: 4.0,
        },
    );

    shell.dispatch_wayland().unwrap();

    let state = state.lock().unwrap();
    assert!(
        state
            .child_inputs
            .iter()
            .any(|(_, input)| matches!(input, ComponentInput::PointerMove { .. }))
    );
    assert_eq!(
        state.surface_sizes,
        vec![(120, 36)],
        "child popup input must not resize the parent component to the child surface"
    );
    assert!(!shell.pending_popover_hides.contains_key(&child_id));
}

#[test]
fn child_popover_hover_bridge_deadline_synthesizes_pointer_leave() {
    let mut shell = Shell::new();
    shell.presentation_engine =
        mesh_core_presentation::PresentationEngine::testing_with_popup_support(true);
    let state = Arc::new(Mutex::new(PopoverHarnessState::default()));
    shell.register_component(Box::new(PopoverHarnessComponent::new(state.clone())));

    render_components_until_child_popup(&mut shell);
    let child_id = shell.components[0].children[0].target.surface_id.clone();

    shell.presentation_engine.testing_push_event(
        mesh_core_presentation::WindowEvent::PointerLeave {
            surface_id: "@test/popover-host".into(),
        },
    );
    shell.dispatch_wayland().unwrap();
    shell
        .pending_popover_hides
        .insert(child_id.clone(), Instant::now() - Duration::from_millis(1));

    shell.complete_due_surface_transitions().unwrap();

    let inputs = &state.lock().unwrap().child_inputs;
    assert!(
        inputs.iter().any(|(node_key, input)| {
            node_key == "root/popover" && matches!(input, ComponentInput::PointerLeave)
        }),
        "bridge deadline should route PointerLeave into the child popup component"
    );
    assert!(
        !shell.pending_popover_hides.contains_key(&child_id),
        "bridge deadline should drain the pending hide"
    );
}

#[test]
fn dismissed_popup_drain_removes_child_surface() {
    let mut shell = Shell::new();
    shell.presentation_engine =
        mesh_core_presentation::PresentationEngine::testing_with_popup_support(true);
    let state = Arc::new(Mutex::new(PopoverHarnessState::default()));
    shell.register_component(Box::new(PopoverHarnessComponent::new(state.clone())));

    render_components_until_child_popup(&mut shell);
    let child_id = shell.components[0].children[0].target.surface_id.clone();
    shell
        .presentation_engine
        .testing_push_dismissed_popup(child_id.clone());
    shell.render_components().unwrap();

    assert!(shell.components[0].children.is_empty());
    assert!(!shell.core.surfaces.contains_key(&child_id));
    assert!(shell.component_target_for_surface(&child_id).is_none());

    {
        let mut state = state.lock().unwrap();
        state.open = false;
    }
    shell.render_components().unwrap();
    {
        let mut state = state.lock().unwrap();
        state.open = true;
    }
    render_components_until_child_popup(&mut shell);
    assert_eq!(
        shell.components[0].children.len(),
        1,
        "a later close/open cycle should create a fresh popup"
    );
}

#[test]
fn child_surface_input_routes_to_local_child_handler_and_profiles() {
    let mut shell = Shell::new();
    shell.presentation_engine =
        mesh_core_presentation::PresentationEngine::testing_with_popup_support(true);
    let state = Arc::new(Mutex::new(PopoverHarnessState::default()));
    shell.register_component(Box::new(PopoverHarnessComponent::new(state.clone())));
    shell
        .apply_request(CoreRequest::ToggleDebugProfiling)
        .unwrap();
    render_components_until_child_popup(&mut shell);
    let child_id = shell.components[0].children[0].target.surface_id.clone();

    shell
        .presentation_engine
        .testing_push_event(mesh_core_presentation::WindowEvent::Scroll {
            surface_id: child_id.clone().into(),
            x: 6.0,
            y: 7.0,
            dx: 0.0,
            dy: -1.0,
        });
    shell.presentation_engine.testing_push_event(
        mesh_core_presentation::WindowEvent::PointerButton {
            surface_id: child_id.clone().into(),
            x: 10.0,
            y: 12.0,
            pressed: true,
        },
    );
    shell.dispatch_wayland().unwrap();

    let inputs = &state.lock().unwrap().child_inputs;
    assert_eq!(inputs.len(), 2);
    assert_eq!(inputs[0].0, "root/popover");
    assert!(matches!(
        inputs[0].1,
        ComponentInput::Scroll {
            x: 6.0,
            y: 7.0,
            dx: 0.0,
            dy: -1.0
        }
    ));
    assert_eq!(inputs[1].0, "root/popover");
    assert!(matches!(
        inputs[1].1,
        ComponentInput::PointerButton {
            x: 10.0,
            y: 12.0,
            pressed: true
        }
    ));
    let snapshot = shell.build_debug_snapshot();
    let child_surface = snapshot
        .profiling
        .expect("profiling should be enabled")
        .surfaces
        .into_iter()
        .find(|surface| surface.surface_id == child_id)
        .expect("child input should be profiled against the popup surface");
    assert!(child_surface.stages.iter().any(|stage| {
        stage.stage == mesh_core_debug::ProfilingStage::InputHandling && stage.sample_count >= 2
    }));
}

/// The same rule one level down: a popover's `xdg_popup` buffer is padded for
/// descendant shadow/filter overshoot, and clicks over that transparent ring
/// must reach whatever is behind the popover rather than being swallowed by it.
#[test]
fn child_popup_input_region_excludes_the_shadow_overshoot_ring() {
    const CONTENT: (u32, u32) = (72, 32);
    const PADDING: (u32, u32, u32, u32) = (24, 8, 24, 40);

    let mut shell = Shell::new();
    shell.presentation_engine =
        mesh_core_presentation::PresentationEngine::testing_with_popup_support(true);
    let state = Arc::new(Mutex::new(PopoverHarnessState {
        content_padding: PADDING,
        ..PopoverHarnessState::default()
    }));
    shell.register_component(Box::new(PopoverHarnessComponent::new(state)));

    render_components_until_child_popup(&mut shell);

    let child_id = shell.components[0].children[0].target.surface_id.clone();
    let config = shell
        .presentation_engine
        .testing_popup_config(&child_id)
        .expect("child popover configures an xdg_popup");
    assert_eq!(
        config.placement.size,
        (
            CONTENT.0 + PADDING.0 + PADDING.2,
            CONTENT.1 + PADDING.1 + PADDING.3
        ),
        "the popup buffer is the content plus its overshoot ring"
    );

    let region = shell
        .presentation_engine
        .input_region(&child_id)
        .expect("a padded popup must confine its input region");
    assert_eq!(
        (region.x, region.y, region.width, region.height),
        (PADDING.0, PADDING.1, CONTENT.0, CONTENT.1),
        "input stops at the visible popover content, not at the padded buffer"
    );
}
