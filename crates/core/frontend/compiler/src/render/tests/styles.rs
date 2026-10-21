use super::super::*;
use super::common::*;
use crate::TemplateExpressionResult;
use mesh_core_component::template::Attribute;
use mesh_core_elements::Prop;
use mesh_core_elements::{ComponentCompositionProps, EventHandlerCall, HandlerTarget};
use std::collections::BTreeMap;

#[test]
fn translated_expression_uses_the_live_composition_resolver() {
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
            _expression: &mesh_core_expression::CompiledExpression,
            _locals: &serde_json::Map<String, serde_json::Value>,
        ) -> Option<TemplateExpressionResult> {
            None
        }

        fn bind_handler_expression(
            &self,
            _instance_key: &str,
            _expression: &mesh_core_expression::CompiledExpression,
            _locals: &serde_json::Map<String, serde_json::Value>,
            source: &str,
            call_binding: bool,
        ) -> Option<HandlerTarget> {
            assert!(call_binding);
            Some(HandlerTarget::Function {
                instance_key: None,
                source: source.into(),
                callback: mesh_core_elements::CallbackHandle::new(
                    if source.contains("primary") { 1 } else { 2 },
                    std::sync::Arc::new(()),
                ),
            })
        }

        fn render_import(
            &self,
            _host: &Manifest,
            _host_instance_key: &str,
            _owner_source_path: Option<&std::path::Path>,
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
local Child = require("./child.mesh")
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
    assert!(!props.values.contains_name("onprimary"));
    assert!(!props.values.contains_name("onsecondary"));
    assert_ne!(props.callbacks["onprimary"], props.callbacks["onsecondary"]);
    assert_eq!(
        props.bindings.get(Prop::Hidden).map(String::as_str),
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
    assert_eq!(calls["onprimary"].handler, props.callbacks["onprimary"]);
    assert_eq!(calls["onprimary"].handler.instance_key(), Some("root"));
    assert_eq!(calls["onsecondary"].handler, props.callbacks["onsecondary"]);
    assert!(calls.values().all(|call| call.args.is_empty()));
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

const PROP_COMPONENT: &str = r#"
<props>
{
    track_width = { type = "size", default = "20px" },
}
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
        active_box.attributes.get(Prop::Class),
        Some(&"active".to_string())
    );
    assert_eq!(
        active_box.computed_style.width,
        mesh_core_elements::Dimension::Px(42.0)
    );
}

#[test]
fn inherited_style_matching_uses_the_node_pseudo_state() {
    let component = mesh_core_component::parse_component(
        r#"
<template>
  <box>
    <button />
    <button disabled="true" />
  </box>
</template>
<style>
box { color: #123456; }
button:hover { color: #abcdef; }
button:disabled { color: #fedcba; }
</style>
"#,
    )
    .unwrap();
    let tree = build_widget_tree_from_component(
        &component,
        &test_manifest(),
        &mesh_core_theme::default_theme(),
        200.0,
        80.0,
        None,
        "root",
        None,
        &[],
    );

    let authored_box = tree.children.first().expect("template box");
    assert_eq!(authored_box.children.len(), 2);
    assert_eq!(
        authored_box.children[0].computed_style.color,
        mesh_core_elements::Color::from_hex("#123456").unwrap()
    );
    assert_eq!(
        authored_box.children[1].computed_style.color,
        mesh_core_elements::Color::from_hex("#fedcba").unwrap()
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
        positioned.attributes.get(Prop::Style).map(String::as_str),
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
{
    width = { type = "size", default = "fit-content", label = t("var.width") },
    density = { type = "enum", options = {"compact", "cozy"}, default = "cozy" },
    hidden = { type = "size", default = "10px", expose = false },
}
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
{
    internal = { type = "size", default = "1px", expose = false },
}
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
            quoted_expression: false,
            span: None,
        },
        Attribute {
            name: "onchange".into(),
            value: AttributeValue::EventHandler("onChange".into()),
            quoted_expression: false,
            span: None,
        },
        Attribute {
            name: "onselect".into(),
            value: AttributeValue::EventHandler("onSelect".into()),
            quoted_expression: false,
            span: None,
        },
        Attribute {
            name: "onactivate".into(),
            value: AttributeValue::EventHandler("onActivate".into()),
            quoted_expression: false,
            span: None,
        },
        Attribute {
            name: "onopenchange".into(),
            value: AttributeValue::EventHandler("onOpenChange".into()),
            quoted_expression: false,
            span: None,
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

#[test]
fn token_props_resolve_css_and_dotted_names_through_the_theme() {
    for reference in [
        "color-primary",
        "color.primary",
        "--color-primary",
        "var(--color-primary)",
    ] {
        let source = format!(
            r#"<props>{{ accent = {{ type = "token", default = "{reference}" }}, scale = {{ type = "token", default = "typography.size.2xl" }} }}</props>
<template><text class="token-value">Value</text></template>
<style>.token-value {{ color: prop(accent); font-size: prop(scale); }}</style>"#
        );
        let component = mesh_core_component::parse_component(&source).unwrap();
        let mut theme = mesh_core_theme::default_theme();
        theme.set_token(
            "color.primary",
            mesh_core_theme::TokenValue::String("#123456".into()),
            mesh_core_theme::ThemeProvenance::UserOverride,
        );
        theme.set_token(
            "typography.size.2xl",
            mesh_core_theme::TokenValue::Number(32.0),
            mesh_core_theme::ThemeProvenance::UserOverride,
        );
        let tree = build_widget_tree_from_component(
            &component,
            &test_manifest(),
            &theme,
            200.0,
            80.0,
            None,
            "root",
            None,
            &[],
        );
        fn find_text(node: &WidgetNode) -> Option<&WidgetNode> {
            if node.tag == "text" {
                Some(node)
            } else {
                node.children.iter().find_map(find_text)
            }
        }
        let text = find_text(&tree).unwrap();
        assert_eq!(
            text.computed_style.color,
            mesh_core_elements::Color::from_hex("#123456").unwrap(),
            "{reference}"
        );
        assert_eq!(text.computed_style.font_size, 32.0, "{reference}");
    }
}

#[test]
fn user_parts_apply_in_preview_and_preserve_explicit_text_inheritance() {
    let component = mesh_core_component::parse_component(r#"
<template><column><text part="label" style="font-size: 10px">Public</text><text>Private</text></column></template>
<style>column { font-size: 12px; } text { height: 20px; }</style>"#).unwrap();
    let mut manifest = test_manifest();
    manifest.package.id = "@test/preview".into();
    let mut theme = mesh_core_theme::default_theme();
    theme
        .set_user_stylesheet(&format!(
            "{}::part(label) {{ font-size: 27px; width: 80px; }}",
            manifest.package.id
        ))
        .unwrap();
    let rules = &component.style.as_ref().unwrap().rules;
    let tree = build_widget_tree_from_component(
        &component, &manifest, &theme, 200.0, 80.0, None, "root", None, rules,
    );
    let public = find_tag(&tree, "text").unwrap();
    assert_eq!(public.computed_style.font_size, 27.0);
    assert_eq!(
        public.computed_style.width,
        mesh_core_elements::style::Dimension::Px(80.0)
    );
}
