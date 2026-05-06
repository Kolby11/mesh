use super::*;
use crate::shell::component::catalog::FrontendCatalogEntry;
use mesh_core_capability::Capability;
use mesh_core_component::parse_component;
use mesh_core_elements::{Color, LayoutRect};
use mesh_core_module::manifest::{
    CapabilitiesSection, CompatibilitySection, DependenciesSection, EntrypointsSection,
    ExportsSection, Manifest, ModuleType, PackageSection,
};
use mesh_core_scripting::ScriptContext;
use mesh_core_service::{
    ContractCapabilities, InterfaceArgument, InterfaceCatalog, InterfaceContract, InterfaceMethod,
    InterfaceProvider, parse_contract_version,
};
use std::fs;
use std::path::PathBuf;

#[test]
fn service_update_marks_component_dirty_only_when_tracked_fields_change() {
    let previous = serde_json::json!({
        "percent": 65,
        "muted": false,
        "source_module": "@mesh/pipewire-audio"
    });
    let unchanged_tracked = serde_json::json!({
        "percent": 65,
        "muted": false,
        "source_module": "@mesh/alternate-audio"
    });
    let changed_tracked = serde_json::json!({
        "percent": 66,
        "muted": false,
        "source_module": "@mesh/alternate-audio"
    });
    let tracked_fields = HashSet::from(["percent".to_string(), "muted".to_string()]);

    assert!(!tracked_service_fields_changed(
        Some(&previous),
        &unchanged_tracked,
        &tracked_fields
    ));
    assert!(tracked_service_fields_changed(
        Some(&previous),
        &changed_tracked,
        &tracked_fields
    ));
}

// ---------- helpers shared by the three integration tests below ----------

fn audio_network_catalog() -> InterfaceCatalog {
    let mut catalog = InterfaceCatalog::default();
    catalog.register_contract(InterfaceContract {
        interface: "mesh.audio".into(),
        version: parse_contract_version("1.0").unwrap(),
        file_path: PathBuf::from("<test>"),
        state_fields: Vec::new(),
        methods: vec![
            InterfaceMethod {
                name: "set_volume".into(),
                args: vec![
                    InterfaceArgument {
                        name: "device_id".into(),
                        arg_type: "string".into(),
                    },
                    InterfaceArgument {
                        name: "volume".into(),
                        arg_type: "float".into(),
                    },
                ],
                returns: None,
            },
            InterfaceMethod {
                name: "volume_up".into(),
                args: Vec::new(),
                returns: None,
            },
            InterfaceMethod {
                name: "volume_down".into(),
                args: Vec::new(),
                returns: None,
            },
            InterfaceMethod {
                name: "toggle_mute".into(),
                args: Vec::new(),
                returns: None,
            },
        ],
        events: Vec::new(),
        types: HashMap::new(),
        capabilities: ContractCapabilities::default(),
    });
    catalog.register_provider(InterfaceProvider {
        interface: "mesh.audio".into(),
        version: Some("1.0".into()),
        base_module: Some("@mesh/audio-interface".into()),
        provider_module: "@mesh/pipewire-audio".into(),
        backend_name: "PipeWire".into(),
        priority: 100,
    });
    catalog.register_contract(InterfaceContract {
        interface: "mesh.network".into(),
        version: parse_contract_version("1.0").unwrap(),
        file_path: PathBuf::from("<test>"),
        state_fields: Vec::new(),
        methods: vec![InterfaceMethod {
            name: "set_wifi_enabled".into(),
            args: vec![InterfaceArgument {
                name: "enabled".into(),
                arg_type: "bool".into(),
            }],
            returns: None,
        }],
        events: Vec::new(),
        types: HashMap::new(),
        capabilities: ContractCapabilities::default(),
    });
    catalog.register_provider(InterfaceProvider {
        interface: "mesh.network".into(),
        version: Some("1.0".into()),
        base_module: Some("@mesh/network-interface".into()),
        provider_module: "@mesh/networkmanager-network".into(),
        backend_name: "NetworkManager".into(),
        priority: 100,
    });
    catalog
}

fn audio_network_power_catalog() -> InterfaceCatalog {
    let mut catalog = audio_network_catalog();
    catalog.register_contract(InterfaceContract {
        interface: "mesh.power".into(),
        version: parse_contract_version("1.0").unwrap(),
        file_path: PathBuf::from("<test>"),
        state_fields: Vec::new(),
        methods: Vec::new(),
        events: Vec::new(),
        types: HashMap::new(),
        capabilities: ContractCapabilities::default(),
    });
    catalog.register_provider(InterfaceProvider {
        interface: "mesh.power".into(),
        version: Some("1.0".into()),
        base_module: Some("@mesh/power-interface".into()),
        provider_module: "@mesh/upower-power".into(),
        backend_name: "UPower".into(),
        priority: 100,
    });
    catalog
}

fn make_audio_ctx() -> ScriptContext {
    let mut caps = CapabilitySet::new();
    caps.grant(Capability::new("service.audio.read"));
    caps.grant(Capability::new("service.audio.control"));
    let mut ctx = ScriptContext::new("@mesh/panel", caps).unwrap();
    ctx.set_interface_catalog(audio_network_catalog());
    ctx
}

fn make_network_ctx() -> ScriptContext {
    let mut caps = CapabilitySet::new();
    caps.grant(Capability::new("service.network.read"));
    caps.grant(Capability::new("service.network.control"));
    let mut ctx = ScriptContext::new("@mesh/quick-settings", caps).unwrap();
    ctx.set_interface_catalog(audio_network_catalog());
    ctx
}

fn make_panel_ctx() -> ScriptContext {
    let mut caps = CapabilitySet::new();
    caps.grant(Capability::new("service.audio.read"));
    caps.grant(Capability::new("service.network.read"));
    caps.grant(Capability::new("service.power.read"));
    let mut ctx = ScriptContext::new("@mesh/panel", caps).unwrap();
    ctx.set_interface_catalog(audio_network_power_catalog());
    ctx
}

fn shipped_component_script(source: &str) -> String {
    parse_component(source)
        .unwrap()
        .script
        .expect("shipped component should contain a script block")
        .source
}

fn assert_no_legacy_service_callbacks(source_name: &str, source: &str) {
    for forbidden in ["mesh.service.bind", "mesh.service.on", ".on_change("] {
        assert!(
            !source.contains(forbidden),
            "{source_name} must not teach or use legacy service callback API {forbidden}"
        );
    }
}

fn minimal_test_manifest(id: &str) -> Manifest {
    Manifest {
        package: PackageSection {
            id: id.to_string(),
            name: None,
            version: "0.1.0".into(),
            module_type: ModuleType::Surface,
            api_version: "0.1".into(),
            license: None,
            description: None,
            authors: Vec::new(),
            repository: None,
        },
        compatibility: CompatibilitySection::default(),
        dependencies: DependenciesSection::default(),
        capabilities: CapabilitiesSection::default(),
        entrypoints: EntrypointsSection {
            main: Some("src/main.mesh".into()),
            settings_ui: None,
        },
        accessibility: None,
        settings: None,
        i18n: None,
        theme: None,
        service: None,
        provides: Vec::new(),
        interface: None,
        extensions: Vec::new(),
        exports: ExportsSection::default(),
        provides_slots: HashMap::new(),
        slot_contributions: HashMap::new(),
        assets: None,
        icon_requirements: mesh_core_module::IconRequirementsSection::default(),
        translations: HashMap::new(),
        surface_layout: None,
    }
}

fn test_frontend_component(source: &str) -> FrontendSurfaceComponent {
    test_frontend_component_with_catalog(source, InterfaceCatalog::default(), &[])
}

fn test_frontend_component_with_required_icons(
    source: &str,
    required_icons: &[&str],
) -> FrontendSurfaceComponent {
    let mut manifest = minimal_test_manifest("@test/reactive-surface");
    manifest.icon_requirements.required = required_icons
        .iter()
        .map(|semantic_name| (*semantic_name).to_string())
        .collect();
    let compiled = CompiledFrontendModule {
        manifest,
        source_path: PathBuf::from("src/main.mesh"),
        component: parse_component(source).unwrap(),
        local_components: HashMap::new(),
        module_component_imports: HashMap::new(),
    };
    let catalog = FrontendCatalog {
        modules: HashMap::new(),
        slot_contributions: HashMap::new(),
    };
    let mut component = FrontendSurfaceComponent::new(
        compiled,
        PathBuf::from("."),
        catalog,
        InterfaceCatalog::default(),
    );
    component
        .mount(ComponentContext {
            component_id: "@test/reactive-surface".into(),
            surface_id: "@test/reactive-surface".into(),
            diagnostics: Diagnostics::new("@test/reactive-surface"),
        })
        .unwrap();
    component.visible = true;
    component
}

fn test_frontend_component_with_catalog(
    source: &str,
    interface_catalog: InterfaceCatalog,
    required_capabilities: &[&str],
) -> FrontendSurfaceComponent {
    let manifest = minimal_test_manifest("@test/reactive-surface");
    let mut manifest = manifest;
    manifest.capabilities.required = required_capabilities
        .iter()
        .map(|capability| (*capability).to_string())
        .collect();
    let compiled = CompiledFrontendModule {
        manifest,
        source_path: PathBuf::from("src/main.mesh"),
        component: parse_component(source).unwrap(),
        local_components: HashMap::new(),
        module_component_imports: HashMap::new(),
    };
    let catalog = FrontendCatalog {
        modules: HashMap::new(),
        slot_contributions: HashMap::new(),
    };
    let mut component =
        FrontendSurfaceComponent::new(compiled, PathBuf::from("."), catalog, interface_catalog);
    component
        .mount(ComponentContext {
            component_id: "@test/reactive-surface".into(),
            surface_id: "@test/reactive-surface".into(),
            diagnostics: Diagnostics::new("@test/reactive-surface"),
        })
        .unwrap();
    component.visible = true;
    component
}

fn runtime_value(component: &FrontendSurfaceComponent, name: &str) -> Option<serde_json::Value> {
    component
        .runtimes
        .lock()
        .unwrap()
        .get(component.id())
        .and_then(|runtime| runtime.script_ctx.state().get(name))
}

fn runtime_number(component: &FrontendSurfaceComponent, name: &str) -> f64 {
    runtime_value(component, name)
        .and_then(|value| value.as_f64())
        .unwrap_or_else(|| panic!("expected numeric runtime value for {name}"))
}

fn runtime_bool(component: &FrontendSurfaceComponent, name: &str) -> bool {
    runtime_value(component, name)
        .and_then(|value| value.as_bool())
        .unwrap_or_else(|| panic!("expected boolean runtime value for {name}"))
}

fn event_node(
    tag: &str,
    key: &str,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    handlers: &[(&str, &str)],
) -> WidgetNode {
    let mut node = WidgetNode::new(tag);
    node.attributes.insert("_mesh_key".into(), key.into());
    node.layout.x = x;
    node.layout.y = y;
    node.layout.width = width;
    node.layout.height = height;
    node.event_handlers = handlers
        .iter()
        .map(|(event, handler)| ((*event).into(), (*handler).into()))
        .collect();
    node
}

fn root_with(children: Vec<WidgetNode>) -> WidgetNode {
    let mut root = WidgetNode::new("box");
    root.attributes.insert("_mesh_key".into(), "root".into());
    root.layout.width = 240.0;
    root.layout.height = 160.0;
    root.children = children;
    root
}

fn text_node(key: &str, x: f32, y: f32, width: f32, height: f32, selectable: bool) -> WidgetNode {
    let mut node = WidgetNode::new("text");
    node.attributes.insert("_mesh_key".into(), key.into());
    node.attributes
        .insert("content".into(), "Selectable text".into());
    if selectable {
        node.attributes.insert("selectable".into(), "true".into());
    }
    node.layout.x = x;
    node.layout.y = y;
    node.layout.width = width;
    node.layout.height = height;
    node
}

fn child_with_attrs(tag: &str, attrs: &[(&str, &str)]) -> WidgetNode {
    let mut node = WidgetNode::new(tag);
    for (name, value) in attrs {
        node.attributes.insert((*name).into(), (*value).into());
    }
    node
}

fn first_node_by_tag<'a>(node: &'a WidgetNode, tag: &str) -> Option<&'a WidgetNode> {
    if node.tag == tag {
        return Some(node);
    }
    node.children
        .iter()
        .find_map(|child| first_node_by_tag(child, tag))
}

fn node_by_mesh_key<'a>(node: &'a WidgetNode, key: &str) -> &'a WidgetNode {
    find_node_by_mesh_key(node, key).unwrap_or_else(|| panic!("expected node with _mesh_key {key}"))
}

fn find_node_by_mesh_key<'a>(node: &'a WidgetNode, key: &str) -> Option<&'a WidgetNode> {
    if node
        .attributes
        .get("_mesh_key")
        .is_some_and(|value| value == key)
    {
        return Some(node);
    }
    node.children
        .iter()
        .find_map(|child| find_node_by_mesh_key(child, key))
}

