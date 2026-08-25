use super::common::*;
use super::*;

#[test]
fn activate_popover_can_immediately_enter_focus_chain() {
    let mut shell = Shell::new();
    let trigger_state = Arc::new(Mutex::new(FocusRecordingState::default()));
    let popover_state = Arc::new(Mutex::new(FocusRecordingState::default()));
    shell.register_component(Box::new(FocusRecordingComponent::new(
        "@mesh/navigation-bar",
        trigger_state.clone(),
    )));
    shell.register_component(Box::new(FocusRecordingComponent::new(
        "@mesh/audio-popover",
        popover_state.clone(),
    )));

    let mut emitted = shell
        .apply_request(CoreRequest::ActivatePopover {
            surface_id: "@mesh/audio-popover".into(),
            trigger_surface: "@mesh/navigation-bar".into(),
            trigger_key: "volume-button".into(),
            focus: true,
        })
        .unwrap();
    shell.drain_requests(&mut emitted).unwrap();

    assert_eq!(
        trigger_state.lock().unwrap().registered_popovers.as_slice(),
        [("volume-button".into(), "@mesh/audio-popover".into())]
    );
    assert_eq!(trigger_state.lock().unwrap().releases, 1);
    assert_eq!(
        popover_state.lock().unwrap().received_focus.as_slice(),
        [(
            TabFocusTarget::First,
            Some(("@mesh/navigation-bar".into(), "volume-button".into())),
            true,
        )]
    );
    assert_eq!(
        shell.keyboard_focus_surface.as_deref(),
        Some("@mesh/audio-popover")
    );
}

#[test]
fn activate_popover_uses_exact_left_edge_anchor_rect() {
    let mut shell = Shell::new();
    shell.presentation_engine =
        mesh_core_presentation::PresentationEngine::testing_with_popup_support(true);
    let trigger_state = Arc::new(Mutex::new(FocusRecordingState::default()));
    let popover_state = Arc::new(Mutex::new(FocusRecordingState::default()));
    shell.register_component(Box::new(FocusRecordingComponent::new(
        "@mesh/navigation-bar",
        trigger_state,
    )));
    shell.register_component(Box::new(FocusRecordingComponent::with_popover_margin_left(
        "@mesh/language-popover",
        popover_state,
        724,
    )));

    let mut emitted = shell
        .apply_request(CoreRequest::ActivatePopover {
            surface_id: "@mesh/language-popover".into(),
            trigger_surface: "@mesh/navigation-bar".into(),
            trigger_key: "language-button".into(),
            focus: false,
        })
        .unwrap();
    shell.drain_requests(&mut emitted).unwrap();

    let config = shell
        .components
        .iter()
        .find(|runtime| runtime.surface_id == "@mesh/language-popover")
        .and_then(|runtime| runtime.parent.popup_config.as_ref())
        .expect("legacy language popover should be marked for xdg_popup promotion");
    assert_eq!(config.placement.anchor_rect.0, 724);
    assert_eq!(config.placement.anchor_rect.2, 1);
}

#[test]
fn promoted_popover_config_uses_content_size_not_stale_surface_size() {
    let mut shell = Shell::new();
    shell.presentation_engine =
        mesh_core_presentation::PresentationEngine::testing_with_popup_support(true);
    let trigger_state = Arc::new(Mutex::new(FocusRecordingState::default()));
    shell.register_component(Box::new(FocusRecordingComponent::new(
        "@mesh/navigation-bar",
        trigger_state,
    )));
    shell.register_component(Box::new(PopupGeometryRecordingComponent::new(
        "@mesh/theme-selector",
        (112, 74),
        (240, 154),
    )));

    let mut emitted = shell
        .apply_request(CoreRequest::ActivatePopover {
            surface_id: "@mesh/theme-selector".into(),
            trigger_surface: "@mesh/navigation-bar".into(),
            trigger_key: "theme-button".into(),
            focus: false,
        })
        .unwrap();
    shell.drain_requests(&mut emitted).unwrap();

    shell.render_components().unwrap();

    let config = shell
        .presentation_engine
        .testing_popup_config("@mesh/theme-selector")
        .expect("promoted popover should be configured");
    assert_eq!(
        config.placement.size,
        (112, 74),
        "popup positioner geometry must use content size, not tooltip-padded surface size"
    );
}

