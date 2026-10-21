use super::contracts::*;
use super::snapshot::*;
use super::*;
use crate::Prop;
use crate::{Dimension, Edges, WidgetScrollMetrics};
use serde_json::json;

/// Pre-index behavior: scan every definition comparing tags.
fn element_contract_for_tag_scanning(tag: &str) -> Option<&'static ElementContractDef> {
    ELEMENT_CONTRACT_DEFS.iter().find(|def| def.tag == tag)
}

#[test]
fn element_contract_dispatch_covers_every_definition() {
    for (slot, def) in ELEMENT_CONTRACT_DEFS.iter().enumerate() {
        assert_eq!(
            contract_slot_for_tag(def.tag),
            Some(slot),
            "`contract_slots!` is missing or misorders `{}`",
            def.tag
        );
        let resolved = element_contract_for_tag(def.tag).expect("definition tag resolves");
        assert_eq!(resolved.tag, def.tag);
        assert_eq!(resolved.kind, def.kind);
    }
}

#[test]
fn element_contract_dispatch_matches_scanning_lookup() {
    let queries = ELEMENT_CONTRACT_DEFS.iter().map(|def| def.tag).chain([
        "",
        "Box",
        "box ",
        "not-an-element",
        "widgets",
        "widge",
    ]);
    for tag in queries {
        assert_eq!(
            element_contract_for_tag(tag).map(|def| def.tag),
            element_contract_for_tag_scanning(tag).map(|def| def.tag),
            "lookup parity differs for `{tag}`"
        );
    }
}

#[test]
fn canonical_schema_keeps_contracts_types_and_runtime_tags_in_lockstep() {
    assert_eq!(ELEMENT_CONTRACT_DEFS.len(), ELEMENT_TYPE_DEFS.len());

    for contract in ELEMENT_CONTRACT_DEFS {
        let type_def = ELEMENT_TYPE_DEFS
            .iter()
            .find(|candidate| candidate.source_tag == contract.source_tag)
            .expect("every contract has a generated type");
        assert_eq!(contract.source_tag, contract.tag);
        assert_eq!(type_def.source_tag, type_def.tag);
        assert_eq!(type_def.kind, contract.kind);
        assert_eq!(type_def.runtime_tag, contract.runtime_tag);
        assert_eq!(type_def.type_name, contract.type_name);
        assert_eq!(
            element_runtime_tag_for_tag(contract.tag),
            Some(contract.runtime_tag)
        );
    }

    for (tag, runtime_tag) in [
        ("label", "text"),
        ("scroll", "scroll"),
        ("scroll-view", "scroll"),
        ("scroll-area", "scroll"),
        ("slider", "slider"),
        ("image", "image"),
        ("grid", "box"),
    ] {
        assert_eq!(element_runtime_tag_for_tag(tag), Some(runtime_tag));
    }
}

#[test]
fn unknown_element_lookups_are_diagnostic_and_do_not_become_box() {
    let attribute = validate_element_attribute("not-an-element", "value", "x")
        .expect("unknown element attribute diagnostic");
    assert_eq!(attribute.kind, ElementDiagnosticKind::UnknownElementTag);

    let event = validate_element_event("not-an-element", "click")
        .expect("unknown element event diagnostic");
    assert_eq!(event.kind, ElementDiagnosticKind::UnknownElementTag);

    assert_eq!(element_runtime_tag_for_tag("not-an-element"), None);
    assert_eq!(
        element_type_for_tag("not-an-element").type_name,
        "MeshElement"
    );
}

#[test]
fn canonical_schema_owns_source_specific_input_defaults() {
    let textarea = element_contract_for_tag("textarea").expect("textarea contract");
    assert_eq!(textarea.runtime_tag, "input");
    assert_eq!(textarea.input_type, Some("textarea"));
    assert_eq!(textarea.default_attributes, &[("multiline", "true")]);

    let password = element_contract_for_tag("password-input").expect("password contract");
    assert_eq!(password.input_type, Some("password"));
    assert_eq!(password.default_attributes, &[("masked", "true")]);
}