#[test]
fn pseudo_state_annotation_uses_stable_keys_after_rebuild() {
    let focused_key = Some("root/0".to_string());
    let hovered_path = vec!["root".to_string(), "root/0".to_string()];
    let active_key = Some("root/0".to_string());
    let checked_values = HashMap::from([("root/1".to_string(), true)]);

    let mut first_tree = root_with(vec![
        child_with_attrs("button", &[]),
        child_with_attrs("checkbox", &[]),
    ]);
    let first_button_id = first_tree.children[0].id;
    annotate_runtime_tree(
        &mut first_tree,
        "root".to_string(),
        &focused_key,
        &hovered_path,
        &active_key,
        &HashMap::new(),
        &HashMap::new(),
        &checked_values,
        &HashMap::new(),
    );

    let mut rebuilt_tree = root_with(vec![
        child_with_attrs("button", &[]),
        child_with_attrs("checkbox", &[]),
    ]);
    assert_ne!(
        first_button_id, rebuilt_tree.children[0].id,
        "rebuilt nodes should have transient ids"
    );
    annotate_runtime_tree(
        &mut rebuilt_tree,
        "root".to_string(),
        &focused_key,
        &hovered_path,
        &active_key,
        &HashMap::new(),
        &HashMap::new(),
        &checked_values,
        &HashMap::new(),
    );

    let button = node_by_mesh_key(&rebuilt_tree, "root/0");
    assert!(button.state.hovered);
    assert!(button.state.focused);
    assert!(button.state.active);

    let checkbox = node_by_mesh_key(&rebuilt_tree, "root/1");
    assert!(checkbox.state.checked);
}

#[test]
fn pseudo_state_annotation_sets_disabled_and_checked_deterministically() {
    let checked_values = HashMap::from([("root/2".to_string(), false)]);
    let mut tree = root_with(vec![
        child_with_attrs("button", &[("disabled", "true")]),
        child_with_attrs("button", &[("aria-disabled", "true")]),
        child_with_attrs("checkbox", &[("checked", "true")]),
        child_with_attrs("checkbox", &[("checked", "checked")]),
    ]);

    annotate_runtime_tree(
        &mut tree,
        "root".to_string(),
        &None,
        &[],
        &None,
        &HashMap::new(),
        &HashMap::new(),
        &checked_values,
        &HashMap::new(),
    );

    assert!(node_by_mesh_key(&tree, "root/0").state.disabled);
    assert!(node_by_mesh_key(&tree, "root/1").state.disabled);
    assert!(
        !node_by_mesh_key(&tree, "root/2").state.checked,
        "runtime checked state should override static checked attributes"
    );
    assert!(node_by_mesh_key(&tree, "root/3").state.checked);
}

#[test]
fn pseudo_state_restyle_applies_runtime_state_after_rebuild() {
    let mut component = test_frontend_component(
        r#"
<style>
button {
  background-color: #101010;
  border-color: #111111;
  opacity: 1;
}
button:hover {
  background-color: #202020;
}
button:active {
  border-color: #303030;
}
button:disabled {
  opacity: 0.4;
}
input {
  background-color: #121212;
  color: #131313;
}
input:focus {
  background-color: #404040;
}
input:focus-visible {
  color: #505050;
}
input:checked {
  background-color: #606060;
}
</style>
<template>
  <column>
    <button disabled="true" />
    <input />
    <button />
    <checkbox checked="true" />
  </column>
</template>
"#,
    );
    component.render_hooks_pending = false;
    component.focused_key = Some("root/0/1".into());
    component.hovered_path = vec!["root".into(), "root/0".into(), "root/0/2".into()];
    component.hovered_key = Some("root/0/2".into());
    component.pointer_down_key = Some("root/0/2".into());
    component.checked_values.insert("root/0/3".into(), true);

    let theme = default_theme();
    let mut buffer = PixelBuffer::new(240, 120);
    component.paint(&theme, 240, 120, &mut buffer).unwrap();
    let tree = component.last_tree.as_ref().unwrap();

    let disabled_button = node_by_mesh_key(tree, "root/0/0");
    assert!(disabled_button.state.disabled);
    assert!((disabled_button.computed_style.opacity - 0.4).abs() < f32::EPSILON);

    let focused_input = node_by_mesh_key(tree, "root/0/1");
    assert!(focused_input.state.focused);
    assert_eq!(
        focused_input.computed_style.background_color,
        Color::from_hex("#404040").unwrap()
    );
    assert_eq!(
        focused_input.computed_style.color,
        Color::from_hex("#505050").unwrap()
    );

    let active_button = node_by_mesh_key(tree, "root/0/2");
    assert!(active_button.state.hovered);
    assert!(active_button.state.active);
    assert_eq!(
        active_button.computed_style.background_color,
        Color::from_hex("#202020").unwrap()
    );
    assert_eq!(
        active_button.computed_style.border_color,
        Color::from_hex("#303030").unwrap()
    );

    let checked_box = node_by_mesh_key(tree, "root/0/3");
    assert!(checked_box.state.checked);
    assert_eq!(
        checked_box.computed_style.background_color,
        Color::from_hex("#606060").unwrap()
    );
}

#[test]
fn pseudo_state_restyle_preserves_runtime_instances_and_local_state() {
    let mut component = test_frontend_component(
        r#"
<style>
input:focus {
  background-color: #404040;
}
input:checked {
  background-color: #606060;
}
</style>
<template>
  <column>
    <input value="initial" />
    <checkbox checked="false" />
  </column>
</template>
<script lang="luau">
render_count = 0
function onRender()
    render_count = render_count + 1
end
</script>
"#,
    );
    let runtime_count_before = component.runtimes.lock().unwrap().len();
    component
        .input_values
        .insert("root/0/0".into(), "local".into());
    component.checked_values.insert("root/0/1".into(), true);
    component.focused_key = Some("root/0/0".into());

    let theme = default_theme();
    let mut buffer = PixelBuffer::new(240, 120);
    component.paint(&theme, 240, 120, &mut buffer).unwrap();
    let render_count_after_first = runtime_number(&component, "render_count");
    let runtime_count_after_first = component.runtimes.lock().unwrap().len();

    component.hovered_path = vec!["root".into(), "root/0".into(), "root/0/1".into()];
    component.hovered_key = Some("root/0/1".into());
    component.dirty = true;
    component.paint(&theme, 240, 120, &mut buffer).unwrap();

    assert_eq!(runtime_count_before, runtime_count_after_first);
    assert_eq!(
        runtime_count_before,
        component.runtimes.lock().unwrap().len()
    );
    assert_eq!(
        runtime_number(&component, "render_count"),
        render_count_after_first
    );

    let tree = component.last_tree.as_ref().unwrap();
    assert_eq!(
        node_by_mesh_key(tree, "root/0/0")
            .attributes
            .get("value")
            .map(String::as_str),
        Some("local")
    );
    assert!(node_by_mesh_key(tree, "root/0/1").state.checked);
}

#[test]
fn container_size_restyle_preserves_runtime_and_local_state() {
    let mut component = test_frontend_component(
        r#"
<style>
.panel {
  width: 100%;
  height: 100%;
  background-color: #222222;
  gap: 4px;
}
scroll {
  height: 20px;
  overflow-y: auto;
}
text {
  height: 100px;
}
@container (min-width: 400px) {
  .panel {
    background-color: #eeeeee;
    gap: 16px;
  }
  input {
    width: 180px;
  }
}
@container (max-width: 399px) {
  input {
    width: 90px;
  }
}
</style>
<template>
  <column class="panel">
    <input value="initial" />
    <slider min="0" max="100" value="25" />
    <checkbox checked="false" />
    <scroll>
      <text>Scrollable content</text>
    </scroll>
  </column>
</template>
<script lang="luau">
render_count = 0
function onRender()
    render_count = render_count + 1
end
</script>
"#,
    );
    component.surface_layout.width = 0;
    component.surface_layout.height = 0;
    component
        .input_values
        .insert("root/0/0".into(), "local".into());
    component.slider_values.insert("root/0/1".into(), 73.0);
    component.checked_values.insert("root/0/2".into(), true);
    component
        .scroll_offsets
        .insert("root/0/3".into(), ScrollOffsetState { x: 3.0, y: 14.0 });

    let theme = default_theme();
    let mut wide_buffer = PixelBuffer::new(420, 160);
    component.paint(&theme, 420, 160, &mut wide_buffer).unwrap();
    let render_count_after_wide = runtime_number(&component, "render_count");
    let runtime_count_after_wide = component.runtimes.lock().unwrap().len();
    let wide_tree = component.last_tree.as_ref().unwrap();
    assert_eq!(
        node_by_mesh_key(wide_tree, "root/0")
            .computed_style
            .background_color,
        Color::from_hex("#eeeeee").unwrap()
    );
    assert_eq!(
        node_by_mesh_key(wide_tree, "root/0/0")
            .attributes
            .get("value")
            .map(String::as_str),
        Some("local")
    );

    component.dirty = false;
    assert!(
        !component.surface_size_changed(420, 160),
        "identical consecutive dimensions should not mark the component dirty"
    );
    assert!(!component.wants_render());

    assert!(component.surface_size_changed(260, 160));
    assert!(component.wants_render());
    let mut narrow_buffer = PixelBuffer::new(260, 160);
    component
        .paint(&theme, 260, 160, &mut narrow_buffer)
        .unwrap();

    assert_eq!(
        runtime_count_after_wide,
        component.runtimes.lock().unwrap().len(),
        "size restyles must reuse the existing runtime"
    );
    assert_eq!(
        render_count_after_wide,
        runtime_number(&component, "render_count"),
        "size restyles should not rerun frontend render hooks"
    );

    let narrow_tree = component.last_tree.as_ref().unwrap();
    assert_eq!(
        node_by_mesh_key(narrow_tree, "root/0")
            .computed_style
            .background_color,
        Color::from_hex("#222222").unwrap()
    );
    let input_width = node_by_mesh_key(narrow_tree, "root/0/0")
        .computed_style
        .width;
    assert!(
        matches!(input_width, mesh_core_elements::Dimension::Px(px) if (px - 90.0).abs() < f32::EPSILON)
    );
    assert_eq!(
        node_by_mesh_key(narrow_tree, "root/0/0")
            .attributes
            .get("value")
            .map(String::as_str),
        Some("local")
    );
    assert_eq!(
        node_by_mesh_key(narrow_tree, "root/0/1")
            .attributes
            .get("value")
            .map(String::as_str),
        Some("73.00")
    );
    assert!(node_by_mesh_key(narrow_tree, "root/0/2").state.checked);
    assert_eq!(
        node_by_mesh_key(narrow_tree, "root/0/3")
            .attributes
            .get("_mesh_scroll_y")
            .map(String::as_str),
        Some("14.00")
    );
}

#[test]
fn slider_change_handler_receives_number_on_pointer_move() {
    let mut component = test_frontend_component(
        r#"
<template><box /></template>
<script lang="luau">
slider_seen = -1
function onSliderChange(value)
    slider_seen = value
end
</script>
"#,
    );
    let mut slider = event_node(
        "slider",
        "root/0",
        0.0,
        0.0,
        100.0,
        20.0,
        &[("change", "onSliderChange")],
    );
    slider.attributes.insert("min".into(), "0".into());
    slider.attributes.insert("max".into(), "1".into());
    slider.attributes.insert("value".into(), "0".into());
    component.last_tree = Some(root_with(vec![slider]));

    let theme = default_theme();
    component
        .handle_input(
            &theme,
            240,
            160,
            ComponentInput::PointerButton {
                x: 0.0,
                y: 10.0,
                pressed: true,
            },
        )
        .unwrap();
    component
        .handle_input(
            &theme,
            240,
            160,
            ComponentInput::PointerMove { x: 75.0, y: 10.0 },
        )
        .unwrap();

    assert!((runtime_number(&component, "slider_seen") - 0.75).abs() < 0.001);
}