#[test]
fn promoted_popover_first_measurement_uses_parent_surface_bound() {
    let mut shell = Shell::new();
    shell.presentation_engine =
        mesh_core_presentation::PresentationEngine::testing_with_popup_support(true);
    let trigger_state = Arc::new(Mutex::new(FocusRecordingState::default()));
    shell.register_component(Box::new(FocusRecordingComponent::new(
        "@mesh/navigation-bar",
        trigger_state,
    )));
    shell.register_component(Box::new(PopupGeometryRecordingComponent::new(
        "@mesh/intrinsic-popover",
        (0, 0),
        (240, 154),
    )));
    // The trigger is already mapped on a real shell, so its configured
    // surface is the useful intrinsic measurement bound before the popup has
    // its own xdg_popup configure.
    let trigger = shell
        .surfaces
        .get_mut("@mesh/navigation-bar")
        .expect("trigger surface should be registered");
    trigger.width = 1920;
    trigger.height = 56;

    let mut emitted = shell
        .apply_request(CoreRequest::ActivatePopover {
            surface_id: "@mesh/intrinsic-popover".into(),
            trigger_surface: "@mesh/navigation-bar".into(),
            trigger_key: "theme-button".into(),
            focus: false,
        })
        .unwrap();
    shell.drain_requests(&mut emitted).unwrap();
    shell.render_components().unwrap();

    let runtime = shell
        .components
        .iter()
        .find(|runtime| runtime.surface_id == "@mesh/intrinsic-popover")
        .expect("intrinsic popup runtime should be registered");
    let buffer = runtime
        .parent
        .paint_buffer
        .as_ref()
        .expect("first popup measurement should allocate a paint buffer");
    assert_eq!(
        (buffer.width(), buffer.height()),
        (1920, 56),
        "first popup layout must use the parent bound, not the 1x1 positioner placeholder"
    );
}

#[test]
fn layer_surface_config_uses_content_size_not_stale_surface_size_on_first_show() {
    let mut shell = Shell::new();
    shell.register_component(Box::new(MeasuredLayerGeometryComponent::new(
        "@mesh/debug-inspector",
        (480, 640),
        (0, 0),
    )));

    shell
        .apply_request(CoreRequest::ShowSurface {
            surface_id: "@mesh/debug-inspector".into(),
        })
        .unwrap();
    shell.render_components().unwrap();

    let runtime = shell
        .components
        .iter()
        .find(|runtime| runtime.surface_id == "@mesh/debug-inspector")
        .expect("devtools runtime should be registered");
    let config = runtime
        .parent
        .last_surface_config
        .as_ref()
        .expect("layer surface should be configured on first show");
    assert_eq!(
        config.surface_size(),
        (480, 640),
        "layer surface configure must use measured content size instead of the stale pre-measure size"
    );
    let buffer = runtime
        .parent
        .paint_buffer
        .as_ref()
        .expect("layer surface should allocate a paint buffer on first show");
    assert_eq!(
        (buffer.width(), buffer.height()),
        (480, 640),
        "first visible frame must allocate a paint buffer at the measured content size"
    );
}

#[test]
fn hover_bridge_hide_defers_promoted_popover_close_until_deadline() {
    let mut shell = Shell::new();
    shell.presentation_engine =
        mesh_core_presentation::PresentationEngine::testing_with_popup_support(true);
    let trigger_state = Arc::new(Mutex::new(FocusRecordingState::default()));
    let popover_state = Arc::new(Mutex::new(FocusRecordingState::default()));
    shell.register_component(Box::new(FocusRecordingComponent::new(
        "@mesh/navigation-bar",
        trigger_state,
    )));
    shell.register_component(Box::new(FocusRecordingComponent::new(
        "@mesh/quick-settings",
        popover_state,
    )));

    let mut emitted = shell
        .apply_request(CoreRequest::ActivatePopover {
            surface_id: "@mesh/quick-settings".into(),
            trigger_surface: "@mesh/navigation-bar".into(),
            trigger_key: "settings-button".into(),
            focus: false,
        })
        .unwrap();
    shell.drain_requests(&mut emitted).unwrap();

    let emitted = shell
        .apply_request(CoreRequest::HidePopover {
            surface_id: "@mesh/quick-settings".into(),
            defer_for_hover_bridge: true,
        })
        .unwrap();
    assert!(emitted.is_empty());
    assert!(
        shell
            .pending_popover_hides
            .contains_key("@mesh/quick-settings")
    );
    assert!(
        shell
            .core
            .surfaces
            .get("@mesh/quick-settings")
            .is_some_and(|state| state.visible)
    );

    shell.pending_popover_hides.insert(
        "@mesh/quick-settings".into(),
        Instant::now() - Duration::from_millis(1),
    );
    let emitted = shell.complete_due_surface_transitions().unwrap();
    assert!(emitted.is_empty());
    assert!(
        !shell
            .pending_popover_hides
            .contains_key("@mesh/quick-settings")
    );
    assert!(
        shell
            .core
            .surfaces
            .get("@mesh/quick-settings")
            .is_some_and(|state| !state.visible)
    );
}

