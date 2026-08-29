use crate::package::installed_graph::extract_backend_event_names;
use crate::package::installed_graph::extract_frontend_interface_event_subscriptions;
use crate::package::installed_graph::extract_mesh_event_publish_channels;
use crate::package::installed_graph::extract_t_keys_from_mesh_source;
use crate::package::installed_graph::scan_mesh_source;

#[test]
fn extract_icon_names_from_mesh_source_finds_static_names() {
    let src = r#"
<template>
  <row>
    <icon name="audio-volume-high" size="24"/>
    <icon name="battery-full"/>
  </row>
</template>
"#;
    let names = scan_mesh_source(src).icon_names;
    assert!(names.contains(&"audio-volume-high".into()));
    assert!(names.contains(&"battery-full".into()));
}

#[test]
fn extract_icon_names_ignores_dynamic_expressions() {
    let src = r#"
<template>
  <row>
    <icon name={icon_name} />
    <icon name="audio-volume-muted"/>
  </row>
</template>
"#;
    let names = scan_mesh_source(src).icon_names;
    assert!(!names.iter().any(|n| n.contains('{')));
    assert!(names.contains(&"audio-volume-muted".into()));
}

#[test]
fn extract_icon_names_from_mesh_source_walks_control_flow_branches() {
    let src = r#"
<template>
  @if muted
    <icon name="audio-volume-muted"/>
  @else
    <icon name="audio-volume-high"/>
  @endif
  @for item in items
    <icon name="battery-full"/>
  @endfor
</template>
"#;
    assert_eq!(
        scan_mesh_source(src).icon_names,
        vec![
            "audio-volume-high".to_string(),
            "audio-volume-muted".to_string(),
            "battery-full".to_string(),
        ]
    );
}

#[test]
fn extract_t_keys_from_mesh_source_finds_static_keys() {
    let src = r#"
<template>
    <box>
        <text>{t('nav.volume')}</text>
        <text aria-label={t("nav.mute")}/>
        <text>{t(dynamic_key)}</text>
    </box>
</template>
    "#;
    let keys = extract_t_keys_from_mesh_source(src);
    assert!(
        keys.contains(&"nav.volume".into()),
        "single-quote key should be found"
    );
    assert!(
        keys.contains(&"nav.mute".into()),
        "double-quote key should be found"
    );
    assert!(
        !keys.iter().any(|k: &String| k.contains("dynamic")),
        "dynamic key must not appear"
    );
}

#[test]
fn extract_t_keys_from_mesh_source_includes_localized_props() {
    let src = r#"
<props>
  volume: { type: "int", label: t("settings.volume.label"), description: t("settings.volume.description") }
  hidden: { type: "bool", label: t("settings.hidden.label"), expose: false }
</props>
<template><text>{t("template.title")}</text></template>
"#;
    assert_eq!(
        extract_t_keys_from_mesh_source(src),
        vec![
            "settings.hidden.label".to_string(),
            "settings.volume.description".to_string(),
            "settings.volume.label".to_string(),
            "template.title".to_string(),
        ]
    );
}

#[test]
fn extract_t_keys_ignores_dynamic_expressions() {
    let src = r#"<template><box>{t(audio_title_key)}{t("audio.fixed")}</box></template>"#;
    let keys = extract_t_keys_from_mesh_source(src);
    assert_eq!(keys, vec!["audio.fixed".to_string()]);
}

#[test]
fn extract_mesh_event_publish_channels_finds_static_channels() {
    let src = r#"
<script>
mesh.events.publish("shell.set-theme", { theme_id = "dark" })
mesh.events.publish('mesh.hyprland.switch_workspace', { id = 1 })
</script>
"#;
    let channels = extract_mesh_event_publish_channels(src);
    assert_eq!(
        channels,
        vec!["mesh.hyprland.switch_workspace", "shell.set-theme"]
    );
}

#[test]
fn extract_mesh_event_publish_channels_ignores_dynamic_channels() {
    let src = r#"
<script>
local channel = "mesh." .. domain
mesh.events.publish(channel, {})
</script>
"#;
    let channels = extract_mesh_event_publish_channels(src);
    assert!(channels.is_empty());
}

#[test]
fn extract_backend_event_names_finds_static_provider_handles() {
    let src = r#"
function on_poll(self)
    self.VolumeChanged:fire({ level = 67 })
    self.DeviceChanged:fire({ id = "default" })
end
"#;
    let names = extract_backend_event_names(src);
    assert_eq!(names, vec!["DeviceChanged", "VolumeChanged"]);
}

#[test]
fn extract_frontend_interface_event_subscriptions_finds_static_proxy_events() {
    let src = r#"
<template><box /></template>
<script lang="luau">
local audio = require("mesh.audio")
local power = require('mesh.power')
local dynamic = import(interface_name)

audio.VolumeChanged:on(function(_event) end)
audio.events.DeviceChanged:subscribe(function(_event) end)
power.BatteryChanged:on(function(_event) end)
dynamic.Ignored:on(function(_event) end)
</script>
"#;

    assert_eq!(
        extract_frontend_interface_event_subscriptions(src),
        vec![
            ("mesh.audio".into(), "DeviceChanged".into()),
            ("mesh.audio".into(), "VolumeChanged".into()),
            ("mesh.power".into(), "BatteryChanged".into()),
        ]
    );
}

#[test]
fn extract_keybind_subscriptions_from_mesh_source_finds_static_actions() {
    let src = r#"
<template>
  <button keybind="{this.keybinds.mute.id}" onkeybind={onMute}></button>
  <button keybind="open"></button>
  <button keybind="{dynamic_id}" onkeybind={onDynamic}></button>
</template>
"#;
    let subscriptions = scan_mesh_source(src).keybind_subscriptions;
    assert_eq!(
        subscriptions,
        vec![("mute".to_string(), true), ("open".to_string(), false)]
    );
}

#[test]
fn extract_keybind_subscriptions_handles_quoted_angle_brackets_in_tag() {
    let src = r#"
<template>
  <button title="2 < 3" keybind="open" data-note="x > y" onkeybind={onOpen}></button>
</template>
"#;
    let subscriptions = scan_mesh_source(src).keybind_subscriptions;
    assert_eq!(subscriptions, vec![("open".to_string(), true)]);
}

#[test]
fn extract_keybind_subscriptions_walks_nested_control_flow_and_deduplicates() {
    let src = r#"
<template>
  <column>
    @if muted
      <button keybind="{this.keybinds.mute.id}" onkeybind={onMute}></button>
    @else
      <button keybind="open"></button>
    @endif
    @for item in items
      <row><button keybind="open" onkeybind={onOpen}></button></row>
    @endfor
    <ActionButton keybind="component" onkeybind={onComponent}></ActionButton>
  </column>
</template>
<script lang="luau">
local ActionButton = require("./action-button.mesh")
</script>
"#;

    assert_eq!(
        scan_mesh_source(src).keybind_subscriptions,
        vec![
            ("component".to_string(), true),
            ("mute".to_string(), true),
            ("open".to_string(), false),
            ("open".to_string(), true),
        ]
    );
}

#[test]
fn extract_keybind_subscriptions_ignores_dynamic_and_malformed_sources() {
    let dynamic = r#"
<template>
  <button keybind="{dynamic_id}" onkeybind={onDynamic}></button>
  <button keybind="invalid.action" onkeybind={onInvalid}></button>
</template>
"#;
    assert!(scan_mesh_source(dynamic).keybind_subscriptions.is_empty());
    assert!(
        scan_mesh_source("<template><button")
            .keybind_subscriptions
            .is_empty()
    );
}