#[test]
fn navigation_volume_slider_proves_event_state_render_flow() {
    let mut component = test_frontend_component_with_catalog(
        r#"
<template>
  <slider min="0" max="1" value="{slider_value}" onchange={onVolumeChange} />
</template>
<script lang="luau">
local audio_ok, audio = pcall(require, "@mesh/audio@>=1.0")
if not audio_ok then audio = nil end

audio_percent = 0
slider_value = 0.0
icon_name = "audio-volume-muted"
audio_tooltip = "Volume unavailable"
handler_value_type = "unset"

local function clamp_volume(value)
    local numeric = tonumber(value) or 0
    if numeric < 0 then return 0.0 end
    if numeric > 1 then return 1.0 end
    return numeric
end

local function update_audio_copy(percent, muted)
    audio_percent = percent
    slider_value = clamp_volume(percent / 100)
    if muted or percent == 0 then
        icon_name = "audio-volume-muted"
    elseif percent < 34 then
        icon_name = "audio-volume-low"
    elseif percent < 67 then
        icon_name = "audio-volume-medium"
    else
        icon_name = "audio-volume-high"
    end
    if muted then
        audio_tooltip = string.format("Volume muted at %d%%", percent)
    else
        audio_tooltip = string.format("Volume %d%%", percent)
    end
end

function onRender()
    if not audio_ok or not audio then
        icon_name = "audio-volume-muted"
        audio_tooltip = "Audio service unavailable"
        audio_percent = 0
        slider_value = 0.0
        return
    end
    local percent = math.floor(tonumber(audio.percent) or 0)
    local muted = audio.muted or false
    update_audio_copy(percent, muted)
end

function onVolumeChange(value)
    handler_value_type = type(value)
    local normalized = clamp_volume(value)
    local percent = math.floor((normalized * 100) + 0.5)
    slider_value = normalized
    update_audio_copy(percent, false)
    if audio_ok and audio then
        audio.set_volume("default", normalized)
    end
end
</script>
"#,
        audio_network_catalog(),
        &["service.audio.read", "service.audio.control"],
    );
    {
        let mut runtimes = component.runtimes.lock().unwrap();
        let runtime = runtimes.get_mut(component.id()).unwrap();
        runtime.script_ctx.apply_service_payload(
            "audio",
            &serde_json::json!({ "percent": 20, "muted": false }),
        );
        runtime.script_ctx.call_handler("onRender", &[]).unwrap();
    }
    component.render_hooks_pending = false;

    let mut slider = event_node(
        "slider",
        "root/0",
        0.0,
        0.0,
        100.0,
        20.0,
        &[("change", "onVolumeChange")],
    );
    slider.attributes.insert("min".into(), "0".into());
    slider.attributes.insert("max".into(), "1".into());
    slider.attributes.insert("value".into(), "0.2".into());
    component.last_tree = Some(root_with(vec![slider]));
    component.clear_runtime_dirty_states();
    component.dirty = false;

    let theme = default_theme();
    component
        .handle_input(
            &theme,
            240,
            160,
            ComponentInput::PointerButton {
                x: 80.0,
                y: 10.0,
                pressed: true,
            },
        )
        .unwrap();
    let requests = component
        .handle_input(
            &theme,
            240,
            160,
            ComponentInput::PointerMove { x: 50.0, y: 10.0 },
        )
        .unwrap();

    assert_eq!(
        runtime_value(&component, "handler_value_type"),
        Some(serde_json::json!("number"))
    );
    assert_eq!(
        runtime_value(&component, "audio_percent"),
        Some(serde_json::json!(50))
    );
    assert!((runtime_number(&component, "slider_value") - 0.5).abs() < 0.001);
    assert_eq!(
        runtime_value(&component, "icon_name"),
        Some(serde_json::json!("audio-volume-medium"))
    );
    assert_eq!(
        runtime_value(&component, "audio_tooltip"),
        Some(serde_json::json!("Volume 50%"))
    );
    assert!(
        component.wants_render(),
        "changed reactive globals should mark dirty"
    );

    match requests.as_slice() {
        [
            CoreRequest::ServiceCommand {
                interface,
                command,
                payload,
                ..
            },
        ] => {
            assert_eq!(interface, "mesh.audio");
            assert_eq!(command, "set_volume");
            assert_eq!(
                payload,
                &serde_json::json!({ "device_id": "default", "volume": 0.5 })
            );
        }
        other => panic!("expected one mesh.audio.set_volume request, got {other:?}"),
    }

    let mut buffer = PixelBuffer::new(240, 40);
    component.paint(&theme, 240, 40, &mut buffer).unwrap();
    let tree = component
        .last_tree
        .as_ref()
        .expect("paint should cache tree");
    let slider = first_node_by_tag(tree, "slider").expect("painted tree should contain slider");
    let rendered_value = slider
        .attributes
        .get("value")
        .and_then(|value| value.parse::<f64>().ok())
        .expect("painted slider value should be numeric");
    assert!(
        (rendered_value - 0.5).abs() < 0.001,
        "next paint should rebuild from the updated reactive slider state"
    );
    assert!(
        !component
            .runtimes
            .lock()
            .unwrap()
            .get(component.id())
            .unwrap()
            .script_ctx
            .state()
            .is_dirty(),
        "paint should consume runtime dirty state after rebuilding"
    );
}

#[test]
fn text_input_change_handler_receives_current_string() {
    let mut component = test_frontend_component(
        r#"
<template><box /></template>
<script lang="luau">
text_seen = ""
function onTextChange(value)
    text_seen = value
end
</script>
"#,
    );
    component.last_tree = Some(root_with(vec![event_node(
        "input",
        "root/0",
        0.0,
        0.0,
        100.0,
        24.0,
        &[("change", "onTextChange")],
    )]));

    let theme = default_theme();
    component
        .handle_input(
            &theme,
            240,
            160,
            ComponentInput::PointerButton {
                x: 4.0,
                y: 4.0,
                pressed: true,
            },
        )
        .unwrap();
    component
        .handle_input(&theme, 240, 160, ComponentInput::Char { ch: 'A' })
        .unwrap();

    assert_eq!(
        runtime_value(&component, "text_seen"),
        Some(serde_json::json!("A"))
    );
}

#[test]
fn switch_change_handler_receives_boolean_on_click() {
    let mut component = test_frontend_component(
        r#"
<template><box /></template>
<script lang="luau">
switch_seen = false
function onSwitchChange(value)
    switch_seen = value
end
</script>
"#,
    );
    component.last_tree = Some(root_with(vec![event_node(
        "switch",
        "root/0",
        0.0,
        0.0,
        48.0,
        24.0,
        &[("change", "onSwitchChange")],
    )]));

    let theme = default_theme();
    component
        .handle_input(
            &theme,
            240,
            160,
            ComponentInput::PointerButton {
                x: 8.0,
                y: 8.0,
                pressed: true,
            },
        )
        .unwrap();

    assert_eq!(
        runtime_value(&component, "switch_seen"),
        Some(serde_json::json!(true))
    );
}

#[test]
fn slider_release_handler_fires_once_with_current_number() {
    let mut component = test_frontend_component(
        r#"
<template><box /></template>
<script lang="luau">
release_count = 0
released_value = -1
function onSliderRelease(value)
    release_count = release_count + 1
    released_value = value
end
</script>
"#,
    );
    let mut slider = event_node(
        "slider",
        "root/0",
        0.0,
        0.0,
        100.0,
        20.0,
        &[("release", "onSliderRelease")],
    );
    slider.attributes.insert("min".into(), "0".into());
    slider.attributes.insert("max".into(), "1".into());
    slider.attributes.insert("value".into(), "0".into());
    component.last_tree = Some(root_with(vec![slider]));

    let theme = default_theme();
    component
        .handle_input(
            &theme,
            240,
            160,
            ComponentInput::PointerButton {
                x: 10.0,
                y: 10.0,
                pressed: true,
            },
        )
        .unwrap();
    component
        .handle_input(
            &theme,
            240,
            160,
            ComponentInput::PointerMove { x: 60.0, y: 10.0 },
        )
        .unwrap();
    component
        .handle_input(
            &theme,
            240,
            160,
            ComponentInput::PointerButton {
                x: 60.0,
                y: 10.0,
                pressed: false,
            },
        )
        .unwrap();

    assert_eq!(runtime_number(&component, "release_count"), 1.0);
    assert!((runtime_number(&component, "released_value") - 0.6).abs() < 0.001);
}

#[test]
fn click_handler_keeps_current_target_position_payload() {
    let mut component = test_frontend_component(
        r#"
<template><box /></template>
<script lang="luau">
click_left = -1
click_bottom = -1
function onButtonClick(event)
    click_left = event.current_target.position.margin_left
    click_bottom = event.current_target.position.margin_bottom
end
</script>
"#,
    );
    component.last_tree = Some(root_with(vec![event_node(
        "button",
        "root/0",
        32.0,
        4.0,
        80.0,
        24.0,
        &[("click", "onButtonClick")],
    )]));

    let theme = default_theme();
    component
        .handle_input(
            &theme,
            240,
            160,
            ComponentInput::PointerButton {
                x: 40.0,
                y: 10.0,
                pressed: true,
            },
        )
        .unwrap();
    component
        .handle_input(
            &theme,
            240,
            160,
            ComponentInput::PointerButton {
                x: 40.0,
                y: 10.0,
                pressed: false,
            },
        )
        .unwrap();

    assert_eq!(runtime_number(&component, "click_left"), 32.0);
    assert_eq!(runtime_number(&component, "click_bottom"), 28.0);
}

#[test]
fn pointer_release_without_requests_still_clears_active_state() {
    let mut component =
        test_frontend_component("<template><button class=\"pressable\" /></template>");
    component.last_tree = Some(root_with(vec![event_node(
        "button",
        "root/0",
        0.0,
        0.0,
        48.0,
        24.0,
        &[],
    )]));

    let theme = default_theme();
    component
        .handle_input(
            &theme,
            240,
            160,
            ComponentInput::PointerButton {
                x: 8.0,
                y: 8.0,
                pressed: true,
            },
        )
        .unwrap();

    assert!(component.wants_render(), "press should dirty the component");
    component.dirty = false;

    let release_requests = component
        .handle_input(
            &theme,
            240,
            160,
            ComponentInput::PointerButton {
                x: 8.0,
                y: 8.0,
                pressed: false,
            },
        )
        .unwrap();

    assert!(
        release_requests.is_empty(),
        "plain button release should not synthesize service requests"
    );
    assert!(
        component.wants_render(),
        "release must dirty the component so :active styling is cleared"
    );
    assert!(component.pointer_down_key.is_none());
    assert!(component.active_slider_key.is_none());
}

#[test]
fn focus_handler_fires_when_node_becomes_focused() {
    let mut component = test_frontend_component(
        r#"
<template><box /></template>
<script lang="luau">
focus_count = 0
function onInputFocus()
    focus_count = focus_count + 1
end
</script>
"#,
    );
    component.last_tree = Some(root_with(vec![event_node(
        "input",
        "root/0",
        0.0,
        0.0,
        100.0,
        24.0,
        &[("focus", "onInputFocus")],
    )]));

    let theme = default_theme();
    for _ in 0..2 {
        component
            .handle_input(
                &theme,
                240,
                160,
                ComponentInput::PointerButton {
                    x: 8.0,
                    y: 8.0,
                    pressed: true,
                },
            )
            .unwrap();
        component
            .handle_input(
                &theme,
                240,
                160,
                ComponentInput::PointerButton {
                    x: 8.0,
                    y: 8.0,
                    pressed: false,
                },
            )
            .unwrap();
    }

    assert_eq!(runtime_number(&component, "focus_count"), 1.0);
}

#[test]
fn failing_handler_is_reported_once_and_does_not_clear_render_state() {
    let mut component = test_frontend_component(
        r#"
<template><box /></template>
<script lang="luau">
function onExplode()
    error("boom")
end
</script>
"#,
    );
    component.last_tree = Some(root_with(vec![event_node(
        "button",
        "root/0",
        0.0,
        0.0,
        80.0,
        24.0,
        &[("click", "onExplode")],
    )]));
    component.dirty = false;

    let first = component.call_namespaced_handler("onExplode", &[]);
    let second = component.call_namespaced_handler("onExplode", &[]);

    assert!(first.unwrap().is_empty());
    assert!(second.unwrap().is_empty());
    assert!(
        component.last_tree.is_some(),
        "last successfully rendered tree should remain available"
    );
    let diagnostics = component.diagnostics.as_ref().expect("diagnostics handle");
    assert_eq!(diagnostics.error_count(), 1);
    assert!(!component.wants_render());
}

#[test]
fn service_update_runs_on_render_before_rebuilding_tree() {
    let mut component = test_frontend_component_with_catalog(
        r#"
<template>
  <box title="{audio_tooltip}" />
</template>
<script lang="luau">
local audio_ok, audio = pcall(require, "@mesh/audio@>=1.0")
if not audio_ok then audio = nil end

audio_tooltip = "Volume unavailable"

function onRender()
    if not audio_ok or not audio then
        audio_tooltip = "Audio service unavailable"
        return
    end
    audio_tooltip = string.format("Volume %d%%", math.floor(tonumber(audio.percent) or 0))
end
</script>
"#,
        audio_network_catalog(),
        &["service.audio.read"],
    );

    component
        .handle_service_event(&ServiceEvent::Updated {
            service: "mesh.audio".into(),
            source_module: "@mesh/pipewire-audio".into(),
            payload: serde_json::json!({ "percent": 64, "muted": false }),
        })
        .unwrap();

    let theme = default_theme();
    let mut buffer = PixelBuffer::new(240, 40);
    component.paint(&theme, 240, 40, &mut buffer).unwrap();

    assert_eq!(
        runtime_value(&component, "audio_tooltip"),
        Some(serde_json::json!("Volume 64%"))
    );
    let tree = component.last_tree.as_ref().unwrap();
    fn first_title(node: &WidgetNode) -> Option<&str> {
        node.attributes
            .get("title")
            .map(String::as_str)
            .or_else(|| node.children.iter().find_map(first_title))
    }
    assert_eq!(first_title(tree), Some("Volume 64%"));
}

#[test]
fn raw_service_state_update_schedules_repaint_without_proxy_tracking() {
    let mut component = test_frontend_component_with_catalog(
        r#"
<template>
  <box title="{last_service_update.name}" />
</template>
<script lang="luau">
</script>
"#,
        InterfaceCatalog::default(),
        &["service.audio.read"],
    );
    let theme = default_theme();
    let mut buffer = PixelBuffer::new(240, 40);
    component.paint(&theme, 240, 40, &mut buffer).unwrap();
    component.dirty = false;

    component
        .handle_service_event(&ServiceEvent::Updated {
            service: "mesh.audio".into(),
            source_module: "@mesh/pipewire-audio".into(),
            payload: serde_json::json!({ "percent": 64, "muted": false }),
        })
        .unwrap();

    assert!(
        component.wants_render(),
        "raw ScriptState changes should schedule repaint even without proxy tracking"
    );
    component.paint(&theme, 240, 40, &mut buffer).unwrap();
    let tree = component.last_tree.as_ref().unwrap();
    fn first_title(node: &WidgetNode) -> Option<&str> {
        node.attributes
            .get("title")
            .map(String::as_str)
            .or_else(|| node.children.iter().find_map(first_title))
    }
    assert_eq!(first_title(tree), Some("audio"));
}

