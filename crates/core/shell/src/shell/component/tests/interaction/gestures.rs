use super::*;
use mesh_core_frontend_shell_adapter::ShellComponent;

#[test]
fn swipe_keeps_begin_target_and_reports_terminal_direction() {
    let mut component = test_frontend_component(
        r#"
<template><box /></template>
<script lang="luau">
swipe_phase = ""
swipe_target = ""
swipe_fingers = 0
swipe_direction = ""
swipe_total_x = 0
function onSwipe(event)
    swipe_phase = event.phase
    swipe_target = event.current.key
    swipe_fingers = event.fingers
    swipe_direction = event.direction or ""
    swipe_total_x = event.total_delta.x
end
</script>
"#,
    );
    component.last_tree = Some(root_with(vec![
        event_node(
            "box",
            "root/0",
            0.0,
            0.0,
            80.0,
            80.0,
            &[("swipe", "onSwipe")],
        ),
        event_node(
            "box",
            "root/1",
            100.0,
            0.0,
            80.0,
            80.0,
            &[("swipe", "onSwipe")],
        ),
    ]));
    component.hovered_pos = (20.0, 20.0);
    let theme = default_theme();

    component
        .handle_input(
            &theme,
            240,
            160,
            ComponentInput::GestureSwipeBegin { fingers: 3 },
        )
        .unwrap();
    component.hovered_pos = (120.0, 20.0);
    component
        .handle_input(
            &theme,
            240,
            160,
            ComponentInput::GestureSwipeUpdate { dx: -12.0, dy: 2.0 },
        )
        .unwrap();
    component
        .handle_input(
            &theme,
            240,
            160,
            ComponentInput::GestureSwipeEnd { cancelled: false },
        )
        .unwrap();

    assert_eq!(
        runtime_value(&component, "swipe_phase"),
        Some(serde_json::json!("end"))
    );
    assert_eq!(
        runtime_value(&component, "swipe_target"),
        Some(serde_json::json!("root/0"))
    );
    assert_eq!(runtime_number(&component, "swipe_fingers"), 3.0);
    assert_eq!(
        runtime_value(&component, "swipe_direction"),
        Some(serde_json::json!("left"))
    );
    assert_eq!(runtime_number(&component, "swipe_total_x"), -12.0);
}

#[test]
fn pinch_hold_and_two_finger_scroll_dispatch_json_payloads() {
    let mut component = test_frontend_component(
        r#"
<template><box /></template>
<script lang="luau">
pinch_phase = ""
pinch_scale = 0
pinch_rotation = 0
pinch_cancelled = false
hold_phase = ""
hold_fingers = 0
finger_dx = 0
function onPinch(event)
    pinch_phase = event.phase
    pinch_scale = event.scale
    pinch_rotation = event.rotation
    pinch_cancelled = event.cancelled
end
function onHold(event)
    hold_phase = event.phase
    hold_fingers = event.fingers
end
function onFingerScroll(event)
    finger_dx = event.delta.x
end
</script>
"#,
    );
    component.last_tree = Some(root_with(vec![event_node(
        "box",
        "root/0",
        0.0,
        0.0,
        120.0,
        80.0,
        &[
            ("pinch", "onPinch"),
            ("hold", "onHold"),
            ("twofingerscroll", "onFingerScroll"),
        ],
    )]));
    component.hovered_pos = (20.0, 20.0);
    let theme = default_theme();

    for input in [
        ComponentInput::GesturePinchBegin { fingers: 2 },
        ComponentInput::GesturePinchUpdate {
            dx: 1.0,
            dy: -2.0,
            scale: 1.25,
            rotation: 17.0,
        },
        ComponentInput::GesturePinchEnd { cancelled: true },
        ComponentInput::GestureHoldBegin { fingers: 3 },
        ComponentInput::GestureHoldEnd { cancelled: false },
        ComponentInput::TwoFingerScroll {
            x: 20.0,
            y: 20.0,
            dx: 4.0,
            dy: -3.0,
        },
    ] {
        component.handle_input(&theme, 240, 160, input).unwrap();
    }

    assert_eq!(
        runtime_value(&component, "pinch_phase"),
        Some(serde_json::json!("end"))
    );
    assert_eq!(runtime_number(&component, "pinch_scale"), 1.25);
    assert_eq!(runtime_number(&component, "pinch_rotation"), 17.0);
    assert_eq!(
        runtime_value(&component, "pinch_cancelled"),
        Some(serde_json::json!(true))
    );
    assert_eq!(
        runtime_value(&component, "hold_phase"),
        Some(serde_json::json!("end"))
    );
    assert_eq!(runtime_number(&component, "hold_fingers"), 3.0);
    assert_eq!(runtime_number(&component, "finger_dx"), 4.0);
}