#[test]
fn pointer_enter_cancels_hover_bridge_popover_hide() {
    let mut shell = Shell::new();
    shell.presentation_engine =
        mesh_core_presentation::PresentationEngine::testing_with_popup_support(true);
    let trigger_state = Arc::new(Mutex::new(FocusRecordingState::default()));
    let popover_state = Arc::new(Mutex::new(FocusRecordingState::default()));
    shell.register_component(Box::new(FocusRecordingComponent::new(
        "@mesh/navigation-bar",
        trigger_state,
    )));
    shell.register_component(Box::new(FocusRecordingComponent::new(
        "@mesh/quick-settings",
        popover_state,
    )));

    let mut emitted = shell
        .apply_request(CoreRequest::ActivatePopover {
            surface_id: "@mesh/quick-settings".into(),
            trigger_surface: "@mesh/navigation-bar".into(),
            trigger_key: "settings-button".into(),
            focus: false,
        })
        .unwrap();
    shell.drain_requests(&mut emitted).unwrap();
    shell
        .apply_request(CoreRequest::HidePopover {
            surface_id: "@mesh/quick-settings".into(),
            defer_for_hover_bridge: true,
        })
        .unwrap();
    assert!(
        shell
            .pending_popover_hides
            .contains_key("@mesh/quick-settings")
    );

    shell.presentation_engine.testing_push_event(
        mesh_core_presentation::WindowEvent::PointerMove {
            surface_id: "@mesh/quick-settings".into(),
            x: 8.0,
            y: 8.0,
        },
    );
    shell.dispatch_wayland().unwrap();

    assert!(
        !shell
            .pending_popover_hides
            .contains_key("@mesh/quick-settings")
    );
    assert!(
        shell
            .core
            .surfaces
            .get("@mesh/quick-settings")
            .is_some_and(|state| state.visible)
    );
}

#[test]
fn pointer_leave_from_promoted_popover_schedules_hover_bridge_hide() {
    let mut shell = Shell::new();
    shell.presentation_engine =
        mesh_core_presentation::PresentationEngine::testing_with_popup_support(true);
    let trigger_state = Arc::new(Mutex::new(FocusRecordingState::default()));
    let popover_state = Arc::new(Mutex::new(FocusRecordingState::default()));
    shell.register_component(Box::new(FocusRecordingComponent::new(
        "@mesh/navigation-bar",
        trigger_state,
    )));
    shell.register_component(Box::new(FocusRecordingComponent::new(
        "@mesh/quick-settings",
        popover_state,
    )));

    let mut emitted = shell
        .apply_request(CoreRequest::ActivatePopover {
            surface_id: "@mesh/quick-settings".into(),
            trigger_surface: "@mesh/navigation-bar".into(),
            trigger_key: "settings-button".into(),
            focus: false,
        })
        .unwrap();
    shell.drain_requests(&mut emitted).unwrap();

    shell.presentation_engine.testing_push_event(
        mesh_core_presentation::WindowEvent::PointerLeave {
            surface_id: "@mesh/quick-settings".into(),
        },
    );
    shell.dispatch_wayland().unwrap();

    assert!(
        shell
            .pending_popover_hides
            .contains_key("@mesh/quick-settings")
    );
    assert!(
        shell
            .core
            .surfaces
            .get("@mesh/quick-settings")
            .is_some_and(|state| state.visible)
    );
}

#[test]
fn activating_popover_closes_promoted_sibling_from_same_trigger_surface() {
    let mut shell = Shell::new();
    shell.presentation_engine =
        mesh_core_presentation::PresentationEngine::testing_with_popup_support(true);
    let trigger_state = Arc::new(Mutex::new(FocusRecordingState::default()));
    let quick_state = Arc::new(Mutex::new(FocusRecordingState::default()));
    let language_state = Arc::new(Mutex::new(FocusRecordingState::default()));
    shell.register_component(Box::new(FocusRecordingComponent::new(
        "@mesh/navigation-bar",
        trigger_state,
    )));
    shell.register_component(Box::new(FocusRecordingComponent::new(
        "@mesh/quick-settings",
        quick_state,
    )));
    shell.register_component(Box::new(FocusRecordingComponent::new(
        "@mesh/language-popover",
        language_state,
    )));

    let mut emitted = shell
        .apply_request(CoreRequest::ActivatePopover {
            surface_id: "@mesh/quick-settings".into(),
            trigger_surface: "@mesh/navigation-bar".into(),
            trigger_key: "settings-button".into(),
            focus: false,
        })
        .unwrap();
    shell.drain_requests(&mut emitted).unwrap();

    let emitted = shell
        .apply_request(CoreRequest::ActivatePopover {
            surface_id: "@mesh/language-popover".into(),
            trigger_surface: "@mesh/navigation-bar".into(),
            trigger_key: "language-button".into(),
            focus: false,
        })
        .unwrap();

    assert!(emitted.iter().any(|request| matches!(
        request,
        CoreRequest::HidePopover {
            surface_id,
            defer_for_hover_bridge: false,
        } if surface_id == "@mesh/quick-settings"
    )));
    assert!(emitted.iter().any(|request| matches!(
        request,
        CoreRequest::ShowSurface { surface_id } if surface_id == "@mesh/language-popover"
    )));
}