#[test]
fn frontend_proxy_state_update_reaches_render_state() {
    let mut component = test_frontend_component_with_catalog(
        r#"
<template>
  <box title="{volumeLevel}" />
</template>
<script lang="luau">
local audio_ok, audio = pcall(require, "@mesh/audio@>=1.0")
if not audio_ok then audio = nil end

volumeLevel = 0

function onRender()
    if not audio_ok or not audio then
        volumeLevel = 0
        return
    end
    volumeLevel = audio.state.percent or 0
end
</script>
"#,
        audio_network_catalog(),
        &["service.audio.read"],
    );

    component
        .handle_service_event(&ServiceEvent::Updated {
            service: "mesh.audio".into(),
            source_module: "@mesh/pipewire-audio".into(),
            payload: serde_json::json!({ "percent": 73, "muted": false }),
        })
        .unwrap();

    let theme = default_theme();
    let mut buffer = PixelBuffer::new(240, 40);
    component.paint(&theme, 240, 40, &mut buffer).unwrap();

    assert_eq!(
        runtime_value(&component, "volumeLevel"),
        Some(serde_json::json!(73))
    );
    let tree = component.last_tree.as_ref().unwrap();
    fn first_title(node: &WidgetNode) -> Option<&str> {
        node.attributes
            .get("title")
            .map(String::as_str)
            .or_else(|| node.children.iter().find_map(first_title))
    }
    assert_eq!(first_title(tree), Some("73"));
}

#[test]
fn frontend_proxy_state_read_tracks_repaint_invalidation() {
    let mut component = test_frontend_component_with_catalog(
        r#"
<template>
  <box title="{volumeLevel}" />
</template>
<script lang="luau">
local audio_ok, audio = pcall(require, "@mesh/audio@>=1.0")
if not audio_ok then audio = nil end

volumeLevel = 0

function onRender()
    if audio_ok and audio then
        volumeLevel = audio.state.percent or 0
    end
end
</script>
"#,
        audio_network_catalog(),
        &["service.audio.read"],
    );

    let theme = default_theme();
    let mut buffer = PixelBuffer::new(240, 40);
    component
        .handle_service_event(&ServiceEvent::Updated {
            service: "mesh.audio".into(),
            source_module: "@mesh/pipewire-audio".into(),
            payload: serde_json::json!({ "percent": 20, "muted": false }),
        })
        .unwrap();
    component.paint(&theme, 240, 40, &mut buffer).unwrap();
    component.clear_runtime_dirty_states();
    component.dirty = false;
    component.render_hooks_pending = false;

    {
        let runtimes = component.runtimes.lock().unwrap();
        let runtime = runtimes.get(component.id()).unwrap();
        assert!(
            runtime
                .script_ctx
                .tracked_fields_for_service("audio")
                .contains("percent"),
            "audio.state.percent should track percent for service invalidation"
        );
    }

    component
        .handle_service_event(&ServiceEvent::Updated {
            service: "mesh.audio".into(),
            source_module: "@mesh/pipewire-audio".into(),
            payload: serde_json::json!({ "percent": 35, "muted": false }),
        })
        .unwrap();

    assert!(
        component.wants_render(),
        "changing audio.state.percent should schedule a repaint"
    );
    component.paint(&theme, 240, 40, &mut buffer).unwrap();
    assert_eq!(
        runtime_value(&component, "volumeLevel"),
        Some(serde_json::json!(35))
    );
}

#[test]
fn pcall_service_lookup_diagnostic_reaches_component_diagnostics() {
    let mut component = test_frontend_component(
        r#"
<template><box /></template>
<script lang="luau">
function onRender()
    pcall(require, "@mesh/missing@>=1.0")
end
</script>
"#,
    );

    let theme = default_theme();
    let mut buffer = PixelBuffer::new(240, 40);
    component.paint(&theme, 240, 40, &mut buffer).unwrap();

    let diagnostics = component.diagnostics.as_ref().expect("diagnostics handle");
    assert_eq!(diagnostics.error_count(), 1);
}

#[test]
fn missing_required_icon_degrades_module_without_unloading() {
    let component_source = r#"
<template>
  <box />
</template>
<script lang="luau">
</script>
"#;
    let component =
        test_frontend_component_with_required_icons(component_source, &["missing-proof"]);
    assert_eq!(component.id(), "@test/reactive-surface");
    assert!(component.visible);

    assert!(
        !component
            .record_missing_icon_diagnostic("missing-proof", vec!["material:no-such-icon".into()])
    );
    assert!(
        !component
            .record_missing_icon_diagnostic("missing-proof", vec!["material:not-present".into()])
    );

    let diagnostics = component.diagnostics.as_ref().expect("diagnostics handle");
    assert_eq!(diagnostics.error_count(), 0);
    assert!(matches!(
        diagnostics.health(),
        mesh_core_diagnostics::HealthStatus::Degraded(message)
            if message.contains("missing-proof")
    ));
    assert!(component.visible);
}

#[test]
fn icon_reliability_core_surfaces_proof() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .unwrap();
    let icon_config_path = root.join("config/icons.toml");
    let icon_config = fs::read_to_string(&icon_config_path).unwrap();
    let config = mesh_core_icon::IconConfig::from_toml_str(&icon_config).unwrap();

    let inventory = [
        "audio-volume-muted",
        "audio-volume-low",
        "audio-volume-medium",
        "audio-volume-high",
        "network-wireless",
        "bluetooth",
        "settings",
        "weather-clear-night",
        "weather-clear",
        "battery-empty",
        "battery-caution",
        "battery-low",
        "battery-good",
        "battery-full",
    ];
    for semantic_name in inventory {
        assert!(
            config.active_profile().icons.contains_key(semantic_name),
            "{semantic_name} must be mapped in config/icons.toml"
        );
    }
    assert!(config.active_profile().icons.contains_key("missing-proof"));

    let surface_manifests = [root.join("modules/frontend/navigation-bar")];
    for module_dir in surface_manifests {
        let loaded = mesh_core_module::manifest::load_manifest(&module_dir).unwrap();
        assert!(
            loaded
                .manifest
                .dependencies
                .icon_packs
                .required
                .contains(&"material".to_string()),
            "{} must declare the material icon pack",
            loaded.manifest.package.id
        );
        for semantic_name in &loaded.manifest.icon_requirements.required {
            assert!(
                config.active_profile().icons.contains_key(semantic_name),
                "{} declares unmapped icon {}",
                loaded.manifest.package.id,
                semantic_name
            );
        }
    }

    for path in [
        "modules/frontend/navigation-bar/src/main.mesh",
        "modules/frontend/navigation-bar/src/components/volume-button.mesh",
        "modules/frontend/navigation-bar/src/components/settings-button.mesh",
        "modules/frontend/navigation-bar/src/components/theme-button.mesh",
        "modules/frontend/navigation-bar/src/components/battery-widget.mesh",
        "modules/frontend/navigation-bar/src/components/battery-button.mesh",
    ] {
        let source = fs::read_to_string(root.join(path)).unwrap();
        for line in source.lines().filter(|line| line.contains("<icon")) {
            assert!(
                !line.contains("material:"),
                "{path} contains pack-specific icon: {line}"
            );
            assert!(
                !line.contains("lucide:"),
                "{path} contains pack-specific icon: {line}"
            );
            assert!(
                !line.contains(".svg"),
                "{path} contains SVG path icon: {line}"
            );
            assert!(
                !line.contains(".png"),
                "{path} contains PNG path icon: {line}"
            );
        }
    }

    let mut svg_node = WidgetNode::new("icon");
    svg_node.attributes.insert("name".into(), "settings".into());
    svg_node.attributes.insert("size".into(), "18".into());
    svg_node.layout = LayoutRect {
        x: 0.0,
        y: 0.0,
        width: 18.0,
        height: 18.0,
    };
    let mut svg_buffer = mesh_core_render::PixelBuffer::new(24, 24);
    mesh_core_render::paint_frontend_tree(&svg_node, &mut svg_buffer, 1.0, None);
    assert!(svg_buffer.data.chunks_exact(4).any(|px| px[3] > 0));

    let td = tempfile::tempdir().unwrap();
    let raster_path = td.path().join("raster.bmp");
    fs::write(&raster_path, two_by_two_bmp()).unwrap();
    let mut raster_node = WidgetNode::new("icon");
    raster_node
        .attributes
        .insert("src".into(), raster_path.to_string_lossy().to_string());
    raster_node.layout = LayoutRect {
        x: 1.0,
        y: 1.0,
        width: 12.0,
        height: 10.0,
    };
    let mut raster_buffer = mesh_core_render::PixelBuffer::new(20, 20);
    mesh_core_render::paint_frontend_tree(&raster_node, &mut raster_buffer, 1.0, None);
    assert!(raster_buffer.data.chunks_exact(4).any(|px| px[3] > 0));

    let mut missing_node = WidgetNode::new("icon");
    missing_node
        .attributes
        .insert("name".into(), "missing-proof".into());
    missing_node.attributes.insert("size".into(), "18".into());
    missing_node.layout = LayoutRect {
        x: 2.0,
        y: 2.0,
        width: 18.0,
        height: 18.0,
    };
    let mut missing_buffer = mesh_core_render::PixelBuffer::new(24, 24);
    mesh_core_render::paint_frontend_tree(&missing_node, &mut missing_buffer, 1.0, None);
    assert!(missing_buffer.data.chunks_exact(4).any(|px| px[3] > 0));

    let component = test_frontend_component(
        r#"
<template><box /></template>
<script lang="luau"></script>
"#,
    );
    assert!(
        component
            .record_missing_icon_diagnostic("missing-proof", vec!["material:no-such-icon".into()])
    );
    assert!(
        !component
            .record_missing_icon_diagnostic("missing-proof", vec!["material:no-such-icon".into()])
    );
    let diagnostics = component.diagnostics.as_ref().unwrap();
    assert_eq!(diagnostics.error_count(), 0);
    assert!(matches!(
        diagnostics.health(),
        mesh_core_diagnostics::HealthStatus::Degraded(message)
            if message.contains("missing-proof")
    ));
}

fn two_by_two_bmp() -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"BM");
    bytes.extend_from_slice(&70u32.to_le_bytes());
    bytes.extend_from_slice(&[0, 0, 0, 0]);
    bytes.extend_from_slice(&54u32.to_le_bytes());
    bytes.extend_from_slice(&40u32.to_le_bytes());
    bytes.extend_from_slice(&2i32.to_le_bytes());
    bytes.extend_from_slice(&(-2i32).to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&32u16.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&[0; 16]);
    bytes.extend_from_slice(&[0, 0, 255, 255]);
    bytes.extend_from_slice(&[0, 255, 0, 255]);
    bytes.extend_from_slice(&[255, 0, 0, 255]);
    bytes.extend_from_slice(&[0, 255, 255, 255]);
    bytes
}

#[test]
fn navigation_volume_slider_handler_error_records_diagnostic_and_keeps_last_tree() {
    let mut component = test_frontend_component(
        r#"
<template><box /></template>
<script lang="luau">
function onVolumeChange(value)
    error("slider handler error")
end
</script>
"#,
    );
    component.last_tree = Some(root_with(vec![event_node(
        "slider",
        "root/0",
        0.0,
        0.0,
        100.0,
        20.0,
        &[("change", "onVolumeChange")],
    )]));
    component.dirty = false;

    let theme = default_theme();
    let requests = component
        .handle_input(
            &theme,
            240,
            160,
            ComponentInput::PointerButton {
                x: 50.0,
                y: 10.0,
                pressed: true,
            },
        )
        .unwrap();

    assert!(requests.is_empty());
    assert!(
        component.last_tree.is_some(),
        "last successfully rendered tree should remain available after slider handler error"
    );
    let diagnostics = component.diagnostics.as_ref().expect("diagnostics handle");
    assert_eq!(diagnostics.error_count(), 1);
}

#[test]
fn handler_without_state_change_does_not_force_rebuild() {
    let mut component = test_frontend_component(
        r#"
<template>
  <button onclick={onClick}>{label}</button>
</template>

<script lang="luau">
label = "Ready"

function onClick()
    label = "Ready"
end
</script>
"#,
    );
    component.clear_runtime_dirty_states();
    component.dirty = false;

    component.call_namespaced_handler("onClick", &[]).unwrap();

    assert!(!component.wants_render());
}

#[test]
fn handler_state_change_rebuilds_next_paint() {
    let mut component = test_frontend_component(
        r#"
<template>
  <button onclick={onClick}>{label}</button>
</template>

<script lang="luau">
label = "Ready"

function onClick()
    label = "Clicked"
end
</script>
"#,
    );
    component.clear_runtime_dirty_states();
    component.dirty = false;

    component.call_namespaced_handler("onClick", &[]).unwrap();
    assert!(component.wants_render());

    let theme = default_theme();
    let mut buffer = PixelBuffer::new(96, 32);
    component.paint(&theme, 96, 32, &mut buffer).unwrap();
    component.dirty = false;

    assert!(
        !component
            .runtimes
            .lock()
            .unwrap()
            .get(component.id())
            .unwrap()
            .script_ctx
            .state()
            .is_dirty()
    );
    assert!(!component.wants_render());
}

