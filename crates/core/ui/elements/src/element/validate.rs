use super::*;

pub fn validate_element_attribute(tag: &str, name: &str, value: &str) -> Option<ElementDiagnostic> {
    let Some(contract) = element_contract_for_tag(tag) else {
        return Some(ElementDiagnostic {
            tag: tag.to_string(),
            name: name.to_string(),
            kind: ElementDiagnosticKind::UnknownElementTag,
            message: format!("unknown element tag <{tag}>"),
            action: "Use a canonical lowercase MESH element tag or a component tag.".into(),
        });
    };
    if let Some(diagnostic) = validate_known_attribute_value(tag, name, value) {
        return Some(diagnostic);
    }
    if contract.attributes.iter().any(|attr| attr.name == name)
        || name.starts_with("data-")
        || name.starts_with("aria-")
        || name.starts_with("bind:")
        || name.starts_with("on")
    {
        return None;
    }

    Some(ElementDiagnostic {
        tag: tag.to_string(),
        name: name.to_string(),
        kind: ElementDiagnosticKind::UnsupportedAttribute,
        message: format!("unsupported attribute '{name}' on <{tag}>"),
        action: format!(
            "Remove the attribute or use one of: {}",
            contract
                .attributes
                .iter()
                .map(|attr| attr.name)
                .collect::<Vec<_>>()
                .join(", ")
        ),
    })
}

pub(super) fn validate_known_attribute_value(
    tag: &str,
    name: &str,
    value: &str,
) -> Option<ElementDiagnostic> {
    match (tag, name) {
        ("grid", "columns" | "rows") => validate_grid_tracks(tag, name, value),
        ("progress", "min" | "max" | "value") => validate_number_attribute(tag, name, value),
        ("progress", "indeterminate") => validate_bool_attribute(tag, name, value),
        ("button", "icon" | "name" | "src") => Some(invalid_attr(
            tag,
            name,
            "buttons do not accept icon shortcut attributes",
            "Put a dedicated <icon> element inside <button> markup instead.",
        )),
        ("button", "busy" | "default" | "destructive" | "pressed" | "disabled") => {
            validate_bool_attribute(tag, name, value)
        }
        (
            "input" | "textarea" | "search" | "password" | "number-input" | "stepper",
            "disabled" | "readonly" | "required" | "invalid" | "multiline" | "masked",
        ) => validate_bool_attribute(tag, name, value),
        (
            "select" | "option" | "checkbox" | "switch" | "radio" | "radio-group" | "menu"
            | "menu-item" | "command-item" | "preference-row",
            "disabled" | "checked" | "selected" | "expanded" | "required" | "invalid",
        ) => validate_bool_attribute(tag, name, value),
        (
            "popover" | "dialog" | "sheet" | "tabs" | "tab" | "accordion" | "details" | "list"
            | "list-item" | "table" | "cell" | "tree" | "empty-state",
            "open" | "expanded" | "selected" | "active" | "disabled" | "hidden",
        ) => validate_bool_attribute(tag, name, value),
        ("dialog" | "popover", "label" | "aria-label") if value.trim().is_empty() => {
            Some(invalid_attr(
                tag,
                name,
                "interactive containers need a non-empty accessible label",
                "Provide visible text, label, or aria-label for the container.",
            ))
        }
        ("option", "value") if value.trim().is_empty() => Some(invalid_attr(
            tag,
            name,
            "options need a non-empty value",
            "Set value to the string that the parent <select> should receive on change.",
        )),
        ("radio", "value") if value.trim().is_empty() => Some(invalid_attr(
            tag,
            name,
            "radio choices need a non-empty value",
            "Set value to the string that the parent <radio-group> should receive on change.",
        )),
        ("radio", "name") if value.trim().is_empty() => Some(invalid_attr(
            tag,
            name,
            "radio choices need group metadata when not nested in a radio-group",
            "Wrap radios in <radio-group> or set a non-empty name.",
        )),
        ("number-input" | "stepper", "min" | "max" | "value") => {
            validate_number_attribute(tag, name, value)
        }
        ("number-input" | "stepper", "step") => {
            validate_positive_number_attribute(tag, name, value)
        }
        ("button" | "command-button" | "link-button", "form" | "action" | "method") => {
            Some(invalid_attr(
                tag,
                name,
                "browser form behavior is not supported by MESH buttons",
                "Use a Luau handler such as onclick or onactivate.",
            ))
        }
        ("tooltip", "tooltip-for") if value.trim().is_empty() => Some(invalid_attr(
            tag,
            name,
            "tooltip-for must reference a non-empty owner id",
            "Set tooltip-for to the id of the element that owns this tooltip.",
        )),
        ("section" | "header" | "footer" | "group" | "form-row", "value" | "checked") => {
            Some(invalid_attr(
                tag,
                name,
                "structure elements do not expose control value state",
                "Move value state to an input/control element or remove the attribute.",
            ))
        }
        _ => None,
    }
}