#[test]
fn dismissed_legacy_promoted_popover_hides_surface_state() {
    let mut shell = Shell::new();
    shell.presentation_engine =
        mesh_core_presentation::PresentationEngine::testing_with_popup_support(true);
    let trigger_state = Arc::new(Mutex::new(FocusRecordingState::default()));
    let popover_state = Arc::new(Mutex::new(FocusRecordingState::default()));
    shell.register_component(Box::new(FocusRecordingComponent::new(
        "@mesh/navigation-bar",
        trigger_state,
    )));
    shell.register_component(Box::new(FocusRecordingComponent::new(
        "@mesh/quick-settings",
        popover_state,
    )));

    let mut emitted = shell
        .apply_request(CoreRequest::ActivatePopover {
            surface_id: "@mesh/quick-settings".into(),
            trigger_surface: "@mesh/navigation-bar".into(),
            trigger_key: "settings-button".into(),
            focus: false,
        })
        .unwrap();
    shell.drain_requests(&mut emitted).unwrap();
    assert!(
        shell
            .core
            .surfaces
            .get("@mesh/quick-settings")
            .is_some_and(|state| state.visible)
    );

    shell
        .presentation_engine
        .testing_push_dismissed_popup("@mesh/quick-settings");
    shell.render_components().unwrap();

    assert!(
        shell
            .core
            .surfaces
            .get("@mesh/quick-settings")
            .is_some_and(|state| !state.visible)
    );
    let runtime = shell
        .components
        .iter()
        .find(|runtime| runtime.surface_id == "@mesh/quick-settings")
        .expect("quick settings runtime should remain registered");
    assert!(runtime.parent.popup_parent_surface.is_none());
    assert!(runtime.parent.popup_config.is_none());
}

#[test]
fn leaving_popover_keeps_return_surface_as_keyboard_owner() {
    let mut shell = Shell::new();
    let trigger_state = Arc::new(Mutex::new(FocusRecordingState::default()));
    let popover_state = Arc::new(Mutex::new(FocusRecordingState::default()));
    shell.register_component(Box::new(FocusRecordingComponent::new(
        "@mesh/navigation-bar",
        trigger_state.clone(),
    )));
    shell.register_component(Box::new(FocusRecordingComponent::new(
        "@mesh/audio-popover",
        popover_state.clone(),
    )));

    let mut emitted = shell
        .apply_request(CoreRequest::TransferTabFocus {
            from_surface: "@mesh/audio-popover".into(),
            to_surface: "@mesh/navigation-bar".into(),
            target: TabFocusTarget::AtKey("volume-button".into()),
            return_target: None,
            target_closes_on_leave: false,
            close_source: Some("@mesh/audio-popover".into()),
        })
        .unwrap();
    shell.drain_requests(&mut emitted).unwrap();

    assert_eq!(
        shell.keyboard_focus_surface.as_deref(),
        Some("@mesh/navigation-bar")
    );
    assert_eq!(
        shell
            .surfaces
            .get("@mesh/navigation-bar")
            .map(|surface| surface.keyboard_mode),
        Some(mesh_core_wayland::KeyboardMode::Exclusive)
    );
    assert_eq!(
        trigger_state.lock().unwrap().received_focus.as_slice(),
        [(TabFocusTarget::AtKey("volume-button".into()), None, false)]
    );
}