// ---------- integration test 1: proxy field reads reach render state ----

/// Proves that a bundled-style frontend (panel or quick-settings) reading
/// service state via direct proxy field access — the same pattern used in
/// the migrated bundled surfaces — picks up the correct value after a
/// `ServiceEvent::Updated`-equivalent payload is applied, without any
/// callback registration.
#[test]
fn frontend_proxy_update_reaches_panel_or_quick_settings_render_state() {
    let mut ctx = make_audio_ctx();
    ctx.load_script(
        r#"
-- Panel-style: read audio.percent and audio.muted directly on rerender.
volumeIcon = "audio-volume-muted"
volumeLevel = 0

function onRender()
    local audio_ok, audio = pcall(require, "@mesh/audio@>=1.0")
    if not audio_ok or not audio then return end
    local pct = audio.percent or 0
    local muted = audio.muted or false
    volumeLevel = pct
    if muted or pct == 0 then
        volumeIcon = "audio-volume-muted"
    elseif pct < 34 then
        volumeIcon = "audio-volume-low"
    elseif pct < 67 then
        volumeIcon = "audio-volume-medium"
    else
        volumeIcon = "audio-volume-high"
    end
end
"#,
    )
    .unwrap();

    // Simulate a ServiceEvent::Updated payload arriving (as apply_service_payload does).
    ctx.apply_service_payload(
        "audio",
        &serde_json::json!({ "percent": 75, "muted": false }),
    );

    // The runtime calls the script's render handler on each rerender.
    ctx.call_handler("onRender", &[]).unwrap();

    // Verify that the template-visible reactive globals reflect the emitted payload,
    // proving rerender-visible service state without any callback registration.
    assert_eq!(
        ctx.state.get("volumeIcon"),
        Some(serde_json::json!("audio-volume-high")),
        "volumeIcon should be high for 75% unmuted"
    );
    assert_eq!(
        ctx.state.get("volumeLevel"),
        Some(serde_json::json!(75)),
        "volumeLevel should equal the emitted percent"
    );
    // Confirm the proxy read was tracked (needed for shell invalidation).
    let tracked = ctx.tracked_fields_for_service("audio");
    assert!(
        tracked.contains("percent"),
        "audio.percent should be in tracked fields"
    );
    assert!(
        tracked.contains("muted"),
        "audio.muted should be in tracked fields"
    );
}

// ---------- integration test 2: proxy command becomes ServiceCommand ----

/// Proves that a bundled control handler (e.g. quick-settings onToggleWiFi)
/// calling a named proxy command method publishes a `CoreRequest::ServiceCommand`
/// through the `script_events_to_requests` routing layer.
#[test]
fn frontend_proxy_command_from_bundled_handler_becomes_service_command_request() {
    let mut ctx = make_network_ctx();
    ctx.load_script(
        r#"
-- Quick-settings style: read wifi_enabled from proxy, then send the command.
wifi_enabled = false

function onToggleWiFi()
    local network_ok, network = pcall(require, "@mesh/network@>=1.0")
    if network_ok and network then
        local enabled = network.wifi_enabled or false
        network.set_wifi_enabled(not enabled)
    end
end
"#,
    )
    .unwrap();

    // Seed proxy state so wifi_enabled read returns false.
    ctx.apply_service_payload("network", &serde_json::json!({ "wifi_enabled": false }));

    ctx.call_handler("onToggleWiFi", &[]).unwrap();
    let events = ctx.drain_published_events();

    // Route published events through the same path the shell uses.
    let requests = super::super::service::script_events_to_requests(events);

    assert!(
        !requests.is_empty(),
        "onToggleWiFi should publish at least one request"
    );
    match &requests[0] {
        CoreRequest::ServiceCommand {
            interface,
            command,
            payload,
            ..
        } => {
            assert_eq!(
                interface, "mesh.network",
                "interface should be mesh.network"
            );
            assert_eq!(
                command, "set_wifi_enabled",
                "command should be set_wifi_enabled"
            );
            assert_eq!(
                payload.get("enabled").and_then(|v| v.as_bool()),
                Some(true),
                "enabled should be true (toggled from false)"
            );
        }
        other => panic!("expected ServiceCommand for network.set_wifi_enabled, got {other:?}"),
    }
}

// ---------- integration test 3: missing service keeps fallback copy -----

/// Proves that when `pcall(require, "@mesh/audio@>=1.0")` fails (e.g. the
/// interface contract is not registered in the catalog), the script still
/// produces user-visible explanatory text rather than a blank or nil surface.
#[test]
fn frontend_missing_service_keeps_visible_fallback_copy() {
    // Intentionally use an empty catalog so the require will fail.
    let mut caps = CapabilitySet::new();
    caps.grant(Capability::new("service.audio.read"));
    let mut ctx = ScriptContext::new("@mesh/panel", caps).unwrap();
    // No interface registered → pcall(require, ...) will catch the error.

    ctx.load_script(
        r#"
-- Panel-style degraded path: pcall catches the missing interface.
volumeLevel = "0"
volumeIcon = "audio-volume-muted"
batteryText = "N/A"

function onRender()
    local audio_ok, audio = pcall(require, "@mesh/audio@>=1.0")
    if not audio_ok or not audio then
        volumeLevel = "0"
        volumeIcon = "audio-volume-muted"
        -- Explicit user-visible copy — not blank.
        batteryText = "N/A"
        return
    end
    volumeLevel = tostring(audio.percent or 0)
end
"#,
    )
    .unwrap();

    // No service payload applied — provider is absent.
    ctx.call_handler("onRender", &[]).unwrap();

    // Template-visible globals must be non-empty explanatory copy.
    assert_eq!(
        ctx.state.get("batteryText"),
        Some(serde_json::json!("N/A")),
        "batteryText should be 'N/A' when service is unavailable"
    );
    assert_eq!(
        ctx.state.get("volumeLevel"),
        Some(serde_json::json!("0")),
        "volumeLevel should be '0' when service is unavailable"
    );
    assert_eq!(
        ctx.state.get("volumeIcon"),
        Some(serde_json::json!("audio-volume-muted")),
        "volumeIcon should fall back to muted when service is unavailable"
    );
}

#[test]
fn quick_settings_audio_render_state_uses_seeded_payload() {
    let mut ctx = make_audio_ctx();
    ctx.load_script(
        r#"
local audio_ok, audio = pcall(require, "@mesh/audio@>=1.0")
if not audio_ok then audio = nil end

audio_label = "0%"
audio_backend = "Unavailable"
audio_tooltip = "Volume unavailable"
icon_name = "audio-volume-muted"

function onRender()
    if not audio_ok or not audio or audio.available == false then
        audio_label = "Audio unavailable"
        audio_backend = "Unavailable"
        audio_tooltip = "Audio unavailable"
        icon_name = "audio-volume-muted"
        return
    end

    local percent = math.floor((tonumber(audio.percent) or 0) + 0.5)
    local muted = audio.muted or false
    audio_label = string.format("%d%%", percent)
    audio_backend = audio.source_module or "Unavailable"
    if muted then
        audio_tooltip = string.format("Volume muted at %d%%", percent)
    else
        audio_tooltip = string.format("Volume %d%%", percent)
    end

    if muted or percent == 0 then
        icon_name = "audio-volume-muted"
    elseif percent < 34 then
        icon_name = "audio-volume-low"
    elseif percent < 67 then
        icon_name = "audio-volume-medium"
    else
        icon_name = "audio-volume-high"
    end
end
"#,
    )
    .unwrap();

    ctx.apply_service_payload(
        "audio",
        &serde_json::json!({
            "available": true,
            "percent": 42,
            "muted": false,
            "source_module": "@mesh/pipewire-audio"
        }),
    );

    ctx.call_handler("onRender", &[]).unwrap();

    assert_eq!(ctx.state.get("audio_label"), Some(serde_json::json!("42%")));
    assert_eq!(
        ctx.state.get("audio_backend"),
        Some(serde_json::json!("@mesh/pipewire-audio"))
    );
    assert_eq!(
        ctx.state.get("audio_tooltip"),
        Some(serde_json::json!("Volume 42%"))
    );
    assert_eq!(
        ctx.state.get("icon_name"),
        Some(serde_json::json!("audio-volume-medium"))
    );
}

#[test]
fn quick_settings_audio_slider_publishes_set_volume_service_command() {
    let mut ctx = make_audio_ctx();
    ctx.load_script(
        r#"
local audio_ok, audio = pcall(require, "@mesh/audio@>=1.0")
if not audio_ok then audio = nil end

audio_percent = 0
audio_status = ""

local function clamp_percent(value)
    local numeric = tonumber(value) or 0
    if numeric < 0 then return 0 end
    if numeric > 100 then return 100 end
    return math.floor(numeric + 0.5)
end

function onVolumeChange(value)
    local percent = clamp_percent(value)
    audio_percent = percent
    if audio_ok and audio and audio.available ~= false then
        audio.set_volume("default", percent / 100)
    else
        audio_status = "Audio controls unavailable"
    end
end
"#,
    )
    .unwrap();
    ctx.apply_service_payload("audio", &serde_json::json!({ "available": true }));

    ctx.call_handler("onVolumeChange", &[serde_json::json!(42)])
        .unwrap();
    let requests = super::super::service::script_events_to_requests(ctx.drain_published_events());

    match requests.as_slice() {
        [
            CoreRequest::ServiceCommand {
                interface,
                command,
                payload,
                ..
            },
        ] => {
            assert_eq!(interface, "mesh.audio");
            assert_eq!(command, "set_volume");
            assert_eq!(
                payload,
                &serde_json::json!({ "device_id": "default", "volume": 0.42 })
            );
        }
        other => panic!("expected one mesh.audio set_volume command, got {other:?}"),
    }
}

#[test]
fn quick_settings_network_toggle_publishes_set_wifi_enabled_service_command() {
    let mut ctx = make_network_ctx();
    ctx.load_script(
        r#"
local network_ok, network = pcall(require, "@mesh/network@>=1.0")
if not network_ok then network = nil end

network_status = ""

function onToggleWiFi()
    if not network_ok or not network or network.available == false then
        network_status = "Network unavailable"
        return
    end
    if network.controls_available == false or network.permission_denied == true then
        network_status = "Network controls unavailable"
        return
    end
    network.set_wifi_enabled(not (network.wifi_enabled or false))
end
"#,
    )
    .unwrap();
    ctx.apply_service_payload(
        "network",
        &serde_json::json!({ "available": true, "wifi_enabled": false }),
    );

    ctx.call_handler("onToggleWiFi", &[]).unwrap();
    let requests = super::super::service::script_events_to_requests(ctx.drain_published_events());

    match requests.as_slice() {
        [
            CoreRequest::ServiceCommand {
                interface,
                command,
                payload,
                ..
            },
        ] => {
            assert_eq!(interface, "mesh.network");
            assert_eq!(command, "set_wifi_enabled");
            assert_eq!(payload, &serde_json::json!({ "enabled": true }));
        }
        other => panic!("expected one mesh.network set_wifi_enabled command, got {other:?}"),
    }
}

#[test]
fn quick_settings_missing_services_keep_visible_fallback_copy() {
    let mut caps = CapabilitySet::new();
    caps.grant(Capability::new("service.audio.read"));
    caps.grant(Capability::new("service.network.read"));
    let mut ctx = ScriptContext::new("@mesh/quick-settings", caps).unwrap();

    ctx.load_script(
        r#"
local audio_ok, audio = pcall(require, "@mesh/audio@>=1.0")
if not audio_ok then audio = nil end
local network_ok, network = pcall(require, "@mesh/network@>=1.0")
if not network_ok then network = nil end

audio_status = ""
network_status = ""

function onRender()
    if not audio_ok or not audio or audio.available == false then
        audio_status = "Audio unavailable"
    end
    if not network_ok or not network or network.available == false then
        network_status = "Network unavailable"
    end
end
"#,
    )
    .unwrap();

    ctx.call_handler("onRender", &[]).unwrap();

    assert_eq!(
        ctx.state.get("audio_status"),
        Some(serde_json::json!("Audio unavailable"))
    );
    assert_eq!(
        ctx.state.get("network_status"),
        Some(serde_json::json!("Network unavailable"))
    );
}

#[test]
fn quick_settings_wifi_row_empty_id_is_display_only() {
    let mut ctx = make_network_ctx();
    ctx.load_script(
        r#"
network_id = ""
connection_status = ""

function onConnectWiFi()
    if not network_id or network_id == "" then
        connection_status = "Connection details unavailable"
        return
    end

    local ok, network = pcall(require, "@mesh/network@>=1.0")
    if ok and network and network.available ~= false then
        network.connect(network_id)
    end
end
"#,
    )
    .unwrap();

    ctx.call_handler("onConnectWiFi", &[]).unwrap();
    let requests = super::super::service::script_events_to_requests(ctx.drain_published_events());

    assert!(
        requests.is_empty(),
        "empty network_id must not publish connect"
    );
    assert_eq!(
        ctx.state.get("connection_status"),
        Some(serde_json::json!("Connection details unavailable"))
    );
}