#[test]
fn canonical_schema_uses_family_specific_attribute_profiles() {
    let text = element_contract_for_tag("text").expect("text contract");
    assert!(
        text.attributes
            .iter()
            .any(|attribute| attribute.name == "label")
    );
    assert!(
        !text
            .attributes
            .iter()
            .any(|attribute| attribute.name == "disabled")
    );

    let grid = element_contract_for_tag("grid").expect("grid contract");
    assert!(
        grid.attributes
            .iter()
            .any(|attribute| attribute.name == "columns")
    );
    assert!(
        !grid
            .attributes
            .iter()
            .any(|attribute| attribute.name == "placeholder")
    );

    let input = element_contract_for_tag("input").expect("input contract");
    assert!(
        input
            .attributes
            .iter()
            .any(|attribute| attribute.name == "placeholder")
    );
    assert!(
        !input
            .attributes
            .iter()
            .any(|attribute| attribute.name == "columns")
    );
}

#[test]
fn direct_f32_json_conversion_matches_serialization() {
    for value in [0.0, -0.0, 1.25, f32::MIN, f32::MAX, f32::NAN, f32::INFINITY] {
        let mut object = Map::new();
        insert_f32(&mut object, "value", value);
        assert_eq!(object["value"], json!(value));
    }
}

#[test]
fn icon_snapshot_exposes_base_and_icon_fields() {
    let mut node = WidgetNode::new("icon");
    node.set_mesh_key("0:1");
    node.attributes.insert(Prop::Ref, "batteryIcon".into());
    node.attributes.insert(Prop::Name, "battery-full".into());
    node.attributes.insert(Prop::Size, "18".into());
    node.layout.x = 10.0;
    node.layout.y = 20.0;
    node.layout.width = 24.0;
    node.layout.height = 24.0;
    node.computed_style.padding = Edges::all(2.0);

    let value = element_snapshot_json(&node, 0.0, 0.0);

    assert_eq!(value["element_type"], "IconElement");
    assert_eq!(value["ref"], "batteryIcon");
    assert_eq!(value["name"], "battery-full");
    assert_eq!(value["size"], 18.0);
    assert_eq!(value["width"], 24.0);
    assert_eq!(value["client_width"], 20.0);
}

#[test]
fn element_snapshot_json_matches_serde_snapshot_shape() {
    fn old_snapshot_json(node: &WidgetNode, offset_x: f32, offset_y: f32) -> Value {
        let snapshot = element_snapshot(node, offset_x, offset_y);
        let mut value = serde_json::to_value(snapshot).expect("snapshot serializes");
        expose_tag_specific_fields(value.as_object_mut().expect("snapshot object"), node);
        value
    }

    let mut node = WidgetNode::new("input");
    node.set_mesh_key("root/0");
    node.attributes.insert(Prop::Id, "search".into());
    node.attributes.insert(Prop::Ref, "searchBox".into());
    node.attributes.insert(Prop::Type, "search".into());
    node.attributes.insert(Prop::Value, "mesh".into());
    node.layout.x = 10.0;
    node.layout.y = 20.0;
    node.layout.width = 160.0;
    node.layout.height = 32.0;
    node.computed_style.padding = Edges::all(4.0);
    node.state.set(NodeState::FOCUSED, true);
    node.scroll_metrics = Some(WidgetScrollMetrics {
        x: 1.0,
        y: 2.0,
        max_x: 3.0,
        max_y: 4.0,
        content_width: 0.0,
        content_height: 0.0,
    });

    assert_eq!(
        element_snapshot_json(&node, 5.0, 7.0),
        old_snapshot_json(&node, 5.0, 7.0)
    );
}

#[test]
fn input_type_def_is_lookupable_by_tag() {
    let def = element_type_for_tag("input");

    assert_eq!(def.type_name, "InputElement");
    assert!(def.fields.iter().any(|field| field.name == "value"));
}

#[test]
fn input_snapshot_keeps_input_type_separate_from_element_type() {
    let mut node = WidgetNode::new("input");
    node.attributes.insert(Prop::Ref, "searchBox".into());
    node.attributes.insert(Prop::Type, "search".into());
    node.attributes.insert(Prop::Value, "mesh".into());

    let value = element_snapshot_json(&node, 0.0, 0.0);

    assert_eq!(value["element_type"], "InputElement");
    assert_eq!(value["type"], "search");
    assert_eq!(value["value"], "mesh");
}