#[test]
fn unhandled_two_finger_scroll_preserves_native_scroll_fallback() {
    let mut component = test_frontend_component(
        r#"
<style>
scroll { height: 60px; overflow-y: auto; }
.content { height: 240px; flex-shrink: 0; }
</style>
<template><scroll><box class="content" /></scroll></template>
<script lang="luau"></script>
"#,
    );
    let theme = default_theme();
    let mut buffer = PixelBuffer::new(160, 120);
    component
        .paint(&theme, SurfaceExtent::unpadded(160, 120), &mut buffer, 1.0)
        .unwrap();

    component
        .handle_input(
            &theme,
            160,
            120,
            ComponentInput::TwoFingerScroll {
                x: 20.0,
                y: 20.0,
                dx: 0.0,
                dy: -1.0,
            },
        )
        .unwrap();

    assert!(
        component
            .scroll_offsets
            .values()
            .any(|offset| offset.y > 0.0),
        "two-finger input without a handler should scroll normally"
    );
}

#[test]
fn touch_sequence_keeps_target_and_reports_active_touches() {
    let mut component = test_frontend_component(
        r#"
<template><box /></template>
<script lang="luau">
touch_type = ""
touch_target = ""
touch_count = -1
changed_id = -1
function onTouch(event)
    touch_type = event.type
    touch_target = event.current.key
    touch_count = #event.touches
    if event.changed_touches[1] then
        changed_id = event.changed_touches[1].id
    end
end
</script>
"#,
    );
    component.last_tree = Some(root_with(vec![
        event_node(
            "box",
            "root/0",
            0.0,
            0.0,
            80.0,
            80.0,
            &[
                ("touchstart", "onTouch"),
                ("touchmove", "onTouch"),
                ("touchend", "onTouch"),
                ("touchcancel", "onTouch"),
            ],
        ),
        event_node(
            "box",
            "root/1",
            100.0,
            0.0,
            80.0,
            80.0,
            &[("touchmove", "onTouch")],
        ),
    ]));
    let theme = default_theme();

    component
        .handle_input(
            &theme,
            240,
            160,
            ComponentInput::TouchDown {
                id: 7,
                x: 20.0,
                y: 20.0,
            },
        )
        .unwrap();
    component
        .handle_input(
            &theme,
            240,
            160,
            ComponentInput::TouchDown {
                id: 9,
                x: 30.0,
                y: 30.0,
            },
        )
        .unwrap();
    component
        .handle_input(
            &theme,
            240,
            160,
            ComponentInput::TouchMove {
                id: 7,
                x: 130.0,
                y: 20.0,
            },
        )
        .unwrap();

    assert_eq!(
        runtime_value(&component, "touch_type"),
        Some(serde_json::json!("touchmove"))
    );
    assert_eq!(
        runtime_value(&component, "touch_target"),
        Some(serde_json::json!("root/0"))
    );
    assert_eq!(runtime_number(&component, "touch_count"), 2.0);

    component
        .handle_input(&theme, 240, 160, ComponentInput::TouchUp { id: 7 })
        .unwrap();
    assert_eq!(
        runtime_value(&component, "touch_type"),
        Some(serde_json::json!("touchend"))
    );
    assert_eq!(runtime_number(&component, "touch_count"), 1.0);
    assert_eq!(runtime_number(&component, "changed_id"), 7.0);

    component
        .handle_input(&theme, 240, 160, ComponentInput::TouchCancel)
        .unwrap();
    assert_eq!(
        runtime_value(&component, "touch_type"),
        Some(serde_json::json!("touchcancel"))
    );
    assert_eq!(runtime_number(&component, "touch_count"), 0.0);
}