#[test]
fn pointer_click_claims_keyboard_owner_without_forcing_exclusive_mode() {
    let mut shell = Shell::new();
    let nav_state = Arc::new(Mutex::new(FocusRecordingState::default()));
    let popover_state = Arc::new(Mutex::new(FocusRecordingState::default()));
    shell.register_component(Box::new(FocusRecordingComponent::new(
        "@mesh/navigation-bar",
        nav_state.clone(),
    )));
    shell.register_component(Box::new(FocusRecordingComponent::new(
        "@mesh/audio-popover",
        popover_state.clone(),
    )));
    shell.keyboard_focus_surface = Some("@mesh/audio-popover".into());
    shell
        .surfaces
        .get_mut("@mesh/navigation-bar")
        .unwrap()
        .keyboard_mode = mesh_core_wayland::KeyboardMode::OnDemand;
    shell
        .surfaces
        .get_mut("@mesh/audio-popover")
        .unwrap()
        .keyboard_mode = mesh_core_wayland::KeyboardMode::Exclusive;

    shell.claim_keyboard_focus_for_surface("@mesh/navigation-bar");

    assert_eq!(
        shell.keyboard_focus_surface.as_deref(),
        Some("@mesh/navigation-bar")
    );
    assert_eq!(
        shell
            .surfaces
            .get("@mesh/navigation-bar")
            .map(|surface| surface.keyboard_mode),
        Some(mesh_core_wayland::KeyboardMode::OnDemand)
    );
    assert_eq!(
        shell
            .surfaces
            .get("@mesh/audio-popover")
            .map(|surface| surface.keyboard_mode),
        Some(mesh_core_wayland::KeyboardMode::Exclusive)
    );
    assert_eq!(
        popover_state
            .lock()
            .unwrap()
            .keyboard_mode_overrides
            .as_slice(),
        [None]
    );
    assert_eq!(
        nav_state.lock().unwrap().keyboard_mode_overrides.as_slice(),
        [None]
    );
}

#[test]
fn pointer_click_inside_keyboard_owner_preserves_exclusive_override() {
    let mut shell = Shell::new();
    let popover_state = Arc::new(Mutex::new(FocusRecordingState::default()));
    shell.register_component(Box::new(FocusRecordingComponent::new(
        "@mesh/audio-popover",
        popover_state.clone(),
    )));
    shell.keyboard_focus_surface = Some("@mesh/audio-popover".into());

    shell.claim_keyboard_focus_for_surface("@mesh/audio-popover");

    assert_eq!(
        shell.keyboard_focus_surface.as_deref(),
        Some("@mesh/audio-popover")
    );
    assert!(
        popover_state
            .lock()
            .unwrap()
            .keyboard_mode_overrides
            .is_empty(),
        "clicking the already-focused popover must not clear its Exclusive keyboard override"
    );
}

#[test]
fn pointer_click_after_transfer_clears_transfer_forced_exclusive_override() {
    let mut shell = Shell::new();
    let nav_state = Arc::new(Mutex::new(FocusRecordingState::default()));
    let popover_state = Arc::new(Mutex::new(FocusRecordingState::default()));
    shell.register_component(Box::new(FocusRecordingComponent::new(
        "@mesh/navigation-bar",
        nav_state.clone(),
    )));
    shell.register_component(Box::new(FocusRecordingComponent::new(
        "@mesh/audio-popover",
        popover_state.clone(),
    )));
    shell.keyboard_focus_surface = Some("@mesh/navigation-bar".into());
    shell
        .surfaces
        .get_mut("@mesh/audio-popover")
        .unwrap()
        .keyboard_mode = mesh_core_wayland::KeyboardMode::OnDemand;
    let mut emitted = shell
        .apply_request(CoreRequest::TransferTabFocus {
            from_surface: "@mesh/navigation-bar".into(),
            to_surface: "@mesh/audio-popover".into(),
            target: TabFocusTarget::First,
            return_target: Some(("@mesh/navigation-bar".into(), "volume-button".into())),
            target_closes_on_leave: true,
            close_source: None,
        })
        .unwrap();
    shell.drain_requests(&mut emitted).unwrap();

    assert_eq!(
        shell
            .surfaces
            .get("@mesh/audio-popover")
            .map(|surface| surface.keyboard_mode),
        Some(mesh_core_wayland::KeyboardMode::Exclusive)
    );
    assert_eq!(
        popover_state
            .lock()
            .unwrap()
            .keyboard_mode_overrides
            .as_slice(),
        [Some(mesh_core_wayland::KeyboardMode::Exclusive)]
    );

    shell.claim_keyboard_focus_for_surface("@mesh/audio-popover");

    assert_eq!(
        popover_state
            .lock()
            .unwrap()
            .keyboard_mode_overrides
            .as_slice(),
        [Some(mesh_core_wayland::KeyboardMode::Exclusive), None]
    );
    assert_eq!(
        shell
            .surfaces
            .get("@mesh/audio-popover")
            .map(|surface| surface.keyboard_mode),
        Some(mesh_core_wayland::KeyboardMode::OnDemand)
    );
}
