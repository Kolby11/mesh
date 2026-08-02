use super::super::*;
use super::common::*;
use mesh_core_component::template::Attribute;
use mesh_core_elements::HandlerTarget;

#[test]
fn event_handler_call_attrs_store_typed_args_not_json_handler_string() {
    let attrs = vec![Attribute {
        name: "onclick".into(),
        value: AttributeValue::EventHandlerCall {
            handler: "selectItem".into(),
            args: vec!["item_id".into(), "\"fallback\"".into()],
        },
    }];
    let store = MapStore(
        [
            ("item_id".to_string(), serde_json::json!("alpha")),
            ("selectItem".to_string(), serde_json::json!("onSelectItem")),
        ]
        .into_iter()
        .collect(),
    );

    let (_, _, _, handlers, handler_calls) = parse_attributes(&attrs, Some(&store));

    assert_eq!(
        handlers.get("click").map(HandlerTarget::as_str),
        Some("onSelectItem")
    );
    let call = handler_calls.get("click").expect("typed call");
    assert_eq!(call.handler, "onSelectItem");
    assert_eq!(
        call.args,
        vec![
            serde_json::Value::String("alpha".into()),
            serde_json::Value::String("fallback".into())
        ]
    );
    assert!(
        !handlers
            .get("click")
            .is_some_and(|handler| handler.starts_with('{')),
        "event handler should no longer be JSON-in-a-string"
    );
}

#[test]
fn two_way_value_binding_still_resolves_attribute_value() {
    let attrs = vec![Attribute {
        name: "value".into(),
        value: AttributeValue::TwoWayBinding("current_value".into()),
    }];

    let (_, _, resolved, _, _) = parse_attributes(&attrs, None);

    assert_eq!(resolved.get("value"), Some(&String::new()));
}

#[test]
fn bound_attributes_retain_runtime_types_through_accessibility_lowering() {
    let attrs = [
        ("disabled", "enabled"),
        ("min", "minimum"),
        ("data-metadata", "metadata"),
    ]
    .map(|(name, binding)| Attribute {
        name: name.into(),
        value: AttributeValue::Binding(binding.into()),
    });
    let store = MapStore(std::collections::HashMap::new());

    let (_, _, resolved, _, _) = parse_attributes_runtime(
        &attrs,
        Some(&store),
        "typed-attributes",
        Some(&TypedExpressionComposition),
        false,
    );

    assert!(resolved.get_value("disabled").unwrap().legacy_bool());
    assert_eq!(resolved.get_value("min").unwrap().parse_f32(), Some(1.5));
    assert_eq!(
        resolved
            .get_value("data-metadata")
            .unwrap()
            .to_legacy_string(),
        r#"{"source":"runtime"}"#
    );

    let accessibility = accessibility_for_element("input", "input", &resolved);
    assert!(accessibility.state.disabled);
    assert_eq!(accessibility.state.value_min, Some(1.5));
    assert_eq!(resolved.get("disabled").map(String::as_str), Some("true"));
    assert_eq!(resolved.get("min").map(String::as_str), Some("1.5"));
}

#[test]
fn phase87_layout_display_source_semantics_survive_lowering() {
    let component = mesh_core_component::parse_component(
        r#"
<template>
  <grid columns="120px auto" label="Main grid">
<progress value="50" min="0" max="100" label="Loading" />
<section label="Details">
  <badge>Ready</badge>
</section>
  </grid>
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
        320.0,
        120.0,
        None,
        "root",
        None,
        &[],
    );

    let grid = tree.children.first().expect("grid node");
    assert_eq!(grid.tag, "box");
    assert_eq!(
        grid.attributes.get("data-mesh-element"),
        Some(&"grid".to_string())
    );
    assert_eq!(
        grid.accessibility.role,
        mesh_core_elements::AccessibilityRole::Region
    );
    assert_eq!(grid.accessibility.label.as_deref(), Some("Main grid"));

    let progress = grid
        .children
        .iter()
        .find(|node| {
            node.attributes
                .get("data-mesh-element")
                .is_some_and(|value| value == "progress")
        })
        .expect("progress node");
    assert_eq!(progress.tag, "text");
    assert_eq!(
        progress.accessibility.role,
        mesh_core_elements::AccessibilityRole::ProgressBar
    );
    assert_eq!(progress.accessibility.state.value.as_deref(), Some("50"));
    assert_eq!(progress.accessibility.state.value_min, Some(0.0));
    assert_eq!(progress.accessibility.state.value_max, Some(100.0));
}

