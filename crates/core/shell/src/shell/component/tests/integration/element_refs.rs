use super::*;
use mesh_core_render::PixelBuffer;
use mesh_core_theme::default_theme;

#[test]
fn refs_focus_method_moves_focus_to_the_live_element() {
    // A script calling `refs.<name>:focus()` through a live element reference
    // enqueues an element action that the shell resolves to the real widget node
    // and routes through the canonical focus path.
    let mut component = test_frontend_component(
        r#"
<template>
    <input ref="field" type="text" />
</template>
<script lang="luau">
function focus_field()
    refs.field:focus()
end
function blur_field()
    refs.field:blur()
end
</script>
"#,
    );

    let theme = default_theme();
    let mut buffer = PixelBuffer::new(200, 60);

    // Paint once so element metrics + the ref-name -> node-key map are populated.
    component
        .paint(&theme, SurfaceExtent::unpadded(200, 60), &mut buffer, 1.0)
        .unwrap();
    assert_eq!(component.focused_key, None);

    // The handler focuses the input through its live reference.
    component
        .call_namespaced_handler("focus_field", &[])
        .unwrap();
    assert_eq!(
        component
            .ref_node_keys
            .borrow()
            .get("field")
            .map(String::as_str),
        Some("root/0"),
        "element action application should keep the ref lookup table populated"
    );
    assert_eq!(
        component.focused_key.as_deref(),
        Some("root/0"),
        "refs.field:focus() should focus the input's live node"
    );

    // Blurring the focused element clears focus.
    component
        .call_namespaced_handler("blur_field", &[])
        .unwrap();
    assert_eq!(component.focused_key, None);
}

#[test]
fn bind_this_core_element_exposes_live_element_proxy() {
    let mut component = test_frontend_component(
        r#"
<style>
.panel { width: 120px; height: 40px; }
</style>
<template>
    <input bind:this={field} class="panel" aria-label="Search" />
</template>
<script lang="luau">
field_width = 0
field_label = ""
function inspect()
    field_width = field.width
    field_label = field.ariaLabel
end
function focus_bound()
    field:focus()
end
</script>
"#,
    );

    let theme = default_theme();
    let mut buffer = PixelBuffer::new(200, 60);
    component
        .paint(&theme, SurfaceExtent::unpadded(200, 60), &mut buffer, 1.0)
        .unwrap();

    component.call_namespaced_handler("inspect", &[]).unwrap();
    let width = runtime_value(&component, "field_width")
        .and_then(|value| value.as_f64())
        .unwrap_or(0.0);
    assert!(
        (width - 120.0).abs() < 2.0,
        "bind:this field.width should reflect live layout, got {width}"
    );
    assert_eq!(
        runtime_value(&component, "field_label"),
        Some(serde_json::json!("Search"))
    );

    component
        .call_namespaced_handler("focus_bound", &[])
        .unwrap();
    assert_eq!(
        component.focused_key.as_deref(),
        Some("root/0"),
        "bind:this field:focus() should focus the live node"
    );
}

#[test]
fn refs_geometry_reads_reflect_the_painted_layout() {
    // `refs.<name>.width` reads the live geometry published from the last paint.
    let mut component = test_frontend_component(
        r#"
<style>
.panel { width: 120px; height: 40px; }
</style>
<template>
    <box ref="panel" class="panel" />
</template>
<script lang="luau">
panel_width = 0
function measure()
    panel_width = refs.panel.width
end
</script>
"#,
    );

    let theme = default_theme();
    let mut buffer = PixelBuffer::new(200, 60);
    component
        .paint(&theme, SurfaceExtent::unpadded(200, 60), &mut buffer, 1.0)
        .unwrap();

    component.call_namespaced_handler("measure", &[]).unwrap();
    let width = runtime_value(&component, "panel_width")
        .and_then(|value| value.as_f64())
        .unwrap_or(0.0);
    assert!(
        (width - 120.0).abs() < 2.0,
        "refs.panel.width should reflect the painted 120px layout, got {width}"
    );
}

