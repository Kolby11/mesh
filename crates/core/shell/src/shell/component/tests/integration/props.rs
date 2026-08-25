//! Integration coverage for the `<props>` configuration model: a declared prop
//! projects its resolved value into CSS `prop(name)` and is readable/writable as
//! `props.name` in script, through the real paint path.

use super::*;

const PROP_SOURCE: &str = r#"
<props>
  track_width: { type: "size", default: "20px" }
  visible: { type: "bool", default: true }
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
    component
        .paint(&theme, SurfaceExtent::unpadded(200, 80), &mut buffer, 1.0)
        .unwrap();

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

#[test]
fn component_runtime_does_not_inject_raw_settings_global() {
    let component = test_frontend_component(PROP_SOURCE);
    let has_raw_settings = component
        .runtimes
        .lock()
        .unwrap()
        .get(component.root_instance_key())
        .and_then(|runtime| runtime.script_ctx.state().get_ref("settings"))
        .is_some();

    assert!(!has_raw_settings);
}

const SETTINGS_PROP_SOURCE: &str = r#"
<props>
  track_width: { type: "size", default: "20px" }
  anim_ms: { type: "duration", default: 120, min: 0, max: 600 }
</props>
<template>
  <box class="panel">
    <slider class="slider"/>
  </box>
</template>
<style>
.panel { transition: opacity prop(anim_ms) linear; }
.slider { width: prop(track_width); }
</style>
"#;

#[test]
fn settings_props_apply_global_and_per_instance_precedence() {
    let mut component = test_frontend_component(SETTINGS_PROP_SOURCE);
    component.settings_json = serde_json::json!({
        "props": {
            "global": { "track_width": "28px" },
            "instances": {
                "@test/reactive-surface": { "track_width": "44px" }
            }
        }
    });
    component.runtimes.lock().unwrap().clear();
    component.init_root_runtime().unwrap();

    let theme = default_theme();
    let mut buffer = PixelBuffer::new(200, 80);
    component
        .paint(&theme, SurfaceExtent::unpadded(200, 80), &mut buffer, 1.0)
        .unwrap();

    let slider = first_node_by_class(component.last_tree.as_ref().unwrap(), "slider").unwrap();
    assert_eq!(
        slider.computed_style.width,
        mesh_core_elements::Dimension::Px(44.0),
        "per-instance setting should beat global setting for the root instance key"
    );
}

