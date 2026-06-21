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