#[test]
fn refs_scroll_into_view_smoothly_scrolls_the_container_to_reveal_the_target() {
    // A script calling `refs.<name>:scroll_into_view()` enqueues an element action
    // that the shell turns into scroll-offset adjustments on the real overflowing
    // container, routed through the same scroll_offsets map the wheel uses.
    let mut component = test_frontend_component(
        r#"
<style>
scroll { height: 60px; overflow-y: auto; }
.content { height: 240px; flex-shrink: 0; }
.spacer { height: 200px; }
.target { height: 40px; }
</style>
<template>
    <scroll>
        <column class="content">
            <box class="spacer" />
            <box ref="target" class="target" />
        </column>
    </scroll>
</template>
<script lang="luau">
function reveal()
    refs.target:scroll_into_view()
end
</script>
"#,
    );

    let theme = default_theme();
    let mut buffer = PixelBuffer::new(160, 120);

    // Paint once so overflow annotation populates scroll limits and the ref map.
    component
        .paint(&theme, SurfaceExtent::unpadded(160, 120), &mut buffer, 1.0)
        .unwrap();
    // Nothing scrolled yet.
    assert!(component.scroll_offsets.values().all(|o| o.y == 0.0));

    component.call_namespaced_handler("reveal", &[]).unwrap();

    // The target sits at ~200px inside a 60px scroll viewport (content 240px).
    // Reveal is smooth by default, so it registers a target without snapping.
    let animation = *component
        .scroll_animations
        .get(&runtime_node_id_for_key("root/0"))
        .expect("scroll_into_view should register a smooth animation");
    assert!(
        animation.target.y > 1.0,
        "scroll_into_view should target the revealed position"
    );
    assert!(
        component
            .scroll_offsets
            .values()
            .all(|offset| offset.y < 1.0),
        "scroll_into_view should not snap, got {:?}",
        component.scroll_offsets
    );
    component.advance_scroll_animations(animation.start_time + animation.duration);
    assert!(
        component
            .scroll_offsets
            .get(&runtime_node_id_for_key("root/0"))
            .is_some_and(|offset| offset.y > 1.0),
        "the smooth reveal should settle at the target"
    );
}

#[test]
fn refs_scroll_to_sets_offset_and_scroll_top_reads_it_back() {
    // `refs.x:scroll_to(top)` sets the container's own offset (clamped to range),
    // and `refs.x.scroll_top` reads the live offset back on the next paint.
    let mut component = test_frontend_component(
        r#"
<style>
scroll { height: 60px; overflow-y: auto; }
.content { height: 240px; flex-shrink: 0; }
</style>
<template>
    <scroll ref="list">
        <box class="content" />
    </scroll>
</template>
<script lang="luau">
scrolled_top = -1
function jump()
    refs.list:scroll_to(100)
end
function read_back()
    scrolled_top = refs.list.scroll_top
end
</script>
"#,
    );

    let theme = default_theme();
    let mut buffer = PixelBuffer::new(160, 120);
    component
        .paint(&theme, SurfaceExtent::unpadded(160, 120), &mut buffer, 1.0)
        .unwrap();

    // Set the offset through the live reference.
    component.call_namespaced_handler("jump", &[]).unwrap();
    let list_id = runtime_node_id_for_key("root/0");
    let list_offset = component
        .scroll_offsets
        .get(&list_id)
        .map(|offset| offset.y)
        .unwrap_or(0.0);
    assert!(
        (list_offset - 100.0).abs() < 0.01,
        "scroll_to(100) should set the container offset to 100, got {list_offset}"
    );

    // Repaint so the new offset is published, then read it back via scroll_top.
    component
        .paint(&theme, SurfaceExtent::unpadded(160, 120), &mut buffer, 1.0)
        .unwrap();
    component.call_namespaced_handler("read_back", &[]).unwrap();
    let read = runtime_value(&component, "scrolled_top")
        .and_then(|value| value.as_f64())
        .unwrap_or(-1.0);
    assert!(
        (read - 100.0).abs() < 0.01,
        "refs.list.scroll_top should read back 100, got {read}"
    );
}

#[test]
fn refs_scroll_to_smooth_eases_the_offset_over_time() {
    // `refs.x:scroll_to(top, { smooth = true })` registers a ScrollAnimation
    // instead of snapping; advance_scroll_animations eases the offset to the
    // target over its duration.
    let mut component = test_frontend_component(
        r#"
<style>
scroll { height: 60px; overflow-y: auto; }
.content { height: 240px; flex-shrink: 0; }
</style>
<template>
    <scroll ref="list">
        <box class="content" />
    </scroll>
</template>
<script lang="luau">
function smooth_jump()
    refs.list:scroll_to(100, { smooth = true, duration = 200 })
end
</script>
"#,
    );

    let theme = default_theme();
    let mut buffer = PixelBuffer::new(160, 120);
    component
        .paint(&theme, SurfaceExtent::unpadded(160, 120), &mut buffer, 1.0)
        .unwrap();

    component
        .call_namespaced_handler("smooth_jump", &[])
        .unwrap();

    // Smooth scroll registers an animation and does NOT snap the offset.
    let animation = *component
        .scroll_animations
        .get(&runtime_node_id_for_key("root/0"))
        .expect("smooth scroll_to should register a ScrollAnimation");
    assert!((animation.target.y - 100.0).abs() < 0.01);
    let offset_now = component
        .scroll_offsets
        .get(&runtime_node_id_for_key("root/0"))
        .map(|o| o.y)
        .unwrap_or(0.0);
    assert!(offset_now < 1.0, "offset should not snap, got {offset_now}");

    // Halfway through, the eased offset is partway to the target (ease-out is
    // past the midpoint at t=0.5).
    component.advance_scroll_animations(animation.start_time + animation.duration / 2);
    let mid = component
        .scroll_offsets
        .get(&runtime_node_id_for_key("root/0"))
        .unwrap()
        .y;
    assert!(
        mid > 50.0 && mid < 100.0,
        "mid-animation offset should be eased between 50 and 100, got {mid}"
    );
    assert!(!component.scroll_animations.is_empty());

    // At/after the full duration, it lands exactly on the target and is dropped.
    component.advance_scroll_animations(animation.start_time + animation.duration);
    let end = component
        .scroll_offsets
        .get(&runtime_node_id_for_key("root/0"))
        .unwrap()
        .y;
    assert!(
        (end - 100.0).abs() < 0.01,
        "should settle at 100, got {end}"
    );
    assert!(
        component.scroll_animations.is_empty(),
        "animation should be dropped once settled"
    );
}

