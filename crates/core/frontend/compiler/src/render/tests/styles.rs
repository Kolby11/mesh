use super::super::*;
use super::common::*;
use crate::TemplateExpressionResult;
use mesh_core_component::template::Attribute;
use mesh_core_elements::{ComponentCompositionProps, EventHandlerCall, HandlerTarget};
use std::collections::BTreeMap;

#[test]
fn translated_expression_uses_locale_store_before_composition_runtime() {
    let value = evaluate_template_expression(
        "t('nav.open_settings')",
        Some(&TranslatingStore),
        "test",
        Some(&IdentityTranslationComposition),
    );

    assert_eq!(value, serde_json::json!("Open settings"));
}

#[test]
fn component_handler_calls_preserve_authored_prop_identity() {
    #[derive(Default)]
    struct CapturingComposition {
        props: std::cell::RefCell<ComponentCompositionProps>,
        calls: std::cell::RefCell<BTreeMap<String, EventHandlerCall>>,
    }

    impl FrontendCompositionResolver for CapturingComposition {
        fn evaluate_template_expression(
            &self,
            _instance_key: &str,
            _expression: &str,
            _locals: &serde_json::Map<String, serde_json::Value>,
        ) -> Option<TemplateExpressionResult> {
            None
        }

        fn render_import(
            &self,
            _host: &Manifest,
            _host_instance_key: &str,
            _alias: &str,
            _source_ordinal: usize,
            _duplicate_ordinal: Option<usize>,
            _repeated_by_loop: bool,
            _loop_identity: Option<&str>,
            props: &ComponentCompositionProps,
            prop_handler_calls: &BTreeMap<String, EventHandlerCall>,
            _container_width: f32,
            _container_height: f32,
        ) -> Option<WidgetNode> {
            self.props.replace(props.clone());
            self.calls.replace(prop_handler_calls.clone());
            Some(WidgetNode::new("box"))
        }

        fn render_slot(
            &self,
            _host: &Manifest,
            _host_instance_key: &str,
            _extension_point: Option<&str>,
            _slot_name: Option<&str>,
            _customizable: bool,
            _container_width: f32,
            _container_height: f32,
        ) -> Vec<WidgetNode> {
            Vec::new()
        }
    }

    let component = mesh_core_component::parse_component(
        r#"
<template>
  <Child hidden="{isHidden}" bind:this={child}
     onprimary={onShared("primary")} onsecondary={onShared("secondary")} />
</template>
<script lang="luau">
import Child from "./child.mesh"
</script>
"#,
    )
    .unwrap();
    let store = MapStore(
        [
            ("onShared".to_string(), serde_json::json!("onShared")),
            ("isHidden".to_string(), serde_json::json!(true)),
        ]
        .into_iter()
        .collect(),
    );
    let composition = CapturingComposition::default();

    build_widget_tree_from_component(
        &component,
        &test_manifest(),
        &mesh_core_theme::default_theme(),
        200.0,
        80.0,
        Some(&composition),
        "root",
        Some(&store),
        &[],
    );

    let props = composition.props.borrow();
    assert!(props.values.contains_key("onprimary"));
    assert!(props.values.contains_key("onsecondary"));
    assert_ne!(
        props.values.get("onprimary"),
        props.values.get("onsecondary")
    );
    assert_eq!(
        props.bindings.get("hidden").map(String::as_str),
        Some("isHidden")
    );
    assert_eq!(props.bind_this.as_deref(), Some("child"));
    assert!(
        props
            .values
            .keys()
            .all(|key| !key.as_str().starts_with("__mesh_"))
    );
    let calls = composition.calls.borrow();
    assert_eq!(calls["onprimary"].handler, "onShared");
    assert_eq!(calls["onprimary"].handler.instance_key(), Some("root"));
    assert_eq!(calls["onsecondary"].handler, calls["onprimary"].handler);
    assert_eq!(calls["onprimary"].args, vec![serde_json::json!("primary")]);
    assert_eq!(
        calls["onsecondary"].args,
        vec![serde_json::json!("secondary")]
    );
}