#[test]
fn touch_taps_synthesize_click_doubletap_and_scheduled_longpress() {
    let mut component = test_frontend_component(
        r#"
<template><box /></template>
<script lang="luau">
tap_count = 0
double_tap_count = 0
long_press_count = 0
click_count = 0
click_source = ""
function onTap(event)
    tap_count = tap_count + 1
end
function onDoubleTap(event)
    double_tap_count = double_tap_count + 1
end
function onLongPress(event)
    long_press_count = long_press_count + 1
end
function onClick(event)
    click_count = click_count + 1
    click_source = event.synthesized_from or ""
end
</script>
"#,
    );
    component.last_tree = Some(root_with(vec![event_node(
        "button",
        "root/0",
        0.0,
        0.0,
        120.0,
        80.0,
        &[
            ("tap", "onTap"),
            ("doubletap", "onDoubleTap"),
            ("longpress", "onLongPress"),
            ("click", "onClick"),
        ],
    )]));
    let theme = default_theme();

    for id in [1, 2] {
        component
            .handle_input(
                &theme,
                160,
                120,
                ComponentInput::TouchDown {
                    id,
                    x: 20.0,
                    y: 20.0,
                },
            )
            .unwrap();
        component
            .handle_input(&theme, 160, 120, ComponentInput::TouchUp { id })
            .unwrap();
    }

    assert_eq!(runtime_number(&component, "tap_count"), 2.0);
    assert_eq!(runtime_number(&component, "double_tap_count"), 1.0);
    assert_eq!(runtime_number(&component, "click_count"), 2.0);
    assert_eq!(
        runtime_value(&component, "click_source"),
        Some(serde_json::json!("touch"))
    );

    component
        .handle_input(
            &theme,
            160,
            120,
            ComponentInput::TouchDown {
                id: 3,
                x: 20.0,
                y: 20.0,
            },
        )
        .unwrap();
    component.touch_gestures.get_mut(&3).unwrap().started_at =
        Instant::now() - input::LONG_PRESS_DELAY;
    component.tick().unwrap();
    component
        .handle_input(&theme, 160, 120, ComponentInput::TouchUp { id: 3 })
        .unwrap();

    assert_eq!(runtime_number(&component, "long_press_count"), 1.0);
    assert_eq!(runtime_number(&component, "tap_count"), 2.0);
    assert_eq!(runtime_number(&component, "click_count"), 2.0);
}

#[test]
fn touch_conveniences_dispatch_prebound_handler_calls() {
    let mut component = test_frontend_component(
        r#"
<template><box /></template>
<script lang="luau">
bound_value = ""
function onTap(bound, event)
    bound_value = bound .. ":" .. event.type
end
</script>
"#,
    );
    let mut node = event_node("button", "root/0", 0.0, 0.0, 120.0, 80.0, &[]);
    node.event_handler_calls.insert(
        "tap".into(),
        mesh_core_elements::EventHandlerCall {
            handler: "onTap".into(),
            args: vec![serde_json::json!("bound")],
        },
    );
    component.last_tree = Some(root_with(vec![node]));
    let theme = default_theme();

    component
        .handle_input(
            &theme,
            160,
            120,
            ComponentInput::TouchDown {
                id: 4,
                x: 20.0,
                y: 20.0,
            },
        )
        .unwrap();
    component
        .handle_input(&theme, 160, 120, ComponentInput::TouchUp { id: 4 })
        .unwrap();

    assert_eq!(
        runtime_value(&component, "bound_value"),
        Some(serde_json::json!("bound:tap"))
    );
}