#[test]
fn phase88_button_aliases_preserve_source_semantics_without_icon_shortcuts() {
    let component = mesh_core_component::parse_component(
        r#"
<template>
  <icon-button onclick={onTap} pressed="true" busy="true" keybind="media.play">
<icon name="media-playback-start" />
<text>Play</text>
  </icon-button>
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
        240.0,
        80.0,
        None,
        "root",
        None,
        &[],
    );

    let button = tree.children.first().expect("button alias");
    assert_eq!(button.tag, "button");
    assert_eq!(
        button.attributes.get("data-mesh-element"),
        Some(&"icon-button".to_string())
    );
    assert_eq!(button.event_handlers.get("click"), Some(&"onTap".into()));
    assert_eq!(
        button.accessibility.role,
        mesh_core_elements::AccessibilityRole::Button
    );
    assert!(button.accessibility.state.pressed);
    assert!(button.accessibility.state.busy);
    assert_eq!(
        button.accessibility.keyboard_shortcut.as_deref(),
        Some("media.play")
    );
    assert!(button.children.iter().any(|child| child.tag == "icon"));
}

#[test]
fn phase88_input_variants_configure_single_runtime_input_path() {
    let component = mesh_core_component::parse_component(
        r#"
<template>
  <textarea value="hello" placeholder="Notes" required="true" />
  <password value="secret" />
  <number-input value="5" min="0" max="10" step="1" />
  <stepper value="2" min="0" max="5" />
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
        240.0,
        160.0,
        None,
        "root",
        None,
        &[],
    );

    let textarea = &tree.children[0];
    assert_eq!(textarea.tag, "input");
    assert_eq!(
        textarea.attributes.get("data-mesh-element"),
        Some(&"textarea".to_string())
    );
    assert_eq!(
        textarea.attributes.get("type"),
        Some(&"textarea".to_string())
    );
    assert_eq!(
        textarea.attributes.get("multiline"),
        Some(&"true".to_string())
    );
    assert!(textarea.accessibility.state.required);

    let password = &tree.children[1];
    assert_eq!(password.tag, "input");
    assert_eq!(
        password.attributes.get("type"),
        Some(&"password".to_string())
    );
    assert_eq!(password.attributes.get("masked"), Some(&"true".to_string()));

    let number = &tree.children[2];
    assert_eq!(number.tag, "input");
    assert_eq!(
        number.attributes.get("data-mesh-element"),
        Some(&"number-input".to_string())
    );
    assert_eq!(number.attributes.get("type"), Some(&"number".to_string()));
    assert_eq!(number.accessibility.state.value.as_deref(), Some("5"));
    assert_eq!(number.accessibility.state.value_min, Some(0.0));
    assert_eq!(number.accessibility.state.value_max, Some(10.0));

    let stepper = &tree.children[3];
    assert_eq!(stepper.tag, "input");
    assert_eq!(stepper.attributes.get("type"), Some(&"number".to_string()));
    assert_eq!(stepper.attributes.get("step"), Some(&"1".to_string()));
}