#[test]
fn element_snapshot_reads_typed_scroll_metrics_before_legacy_attributes() {
    let mut node = WidgetNode::new("scroll-area");
    node.layout.width = 120.0;
    node.layout.height = 80.0;
    node.scroll_metrics.get_or_insert_default().x = 1.0;
    node.scroll_metrics.get_or_insert_default().y = 2.0;
    node.scroll_metrics.get_or_insert_default().max_x = 3.0;
    node.scroll_metrics.get_or_insert_default().max_y = 4.0;
    node.scroll_metrics = Some(WidgetScrollMetrics {
        x: 11.0,
        y: 22.0,
        max_x: 33.0,
        max_y: 44.0,
        content_width: 153.0,
        content_height: 124.0,
    });

    let snapshot = element_snapshot(&node, 0.0, 0.0);

    assert_eq!(snapshot.scroll_x, 11.0);
    assert_eq!(snapshot.scroll_y, 22.0);
    assert_eq!(snapshot.max_scroll_left, 33.0);
    assert_eq!(snapshot.max_scroll_top, 44.0);
    assert_eq!(snapshot.scroll_width, 153.0);
    assert_eq!(snapshot.scroll_height, 124.0);
}

#[test]
fn unknown_tags_fall_back_to_mesh_element() {
    let mut node = WidgetNode::new("custom");
    node.computed_style.width = Dimension::Auto;

    let value = element_snapshot_json(&node, 0.0, 0.0);

    assert_eq!(value["element_type"], "MeshElement");
}

#[test]
fn element_contract_metadata_types_are_available() {
    let contract = element_contract_for_tag("button").expect("button contract");

    assert_eq!(contract.family, ElementFamily::Action);
    assert_eq!(contract.accessibility.role, AccessibilityRole::Button);
    assert!(
        contract
            .attributes
            .iter()
            .any(|attribute| attribute.name == "disabled")
    );
    assert!(contract.events.iter().any(|event| event.name == "change"));
}

#[test]
fn element_contract_covers_v1_16_taxonomy() {
    let required = [
        "grid",
        "scroll-area",
        "form-row",
        "badge",
        "progress",
        "tooltip",
        "toggle-button",
        "textarea",
        "number-input",
        "select",
        "radio-group",
        "segmented-control",
        "menu",
        "command-item",
        "popover",
        "tabs",
        "table",
        "tree",
        "empty-state",
        "surface",
    ];

    for tag in required {
        assert!(
            element_contract_for_tag(tag).is_some(),
            "missing contract for {tag}"
        );
    }

    let families: std::collections::BTreeSet<_> = ELEMENT_CONTRACT_DEFS
        .iter()
        .map(|contract| contract.family)
        .collect();
    assert!(families.contains(&ElementFamily::Layout));
    assert!(families.contains(&ElementFamily::Display));
    assert!(families.contains(&ElementFamily::Action));
    assert!(families.contains(&ElementFamily::TextInput));
    assert!(families.contains(&ElementFamily::ChoiceMenu));
    assert!(families.contains(&ElementFamily::Container));
    assert!(families.contains(&ElementFamily::Collection));
    assert!(families.contains(&ElementFamily::Shell));
}

#[test]
fn element_state_snapshot_exposes_shared_control_state() {
    let state = NodeState::CSS - NodeState::WINDOW;

    let snapshot = ElementStateSnapshot::from(state);

    assert!(snapshot.read_only);
    assert!(snapshot.required);
    assert!(snapshot.selected);
    assert!(snapshot.expanded);
    assert!(snapshot.pressed);
    assert!(snapshot.invalid);
    assert!(snapshot.value);
}

#[test]
fn element_contract_common_state_flags_cover_required_set() {
    let flags = common_state_flags();

    for flag in [
        ElementStateFlag::Disabled,
        ElementStateFlag::ReadOnly,
        ElementStateFlag::Required,
        ElementStateFlag::Focused,
        ElementStateFlag::Selected,
        ElementStateFlag::Checked,
        ElementStateFlag::Expanded,
        ElementStateFlag::Pressed,
        ElementStateFlag::Invalid,
        ElementStateFlag::Active,
        ElementStateFlag::Value,
    ] {
        assert!(flags.contains(&flag), "missing {flag:?}");
    }
}