pub(super) fn validate_grid_tracks(
    tag: &str,
    name: &str,
    value: &str,
) -> Option<ElementDiagnostic> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Some(invalid_attr(
            tag,
            name,
            "grid tracks cannot be empty",
            "Use a space-separated list of fixed pixel values or auto tracks.",
        ));
    }

    for track in trimmed.split_whitespace() {
        if track == "auto" {
            continue;
        }
        if let Some(px) = track.strip_suffix("px")
            && px.parse::<f32>().is_ok_and(|value| value >= 0.0)
        {
            continue;
        }
        return Some(invalid_attr(
            tag,
            name,
            "unsupported grid track value",
            "Use only fixed pixel tracks like 120px or auto in Phase 87.",
        ));
    }

    None
}

pub(super) fn validate_number_attribute(
    tag: &str,
    name: &str,
    value: &str,
) -> Option<ElementDiagnostic> {
    if value.trim().is_empty() || value.trim().parse::<f32>().is_ok() {
        return None;
    }

    Some(invalid_attr(
        tag,
        name,
        "expected a numeric value",
        "Use a numeric literal or a binding that resolves to a number.",
    ))
}

pub(super) fn validate_positive_number_attribute(
    tag: &str,
    name: &str,
    value: &str,
) -> Option<ElementDiagnostic> {
    if value.trim().is_empty() || value.trim().parse::<f32>().is_ok_and(|parsed| parsed > 0.0) {
        return None;
    }

    Some(invalid_attr(
        tag,
        name,
        "expected a positive numeric value",
        "Use a positive numeric literal or a binding that resolves to one.",
    ))
}

pub(super) fn validate_bool_attribute(
    tag: &str,
    name: &str,
    value: &str,
) -> Option<ElementDiagnostic> {
    if matches!(value.trim(), "" | "true" | "false") {
        return None;
    }

    Some(invalid_attr(
        tag,
        name,
        "expected a boolean value",
        "Use true, false, or omit the value for true.",
    ))
}

pub(super) fn invalid_attr(
    tag: &str,
    name: &str,
    message: &str,
    action: &str,
) -> ElementDiagnostic {
    ElementDiagnostic {
        tag: tag.to_string(),
        name: name.to_string(),
        kind: ElementDiagnosticKind::InvalidAttributeValue,
        message: format!("invalid attribute '{name}' on <{tag}>: {message}"),
        action: action.to_string(),
    }
}

pub fn validate_element_event(tag: &str, event_name: &str) -> Option<ElementDiagnostic> {
    let Some(contract) = element_contract_for_tag(tag) else {
        return Some(ElementDiagnostic {
            tag: tag.to_string(),
            name: event_name.to_string(),
            kind: ElementDiagnosticKind::UnknownElementTag,
            message: format!("unknown element tag <{tag}>"),
            action: "Use a canonical lowercase MESH element tag or a component tag.".into(),
        });
    };
    if contract.events.iter().any(|event| event.name == event_name) {
        return None;
    }

    Some(ElementDiagnostic {
        tag: tag.to_string(),
        name: event_name.to_string(),
        kind: ElementDiagnosticKind::UnsupportedEvent,
        message: format!("unsupported event '{event_name}' on <{tag}>"),
        action: format!(
            "Remove the handler or use one of: {}",
            contract
                .events
                .iter()
                .map(|event| event.name)
                .collect::<Vec<_>>()
                .join(", ")
        ),
    })
}