#[test]
fn quick_settings_wifi_row_publishes_connect_for_wifi_network_ids() {
    let mut ctx = make_network_ctx();
    ctx.load_script(
        r#"
network_id = "wifi:OfficeNet"

function onConnectWiFi()
    local ok, network = pcall(require, "@mesh/network@>=1.0")
    if ok and network and network.available ~= false then
        network.connect(network_id)
    end
end
"#,
    )
    .unwrap();
    ctx.apply_service_payload("network", &serde_json::json!({ "available": true }));

    ctx.call_handler("onConnectWiFi", &[]).unwrap();
    let requests = super::super::service::script_events_to_requests(ctx.drain_published_events());

    match requests.as_slice() {
        [
            CoreRequest::ServiceCommand {
                interface,
                command,
                payload,
                ..
            },
        ] => {
            assert_eq!(interface, "mesh.network");
            assert_eq!(command, "connect");
            assert_eq!(
                payload,
                &serde_json::json!({ "connection_id": "wifi:OfficeNet" })
            );
        }
        other => panic!("expected one mesh.network connect command, got {other:?}"),
    }
}

#[test]
fn quick_settings_bluetooth_chip_selects_bluetooth_section() {
    let mut ctx = make_network_ctx();
    ctx.load_script(
        r#"
active_section = "wifi"
wifi_nav_class = "nav-btn nav-active"
bt_nav_class = "nav-btn"
audio_nav_class = "nav-btn"

function sync_nav_classes()
    wifi_nav_class = active_section == "wifi" and "nav-btn nav-active" or "nav-btn"
    bt_nav_class = active_section == "bluetooth" and "nav-btn nav-active" or "nav-btn"
    audio_nav_class = active_section == "audio" and "nav-btn nav-active" or "nav-btn"
end

function onSelectBluetooth()
    if active_section == "bluetooth" then
        active_section = ""
    else
        active_section = "bluetooth"
    end
    sync_nav_classes()
end

function onToggleBluetooth()
    onSelectBluetooth()
end
"#,
    )
    .unwrap();

    ctx.call_handler("onToggleBluetooth", &[]).unwrap();

    assert_eq!(
        ctx.state.get("active_section"),
        Some(serde_json::json!("bluetooth"))
    );
    assert_eq!(
        ctx.state.get("bt_nav_class"),
        Some(serde_json::json!("nav-btn nav-active"))
    );
    assert_eq!(
        ctx.state.get("wifi_nav_class"),
        Some(serde_json::json!("nav-btn"))
    );
}

#[test]
fn real_core_surfaces_quick_settings_close_publishes_hide_surface() {
    let mut ctx = make_network_ctx();
    ctx.load_script(
        r#"
function onClose()
    mesh.events.publish("shell.hide-surface", { surface_id = "@mesh/quick-settings" })
end
"#,
    )
    .unwrap();

    ctx.call_handler("onClose", &[]).unwrap();
    let requests = super::super::service::script_events_to_requests(ctx.drain_published_events());

    match requests.as_slice() {
        [CoreRequest::HideSurface { surface_id }] => {
            assert_eq!(surface_id, "@mesh/quick-settings");
        }
        other => panic!("expected quick settings HideSurface request, got {other:?}"),
    }
}

#[test]
fn navigation_volume_button_opens_audio_surface_via_parent_handler() {
    let button_component = parse_component(
        r#"
<template>
  <button onclick={onActivate}>Volume</button>
</template>

<script lang="luau">
function onActivate()
end
</script>
"#,
    )
    .unwrap();
    let root_component = parse_component(
        r#"
<template>
  <row>
    <VolumeButton onActivate={audio_surface_handler} />
    <AudioPopover hidden={audio_surface_hidden} />
  </row>
</template>

<script lang="luau">
import AudioPopover from "@mesh/audio-popover"
import VolumeButton from "./components/volume-button.mesh"

audio_surface_id = "@mesh/audio-popover"
audio_surface_hidden = true
audio_surface_handler = "__mesh_embed__::@mesh/navigation-bar::onToggleAudioSurface"

function onToggleAudioSurface(event)
    local position = event.current_target.position or {}
    local margin_left = tonumber(position.margin_left) or 0
    local margin_top = (tonumber(position.margin_bottom) or 0) + 8

    if audio_surface_hidden then
        mesh.events.publish("shell.position-surface", {
            surface_id = audio_surface_id,
            margin_top = margin_top,
            margin_left = margin_left
        })
    end

    audio_surface_hidden = not audio_surface_hidden
end
</script>
"#,
    )
    .unwrap();
    let popover_component = parse_component("<template><box /></template>").unwrap();

    let mut root_manifest = minimal_test_manifest("@mesh/navigation-bar");
    root_manifest.dependencies.modules.insert(
        "@mesh/audio-popover".into(),
        mesh_core_module::manifest::DependencySpec::Simple(">=0.1.0".into()),
    );
    let popover_manifest = minimal_test_manifest("@mesh/audio-popover");

    let root_compiled = CompiledFrontendModule {
        manifest: root_manifest,
        source_path: PathBuf::from("src/main.mesh"),
        component: root_component,
        local_components: HashMap::from([("VolumeButton".into(), button_component)]),
        module_component_imports: HashMap::from([(
            "AudioPopover".into(),
            "@mesh/audio-popover".into(),
        )]),
    };
    let popover_compiled = CompiledFrontendModule {
        manifest: popover_manifest,
        source_path: PathBuf::from("src/main.mesh"),
        component: popover_component,
        local_components: HashMap::new(),
        module_component_imports: HashMap::new(),
    };
    let catalog = FrontendCatalog {
        modules: HashMap::from([
            (
                "@mesh/navigation-bar".into(),
                FrontendCatalogEntry {
                    module_dir: PathBuf::from("."),
                    compiled: root_compiled.clone(),
                },
            ),
            (
                "@mesh/audio-popover".into(),
                FrontendCatalogEntry {
                    module_dir: PathBuf::from("."),
                    compiled: popover_compiled,
                },
            ),
        ]),
        slot_contributions: HashMap::new(),
    };
    let mut component = FrontendSurfaceComponent::new(
        root_compiled,
        PathBuf::from("."),
        catalog,
        InterfaceCatalog::default(),
    );
    component
        .mount(ComponentContext {
            component_id: "@mesh/navigation-bar".into(),
            surface_id: "@mesh/navigation-bar".into(),
            diagnostics: Diagnostics::new("@mesh/navigation-bar"),
        })
        .unwrap();
    component.visible = true;

    let theme = default_theme();
    let mut buffer = PixelBuffer::new(220, 80);
    component.paint(&theme, 220, 80, &mut buffer).unwrap();
    let tree = component.last_tree.as_ref().expect("rendered tree");
    let button = first_node_by_tag(tree, "button").expect("button node");
    let handler = button
        .event_handlers
        .get("click")
        .expect("click handler")
        .clone();

    assert_eq!(
        handler,
        "__mesh_embed__::@mesh/navigation-bar::onToggleAudioSurface"
    );

    let requests = component
        .call_namespaced_handler(
            &handler,
            &[serde_json::json!({
                "current_target": {
                    "position": {
                        "margin_left": 32,
                        "margin_bottom": 40
                    }
                }
            })],
        )
        .unwrap();

    match requests.as_slice() {
        [
            CoreRequest::PositionSurface {
                surface_id,
                margin_top,
                margin_left,
            },
        ] => {
            assert_eq!(surface_id, "@mesh/audio-popover");
            assert_eq!(*margin_left, 32);
            assert_eq!(*margin_top, 48);
        }
        other => panic!("expected audio popover position request, got {other:?}"),
    }

    assert!(!runtime_bool(&component, "audio_surface_hidden"));

    component.paint(&theme, 220, 80, &mut buffer).unwrap();
    let visibility_requests = component.tick().unwrap();
    match visibility_requests.as_slice() {
        [CoreRequest::ShowSurface { surface_id }] => {
            assert_eq!(surface_id, "@mesh/audio-popover");
        }
        other => {
            panic!("expected audio popover show request from portal visibility, got {other:?}")
        }
    }
}

#[test]
fn navigation_volume_button_second_click_hides_audio_surface_via_parent_handler() {
    let button_component = parse_component(
        r#"
<template>
  <button onclick={onActivate}>Volume</button>
</template>

<script lang="luau">
function onActivate()
end
</script>
"#,
    )
    .unwrap();
    let root_component = parse_component(
        r#"
<template>
  <row>
    <VolumeButton onActivate={audio_surface_handler} />
    <AudioPopover hidden={audio_surface_hidden} />
  </row>
</template>

<script lang="luau">
import AudioPopover from "@mesh/audio-popover"
import VolumeButton from "./components/volume-button.mesh"

audio_surface_id = "@mesh/audio-popover"
audio_surface_hidden = true
audio_surface_handler = "__mesh_embed__::@mesh/navigation-bar::onToggleAudioSurface"

function onToggleAudioSurface(event)
    local position = event.current_target.position or {}
    local margin_left = tonumber(position.margin_left) or 0
    local margin_top = (tonumber(position.margin_bottom) or 0) + 8

    if audio_surface_hidden then
        mesh.events.publish("shell.position-surface", {
            surface_id = audio_surface_id,
            margin_top = margin_top,
            margin_left = margin_left
        })
    end

    audio_surface_hidden = not audio_surface_hidden
end
</script>
"#,
    )
    .unwrap();
    let popover_component = parse_component("<template><box /></template>").unwrap();

    let mut root_manifest = minimal_test_manifest("@mesh/navigation-bar");
    root_manifest.dependencies.modules.insert(
        "@mesh/audio-popover".into(),
        mesh_core_module::manifest::DependencySpec::Simple(">=0.1.0".into()),
    );
    let popover_manifest = minimal_test_manifest("@mesh/audio-popover");

    let root_compiled = CompiledFrontendModule {
        manifest: root_manifest,
        source_path: PathBuf::from("src/main.mesh"),
        component: root_component,
        local_components: HashMap::from([("VolumeButton".into(), button_component)]),
        module_component_imports: HashMap::from([(
            "AudioPopover".into(),
            "@mesh/audio-popover".into(),
        )]),
    };
    let popover_compiled = CompiledFrontendModule {
        manifest: popover_manifest,
        source_path: PathBuf::from("src/main.mesh"),
        component: popover_component,
        local_components: HashMap::new(),
        module_component_imports: HashMap::new(),
    };
    let catalog = FrontendCatalog {
        modules: HashMap::from([
            (
                "@mesh/navigation-bar".into(),
                FrontendCatalogEntry {
                    module_dir: PathBuf::from("."),
                    compiled: root_compiled.clone(),
                },
            ),
            (
                "@mesh/audio-popover".into(),
                FrontendCatalogEntry {
                    module_dir: PathBuf::from("."),
                    compiled: popover_compiled,
                },
            ),
        ]),
        slot_contributions: HashMap::new(),
    };
    let mut component = FrontendSurfaceComponent::new(
        root_compiled,
        PathBuf::from("."),
        catalog,
        InterfaceCatalog::default(),
    );
    component
        .mount(ComponentContext {
            component_id: "@mesh/navigation-bar".into(),
            surface_id: "@mesh/navigation-bar".into(),
            diagnostics: Diagnostics::new("@mesh/navigation-bar"),
        })
        .unwrap();
    component.visible = true;

    let theme = default_theme();
    let mut buffer = PixelBuffer::new(220, 80);
    component.paint(&theme, 220, 80, &mut buffer).unwrap();
    let tree = component.last_tree.as_ref().expect("rendered tree");
    let button = first_node_by_tag(tree, "button").expect("button node");
    let handler = button
        .event_handlers
        .get("click")
        .expect("click handler")
        .clone();

    let click_event = serde_json::json!({
        "current_target": {
            "position": {
                "margin_left": 32,
                "margin_bottom": 40
            }
        }
    });
    component
        .call_namespaced_handler(&handler, std::slice::from_ref(&click_event))
        .unwrap();
    component.paint(&theme, 220, 80, &mut buffer).unwrap();
    let show_requests = component.tick().unwrap();
    assert!(matches!(
        show_requests.as_slice(),
        [CoreRequest::ShowSurface { surface_id }] if surface_id == "@mesh/audio-popover"
    ));

    let requests = component
        .call_namespaced_handler(&handler, &[click_event])
        .unwrap();
    assert!(
        requests.is_empty(),
        "closing toggle should not publish direct shell events"
    );
    assert!(runtime_bool(&component, "audio_surface_hidden"));

    component.paint(&theme, 220, 80, &mut buffer).unwrap();
    let requests = component.tick().unwrap();
    match requests.as_slice() {
        [CoreRequest::HideSurface { surface_id }] => {
            assert_eq!(surface_id, "@mesh/audio-popover");
        }
        other => {
            panic!("expected audio popover hide request from portal visibility, got {other:?}")
        }
    }
}