#[test]
fn reduced_motion_snaps_smooth_scroll_without_starting_inertia() {
    let mut component = scroll_interaction_component();
    component.motion_policy = mesh_core_animation::MotionPolicy::new(true);
    let viewport_id = runtime_node_id_for_key("root/0");

    assert!(component.apply_scroll_target(
        viewport_id,
        ScrollOffsetState::default(),
        ScrollOffsetState { x: 0.0, y: 100.0 },
        &serde_json::json!({ "smooth": true, "duration": 250 }),
    ));
    assert!(component.scroll_animations.is_empty());
    assert_eq!(component.scroll_offsets.get(&viewport_id).unwrap().y, 100.0);

    let now = Instant::now();
    component.scroll_inertia.insert(
        viewport_id,
        ScrollInertia {
            velocity: ScrollOffsetState { x: 0.0, y: 120.0 },
            samples: 2,
            travel: 24.0,
            last_input: now,
            last_tick: now,
            max_x: 0.0,
            max_y: 200.0,
        },
    );
    component.advance_scroll_inertia(now + Duration::from_millis(100));
    assert!(component.scroll_inertia.is_empty());
}

fn scroll_interaction_component() -> FrontendSurfaceComponent {
    test_frontend_component(
        r#"
<style>
scroll { width: 100px; height: 100px; overflow-y: auto; }
.content { width: 100px; height: 400px; flex-shrink: 0; }
</style>
<template>
    <scroll><box class="content" /></scroll>
</template>
"#,
    )
}

#[test]
fn scrollbar_thumb_can_be_dragged_with_the_pointer() {
    let mut component = scroll_interaction_component();
    let theme = default_theme();
    let mut buffer = PixelBuffer::new(120, 120);
    component
        .paint(&theme, SurfaceExtent::unpadded(120, 120), &mut buffer, 1.0)
        .unwrap();

    let tree = component.last_tree.as_ref().expect("painted tree");
    let viewport = first_node_by_tag(tree, "scroll").expect("scroll viewport");
    let x = viewport.layout.x + viewport.layout.width - 7.0;
    let initial_hit = mesh_core_interaction::find_scrollbar_at(tree, x, viewport.layout.y + 10.0)
        .expect("painted vertical scrollbar");
    assert!(initial_hit.on_thumb);
    let press_y = initial_hit.thumb_start + initial_hit.thumb_extent / 2.0;

    component
        .handle_input(
            &theme,
            120,
            120,
            ComponentInput::PointerButton {
                button: 0x110,
                x,
                y: press_y,
                pressed: true,
            },
        )
        .unwrap();
    assert!(component.active_scrollbar_drag.is_some());

    component
        .handle_input(
            &theme,
            120,
            120,
            ComponentInput::PointerMove {
                x,
                y: initial_hit.track_start + initial_hit.track_extent,
            },
        )
        .unwrap();
    let offset = component
        .scroll_offsets
        .get(&initial_hit.node_id)
        .copied()
        .unwrap_or_default();
    assert!(
        (offset.y - initial_hit.max_scroll).abs() < 0.1,
        "dragging the thumb to the end should reach max scroll: {offset:?}"
    );

    component
        .handle_input(
            &theme,
            120,
            120,
            ComponentInput::PointerButton {
                button: 0x110,
                x,
                y: initial_hit.track_start + initial_hit.track_extent,
                pressed: false,
            },
        )
        .unwrap();
    assert!(component.active_scrollbar_drag.is_none());
}