#[test]
fn phase89_choice_menu_source_semantics_survive_lowering() {
    let component = mesh_core_component::parse_component(
        r#"
<template>
  <select value="en" onchange={onLocaleChange} label="Language">
<option value="en">English</option>
<option value="sk" selected="true">Slovak</option>
  </select>
  <menu label="Commands">
<menu-item onactivate={onCommand} keybind="ctrl+k">Command</menu-item>
  </menu>
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
        320.0,
        160.0,
        None,
        "root",
        None,
        &[],
    );

    let select = &tree.children[0];
    assert_eq!(select.tag, "input");
    assert_eq!(
        select.attributes.get("data-mesh-element"),
        Some(&"select".to_string())
    );
    assert_eq!(
        select.event_handlers.get("change"),
        Some(&"onLocaleChange".into())
    );
    assert_eq!(
        select.accessibility.role,
        mesh_core_elements::AccessibilityRole::Menu
    );
    assert_eq!(select.accessibility.state.value.as_deref(), Some("en"));
    assert_eq!(select.children.len(), 2);

    let option = &select.children[1];
    assert_eq!(
        option.attributes.get("data-mesh-element"),
        Some(&"option".to_string())
    );
    assert!(option.accessibility.state.selected);

    let menu = &tree.children[1];
    assert_eq!(
        menu.attributes.get("data-mesh-element"),
        Some(&"menu".into())
    );
    assert_eq!(
        menu.accessibility.role,
        mesh_core_elements::AccessibilityRole::Menu
    );
    let item = &menu.children[0];
    assert_eq!(
        item.attributes.get("data-mesh-element"),
        Some(&"menu-item".to_string())
    );
    assert_eq!(
        item.event_handlers.get("activate"),
        Some(&"onCommand".into())
    );
    assert_eq!(
        item.accessibility.keyboard_shortcut.as_deref(),
        Some("ctrl+k")
    );
}

#[test]
fn phase90_container_collection_source_semantics_survive_lowering() {
    let component = mesh_core_component::parse_component(
        r#"
<template>
  <tabs label="Debug views">
<tab selected="true" onactivate={onOverview}>Overview</tab>
<tab onactivate={onSurfaces}>Surfaces</tab>
  </tabs>
  <list label="Surfaces">
<list-item selected="true" onactivate={onSurface}>Navigation</list-item>
<empty-state>No rows</empty-state>
  </list>
  <details open="true" label="Advanced">
<text>Details body</text>
  </details>
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
        320.0,
        160.0,
        None,
        "root",
        None,
        &[],
    );

    let tabs = &tree.children[0];
    assert_eq!(
        tabs.attributes.get("data-mesh-element"),
        Some(&"tabs".into())
    );
    let tab = &tabs.children[0];
    assert_eq!(tab.attributes.get("data-mesh-element"), Some(&"tab".into()));
    assert_eq!(
        tab.event_handlers.get("activate"),
        Some(&"onOverview".into())
    );
    assert_eq!(
        tab.accessibility.role,
        mesh_core_elements::AccessibilityRole::Tab
    );
    assert!(tab.accessibility.state.selected);

    let list = &tree.children[1];
    assert_eq!(list.tag, "column");
    assert_eq!(
        list.attributes.get("data-mesh-element"),
        Some(&"list".into())
    );
    assert_eq!(
        list.accessibility.role,
        mesh_core_elements::AccessibilityRole::List
    );
    let item = &list.children[0];
    assert_eq!(
        item.attributes.get("data-mesh-element"),
        Some(&"list-item".to_string())
    );
    assert_eq!(
        item.accessibility.role,
        mesh_core_elements::AccessibilityRole::ListItem
    );
    assert!(item.accessibility.state.selected);

    let details = &tree.children[2];
    assert_eq!(
        details.attributes.get("data-mesh-element"),
        Some(&"details".to_string())
    );
    assert_eq!(details.accessibility.state.expanded, Some(true));
}

impl mesh_core_elements::VariableStore for MapStore {
    fn get(&self, name: &str) -> Option<serde_json::Value> {
        self.0.get(name).cloned()
    }
    fn get_ref<'a>(&'a self, name: &str) -> Option<&'a serde_json::Value> {
        self.0.get(name)
    }
    fn keys(&self) -> Vec<String> {
        self.0.keys().cloned().collect()
    }
}

#[test]
fn tracking_store_records_dotted_reads() {
    let mut map = std::collections::HashMap::new();
    map.insert("audio".to_string(), serde_json::json!({"percent": 80}));
    let inner = MapStore(map);
    let t = TrackingVariableStore::new(&inner);
    let _ = mesh_core_elements::VariableStore::get(&t, "audio.percent");
    let reads = t.into_reads();
    assert_eq!(reads, vec![("audio".to_string(), "percent".to_string())]);
}