#[test]
fn real_core_surfaces_quick_settings_commands_publish_service_requests() {
    let mut audio_ctx = make_audio_ctx();
    audio_ctx
        .load_script(
            r#"
local audio_ok, audio = pcall(require, "@mesh/audio@>=1.0")
if not audio_ok then audio = nil end

function onVolumeChange(value)
    local percent = math.floor((tonumber(value) or 0) + 0.5)
    if audio_ok and audio and audio.available ~= false then
        audio.set_volume("default", percent / 100)
    end
end
"#,
        )
        .unwrap();
    audio_ctx.apply_service_payload("audio", &serde_json::json!({ "available": true }));
    audio_ctx
        .call_handler("onVolumeChange", &[serde_json::json!(55)])
        .unwrap();
    let audio_requests =
        super::super::service::script_events_to_requests(audio_ctx.drain_published_events());

    match audio_requests.as_slice() {
        [
            CoreRequest::ServiceCommand {
                interface,
                command,
                payload,
                ..
            },
        ] => {
            assert_eq!(interface, "mesh.audio");
            assert_eq!(command, "set_volume");
            assert_eq!(
                payload,
                &serde_json::json!({ "device_id": "default", "volume": 0.55 })
            );
        }
        other => panic!("expected one mesh.audio set_volume command, got {other:?}"),
    }

    let mut network_ctx = make_network_ctx();
    network_ctx
        .load_script(
            r#"
local network_ok, network = pcall(require, "@mesh/network@>=1.0")
if not network_ok then network = nil end

function onToggleWiFi()
    if network_ok and network and network.available ~= false then
        network.set_wifi_enabled(not (network.wifi_enabled or false))
    end
end
"#,
        )
        .unwrap();
    network_ctx.apply_service_payload(
        "network",
        &serde_json::json!({ "available": true, "wifi_enabled": false }),
    );
    network_ctx.call_handler("onToggleWiFi", &[]).unwrap();
    let network_requests =
        super::super::service::script_events_to_requests(network_ctx.drain_published_events());

    match network_requests.as_slice() {
        [
            CoreRequest::ServiceCommand {
                interface,
                command,
                payload,
                ..
            },
        ] => {
            assert_eq!(interface, "mesh.network");
            assert_eq!(command, "set_wifi_enabled");
            assert_eq!(payload, &serde_json::json!({ "enabled": true }));
        }
        other => panic!("expected one mesh.network set_wifi_enabled command, got {other:?}"),
    }
}

#[test]
fn real_core_surfaces_reject_legacy_service_callback_api_in_shipped_surfaces() {
    let sources = [
        (
            "navigation-bar root",
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../../modules/frontend/navigation-bar/src/main.mesh"
            )),
        ),
        (
            "navigation-bar volume button",
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../../modules/frontend/navigation-bar/src/components/volume-button.mesh"
            )),
        ),
        (
            "navigation-bar settings button",
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../../modules/frontend/navigation-bar/src/components/settings-button.mesh"
            )),
        ),
    ];

    for (name, source) in sources {
        assert_no_legacy_service_callbacks(name, source);
    }
}

// ---- 09-03: post-restyle synchronization tests ----

/// D-04: Hit testing uses final post-restyle bounds.
///
/// When a hover restyle changes an element's size (e.g., from 40px to 80px),
/// the next pointer event must resolve against the updated layout, not the
/// pre-restyle bounds. This proves that `build_tree` recomputes layout after
/// `restyle_subtree`.
#[test]
fn restyle_hit_test_uses_post_restyle_bounds() {
    // The button starts at width: 40px.  On hover the style rule widens it to 80px.
    // We set the hovered path so the restyle fires immediately on the first paint.
    // After paint, a pointer click at x=60 (inside 80px, outside 40px) must find
    // a click handler on the button, proving the post-restyle bounds were used.
    let mut component = test_frontend_component(
        r#"
<style>
button {
  width: 40px;
  height: 20px;
  background-color: #111111;
}
button:hover {
  width: 80px;
}
</style>
<template>
  <button onclick={onClick} />
</template>
<script lang="luau">
clicked = false
function onClick()
    clicked = true
end
</script>
"#,
    );
    // Pre-hover paint: button is 40px wide, no hover state yet.
    let theme = default_theme();
    let mut buffer = PixelBuffer::new(200, 60);
    component.paint(&theme, 200, 60, &mut buffer).unwrap();

    // Simulate a hover over the button region.  The button key is "root/0/0"
    // (surface → column/row → button, index 0 in the single-child template).
    component.hovered_path = vec!["root".into(), "root/0".into()];
    component.hovered_key = Some("root/0".into());
    component.dirty = true;
    component.paint(&theme, 200, 60, &mut buffer).unwrap();

    // After the hover restyle the button should be 80px wide.
    let tree = component.last_tree.as_ref().unwrap();
    let button = node_by_mesh_key(tree, "root/0");
    assert!(
        button.state.hovered,
        "button should be annotated as hovered"
    );
    assert!(
        button.layout.width >= 79.0,
        "post-restyle layout width should be ~80px, got {}",
        button.layout.width
    );

    // Click at x=60 — inside the restyled 80px bounds but outside the original 40px.
    // The handler must fire, confirming hit testing used the post-restyle bounds.
    component
        .handle_input(
            &theme,
            200,
            60,
            ComponentInput::PointerButton {
                x: 60.0,
                y: 5.0,
                pressed: true,
            },
        )
        .unwrap();
    component
        .handle_input(
            &theme,
            200,
            60,
            ComponentInput::PointerButton {
                x: 60.0,
                y: 5.0,
                pressed: false,
            },
        )
        .unwrap();
    assert!(
        runtime_bool(&component, "clicked"),
        "click at x=60 should land inside the post-restyle 80px button"
    );
}

/// D-11: Ref and element metrics reflect final post-restyle bounds.
///
/// When a pseudo-state restyle changes an element's computed width, the
/// `refs` / `elements` host values published to the Lua context must report
/// the new width, not the pre-restyle one.
#[test]
fn restyle_metrics_reflect_post_restyle_bounds() {
    let mut component = test_frontend_component(
        r#"
<style>
button {
  width: 40px;
  height: 20px;
}
button:focus {
  width: 80px;
}
</style>
<template>
  <button ref="btn" />
</template>
<script lang="luau">
</script>
"#,
    );
    let theme = default_theme();
    let mut buffer = PixelBuffer::new(200, 60);

    // First paint: no focus — button width should be 40px in metrics.
    component.paint(&theme, 200, 60, &mut buffer).unwrap();
    let width_before = {
        let runtimes = component.runtimes.lock().unwrap();
        let state = runtimes.get(component.id()).unwrap().script_ctx.state();
        state
            .get("refs")
            .and_then(|v| v.get("btn").and_then(|b| b.get("width")).cloned())
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0) as f32
    };

    // Focus the button and repaint.
    component.focused_key = Some("root/0".into());
    component.dirty = true;
    component.paint(&theme, 200, 60, &mut buffer).unwrap();
    let width_after = {
        let runtimes = component.runtimes.lock().unwrap();
        let state = runtimes.get(component.id()).unwrap().script_ctx.state();
        state
            .get("refs")
            .and_then(|v| v.get("btn").and_then(|b| b.get("width")).cloned())
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0) as f32
    };

    assert!(
        (width_before - 40.0).abs() < 2.0,
        "unfocused metrics width should be ~40px, got {width_before}"
    );
    assert!(
        (width_after - 80.0).abs() < 2.0,
        "focused metrics width should be ~80px after restyle, got {width_after}"
    );
}

/// D-13: Accessibility data stays synchronized with focused/checked state
/// and final layout bounds after a restyle.
///
/// When a `:focus` style rule widens a button, the `AccessibilityTree`
/// built from the post-restyle widget tree must report the wider bounds.
#[test]
fn accessibility_data_synchronized_after_restyle() {
    use mesh_core_elements::accessibility::AccessibilityTree;

    let mut component = test_frontend_component(
        r#"
<style>
button {
  width: 60px;
  height: 24px;
}
button:focus {
  width: 120px;
}
</style>
<template>
  <button aria-label="Save" />
</template>
<script lang="luau">
</script>
"#,
    );
    let theme = default_theme();
    let mut buffer = PixelBuffer::new(300, 80);

    // Paint without focus.
    component.paint(&theme, 300, 80, &mut buffer).unwrap();
    let tree_unfocused = component.last_tree.as_ref().unwrap().clone();
    let a11y_unfocused = AccessibilityTree::from_widget_tree(&tree_unfocused);

    // Find the button by its role (Button) in the a11y tree.
    let btn_unfocused_width = a11y_unfocused
        .nodes
        .iter()
        .find(|n| {
            matches!(
                n.info.role,
                mesh_core_elements::accessibility::AccessibilityRole::Button
            )
        })
        .map(|n| n.bounds.width)
        .unwrap_or(0.0);

    // Focus the button and repaint.
    component.focused_key = Some("root/0".into());
    component.dirty = true;
    component.paint(&theme, 300, 80, &mut buffer).unwrap();
    let tree_focused = component.last_tree.as_ref().unwrap().clone();
    let a11y_focused = AccessibilityTree::from_widget_tree(&tree_focused);

    // After `:focus` restyle the button bounds must be wider (120px).
    let btn_focused_width = a11y_focused
        .nodes
        .iter()
        .find(|n| {
            matches!(
                n.info.role,
                mesh_core_elements::accessibility::AccessibilityRole::Button
            )
        })
        .map(|n| n.bounds.width)
        .unwrap_or(0.0);

    assert!(
        (btn_unfocused_width - 60.0).abs() < 2.0,
        "unfocused a11y bounds width should be ~60px, got {btn_unfocused_width}"
    );
    assert!(
        btn_focused_width >= 119.0,
        "focused a11y bounds width should be ~120px after restyle, got {btn_focused_width}"
    );
    assert!(
        btn_focused_width > btn_unfocused_width,
        "focused a11y bounds ({btn_focused_width}) must exceed unfocused ({btn_unfocused_width})"
    );

    // Confirm the focused node state flag is set in the widget tree itself
    // (separate from AccessibilityInfo which is populated statically from tag).
    let focused_button = node_by_mesh_key(&tree_focused, "root/0");
    assert!(
        focused_button.state.focused,
        "WidgetNode.state.focused must be true for the focused button"
    );
}

// -----------------------------------------------------------------------
// 09-04-01: State preservation through restyles
// -----------------------------------------------------------------------

/// A service payload applied directly to the ScriptContext (`__mesh_svc_*`)
/// must survive a pseudo-state restyle (hover triggers a repaint). The
/// runtime is reused; service globals are not wiped between paints.
#[test]
fn state_preservation_restyle_service_payload_survives_hover_restyle() {
    let mut component = test_frontend_component(
        r#"
<template>
  <button />
</template>
<script lang="luau">
-- Track whenever a reactive global is updated to detect accidental wipes.
vol_pct = -1
function onRender()
    -- Read directly from the service state table if it exists.
    if __mesh_svc_audio then
        vol_pct = __mesh_svc_audio.percent or -1
    end
end
</script>
"#,
    );

    let theme = default_theme();
    let mut buffer = PixelBuffer::new(240, 80);

    // First paint — no service payload yet.
    component.paint(&theme, 240, 80, &mut buffer).unwrap();

    // Apply a service payload directly to the ScriptContext, simulating a
    // backend service emit reaching the frontend runtime.
    {
        let mut runtimes = component.runtimes.lock().unwrap();
        let runtime = runtimes.get_mut(component.id()).unwrap();
        runtime
            .script_ctx
            .apply_service_payload("audio", &serde_json::json!({ "percent": 72 }));
        // Mark render hooks pending so onRender fires on next paint.
    }
    component.render_hooks_pending = true;
    component.dirty = true;

    // First paint with the service payload — onRender fires, vol_pct == 72.
    component.paint(&theme, 240, 80, &mut buffer).unwrap();
    let pct_after_payload = runtime_number(&component, "vol_pct");
    assert!(
        (pct_after_payload - 72.0).abs() < 0.1,
        "vol_pct should be 72 after service payload applied, got {pct_after_payload}"
    );

    // Trigger a pseudo-state restyle by setting hover (no service re-emit).
    component.hovered_key = Some("root/0".into());
    component.hovered_path = vec!["root".into(), "root/0".into()];
    component.dirty = true;
    component.paint(&theme, 240, 80, &mut buffer).unwrap();

    // vol_pct must still reflect the last service update, not be wiped.
    let pct_after_hover_restyle = runtime_number(&component, "vol_pct");
    assert!(
        (pct_after_hover_restyle - 72.0).abs() < 0.1,
        "service payload must survive a hover-triggered restyle; vol_pct={pct_after_hover_restyle}"
    );

    // The hovered button should show :hover state in the tree.
    let tree = component.last_tree.as_ref().unwrap();
    assert!(
        node_by_mesh_key(tree, "root/0").state.hovered,
        "button must be marked hovered after restyle"
    );
}

/// Pseudo-state restyles (hover, focus) must not increment the runtime
/// instance count — the same `EmbeddedFrontendRuntime` must be reused.
/// Reusing the runtime also implicitly preserves all Lua global state
/// (reactive variables, imported service proxies, etc.).
#[test]
fn state_preservation_restyle_does_not_reinitialize_runtime() {
    let mut component = test_frontend_component(
        r#"
<template>
  <button />
</template>
<script lang="luau">
init_count = 0
init_count = init_count + 1
</script>
"#,
    );
    let theme = default_theme();
    let mut buffer = PixelBuffer::new(240, 80);

    // First paint — runtime is initialized, init_count == 1.
    component.paint(&theme, 240, 80, &mut buffer).unwrap();
    let count_after_first = runtime_number(&component, "init_count");
    let runtime_instances_after_first = component.runtimes.lock().unwrap().len();
    assert_eq!(
        count_after_first as u32, 1,
        "init_count should be 1 after first paint"
    );
    assert_eq!(
        runtime_instances_after_first, 1,
        "should have exactly 1 runtime after first paint"
    );

    // Trigger a pseudo-state restyle by focusing.
    component.focused_key = Some("root/0".into());
    component.dirty = true;
    component.paint(&theme, 240, 80, &mut buffer).unwrap();

    let count_after_focus = runtime_number(&component, "init_count");
    let runtime_instances_after_focus = component.runtimes.lock().unwrap().len();

    // init_count must still be 1 — the top-level Luau block must not run again.
    assert_eq!(
        count_after_focus as u32, 1,
        "pseudo-state restyle must not re-execute the top-level Luau block (init_count={count_after_focus})"
    );
    // Runtime instance count must not grow.
    assert_eq!(
        runtime_instances_after_focus, runtime_instances_after_first,
        "pseudo-state restyle must reuse the existing runtime (expected {runtime_instances_after_first}, got {runtime_instances_after_focus})"
    );
}