#[test]
fn wheel_scroll_eases_instead_of_snapping() {
    let mut component = scroll_interaction_component();
    let theme = default_theme();
    let mut buffer = PixelBuffer::new(120, 120);
    component
        .paint(&theme, SurfaceExtent::unpadded(120, 120), &mut buffer, 1.0)
        .unwrap();

    component
        .handle_input(
            &theme,
            120,
            120,
            ComponentInput::Scroll {
                x: 20.0,
                y: 20.0,
                dx: 0.0,
                dy: -1.0,
            },
        )
        .unwrap();
    let viewport_id = runtime_node_id_for_key("root/0");
    let animation = component
        .scroll_animations
        .get(&viewport_id)
        .expect("wheel input should create a short easing animation");
    assert!((animation.target.y - 28.0).abs() < 0.1);
    assert!(
        component
            .scroll_offsets
            .get(&viewport_id)
            .is_none_or(|offset| offset.y < 1.0),
        "wheel input should not snap to its target"
    );
}

#[test]
fn touchpad_scroll_is_pixel_precise_and_keeps_moving_after_a_flick() {
    let mut component = scroll_interaction_component();
    let theme = default_theme();
    let mut buffer = PixelBuffer::new(120, 120);
    component
        .paint(&theme, SurfaceExtent::unpadded(120, 120), &mut buffer, 1.0)
        .unwrap();

    component
        .handle_input(
            &theme,
            120,
            120,
            ComponentInput::TwoFingerScroll {
                x: 20.0,
                y: 20.0,
                dx: 0.0,
                dy: -12.0,
            },
        )
        .unwrap();
    component
        .handle_input(
            &theme,
            120,
            120,
            ComponentInput::TwoFingerScroll {
                x: 20.0,
                y: 20.0,
                dx: 0.0,
                dy: -12.0,
            },
        )
        .unwrap();
    let viewport_id = runtime_node_id_for_key("root/0");
    let direct = component
        .scroll_offsets
        .get(&viewport_id)
        .copied()
        .unwrap_or_default();
    assert!(
        (direct.y - 24.0).abs() < 0.1,
        "touchpad pixels should map one-to-one, got {direct:?}"
    );
    let inertia = *component
        .scroll_inertia
        .get(&viewport_id)
        .expect("a touchpad flick should retain velocity");
    component.advance_scroll_inertia(inertia.last_input + Duration::from_millis(100));
    let carried = component
        .scroll_offsets
        .get(&viewport_id)
        .copied()
        .unwrap_or_default();
    assert!(
        carried.y > direct.y,
        "momentum should carry the viewport after finger input: {direct:?} -> {carried:?}"
    );
}

#[test]
fn refs_click_method_fires_the_nodes_onclick_handler() {
    // `refs.<name>:click()` synthesizes a click on the live node, routing through
    // the same dispatch a real pointer release uses, so the node's onclick runs.
    let mut component = test_frontend_component(
        r#"
<template>
    <button ref="action" onclick="on_action">Go</button>
</template>
<script lang="luau">
clicks = 0
function on_action()
    clicks = clicks + 1
end
function trigger()
    refs.action:click()
end
</script>
"#,
    );

    let theme = default_theme();
    let mut buffer = PixelBuffer::new(160, 60);
    component
        .paint(&theme, SurfaceExtent::unpadded(160, 60), &mut buffer, 1.0)
        .unwrap();

    component.call_namespaced_handler("trigger", &[]).unwrap();
    let clicks = runtime_value(&component, "clicks")
        .and_then(|value| value.as_f64())
        .unwrap_or(0.0);
    assert!(
        (clicks - 1.0).abs() < 0.01,
        "refs.action:click() should fire onclick once, got {clicks}"
    );
}

#[test]
fn refs_value_write_sets_input_text_and_reads_back() {
    // `refs.input.value = "..."` sets the input's text; refs.input.value reads the
    // live string (not the a11y bool) back on the next paint.
    let mut component = test_frontend_component(
        r#"
<template>
    <input ref="field" type="text" value="start" />
</template>
<script lang="luau">
read_value = ""
function set_it()
    refs.field.value = "typed"
end
function read_it()
    read_value = refs.field.value
end
</script>
"#,
    );

    let theme = default_theme();
    let mut buffer = PixelBuffer::new(200, 60);
    component
        .paint(&theme, SurfaceExtent::unpadded(200, 60), &mut buffer, 1.0)
        .unwrap();

    component.call_namespaced_handler("set_it", &[]).unwrap();
    assert_eq!(
        component
            .input_values
            .get(&runtime_node_id_for_key("root/0"))
            .map(String::as_str),
        Some("typed"),
        "writing refs.field.value should update the input's stored text"
    );

    // Repaint so the new value is published, then read it back through the ref.
    component
        .paint(&theme, SurfaceExtent::unpadded(200, 60), &mut buffer, 1.0)
        .unwrap();
    component.call_namespaced_handler("read_it", &[]).unwrap();
    assert_eq!(
        runtime_value(&component, "read_value").and_then(|value| value.as_str().map(String::from)),
        Some("typed".to_string()),
        "refs.field.value should read back the live input string"
    );
}