#[test]
fn phase87_layout_display_contract_exposes_required_metadata() {
    let grid = element_contract_for_tag("grid").expect("grid contract");
    assert!(grid.attributes.iter().any(|attr| attr.name == "columns"));
    assert!(grid.attributes.iter().any(|attr| attr.name == "rows"));
    assert!(grid.attributes.iter().any(|attr| attr.name == "column"));
    assert!(grid.style_hooks.contains(&"layout"));

    let scroll_area = element_contract_for_tag("scroll-area").expect("scroll-area contract");
    assert!(
        scroll_area
            .attributes
            .iter()
            .any(|attr| attr.name == "overflow-y")
    );

    for tag in ["section", "header", "footer", "group", "form-row"] {
        let contract = element_contract_for_tag(tag).expect("structure contract");
        assert_eq!(contract.family, ElementFamily::Layout);
        assert!(contract.attributes.iter().any(|attr| attr.name == "label"));
        assert!(contract.style_hooks.contains(&"structure"));
    }

    let progress = element_contract_for_tag("progress").expect("progress contract");
    assert_eq!(progress.accessibility.role, AccessibilityRole::ProgressBar);
    assert!(progress.attributes.iter().any(|attr| attr.name == "min"));
    assert!(progress.attributes.iter().any(|attr| attr.name == "max"));
    assert!(
        progress
            .attributes
            .iter()
            .any(|attr| attr.name == "indeterminate")
    );
    assert!(progress.style_hooks.contains(&"progress"));

    let meter = element_contract_for_tag("meter").expect("meter contract");
    assert_eq!(meter.family, ElementFamily::Display);
    assert_eq!(meter.accessibility.role, AccessibilityRole::ProgressBar);

    for tag in ["badge", "avatar", "shortcut", "tooltip"] {
        let contract = element_contract_for_tag(tag).expect("display contract");
        assert_eq!(contract.family, ElementFamily::Display);
        assert!(contract.style_hooks.contains(&"display"));
    }
}

#[test]
fn phase87_layout_display_diagnostics_validate_values() {
    let diagnostic = validate_element_attribute("grid", "columns", "1fr 2fr")
        .expect("invalid grid track diagnostic");
    assert_eq!(
        diagnostic.kind,
        ElementDiagnosticKind::InvalidAttributeValue
    );
    assert!(diagnostic.message.contains("unsupported grid track"));

    assert!(validate_element_attribute("grid", "columns", "120px auto").is_none());

    let diagnostic = validate_element_attribute("progress", "value", "half")
        .expect("invalid progress value diagnostic");
    assert_eq!(
        diagnostic.kind,
        ElementDiagnosticKind::InvalidAttributeValue
    );
    assert!(diagnostic.message.contains("expected a numeric value"));

    let diagnostic = validate_element_attribute("progress", "indeterminate", "maybe")
        .expect("invalid progress boolean diagnostic");
    assert_eq!(
        diagnostic.kind,
        ElementDiagnosticKind::InvalidAttributeValue
    );

    let diagnostic = validate_element_attribute("tooltip", "tooltip-for", "")
        .expect("invalid tooltip owner diagnostic");
    assert_eq!(
        diagnostic.kind,
        ElementDiagnosticKind::InvalidAttributeValue
    );

    let diagnostic = validate_element_attribute("section", "value", "active")
        .expect("structure value diagnostic");
    assert_eq!(
        diagnostic.kind,
        ElementDiagnosticKind::InvalidAttributeValue
    );
}

#[test]
fn phase88_single_button_contract_rejects_icon_shortcut_attributes() {
    let button = element_contract_for_tag("button").expect("button contract");

    assert_eq!(button.family, ElementFamily::Action);
    assert_eq!(button.accessibility.role, AccessibilityRole::Button);
    for attr in ["pressed", "busy", "default", "destructive", "keybind"] {
        assert!(
            button
                .attributes
                .iter()
                .any(|candidate| candidate.name == attr),
            "button should expose {attr}"
        );
    }

    for attr in ["icon", "name", "src"] {
        let diagnostic =
            validate_element_attribute("button", attr, "audio-volume-high").expect(attr);
        assert_eq!(
            diagnostic.kind,
            ElementDiagnosticKind::InvalidAttributeValue
        );
        assert!(diagnostic.action.contains("<icon>"));
    }
}

#[test]
fn phase88_input_variant_contract_exposes_configured_input_metadata() {
    for tag in [
        "input",
        "textarea",
        "search",
        "password",
        "number-input",
        "stepper",
    ] {
        let contract = element_contract_for_tag(tag).expect("input contract");
        assert_eq!(contract.family, ElementFamily::TextInput);
        assert_eq!(contract.accessibility.role, AccessibilityRole::TextInput);
        for attr in [
            "value",
            "placeholder",
            "readonly",
            "required",
            "invalid",
            "type",
        ] {
            assert!(
                contract
                    .attributes
                    .iter()
                    .any(|candidate| candidate.name == attr),
                "{tag} should expose {attr}"
            );
        }
    }

    for attr in ["min", "max", "step"] {
        assert!(
            element_contract_for_tag("number-input")
                .expect("number-input")
                .attributes
                .iter()
                .any(|candidate| candidate.name == attr),
            "number-input should expose {attr}"
        );
    }
}