/// Input, slider, and checked state must be preserved through a pseudo-state
/// (focus) restyle — all three shell-side maps must survive unchanged.
/// Scroll offset maps are also preserved; the annotated `_mesh_scroll_y`
/// value is clamped by `annotate_overflow_tree` to the actual overflow range,
/// so preservation of the raw map entry is verified instead.
#[test]
fn state_preservation_restyle_user_input_state_survives_focus_restyle() {
    let mut component = test_frontend_component(
        r#"
<style>
scroll {
  height: 20px;
  overflow-y: auto;
}
text {
  height: 100px;
}
</style>
<template>
  <column>
    <input value="initial" />
    <slider min="0" max="100" value="25" />
    <checkbox checked="false" />
    <scroll><text>scrollable content long enough to overflow</text></scroll>
  </column>
</template>
<script lang="luau">
</script>
"#,
    );
    // Seed shell-side interaction state maps directly.
    component
        .input_values
        .insert("root/0/0".into(), "typed-text".into());
    component.slider_values.insert("root/0/1".into(), 88.0);
    component.checked_values.insert("root/0/2".into(), true);
    component
        .scroll_offsets
        .insert("root/0/3".into(), ScrollOffsetState { x: 0.0, y: 10.0 });

    let theme = default_theme();
    let mut buffer = PixelBuffer::new(240, 300);

    // First paint to establish baseline.
    component.paint(&theme, 240, 300, &mut buffer).unwrap();

    // Trigger a focus-driven pseudo-state restyle.
    component.focused_key = Some("root/0/0".into());
    component.dirty = true;
    component.paint(&theme, 240, 300, &mut buffer).unwrap();

    let tree = component.last_tree.as_ref().unwrap();

    // Input value must survive the restyle.
    assert_eq!(
        node_by_mesh_key(tree, "root/0/0")
            .attributes
            .get("value")
            .map(String::as_str),
        Some("typed-text"),
        "input value must survive focus restyle"
    );

    // Slider value must survive.
    assert_eq!(
        node_by_mesh_key(tree, "root/0/1")
            .attributes
            .get("value")
            .map(String::as_str),
        Some("88.00"),
        "slider value must survive focus restyle"
    );

    // Checked state must survive.
    assert!(
        node_by_mesh_key(tree, "root/0/2").state.checked,
        "checked state must survive focus restyle"
    );

    // Scroll offset raw map entry must survive (the annotated _mesh_scroll_y is
    // clamp-bounded by annotate_overflow_tree to the actual overflow range).
    assert!(
        component.scroll_offsets.contains_key("root/0/3"),
        "scroll_offsets map must retain the entry for root/0/3 across focus restyle"
    );
}

// -----------------------------------------------------------------------
// 09-04-02: Clear invalid interaction targets deterministically
// -----------------------------------------------------------------------

/// When a conditionally rendered node (removed from the tree by restyle) was
/// the hovered target, the hover state must be cleared deterministically after
/// the next paint. Valid siblings must retain their state.
#[test]
fn restyle_state_cleanup_hover_cleared_when_node_removed() {
    let mut component = test_frontend_component(
        r#"
<template>
  <column>
    <button />
    <button />
  </column>
</template>
<script lang="luau">
</script>
"#,
    );
    let theme = default_theme();
    let mut buffer = PixelBuffer::new(240, 80);

    // First paint to establish the tree structure.
    component.paint(&theme, 240, 80, &mut buffer).unwrap();

    // Simulate hovering the second button.
    component.hovered_key = Some("root/0/1".into());
    component.hovered_path = vec!["root".into(), "root/0".into(), "root/0/1".into()];
    component.hover_start = Some(std::time::Instant::now());
    component.dirty = true;
    component.paint(&theme, 240, 80, &mut buffer).unwrap();

    let tree = component.last_tree.as_ref().unwrap();
    assert!(
        node_by_mesh_key(tree, "root/0/1").state.hovered,
        "second button must be hovered before removal"
    );
    assert!(
        component.hovered_key.is_some(),
        "hovered_key must be set before node removal"
    );

    // Now simulate node removal: pretend the second button is gone by manually
    // removing its key from the tree. We do this by injecting a component
    // that only has one button, so "root/0/1" will not appear in the final tree.
    let component2 = test_frontend_component(
        r#"
<template>
  <column>
    <button />
  </column>
</template>
<script lang="luau">
</script>
"#,
    );
    // Transplant the hovered state into the new component to test cleanup.
    let mut component = component2;
    component.hovered_key = Some("root/0/1".into()); // stale key
    component.hovered_path = vec!["root".into(), "root/0".into(), "root/0/1".into()];
    component.hover_start = Some(std::time::Instant::now());
    component.dirty = true;
    component.paint(&theme, 240, 80, &mut buffer).unwrap();

    // After the paint, the stale hovered_key must be cleared.
    assert!(
        component.hovered_key.is_none(),
        "hovered_key must be cleared after the hovered node is removed from the tree"
    );
    assert!(
        component.hovered_path.is_empty(),
        "hovered_path must be cleared when hovered node is removed"
    );
    assert!(
        component.hover_start.is_none(),
        "hover_start must be cleared when hovered node is removed"
    );

    // The remaining sibling must not be affected.
    let tree = component.last_tree.as_ref().unwrap();
    assert!(
        !node_by_mesh_key(tree, "root/0/0").state.hovered,
        "remaining sibling must not inherit stale hover state"
    );
}

/// When the focused node is removed from the tree, `focused_key` must be
/// cleared. Valid sibling focus state is not affected.
#[test]
fn restyle_state_cleanup_focus_cleared_when_node_removed() {
    let mut component = test_frontend_component(
        r#"
<template>
  <column>
    <button />
  </column>
</template>
<script lang="luau">
</script>
"#,
    );

    // Set a focused_key that does not exist in this single-button tree.
    component.focused_key = Some("root/0/1".into()); // stale — no such node
    component.dirty = true;

    let theme = default_theme();
    let mut buffer = PixelBuffer::new(240, 80);
    component.paint(&theme, 240, 80, &mut buffer).unwrap();

    assert!(
        component.focused_key.is_none(),
        "focused_key must be cleared when the focused node is absent from the final tree"
    );

    // The existing button must not gain accidental focus.
    let tree = component.last_tree.as_ref().unwrap();
    assert!(
        !node_by_mesh_key(tree, "root/0/0").state.focused,
        "existing button must not inherit stale focused_key"
    );
}

/// When the active (pointer-down) node is removed from the tree, the
/// `pointer_down_key` must be cleared deterministically.
#[test]
fn restyle_state_cleanup_active_cleared_when_node_removed() {
    let mut component = test_frontend_component(
        r#"
<template>
  <column>
    <button />
  </column>
</template>
<script lang="luau">
</script>
"#,
    );

    // Set a stale pointer_down_key pointing to a non-existent node.
    component.pointer_down_key = Some("root/0/99".into());
    component.dirty = true;

    let theme = default_theme();
    let mut buffer = PixelBuffer::new(240, 80);
    component.paint(&theme, 240, 80, &mut buffer).unwrap();

    assert!(
        component.pointer_down_key.is_none(),
        "pointer_down_key must be cleared when the active node is absent from the final tree"
    );

    // Existing button must not show stale active styling.
    let tree = component.last_tree.as_ref().unwrap();
    assert!(
        !node_by_mesh_key(tree, "root/0/0").state.active,
        "existing button must not inherit stale active (pointer-down) state"
    );
}

/// Valid interaction targets whose keys exist in the final tree must NOT be
/// cleared — prune only removes absent keys.
#[test]
fn restyle_state_cleanup_preserves_valid_interaction_targets() {
    let mut component = test_frontend_component(
        r#"
<style>
button:focus {
  width: 80px;
}
button:hover {
  height: 30px;
}
</style>
<template>
  <column>
    <button />
    <button />
  </column>
</template>
<script lang="luau">
</script>
"#,
    );
    let theme = default_theme();
    let mut buffer = PixelBuffer::new(240, 80);

    component.paint(&theme, 240, 80, &mut buffer).unwrap();

    // Both keys are valid — set focus on first, hover on second.
    component.focused_key = Some("root/0/0".into());
    component.hovered_key = Some("root/0/1".into());
    component.hovered_path = vec!["root".into(), "root/0".into(), "root/0/1".into()];
    component.pointer_down_key = Some("root/0/0".into());
    component.dirty = true;
    component.paint(&theme, 240, 80, &mut buffer).unwrap();

    // All valid targets must survive pruning.
    assert_eq!(
        component.focused_key.as_deref(),
        Some("root/0/0"),
        "focused_key for a present node must not be pruned"
    );
    assert_eq!(
        component.hovered_key.as_deref(),
        Some("root/0/1"),
        "hovered_key for a present node must not be pruned"
    );
    assert_eq!(
        component.pointer_down_key.as_deref(),
        Some("root/0/0"),
        "pointer_down_key for a present node must not be pruned"
    );

    // State flags must be applied correctly.
    let tree = component.last_tree.as_ref().unwrap();
    assert!(node_by_mesh_key(tree, "root/0/0").state.focused);
    assert!(node_by_mesh_key(tree, "root/0/1").state.hovered);
    assert!(node_by_mesh_key(tree, "root/0/0").state.active);
}

#[test]
fn selection_boundaries_ignore_selectable_text_inside_controls() {
    let mut component = test_frontend_component("<template><box /></template>");
    let mut button = event_node(
        "button",
        "root/0",
        0.0,
        0.0,
        120.0,
        32.0,
        &[("click", "noop")],
    );
    button
        .children
        .push(text_node("root/0/0", 4.0, 4.0, 100.0, 20.0, true));
    component.last_tree = Some(root_with(vec![button]));

    let theme = default_theme();
    component
        .handle_input(
            &theme,
            240,
            160,
            ComponentInput::PointerButton {
                x: 8.0,
                y: 8.0,
                pressed: true,
            },
        )
        .unwrap();

    assert!(
        component.selection.is_none(),
        "selectable text nested inside controls must not start Phase 10 selection"
    );
    assert_eq!(
        component.pointer_down_key.as_deref(),
        Some("root/0"),
        "control pointer handling should still win when text lives inside a button"
    );
}

#[test]
fn selection_boundaries_clamp_drag_to_same_text_node() {
    let mut component = test_frontend_component("<template><box /></template>");
    component.last_tree = Some(root_with(vec![
        text_node("root/0", 0.0, 0.0, 100.0, 20.0, true),
        text_node("root/1", 120.0, 0.0, 100.0, 20.0, true),
    ]));

    let theme = default_theme();
    component
        .handle_input(
            &theme,
            240,
            160,
            ComponentInput::PointerButton {
                x: 8.0,
                y: 8.0,
                pressed: true,
            },
        )
        .unwrap();
    component
        .handle_input(
            &theme,
            240,
            160,
            ComponentInput::PointerMove { x: 140.0, y: 8.0 },
        )
        .unwrap();

    let selection = component
        .selection
        .as_ref()
        .expect("selection should start");
    assert_eq!(selection.anchor.node_key, "root/0");
    assert_eq!(
        selection.focus.node_key, "root/0",
        "Phase 10 selection must stay within the first selectable text node"
    );
}

#[test]
fn selection_boundaries_clear_when_selected_node_is_removed() {
    let mut component = test_frontend_component("<template><box /></template>");
    component.selection = Some(TextSelectionState {
        anchor: TextSelectionPoint {
            node_key: "root/0".into(),
            x: 4.0,
            y: 4.0,
        },
        focus: TextSelectionPoint {
            node_key: "root/0".into(),
            x: 24.0,
            y: 4.0,
        },
    });
    component.prune_stale_interaction_targets(&root_with(vec![]));

    assert!(
        component.selection.is_none(),
        "selection must clear when the selected node disappears during rebuild"
    );
}

#[test]
fn selection_boundaries_clear_when_surface_hides() {
    let mut component = test_frontend_component("<template><box /></template>");
    component.selection = Some(TextSelectionState {
        anchor: TextSelectionPoint {
            node_key: "root/0".into(),
            x: 4.0,
            y: 4.0,
        },
        focus: TextSelectionPoint {
            node_key: "root/0".into(),
            x: 16.0,
            y: 4.0,
        },
    });

    component
        .handle_core_event(&CoreEvent::SurfaceVisibilityChanged {
            surface_id: component.surface_id().to_string(),
            visible: false,
        })
        .unwrap();

    assert!(
        component.selection.is_none(),
        "surface hide should clear shell-owned selection state"
    );
}
