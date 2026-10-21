use super::super::*;
use mesh_core_elements::Prop;
use mesh_core_elements::accessibility::AccessibilityInfo;
use mesh_core_elements::{AttrKey, AttributeMap};

fn accessibility_info_eq(a: &AccessibilityInfo, b: &AccessibilityInfo) -> bool {
    a.role == b.role
        && a.label == b.label
        && a.description == b.description
        && a.focusable == b.focusable
        && a.focused == b.focused
        && a.keyboard_shortcut == b.keyboard_shortcut
        && a.state.disabled == b.state.disabled
        && a.state.checked == b.state.checked
        && a.state.expanded == b.state.expanded
        && a.state.selected == b.state.selected
        && a.state.pressed == b.state.pressed
        && a.state.busy == b.state.busy
        && a.state.invalid == b.state.invalid
        && a.state.required == b.state.required
        && a.state.value == b.state.value
        && a.state.value_min == b.state.value_min
        && a.state.value_max == b.state.value_max
}

#[test]
fn accessibility_for_element_empty_attributes_matches_unguarded_chain() {
    let empty = AttributeMap::new();
    for tag in ["box", "row", "column", "button", "text", "custom-widget"] {
        assert!(accessibility_info_eq(
            &accessibility_for_element(tag, tag, &empty),
            &accessibility_for_element_unguarded(tag, tag, &empty)
        ));
    }
}

#[test]
fn accessibility_for_element_non_empty_attributes_matches_unguarded_chain() {
    let mut attributes = AttributeMap::new();
    attributes.insert(Prop::Class, "active".to_string());
    attributes.insert(Prop::Disabled, "true".to_string());
    attributes.insert(Prop::Min, "1".to_string());
    assert!(accessibility_info_eq(
        &accessibility_for_element("input", "input", &attributes),
        &accessibility_for_element_unguarded("input", "input", &attributes)
    ));
}

#[test]
fn accessibility_single_pass_matches_lookup_chain_including_precedence() {
    // Every attribute the chain reads, in every alternative spelling, plus
    // unrelated attributes that must be ignored.
    let all: Vec<(&str, &str)> = vec![
        ("aria-label", "aria"),
        ("label", "label"),
        ("alt", "alt"),
        ("title", "title"),
        ("tooltip", "tooltip"),
        ("key", "ctrl+k"),
        ("keybind", "ctrl+b"),
        ("shortcut", "ctrl+s"),
        ("expanded", "true"),
        ("open", "false"),
        ("disabled", "1"),
        ("checked", "false"),
        ("selected", ""),
        ("pressed", "true"),
        ("busy", "nope"),
        ("invalid", "true"),
        ("required", "1"),
        ("value", " 42 "),
        ("min", " 1.5 "),
        ("max", "not-a-number"),
        ("class", "primary"),
        ("data-mesh-element", "button"),
    ];

    // Full set, then every single-attribute map, then each attribute with
    // its higher-precedence sibling removed.
    let mut cases: Vec<AttributeMap> = vec![
        all.iter()
            .map(|(k, v)| (AttrKey::new(k), v.to_string()))
            .collect(),
    ];
    for (name, value) in &all {
        cases.push(AttributeMap::from([(
            AttrKey::new(name),
            value.to_string(),
        )]));
    }
    for skip in ["aria-label", "label", "title", "key", "keybind", "expanded"] {
        cases.push(
            all.iter()
                .filter(|(name, _)| *name != skip)
                .map(|(k, v)| (AttrKey::new(k), v.to_string()))
                .collect(),
        );
    }

    for attributes in cases {
        for tag in ["input", "button", "box", "custom-widget"] {
            let single_pass = accessibility_for_element(tag, tag, &attributes);
            let chain = accessibility_for_element_unguarded(tag, tag, &attributes);
            assert!(
                accessibility_info_eq(&single_pass, &chain),
                "mismatch for <{tag}> with {attributes:?}"
            );
            assert_eq!(single_pass.label, chain.label);
            assert_eq!(single_pass.description, chain.description);
            assert_eq!(single_pass.keyboard_shortcut, chain.keyboard_shortcut);
        }
    }
}

#[test]
fn shared_module_ids_reuse_one_allocation_per_module() {
    let mut first = WidgetNode::new("row");
    let mut second = WidgetNode::new("column");
    attach_module_id(&mut first, "@mesh/panel");
    attach_module_id(&mut second, "@mesh/panel");
    assert_eq!(first.module_id(), Some("@mesh/panel"));
    assert!(std::sync::Arc::ptr_eq(
        first.shared_module_id().unwrap(),
        second.shared_module_id().unwrap()
    ));

    let mut other = WidgetNode::new("box");
    attach_module_id(&mut other, "@mesh/launcher");
    assert_eq!(other.module_id(), Some("@mesh/launcher"));
    assert!(!std::sync::Arc::ptr_eq(
        first.shared_module_id().unwrap(),
        other.shared_module_id().unwrap()
    ));

    // The bounded cache must keep returning correct ids after eviction.
    for index in 0..(SHARED_MODULE_ID_CACHE_LIMIT * 2) {
        let id = format!("@mesh/module-{index}");
        let mut node = WidgetNode::new("row");
        attach_module_id(&mut node, &id);
        assert_eq!(node.module_id(), Some(id.as_str()));
    }
    let mut again = WidgetNode::new("row");
    attach_module_id(&mut again, "@mesh/panel");
    assert_eq!(again.module_id(), Some("@mesh/panel"));
}