fn find_tag<'a>(node: &'a WidgetNode, tag: &str) -> Option<&'a WidgetNode> {
    if node.tag == tag {
        return Some(node);
    }
    node.children.iter().find_map(|child| find_tag(child, tag))
}

#[test]
fn event_handler_attributes_normalize_to_widget_event_keys() {
    let component = mesh_core_component::parse_component(
        r#"
<template>
  <box>
<button onclick={onTap}>Tap</button>
<input onchange={onInputChange} onfocus={onInputFocus} />
<slider onrelease={onSliderRelease} />
  </box>
</template>
"#,
    )
    .unwrap();
    let manifest = test_manifest();
    let theme = mesh_core_theme::default_theme();

    let tree = build_widget_tree_from_component(
        &component,
        &manifest,
        &theme,
        200.0,
        80.0,
        None,
        "root",
        None,
        &[],
    );

    let button = find_tag(&tree, "button").expect("button node");
    assert_eq!(button.event_handlers.get("click"), Some(&"onTap".into()));

    let input = find_tag(&tree, "input").expect("input node");
    assert_eq!(
        input.event_handlers.get("change"),
        Some(&"onInputChange".into())
    );
    assert_eq!(
        input.event_handlers.get("focus"),
        Some(&"onInputFocus".into())
    );

    let slider = find_tag(&tree, "slider").expect("slider node");
    assert_eq!(
        slider.event_handlers.get("release"),
        Some(&"onSliderRelease".into())
    );
}

