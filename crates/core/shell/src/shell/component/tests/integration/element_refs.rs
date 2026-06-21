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
    component.paint(&theme, 200, 60, &mut buffer, 1.0).unwrap();
    assert_eq!(component.focused_key, None);

    // The handler focuses the input through its live reference.
    component.call_namespaced_handler("focus_field", &[]).unwrap();
    assert_eq!(
        component.focused_key.as_deref(),
        Some("root/0"),
        "refs.field:focus() should focus the input's live node"
    );

    // Blurring the focused element clears focus.
    component.call_namespaced_handler("blur_field", &[]).unwrap();
    assert_eq!(component.focused_key, None);
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
    component.paint(&theme, 200, 60, &mut buffer, 1.0).unwrap();

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
fn refs_scroll_into_view_scrolls_the_container_to_reveal_the_target() {
    // A script calling `refs.<name>:scroll_into_view()` enqueues an element action
    // that the shell turns into scroll-offset adjustments on the real overflowing
    // container, routed through the same scroll_offsets map the wheel uses.
    let mut component = test_frontend_component(
        r#"
<style>
scroll { height: 60px; overflow-y: auto; }
.content { height: 240px; }
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
    component.paint(&theme, 160, 120, &mut buffer, 1.0).unwrap();
    // Nothing scrolled yet.
    assert!(component.scroll_offsets.values().all(|o| o.y == 0.0));

    component.call_namespaced_handler("reveal", &[]).unwrap();

    // The target sits at ~200px inside a 60px scroll viewport (content 240px), so
    // the container must scroll its trailing edge into view.
    let scrolled = component
        .scroll_offsets
        .values()
        .any(|offset| offset.y > 1.0);
    assert!(
        scrolled,
        "scroll_into_view should move the container offset, got {:?}",
        component.scroll_offsets
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
.content { height: 240px; }
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
    component.paint(&theme, 160, 120, &mut buffer, 1.0).unwrap();

    // Set the offset through the live reference.
    component.call_namespaced_handler("jump", &[]).unwrap();
    let list_offset = component
        .scroll_offsets
        .iter()
        .find(|(key, _)| key.as_str() == "root/0")
        .map(|(_, offset)| offset.y)
        .unwrap_or(0.0);
    assert!(
        (list_offset - 100.0).abs() < 0.01,
        "scroll_to(100) should set the container offset to 100, got {list_offset}"
    );

    // Repaint so the new offset is published, then read it back via scroll_top.
    component.paint(&theme, 160, 120, &mut buffer, 1.0).unwrap();
    component.call_namespaced_handler("read_back", &[]).unwrap();
    let read = runtime_value(&component, "scrolled_top")
        .and_then(|value| value.as_f64())
        .unwrap_or(-1.0);
    assert!(
        (read - 100.0).abs() < 0.01,
        "refs.list.scroll_top should read back 100, got {read}"
    );
}
