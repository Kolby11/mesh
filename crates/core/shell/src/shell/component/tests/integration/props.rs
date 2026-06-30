//! Integration coverage for the `<props>` configuration model: a declared prop
//! projects its resolved value into CSS `prop(name)` and is readable/writable as
//! `props.name` in script, through the real paint path.

use super::*;

const PROP_SOURCE: &str = r#"
<props>
  track_width: { type: "size", default: "20px" }
</props>
<template>
  <box>
    <slider class="slider"/>
    <text>{label}</text>
  </box>
</template>
<style>
.slider { width: prop(track_width); }
</style>
<script lang="luau">
label = ""
function render(self)
  label = props.track_width
end
</script>
"#;

#[test]
fn prop_default_projects_to_css_and_is_readable_in_script() {
    let mut component = test_frontend_component(PROP_SOURCE);
    let theme = default_theme();
    let mut buffer = PixelBuffer::new(200, 80);
    component.paint(&theme, 200, 80, &mut buffer, 1.0).unwrap();

    let tree = component.last_tree.as_ref().unwrap();
    let slider = first_node_by_class(tree, "slider").expect("slider node");
    assert_eq!(
        slider.computed_style.width,
        mesh_core_elements::Dimension::Px(20.0),
        "prop default should project into prop(track_width)"
    );

    let text = rendered_text(&component);
    assert!(
        text.iter().any(|line| line == "20px"),
        "script should read props.track_width; rendered text was {text:?}"
    );
}

const PROP_WRITE_SOURCE: &str = r#"
<props>
  track_width: { type: "size", default: "20px" }
</props>
<template>
  <box>
    <slider class="slider"/>
  </box>
</template>
<style>
.slider { width: prop(track_width); }
</style>
<script lang="luau">
function bump(self)
  props.track_width = "36px"
end
</script>
"#;

#[test]
fn script_write_to_props_reprojects_into_css() {
    let mut component = test_frontend_component(PROP_WRITE_SOURCE);
    let theme = default_theme();
    let mut buffer = PixelBuffer::new(200, 80);

    component.paint(&theme, 200, 80, &mut buffer, 1.0).unwrap();
    let slider = first_node_by_class(component.last_tree.as_ref().unwrap(), "slider").unwrap();
    assert_eq!(
        slider.computed_style.width,
        mesh_core_elements::Dimension::Px(20.0)
    );

    // A script write to props.track_width must round-trip to state and reproject.
    component.call_namespaced_handler("bump", &[]).unwrap();
    component.invalidate_script_state();
    component.paint(&theme, 200, 80, &mut buffer, 1.0).unwrap();

    let slider = first_node_by_class(component.last_tree.as_ref().unwrap(), "slider").unwrap();
    assert_eq!(
        slider.computed_style.width,
        mesh_core_elements::Dimension::Px(36.0),
        "a script write to props.track_width must reproject into prop(track_width)"
    );
}