#[test]
fn prepared_component_styles_match_per_build_rule_merging() {
    let component = mesh_core_component::parse_component(
        r#"
<template><box class="target" /></template>
<style>.target { height: 12px; color: #abcdef; }</style>
"#,
    )
    .unwrap();
    let host = mesh_core_component::parse_component(
        r#"
<template><box /></template>
<style>.target { width: 34px; padding: 2px; }</style>
"#,
    )
    .unwrap();
    let host_rules = &host.style.as_ref().unwrap().rules;
    let manifest = test_manifest();
    let theme = mesh_core_theme::default_theme();

    let legacy = build_embedded_widget_tree_from_component(
        &component,
        &manifest,
        &theme,
        200.0,
        80.0,
        None,
        "root/local:target",
        None,
        host_rules,
    );
    let prepared = PreparedComponentStyleRules::new(&component, host_rules);
    let cached = build_embedded_widget_tree_from_component_with_prepared_styles(
        &component,
        &manifest,
        &theme,
        200.0,
        80.0,
        None,
        "root/local:target",
        None,
        &prepared,
    );

    assert_eq!(cached.tag, legacy.tag);
    assert_eq!(cached.attributes, legacy.attributes);
    assert_eq!(cached.children.len(), legacy.children.len());
    let cached_target = cached.children.first().expect("prepared target");
    let legacy_target = legacy.children.first().expect("legacy target");
    assert_eq!(cached_target.tag, legacy_target.tag);
    assert_eq!(cached_target.attributes, legacy_target.attributes);
    assert_eq!(
        format!("{:?}", cached_target.computed_style),
        format!("{:?}", legacy_target.computed_style)
    );
}

// cargo test -p mesh-core-frontend --release -- prepared_component_style_rules_avoid_remerge_and_reindex --ignored --nocapture
#[test]
#[ignore = "release-only prepared component-style benchmark"]
fn prepared_component_style_rules_avoid_remerge_and_reindex() {
    use std::time::Instant;

    let rules = (0..32)
        .map(|index| {
            format!(
                ".item-{index} {{ width: {}px; height: {}px; color: #abcdef; }}",
                index + 1,
                index + 2
            )
        })
        .collect::<String>();
    let component = mesh_core_component::parse_component(&format!(
        "<template><box class=\"item-31\" /></template><style>{rules}</style>"
    ))
    .unwrap();
    let host = mesh_core_component::parse_component(&format!(
        "<template><box /></template><style>{rules}</style>"
    ))
    .unwrap();
    let host_rules = &host.style.as_ref().unwrap().rules;
    let iterations = 20_000usize;

    let rebuild_started = Instant::now();
    let mut rebuilt_rules = 0usize;
    for _ in 0..iterations {
        let prepared = PreparedComponentStyleRules::new(
            std::hint::black_box(&component),
            std::hint::black_box(host_rules),
        );
        rebuilt_rules += std::hint::black_box(prepared.rules.len());
    }
    let rebuild_time = rebuild_started.elapsed();

    let prepared = PreparedComponentStyleRules::new(&component, host_rules);
    let reuse_started = Instant::now();
    let mut reused_rules = 0usize;
    for _ in 0..iterations {
        reused_rules += std::hint::black_box(&prepared).rules.len();
    }
    let reuse_time = reuse_started.elapsed();

    eprintln!(
        "component style preparation: rebuild {rebuild_time:?}; cached reuse {reuse_time:?}; ratio {:.1}x; rules={rebuilt_rules}/{reused_rules}",
        rebuild_time.as_secs_f64() / reuse_time.as_secs_f64()
    );
    assert_eq!(rebuilt_rules, reused_rules);
    assert!(reuse_time < rebuild_time);
}

const PROP_COMPONENT: &str = r#"
<props>
  track_width: { type: "size", default: "20px" }
</props>
<template>
  <box>
<slider class="audio-slider"/>
  </box>
</template>
<style>
.audio-slider { width: prop(track_width); }
</style>
"#;

#[test]
fn prop_default_projects_into_painted_width() {
    let component = mesh_core_component::parse_component(PROP_COMPONENT).unwrap();
    let manifest = test_manifest();
    let theme = mesh_core_theme::default_theme();

    let tree = build_widget_tree_from_component(
        &component,
        &manifest,
        &theme,
        200.0,
        80.0,
        None,
        "root",
        None,
        &[],
    );

    let slider = find_tag(&tree, "slider").expect("slider node");
    assert_eq!(
        slider.computed_style.width,
        mesh_core_elements::Dimension::Px(20.0)
    );
}

#[test]
fn prop_state_override_beats_default() {
    let component = mesh_core_component::parse_component(PROP_COMPONENT).unwrap();
    let manifest = test_manifest();
    let theme = mesh_core_theme::default_theme();
    let state = MapStore(std::collections::HashMap::from([(
        "props".to_string(),
        serde_json::json!({ "track_width": "36px" }),
    )]));

    let tree = build_widget_tree_from_component(
        &component,
        &manifest,
        &theme,
        200.0,
        80.0,
        None,
        "root",
        Some(&state),
        &[],
    );

    let slider = find_tag(&tree, "slider").expect("slider node");
    assert_eq!(
        slider.computed_style.width,
        mesh_core_elements::Dimension::Px(36.0)
    );
}

#[test]
fn dynamic_class_participates_in_initial_style_resolution() {
    let component = mesh_core_component::parse_component(
        r#"
<template>
  <box class="{active_class}"/>
</template>
<style>
.active { width: 42px; }
</style>
"#,
    )
    .unwrap();
    let manifest = test_manifest();
    let theme = mesh_core_theme::default_theme();
    let state = MapStore(std::collections::HashMap::from([(
        "active_class".to_string(),
        serde_json::json!("active"),
    )]));

    let tree = build_widget_tree_from_component(
        &component,
        &manifest,
        &theme,
        200.0,
        80.0,
        None,
        "root",
        Some(&state),
        &[],
    );

    let active_box = tree.children.first().expect("template root");
    assert_eq!(
        active_box.attributes.get("class"),
        Some(&"active".to_string())
    );
    assert_eq!(
        active_box.computed_style.width,
        mesh_core_elements::Dimension::Px(42.0)
    );
}

#[test]
fn dynamic_inline_style_overrides_stylesheet_layout() {
    let component = mesh_core_component::parse_component(
        r#"
<template>
  <box class="positioned" style={position_style}/>
</template>
<style>
.positioned {
  position: absolute;
  left: 1px;
  top: 2px;
}
</style>
"#,
    )
    .unwrap();
    let manifest = test_manifest();
    let theme = mesh_core_theme::default_theme();
    let state = MapStore(std::collections::HashMap::from([(
        "position_style".to_string(),
        serde_json::json!("left: 38px; top: 32px;"),
    )]));

    let tree = build_widget_tree_from_component(
        &component,
        &manifest,
        &theme,
        200.0,
        80.0,
        None,
        "root",
        Some(&state),
        &[],
    );

    let positioned = tree.children.first().expect("template root");
    assert_eq!(
        positioned.attributes.get("style").map(String::as_str),
        Some("left: 38px; top: 32px;")
    );
    assert_eq!(positioned.computed_style.inset_left, Some(38.0));
    assert_eq!(positioned.computed_style.inset_top, Some(32.0));
}

#[test]
fn props_settings_schema_projects_typed_fields() {
    let component = mesh_core_component::parse_component(
        r#"
<props>
  width:   { type: "size", default: "fit-content", label: t("var.width") }
  density: { type: "enum", options: ["compact", "cozy"], default: "cozy" }
  hidden:  { type: "size", default: "10px", expose: false }
</props>
<template><box/></template>
"#,
    )
    .unwrap();

    let schema = props_settings_schema(component.props.as_ref()).expect("schema");
    let props = &schema["properties"];
    // Exposed fields present; expose:false omitted.
    assert_eq!(props["width"]["type"], "size");
    assert_eq!(props["width"]["default"], "fit-content");
    assert_eq!(props["width"]["label"]["t"], "var.width");
    assert_eq!(props["density"]["enum"][0], "compact");
    assert!(props.get("hidden").is_none());
}

#[test]
fn props_settings_schema_is_none_without_exposed_props() {
    let component = mesh_core_component::parse_component(
        r#"
<props>
  internal: { type: "size", default: "1px", expose: false }
</props>
<template><box/></template>
"#,
    )
    .unwrap();
    assert!(props_settings_schema(component.props.as_ref()).is_none());
    assert!(props_settings_schema(None).is_none());
}

#[test]
fn accessibility_for_tag_marks_switch_and_checkbox_focusable() {
    let checkbox = accessibility_for_tag("checkbox");
    assert_eq!(
        checkbox.role,
        mesh_core_elements::AccessibilityRole::Checkbox
    );
    assert!(checkbox.focusable);

    let switch = accessibility_for_tag("switch");
    assert_eq!(switch.role, mesh_core_elements::AccessibilityRole::Switch);
    assert!(switch.focusable);
}

#[test]
fn shared_value_change_handlers_are_normalized() {
    let attrs = vec![
        Attribute {
            name: "oninput".into(),
            value: AttributeValue::EventHandler("onInput".into()),
        },
        Attribute {
            name: "onchange".into(),
            value: AttributeValue::EventHandler("onChange".into()),
        },
        Attribute {
            name: "onselect".into(),
            value: AttributeValue::EventHandler("onSelect".into()),
        },
        Attribute {
            name: "onactivate".into(),
            value: AttributeValue::EventHandler("onActivate".into()),
        },
        Attribute {
            name: "onopenchange".into(),
            value: AttributeValue::EventHandler("onOpenChange".into()),
        },
    ];

    let (_, _, _, handlers, _) = parse_attributes(&attrs, None);

    assert_eq!(
        handlers.get("input").map(HandlerTarget::as_str),
        Some("onInput")
    );
    assert_eq!(
        handlers.get("change").map(HandlerTarget::as_str),
        Some("onChange")
    );
    assert_eq!(
        handlers.get("select").map(HandlerTarget::as_str),
        Some("onSelect")
    );
    assert_eq!(
        handlers.get("activate").map(HandlerTarget::as_str),
        Some("onActivate")
    );
    assert_eq!(
        handlers.get("openchange").map(HandlerTarget::as_str),
        Some("onOpenChange")
    );
}