#[test]
fn phase88_input_diagnostics_validate_numeric_and_boolean_values() {
    let diagnostic =
        validate_element_attribute("number-input", "value", "many").expect("invalid numeric value");
    assert_eq!(
        diagnostic.kind,
        ElementDiagnosticKind::InvalidAttributeValue
    );
    assert!(diagnostic.message.contains("expected a numeric value"));

    let diagnostic =
        validate_element_attribute("stepper", "step", "0").expect("invalid step value");
    assert_eq!(
        diagnostic.kind,
        ElementDiagnosticKind::InvalidAttributeValue
    );
    assert!(diagnostic.message.contains("positive numeric"));

    let diagnostic =
        validate_element_attribute("textarea", "multiline", "sometimes").expect("bool value");
    assert_eq!(
        diagnostic.kind,
        ElementDiagnosticKind::InvalidAttributeValue
    );

    assert!(validate_element_attribute("number-input", "min", "0").is_none());
    assert!(validate_element_attribute("number-input", "max", "100").is_none());
    assert!(validate_element_attribute("number-input", "step", "5").is_none());
}

#[test]
fn phase89_choice_and_menu_diagnostics_validate_authoring_state() {
    let option = validate_element_attribute("option", "value", "").expect("option diagnostic");
    assert_eq!(option.kind, ElementDiagnosticKind::InvalidAttributeValue);
    assert!(option.message.contains("options need"));

    let radio = validate_element_attribute("radio", "value", "").expect("radio diagnostic");
    assert_eq!(radio.kind, ElementDiagnosticKind::InvalidAttributeValue);
    assert!(radio.message.contains("radio choices"));

    let checked =
        validate_element_attribute("checkbox", "checked", "maybe").expect("bool diagnostic");
    assert_eq!(checked.kind, ElementDiagnosticKind::InvalidAttributeValue);

    assert!(validate_element_attribute("menu-item", "disabled", "true").is_none());
    assert!(validate_element_event("menu-item", "activate").is_none());
    assert!(validate_element_event("select", "change").is_none());
}

#[test]
fn phase90_container_and_collection_diagnostics_validate_state() {
    let dialog = validate_element_attribute("dialog", "aria-label", "").expect("label diagnostic");
    assert_eq!(dialog.kind, ElementDiagnosticKind::InvalidAttributeValue);
    assert!(dialog.message.contains("accessible label"));

    let tab = validate_element_attribute("tab", "selected", "sometimes").expect("bool diagnostic");
    assert_eq!(tab.kind, ElementDiagnosticKind::InvalidAttributeValue);

    assert!(validate_element_attribute("details", "open", "true").is_none());
    assert!(validate_element_attribute("list-item", "selected", "false").is_none());
    assert!(validate_element_event("tab", "activate").is_none());
    assert!(validate_element_event("list-item", "activate").is_none());
}

#[test]
fn element_diagnostic_unsupported_attribute_reports_author_action() {
    let diagnostic =
        validate_element_attribute("button", "browser-form-action", "submit").expect("diagnostic");

    assert_eq!(diagnostic.kind, ElementDiagnosticKind::UnsupportedAttribute);
    assert_eq!(diagnostic.tag, "button");
    assert_eq!(diagnostic.name, "browser-form-action");
    assert!(
        diagnostic
            .action
            .contains("Remove the attribute or use one of")
    );
}

#[test]
fn element_diagnostic_unsupported_event_reports_author_action() {
    let diagnostic = validate_element_event("button", "formsubmit").expect("diagnostic");

    assert_eq!(diagnostic.kind, ElementDiagnosticKind::UnsupportedEvent);
    assert_eq!(diagnostic.tag, "button");
    assert_eq!(diagnostic.name, "formsubmit");
    assert!(
        diagnostic
            .action
            .contains("Remove the handler or use one of")
    );
}

#[test]
fn element_diagnostic_known_common_attribute_does_not_report_diagnostic() {
    assert!(validate_element_attribute("button", "disabled", "true").is_none());
}
