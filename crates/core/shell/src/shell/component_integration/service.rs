use super::*;
#[test]
fn effect_shell_requests_settled_during_paint_reach_the_shell_in_that_frame() {
    let component = test_frontend_component_with_catalog(
        r#"
<script lang="luau">
local audio = require("mesh.audio@>=1.0")
seen_percent = -1
effect(function()
    seen_percent = audio.percent or -1
    if seen_percent > 50 then
        mesh.shell.hide_surface("@test/reactive-surface")
    end
end)
</script>
<template><column style="width: 160px; height: 60px"><text>{audio.percent}</text><text>{seen_percent}</text></column></template>
<style>* { transition: none; }</style>
"#,
        audio_network_catalog(),
        &["service.audio.read", "shell.surface"],
    );
    let mut shell = crate::shell::Shell::new();
    shell.presentation_engine =
        mesh_core_presentation::PresentationEngine::testing_with_popup_support(false);
    shell.register_component(Box::new(component));
    shell
        .core
        .surfaces
        .get_mut("@test/reactive-surface")
        .unwrap()
        .visible = true;
    shell.render_components().unwrap();
    assert!(shell.core.surfaces["@test/reactive-surface"].visible);
    let requests = shell
        .broadcast_service_event(ServiceEvent::Updated {
            service: "mesh.audio".into(),
            source_module: "@mesh/pipewire-audio".into(),
            payload: serde_json::json!({"available": true, "percent": 64, "muted": false}),
        })
        .unwrap();
    assert!(
        !requests
            .iter()
            .any(|request| matches!(request.request, CoreRequest::HideSurface { .. })),
        "the effect must settle during paint"
    );
    shell.enqueue_effects(requests);
    shell.render_components().unwrap();
    assert!(
        !shell.core.surfaces["@test/reactive-surface"].visible,
        "paint-time effect must not wait for another input handler"
    );
}