#[test]
fn duration_prop_projects_to_css_milliseconds() {
    let mut component = test_frontend_component(SETTINGS_PROP_SOURCE);
    component.settings_json = serde_json::json!({
        "props": { "global": { "anim_ms": 240 } }
    });
    component.runtimes.lock().unwrap().clear();
    component.init_root_runtime().unwrap();

    let theme = default_theme();
    let mut buffer = PixelBuffer::new(200, 80);
    component
        .paint(&theme, SurfaceExtent::unpadded(200, 80), &mut buffer, 1.0)
        .unwrap();

    let panel = first_node_by_class(component.last_tree.as_ref().unwrap(), "panel").unwrap();
    assert!(
        panel
            .computed_style
            .transitions
            .iter()
            .any(|transition| { transition.properties.opacity && transition.duration_ms == 240 }),
        "duration prop should project numeric 240 as 240ms transition; transitions were {:?}",
        panel.computed_style.transitions
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

    component
        .paint(&theme, SurfaceExtent::unpadded(200, 80), &mut buffer, 1.0)
        .unwrap();
    let slider = first_node_by_class(component.last_tree.as_ref().unwrap(), "slider").unwrap();
    assert_eq!(
        slider.computed_style.width,
        mesh_core_elements::Dimension::Px(20.0)
    );

    // A script write to props.track_width must round-trip to state and reproject.
    component.call_namespaced_handler("bump", &[]).unwrap();
    component.invalidate_script_state();
    component
        .paint(&theme, SurfaceExtent::unpadded(200, 80), &mut buffer, 1.0)
        .unwrap();

    let slider = first_node_by_class(component.last_tree.as_ref().unwrap(), "slider").unwrap();
    assert_eq!(
        slider.computed_style.width,
        mesh_core_elements::Dimension::Px(36.0),
        "a script write to props.track_width must reproject into prop(track_width)"
    );
}

const INVALID_PROP_WRITE_SOURCE: &str = r#"
<props>
  track_width: { type: "size", default: "20px" }
</props>
<template>
  <slider class="slider"/>
</template>
<style>
.slider { width: prop(track_width); }
</style>
<script lang="luau">
function invalidate(self)
  props.track_width = "not-a-size"
end
</script>
"#;

#[test]
fn invalid_script_override_falls_back_to_the_valid_host_layer() {
    let mut component = test_frontend_component(INVALID_PROP_WRITE_SOURCE);
    component.settings_json = serde_json::json!({
        "props": { "global": { "track_width": "28px" } }
    });
    component.runtimes.lock().unwrap().clear();
    component.init_root_runtime().unwrap();

    component
        .call_namespaced_handler("invalidate", &[])
        .unwrap();
    component.invalidate_script_state();

    let theme = default_theme();
    let mut buffer = PixelBuffer::new(200, 80);
    component
        .paint(&theme, SurfaceExtent::unpadded(200, 80), &mut buffer, 1.0)
        .unwrap();

    let slider = first_node_by_class(component.last_tree.as_ref().unwrap(), "slider").unwrap();
    assert_eq!(
        slider.computed_style.width,
        mesh_core_elements::Dimension::Px(28.0),
        "an invalid script value must not displace a valid global prop"
    );
    let props = component
        .runtimes
        .lock()
        .unwrap()
        .get(component.root_instance_key())
        .and_then(|runtime| runtime.script_ctx.state().get_ref("props").cloned())
        .expect("runtime props");
    assert_eq!(props["track_width"], serde_json::json!("28px"));
}

#[test]
fn settings_reload_drops_an_invalid_override_to_the_declared_default() {
    let mut component = test_frontend_component(SETTINGS_PROP_SOURCE);
    let valid = test_settings_store_with(
        "@test/reactive-surface",
        serde_json::json!({ "props": { "global": { "anim_ms": 240 } } }),
    );
    component.apply_settings(&valid).unwrap();

    let invalid = test_settings_store_with(
        "@test/reactive-surface",
        serde_json::json!({ "props": { "global": { "anim_ms": 900 } } }),
    );
    component.apply_settings(&invalid).unwrap();

    let props = component
        .runtimes
        .lock()
        .unwrap()
        .get(component.root_instance_key())
        .and_then(|runtime| runtime.script_ctx.state().get_ref("props").cloned())
        .expect("runtime props");
    assert_eq!(props["anim_ms"], serde_json::json!(120.0));
}

#[test]
fn settings_and_direct_role_changes_cannot_bypass_the_author_promotable_guard() {
    let mut component = test_frontend_component("<template><box /></template>");
    let reloaded = test_settings_store_with(
        "@test/reactive-surface",
        serde_json::json!({ "surface": { "role": "window" } }),
    );

    component.apply_settings(&reloaded).unwrap();
    assert_eq!(
        component.surface_role(),
        mesh_core_wayland::SurfaceRole::Layer
    );
    assert!(
        !component.surface_role_changed(mesh_core_wayland::SurfaceRole::Window),
        "the component boundary must reject an unauthorized live role change"
    );
    assert_eq!(
        component.surface_role(),
        mesh_core_wayland::SurfaceRole::Layer
    );
}

#[test]
fn settings_reload_preserves_a_higher_precedence_script_prop() {
    let mut component = test_frontend_component(PROP_WRITE_SOURCE);
    component.call_namespaced_handler("bump", &[]).unwrap();
    component.invalidate_script_state();

    let reloaded = test_settings_store_with(
        "@test/reactive-surface",
        serde_json::json!({ "props": { "global": { "track_width": "28px" } } }),
    );
    component.apply_settings(&reloaded).unwrap();

    let props = component
        .runtimes
        .lock()
        .unwrap()
        .get(component.root_instance_key())
        .and_then(|runtime| runtime.script_ctx.state().get_ref("props").cloned())
        .expect("runtime props");
    assert_eq!(props["track_width"], serde_json::json!("36px"));
}
